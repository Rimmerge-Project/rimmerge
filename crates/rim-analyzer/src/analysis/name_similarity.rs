//! Fuzzy matching for "possible typo" mod-reference detection
//! (`checks::near_miss_mod_references`) — normalization, tokenization,
//! and the sequel/fork suppression rules. Pure string operations, no
//! dependency on any domain type beyond [`crate::domain::NearMissRule`]
//! itself, so this is unit-tested against plain strings.

use std::collections::BTreeMap;
use std::ops::Range;

use crate::domain::NearMissRule;

/// The **longer** of the two normalized forms must clear this length
/// before rule (c) even tries a distance comparison — a short pair has
/// too few characters for "close by edit distance" to mean anything.
/// Only the longer side needs to clear the floor, not both: Levenshtein
/// distance is never smaller than the two strings' own length
/// difference, so a genuinely short candidate (say, 4 characters) can
/// never reach the 0.85 similarity floor against an 8+-character written
/// value anyway (the length gap alone forces too large a distance) — the
/// similarity check itself is what excludes that case, this floor only
/// needs to rule out comparing two short strings to each other, where a
/// single edit is a large fraction of either one (see
/// `a_short_candidate_still_near_misses_against_a_long_enough_written_value`,
/// a confirmed-typo shape this crate must catch: `"Harvest"` itself
/// normalizes to 7 characters).
const MIN_NEAR_MISS_LEN: usize = 8;

/// Rule (c)'s own similarity floor: `1 - distance / max_len >= 0.85`.
const NEAR_MISS_THRESHOLD: f64 = 0.85;

/// Rule (d)'s own cap: the written value's extra leading tokens, at most.
const MAX_LEADING_TOKENS: usize = 2;

/// Rule (d)'s own floor on the **candidate**'s own token count. A
/// one- or two-word candidate (`"Core"`, `"More Gadgets"`) is no
/// evidence at all once it sits at the end of an unrelated compound
/// name — real-install measurement found 22 display names ending in the
/// word "Core" alone (`"Example Effect: Core"`, `"ExampleThunder -
/// Core"`, `"Example Segment - Core"`, and 16 more, none of them a
/// `Core`/DLC typo, plus a handful of two-word candidates that were
/// themselves plausible distinct "More X"/"Big X"-family mods, not
/// typos of the shorter one). A three-or-more-word candidate
/// (`"Example Compat - Fluids"`) has enough of its own content that a
/// shared tail is real evidence instead — every real confirmed typo
/// this rule needs to keep has a candidate at least this long.
const MIN_LEADING_TOKEN_CANDIDATE_LEN: usize = 3;

/// Fork markers rule suppresses, in or out of brackets — see
/// [`is_fork_pair`].
const FORK_MARKERS: [&str; 7] = [
    "continued",
    "updated",
    "fork",
    "forked",
    "redux",
    "reupload",
    "unofficial",
];

/// One flagged similarity, ready to become a
/// [`crate::domain::NearMissModReference`] once the caller picks a
/// candidate mod to attach it to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Match {
    pub rule: NearMissRule,
    /// `1.0` for an exact-after-normalization or case-only match; the
    /// rule (c)/(d) rows carry their own real fraction, used only to
    /// break a tie between two candidates matching under the same rule.
    pub similarity: f64,
}

/// Lowercase, drop every bracketed/parenthesised group (punctuation and
/// contents both), then drop every remaining non-alphanumeric character.
/// The comparison key rules (b) and (c) operate on.
#[must_use]
pub fn normalize(s: &str) -> String {
    strip_bracketed_groups(s)
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Removes every `(...)`/`[...]` group, including its own contents.
/// Nested groups of the *same* bracket kind are handled via a depth
/// counter; a `(` inside a `[...]` group (or vice versa) is treated as
/// part of that outer group rather than its own nesting level — good
/// enough for mod display names and package ids, which never mix bracket
/// kinds in practice.
fn strip_bracketed_groups(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut paren_depth = 0u32;
    let mut bracket_depth = 0u32;
    for c in s.chars() {
        match c {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            _ if paren_depth == 0 && bracket_depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Lowercased, whitespace-separated word tokens of the *original* text —
/// rule (d)'s own comparison unit, and [`is_fork_pair`]'s. Bracket
/// punctuation is stripped to a plain space (unlike [`normalize`], which
/// drops the whole group) so `"Orchard Keeper (Continued)"` tokenizes as
/// `["orchard", "keeper", "continued"]`, not one fused word. A token with no
/// alphanumeric character at all (a lone `"-"` left over from a
/// `"Example Name - Fluids"`-shaped ` - ` separator) is dropped
/// outright: real-install measurement found a real forked mod's own
/// `"<full base name>"` / `"... - Forked"` pair failing
/// [`is_fork_pair`]'s own `longer.len() == shorter.len() + 1` test only
/// because the stray `"-"` token inflated the longer side's count by
/// one — a token that carries no word content should never change how
/// many *real* tokens either side has.
#[must_use]
pub fn tokens(s: &str) -> Vec<String> {
    s.chars()
        .map(|c| {
            if matches!(c, '(' | ')' | '[' | ']') {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .filter(|token| token.chars().any(char::is_alphanumeric))
        .map(str::to_string)
        .collect()
}

/// The bracketed/parenthesised content itself (concatenated, normalized)
/// — [`classify`]'s complement to [`normalize`], which drops this same
/// content entirely. `"[AA] Example Traits"` and `"[BB] Example
/// Traits"` normalize *equal* (rule (b) would otherwise call them a
/// match), but their bracket content is real identity information, not
/// decoration — an author-initials tag distinguishing two different
/// mods sharing a generic title, ground-truthed against a real-install
/// pair with exactly this shape (an uninstalled mod's own author tag
/// vs. the installed one's).
#[must_use]
fn bracket_content(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut paren_depth = 0u32;
    let mut bracket_depth = 0u32;
    for c in s.chars() {
        match c {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            _ if paren_depth > 0 || bracket_depth > 0 => out.push(c),
            _ => {}
        }
    }
    out.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Normalized `.`/whitespace/bracket-delimited segments of `s` — the
/// unit [`near_miss_scope`] trims shared leading/trailing segments from,
/// unifying a dotted package id's own segments (`"author.example.genes"`)
/// and a multi-word display name's own tokens (`"Example Furniture
/// Expanded Pack"`) under one mechanism, since both are "a sequence of
/// meaningful chunks separated by a structural delimiter" the same way.
/// Each segment is passed through [`normalize`] individually (not just
/// lowercased), so `"Example:"` and `"Example"` compare equal as
/// segments and a segment with no alphanumeric content (a lone `.`-
/// separated empty string, or a bare `"-"`) drops out entirely.
fn segments(s: &str) -> Vec<String> {
    s.chars()
        .map(|c| {
            if matches!(c, '(' | ')' | '[' | ']' | '.') {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .map(normalize)
        .filter(|segment| !segment.is_empty())
        .collect()
}

/// The portion of `written`/`candidate` that actually differs, once
/// identical leading and trailing [`segments`] are trimmed from both —
/// rule (c)'s own comparison scope, replacing a whole-string distance
/// measurement that dilutes a short, weak difference sitting inside two
/// long, mostly-identical strings into a falsely high similarity ratio.
/// Ground-truthed against the real install: a mod-framework module id
/// with a long shared dotted prefix, differing only in an 8-character
/// final word (two distinct real module names sharing a `"module"`
/// suffix) that a whole-string ratio all but hides; scoped down to just
/// that differing segment, the same edit distance reads as the weak
/// signal it actually is. Returns the two full, unscoped strings
/// unchanged when either side tokenizes to fewer than two segments (a
/// single-word name has nothing to trim). Both read back [`normalize`]d:
/// normalizing the scoped segments joined by a space is the same as
/// concatenating each segment's own normalized form, which
/// [`PreparedName`] computes once per name.
///
/// Returned as borrowed [`ScopedSide`]s rather than built strings: a
/// caller checks cheap lower bounds on the two scopes first, which almost
/// every pair fails, so the text is only built for the few pairs that
/// reach the edit-distance computation.
fn near_miss_scope<'a>(
    written: &'a PreparedName,
    candidate: &'a PreparedName,
) -> (ScopedSide<'a>, ScopedSide<'a>) {
    let w = &written.segments;
    let c = &candidate.segments;
    if w.len() < 2 || c.len() < 2 {
        return (ScopedSide::whole(written), ScopedSide::whole(candidate));
    }
    let prefix = w.iter().zip(c).take_while(|(a, b)| a == b).count();
    let max_suffix = w.len().min(c.len()) - prefix;
    let suffix = w[prefix..]
        .iter()
        .rev()
        .zip(c[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(max_suffix);
    (
        ScopedSide::segments(written, prefix..w.len() - suffix),
        ScopedSide::segments(candidate, prefix..c.len() - suffix),
    )
}

/// One side of a [`near_miss_scope`]: either the whole normalized name or
/// a run of its normalized segments, read without building the text
/// until a caller actually needs it.
struct ScopedSide<'a> {
    name: &'a PreparedName,
    /// `None` is the whole normalized name.
    segments: Option<Range<usize>>,
}

impl<'a> ScopedSide<'a> {
    fn whole(name: &'a PreparedName) -> Self {
        Self {
            name,
            segments: None,
        }
    }

    fn segments(name: &'a PreparedName, range: Range<usize>) -> Self {
        Self {
            name,
            segments: Some(range),
        }
    }

    /// The scope's length in characters, without building it.
    fn char_len(&self) -> usize {
        match &self.segments {
            None => self.name.normalized_len,
            Some(range) => self.name.normalized_segment_lens[range.clone()]
                .iter()
                .sum(),
        }
    }

    /// The scope's characters, in order.
    fn chars(&self) -> impl Iterator<Item = char> + 'a {
        let pieces: &'a [String] = match &self.segments {
            None => std::slice::from_ref(&self.name.normalized),
            Some(range) => &self.name.normalized_segments[range.clone()],
        };
        pieces.iter().flat_map(|piece| piece.chars())
    }

    /// The scope's own normalized text.
    fn text(&self) -> String {
        match &self.segments {
            None => self.name.normalized.clone(),
            Some(range) => self.name.normalized_segments[range.clone()].concat(),
        }
    }
}

/// One token list is the other plus exactly one recognized fork-marker
/// token, anywhere in the sequence (trailing is the common real-world
/// shape, but this doesn't assume it) — `Orchard Keeper`/`Orchard Keeper
/// (Continued)`. Suppresses what would otherwise be rule (b) once
/// [`normalize`] has dropped the bracketed marker group entirely and left
/// the two strings equal.
fn is_fork_pair(written_tokens: &[String], candidate_tokens: &[String]) -> bool {
    let (shorter, longer) = if written_tokens.len() <= candidate_tokens.len() {
        (written_tokens, candidate_tokens)
    } else {
        (candidate_tokens, written_tokens)
    };
    if longer.len() != shorter.len() + 1 {
        return false;
    }
    (0..longer.len()).any(|i| {
        if !FORK_MARKERS.contains(&longer[i].as_str()) {
            return false;
        }
        let mut without_marker = longer.to_vec();
        without_marker.remove(i);
        without_marker == shorter
    })
}

/// One normalized string is the other plus a trailing run of digits —
/// `Dragonflies`/`Dragonflies 2`, `example.mod.creature`/`...creature2`.
/// Deliberately narrower than "any trailing extra content": an
/// appended digit is the one shape common enough in real sequels to
/// suppress unconditionally without also swallowing a genuine
/// single-character typo (a doubled trailing letter, e.g. `hygiene` ->
/// `hygienee`, is **not** a digit suffix and must still flag).
fn is_digit_sequel(written_normalized: &str, candidate_normalized: &str) -> bool {
    let (shorter, longer) = if written_normalized.len() <= candidate_normalized.len() {
        (written_normalized, candidate_normalized)
    } else {
        (candidate_normalized, written_normalized)
    };
    if shorter.is_empty() || shorter == longer {
        return false;
    }
    longer
        .strip_prefix(shorter)
        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}

/// One side of a [`classify`] comparison, with every derived form
/// the rules read computed once. A caller comparing one written value
/// against a whole mod list prepares each string once instead of
/// re-deriving these per pair — dozens of small allocations per pair,
/// which dominated `checks::near_miss_mod_references` on a real install.
pub struct PreparedName {
    raw: String,
    lowercase: String,
    normalized: String,
    normalized_len: usize,
    tokens: Vec<String>,
    brackets: String,
    segments: Vec<String>,
    /// Each of [`Self::segments`] passed through [`normalize`] once more
    /// — the pieces [`near_miss_scope`] concatenates.
    normalized_segments: Vec<String>,
    /// Each of [`Self::normalized_segments`]' length in characters.
    normalized_segment_lens: Vec<usize>,
}

impl PreparedName {
    /// Derives every form [`classify`] reads from `raw`.
    #[must_use]
    pub fn new(raw: &str) -> Self {
        let normalized = normalize(raw);
        let segments = segments(raw);
        let normalized_segments: Vec<String> =
            segments.iter().map(|segment| normalize(segment)).collect();
        Self {
            raw: raw.to_string(),
            lowercase: raw.to_lowercase(),
            normalized_len: normalized.chars().count(),
            normalized,
            tokens: tokens(raw),
            brackets: bracket_content(raw),
            normalized_segment_lens: normalized_segments
                .iter()
                .map(|segment| segment.chars().count())
                .collect(),
            normalized_segments,
            segments,
        }
    }
}

/// Classifies `written` against one `candidate`, applying every match
/// rule and the sequel/fork suppression in order. `allow_case_only` gates
/// rule (a), which only makes sense for a `FindMod` display name
/// (`PatchOperationFindMod.ApplyWorker` matches it exactly and
/// case-sensitively) — never for a `MayRequire` id, which the game
/// already matches case-insensitively (`ModLister.GetActiveModWithIdentifier`),
/// so a case-only difference there isn't a miss at all.
#[must_use]
pub fn classify(
    written: &PreparedName,
    candidate: &PreparedName,
    allow_case_only: bool,
) -> Option<Match> {
    if written.raw == candidate.raw {
        return None;
    }
    if allow_case_only && written.lowercase == candidate.lowercase {
        return Some(Match {
            rule: NearMissRule::CaseOnly,
            similarity: 1.0,
        });
    }

    let written_normalized = &written.normalized;
    let candidate_normalized = &candidate.normalized;
    if written_normalized.is_empty() || candidate_normalized.is_empty() {
        return None;
    }
    if is_fork_pair(&written.tokens, &candidate.tokens)
        || is_digit_sequel(written_normalized, candidate_normalized)
    {
        return None;
    }
    // Bracket content the two sides don't share is real identity
    // information (an author-initials tag, not decoration) — see
    // `bracket_content`'s own doc comment. Checked before rule (b), since
    // `normalize` would otherwise drop it and call the two strings equal.
    let written_brackets = &written.brackets;
    let candidate_brackets = &candidate.brackets;
    if !written_brackets.is_empty()
        && !candidate_brackets.is_empty()
        && written_brackets != candidate_brackets
    {
        return None;
    }

    if written_normalized == candidate_normalized {
        return Some(Match {
            rule: NearMissRule::Normalized,
            similarity: 1.0,
        });
    }

    if written.normalized_len.max(candidate.normalized_len) >= MIN_NEAR_MISS_LEN {
        // Scoped to the differing segment, not the whole string — see
        // `near_miss_scope`'s own doc comment. The floor above still
        // reads the *full* normalized length, so a short scoped
        // difference inside a long, mostly-shared name (`"roaylty"` /
        // `"royalty"`, 7 characters each, inside a 21-character id) still
        // reaches this distance check; only the ratio itself is scoped.
        let (written_scope, candidate_scope) = near_miss_scope(written, candidate);
        let written_scope_len = written_scope.char_len();
        let candidate_scope_len = candidate_scope.char_len();
        let max_len = written_scope_len.max(candidate_scope_len);
        // The length difference, then the character-count difference
        // ([`char_bag_distance`]), are lower bounds on the edit distance,
        // so a pair that misses the threshold even at those bounds skips
        // the quadratic distance computation — most of the active-mod
        // list, for every written value.
        let could_reach = |lower_bound: usize| {
            max_len > 0 && scope_similarity(lower_bound, max_len) >= NEAR_MISS_THRESHOLD
        };
        if could_reach(written_scope_len.abs_diff(candidate_scope_len))
            && could_reach(char_bag_distance(&written_scope, &candidate_scope))
        {
            let distance =
                strsim::damerau_levenshtein(&written_scope.text(), &candidate_scope.text());
            let similarity = scope_similarity(distance, max_len);
            if similarity >= NEAR_MISS_THRESHOLD {
                return Some(Match {
                    rule: NearMissRule::NearMiss,
                    similarity,
                });
            }
        }
    }

    let written_tokens = &written.tokens;
    let candidate_tokens = &candidate.tokens;
    if candidate_tokens.len() >= MIN_LEADING_TOKEN_CANDIDATE_LEN
        && written_tokens.len() > candidate_tokens.len()
        && written_tokens.len() - candidate_tokens.len() <= MAX_LEADING_TOKENS
        && written_tokens.ends_with(candidate_tokens.as_slice())
    {
        let similarity = candidate_tokens.len() as f64 / written_tokens.len() as f64;
        return Some(Match {
            rule: NearMissRule::LeadingToken,
            similarity,
        });
    }

    None
}

/// Rule (c)'s similarity ratio for an edit `distance` between two scoped
/// segments whose longer one is `max_len` characters.
fn scope_similarity(distance: usize, max_len: usize) -> f64 {
    1.0 - (distance as f64 / max_len as f64)
}

/// The bag distance between two scopes: how many characters one has
/// that the other lacks, counted with multiplicity, whichever side has
/// more. A lower bound on their Damerau-Levenshtein distance — an
/// insertion, deletion or substitution changes each side's surplus by at
/// most one, and a transposition changes neither — and far cheaper to
/// compute: one pass, and no allocation for the ASCII text normalized
/// names almost always are.
fn char_bag_distance(a: &ScopedSide<'_>, b: &ScopedSide<'_>) -> usize {
    let mut ascii_surplus = [0_isize; 128];
    let mut other_surplus: BTreeMap<char, isize> = BTreeMap::new();
    let mut tally = |c: char, delta: isize| match ascii_surplus.get_mut(c as usize) {
        Some(count) => *count += delta,
        None => *other_surplus.entry(c).or_default() += delta,
    };
    a.chars().for_each(|c| tally(c, 1));
    b.chars().for_each(|c| tally(c, -1));
    let (only_in_a, only_in_b) =
        ascii_surplus
            .iter()
            .chain(other_surplus.values())
            .fold((0, 0), |(more, fewer), &count| {
                (
                    more + count.max(0).unsigned_abs(),
                    fewer + count.min(0).unsigned_abs(),
                )
            });
    only_in_a.max(only_in_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify(written: &str, candidate: &str, allow_case_only: bool) -> Option<Match> {
        super::classify(
            &PreparedName::new(written),
            &PreparedName::new(candidate),
            allow_case_only,
        )
    }

    #[test]
    fn scoped_segments_match_normalizing_the_joined_scope() {
        let pairs = [
            ("author.example.genes", "author.example.genez"),
            (
                "Example Furniture Expanded Pack",
                "Example Furnitur Expanded Pack",
            ),
            ("İstanbul Ünïcode.Mod", "İstanbul Unicode.Mod"),
            ("[AA] Example: Traits (Fork)", "[AA] Example Trait"),
            ("single", "author.example"),
        ];
        for (written, candidate) in pairs {
            let written_segments = segments(written);
            let candidate_segments = segments(candidate);
            let (written_prepared, candidate_prepared) =
                (PreparedName::new(written), PreparedName::new(candidate));
            let (written_side, candidate_side) =
                near_miss_scope(&written_prepared, &candidate_prepared);
            let (written_scope, candidate_scope) = (written_side.text(), candidate_side.text());
            assert_eq!(
                (written_side.char_len(), candidate_side.char_len()),
                (
                    written_scope.chars().count(),
                    candidate_scope.chars().count()
                ),
                "scope lengths of {written:?} vs {candidate:?}"
            );

            let (expected_written, expected_candidate) = if written_segments.len() < 2
                || candidate_segments.len() < 2
            {
                (normalize(written), normalize(candidate))
            } else {
                let prefix = written_segments
                    .iter()
                    .zip(&candidate_segments)
                    .take_while(|(a, b)| a == b)
                    .count();
                let max_suffix = written_segments.len().min(candidate_segments.len()) - prefix;
                let suffix = written_segments[prefix..]
                    .iter()
                    .rev()
                    .zip(candidate_segments[prefix..].iter().rev())
                    .take_while(|(a, b)| a == b)
                    .count()
                    .min(max_suffix);
                (
                    normalize(&written_segments[prefix..written_segments.len() - suffix].join(" ")),
                    normalize(
                        &candidate_segments[prefix..candidate_segments.len() - suffix].join(" "),
                    ),
                )
            };

            assert_eq!(
                (written_scope, candidate_scope),
                (expected_written, expected_candidate),
                "{written:?} vs {candidate:?}"
            );
        }
    }

    #[test]
    fn case_only_difference_is_rule_a() {
        let result = classify("example sidearms", "Example Sidearms", true);
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::CaseOnly));
    }

    #[test]
    fn case_only_is_never_offered_when_disallowed() {
        // Not equal, not normalized-equal (differ only by case, but
        // normalize() lowercases both anyway) — so with case-only off this
        // must fall through to rule (b) instead of vanishing outright.
        let result = classify("example sidearms", "Example Sidearms", false);
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::Normalized));
    }

    #[test]
    fn bracketed_suffix_is_normalized_equal() {
        // No fork marker inside the brackets, so nothing suppresses this
        // — it's a genuine normalized-equal match.
        let result = classify(
            "Example Sidearms",
            "Example Sidearms [Optional Addon]",
            false,
        );
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::Normalized));
    }

    #[test]
    fn one_transposition_in_long_name_is_near_miss() {
        let result = classify("ludeon.rimworld.roaylty", "ludeon.rimworld.royalty", false);
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::NearMiss));
    }

    /// Regression: `"Harvest"` normalizes to 7 characters, one short of
    /// `MIN_NEAR_MISS_LEN` — before the `written_len.max(candidate_len)`
    /// fix, a strictly-both-sides gate silently dropped this real
    /// typo shape (`"Harvvest"`, an extra `v`) even though the
    /// written value itself clears the floor comfortably.
    #[test]
    fn a_short_candidate_still_near_misses_against_a_long_enough_written_value() {
        let result = classify("Harvvest", "Harvest", true);
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::NearMiss));
    }

    #[test]
    fn a_doubled_trailing_letter_is_near_miss_not_a_suppressed_sequel() {
        let result = classify(
            "example.modframework.hygienee",
            "example.modframework.hygiene",
            false,
        );
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::NearMiss));
    }

    #[test]
    fn short_names_never_near_miss() {
        // "Exampl" / "Example" normalize to 6 and 7 characters, below the
        // floor, even though one edit is a 0.857 whole-string similarity.
        assert_eq!(classify("Exampl", "Example", false), None);
    }

    #[test]
    fn trailing_digit_sequel_is_suppressed() {
        assert_eq!(classify("Dragonflies", "Dragonflies 2", false), None);
        assert_eq!(
            classify("example.mod.creature", "example.mod.creature2", false),
            None
        );
    }

    #[test]
    fn continued_fork_marker_is_suppressed() {
        assert_eq!(
            classify("Orchard Keeper", "Orchard Keeper (Continued)", false),
            None
        );
    }

    #[test]
    fn leading_author_token_is_rule_d() {
        let result = classify(
            "Zorrin Example Faction - Addon",
            "Example Faction - Addon",
            false,
        );
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::LeadingToken));
    }

    #[test]
    fn three_extra_leading_tokens_is_not_rule_d() {
        let result = classify(
            "A B C Example Compat Fluids",
            "Example Compat Fluids",
            false,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn identical_strings_never_match() {
        assert_eq!(classify("Same Name", "Same Name", true), None);
    }

    // -- false-positive regressions from the real-install precision pass --

    /// A single-word candidate is no evidence for rule (d): real-install
    /// measurement found 22 distinct framework mods all ending in the
    /// generic word `"Core"` alone, none of them a typo'd reference to
    /// `ludeon.rimworld`'s own display name `"Core"`.
    #[test]
    fn a_single_word_candidate_is_never_a_leading_token_match() {
        assert_eq!(
            classify("Example Framework: Core", "Core", false),
            None,
            "a shared trailing generic word is not evidence of a typo"
        );
    }

    /// A two-word candidate is also no evidence — real-install case: a
    /// two-word "More X"/"Big X"-family mod name is plausibly a distinct,
    /// genuinely different real mod, not a typo of the shorter one; every
    /// real confirmed typo this rule needs to keep has a candidate at
    /// least three words long.
    #[test]
    fn a_two_word_candidate_is_never_a_leading_token_match() {
        assert_eq!(
            classify("Even More Example Linkables", "Example Linkables", false),
            None,
            "a two-word candidate is still too weak — a real distinct mod family, not a typo"
        );
    }

    /// A multi-word candidate still has real content, so a genuine
    /// author-name prefix in front of it stays rule (d) — the real
    /// confirmed-typo shape this crate must still catch (an author name
    /// glued in front of an otherwise-exact compatibility-patch title).
    #[test]
    fn a_multi_word_candidate_still_matches_leading_token() {
        let result = classify(
            "SomeAuthor Example Compat - Fluids",
            "Example Compat - Fluids",
            false,
        );
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::LeadingToken));
    }

    /// A fork-marker token next to a stray punctuation-only token (`"-"`)
    /// must still suppress: `"... - (Forked)"` failed `is_fork_pair`'s own
    /// length check before `tokens` dropped the lone `"-"`.
    #[test]
    fn fork_marker_with_a_stray_hyphen_token_still_suppresses() {
        assert_eq!(
            classify("Example Mod Name", "Example Mod Name - (Forked)", false),
            None
        );
    }

    /// Two dotted ids sharing a long common prefix but differing in a
    /// short, unrelated final segment are two different mods, not a
    /// typo. The whole-string similarity here is about 0.93, which would
    /// flag; scoped to the differing segment it is about 0.79, which
    /// does not.
    #[test]
    fn distinct_dotted_ids_with_a_long_shared_prefix_do_not_near_miss() {
        assert_eq!(
            classify(
                "author.exampleframeworkexpanded.mountainmodule",
                "author.exampleframeworkexpanded.mountedmodule",
                false,
            ),
            None
        );
    }

    /// A candidate id that *extends* the written id with more characters
    /// in its own final segment is a different mod: the whole-string
    /// similarity is about 0.91, the scoped one about 0.77.
    #[test]
    fn a_candidate_id_extending_the_written_ids_own_segment_does_not_near_miss() {
        assert_eq!(
            classify(
                "author.exampleframework.examplemod",
                "author.exampleframework.examplemodzoo",
                false
            ),
            None
        );
    }

    /// Two multi-word display names sharing a long leading phrase but
    /// differing in their own last word are two different mods: the
    /// whole-string similarity is about 0.97, the scoped one 0.83.
    #[test]
    fn distinct_names_with_a_long_shared_suffix_do_not_near_miss() {
        assert_eq!(
            classify(
                "Example Item Collection Mythic",
                "Example Item Collection Mystic",
                false
            ),
            None
        );
    }

    /// A short, differing segment inside an otherwise-identical long
    /// dotted id must still reach the distance check — the floor reads
    /// the *full* id's length, not the scoped segment's, so this must
    /// not regress alongside the "distinct ids" fixes above.
    #[test]
    fn a_short_differing_segment_in_a_long_shared_id_still_near_misses() {
        let result = classify(
            "author.exampleframework.roaylty",
            "author.exampleframework.royalty",
            false,
        );
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::NearMiss));
    }

    /// Differing, non-empty bracket content on both sides is real
    /// identity information (an author-initials tag), not decoration —
    /// real-install case: `"[AA] Example Traits"` / `"[BB]
    /// Example Traits"`, which `normalize`'s own bracket-dropping
    /// otherwise made compare equal under rule (b).
    #[test]
    fn differing_bracket_tags_on_both_sides_suppress_the_match() {
        assert_eq!(
            classify("[AA] Example Traits", "[BB] Example Traits", false),
            None
        );
    }

    /// The bracket-conflict guard only fires when *both* sides carry
    /// bracket content — one side having none (a plain, undecorated
    /// name) is the ordinary fork/version-tag shape and must still
    /// match.
    #[test]
    fn a_bracket_on_only_one_side_still_matches_normalized() {
        let result = classify("Example Traits", "[BB] Example Traits", false);
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::Normalized));
    }

    /// The segment-scoping fixes above must not swallow a genuine single-
    /// character-missing typo at the very start of a multi-word name —
    /// a confirmed real-install case, a written value missing its own
    /// leading letter against the real mod's full display name.
    #[test]
    fn a_missing_leading_letter_in_a_multi_word_name_still_near_misses() {
        let result = classify(
            "xample Framework Addon - Android",
            "Example Framework Addon - Android",
            false,
        );
        assert_eq!(result.map(|m| m.rule), Some(NearMissRule::NearMiss));
    }

    /// The bag distance of two whole single-segment names.
    fn bag_distance(a: &str, b: &str) -> usize {
        let (a, b) = (PreparedName::new(a), PreparedName::new(b));
        char_bag_distance(&ScopedSide::whole(&a), &ScopedSide::whole(&b))
    }

    #[test]
    fn bag_distance_counts_the_larger_surplus_with_multiplicity() {
        // "aab" vs "bcc": `a` twice only on the left, `c` twice only on
        // the right — two substitutions, and the bound says two.
        assert_eq!(bag_distance("aab", "bcc"), 2);
        // A transposition changes no character counts at all.
        assert_eq!(bag_distance("abcd", "abdc"), 0);
        // Pure insertions: the surplus is entirely on one side.
        assert_eq!(bag_distance("abc", "abcxyz"), 3);
    }

    #[test]
    fn bag_distance_counts_non_ascii_characters_too() {
        assert_eq!(bag_distance("ünï", "uni"), 2);
        assert_eq!(bag_distance("ünï", "ïün"), 0);
    }

    proptest::proptest! {
        /// The prefilter in `classify` is only sound if the bag distance
        /// never exceeds the real Damerau-Levenshtein distance — otherwise
        /// it would skip a pair that is really a near miss.
        #[test]
        fn bag_distance_never_exceeds_the_edit_distance(
            a in "[a-dü0-2]{0,12}",
            b in "[a-dü0-2]{0,12}",
        ) {
            let (prepared_a, prepared_b) = (PreparedName::new(&a), PreparedName::new(&b));
            let (side_a, side_b) = (ScopedSide::whole(&prepared_a), ScopedSide::whole(&prepared_b));
            let bound = char_bag_distance(&side_a, &side_b);
            let distance = strsim::damerau_levenshtein(&side_a.text(), &side_b.text());
            proptest::prop_assert!(bound <= distance, "{a:?} vs {b:?}: {bound} > {distance}");
        }
    }
}
