//! Best-effort parsing of the def(s) a patch `<xpath>` targets.
//!
//! Patch xpaths are free-form XPath and RimWorld mods use it loosely, but
//! the overwhelming majority follow one of a few shapes:
//! `Defs/ThingDef[defName="Wall"]/statBases`,
//! `/Defs/ThingDef[defName="Wall"]`,
//! `*/ThingDef[defName="Wall"]` (a wildcard-root prefix used by many
//! workshop mods — equivalent to `Defs/ThingDef` since the document root
//! is always `<Defs>`), `//ThingDef[defName="Wall"]`,
//! `Defs/ThingDef[@Name="WallBase"]`,
//! `Defs/ThingDef[defName="X" or defName="Y"]/...` (**both** defs are
//! targeted — see [`parse_all`]), `Defs/ThingDef[defName="X"][not(comps)]`
//! (extra predicates on the def node itself), and `defName`/`@Name` values
//! quoted with either `"` or `'`.
//! Anything that doesn't match this shape yields no target — the raw
//! xpath string is kept on [`crate::domain::PatchOp`] regardless.
//!
//! This module is deliberately *loose*: it exists to answer "which def(s)
//! does this op touch" for collision detection, and a missed name there
//! only costs a report entry. The strict, quote-aware grammar a *replay*
//! needs (which must never mis-apply an op) lives in
//! [`crate::extract::xpath_expr`], which shares this module's head
//! location/bracket scanning but derives the targeted def names from its
//! own parsed predicate tree rather than from the regex scan below.

use std::sync::LazyLock;

use regex::Regex;

use crate::domain::{DefTarget, Selector};

/// The head of a patch xpath, up to and including the def predicate's
/// closing `]`: `Defs/ThingDef[defName="Wall"]`.
static HEAD_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Invariant: this is a fixed literal pattern, not user input — a typo
    // here is a compile-time-discoverable bug, caught by the tests below.
    #[allow(clippy::expect_used)]
    {
        Regex::new(
            r"(?x)
            (?:/?Defs/|\*/|//)
            (?P<def_type>[A-Za-z_][\w.:-]*)
            \s*\[
        ",
        )
        .expect("HEAD_RE is a fixed, compile-time-checked pattern")
    }
});

/// Every `defName="..."` / `@Name="..."` equality inside a head
/// predicate, in source order. `\b` before `defName` keeps a longer
/// identifier ending in it (`myDefName=`) from matching.
static HEAD_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(
            r#"(?x)
            (?: \bdefName \s* = \s*
                  (?: "(?P<def_name_dq>[^"]*)" | '(?P<def_name_sq>[^']*)' )
              | @Name \s* = \s*
                  (?: "(?P<name_attr_dq>[^"]*)" | '(?P<name_attr_sq>[^']*)' ) )
        "#,
        )
        .expect("HEAD_NAME_RE is a fixed, compile-time-checked pattern")
    }
});

/// A `@ParentName="..."` equality inside a head predicate — the first
/// one, same "tolerant, take the first" convention as every other
/// pattern here. Deliberately its own regex, not folded into
/// [`HEAD_NAME_RE`]: unlike `defName=`/`@Name=`, this predicate never
/// *names* its own target — it selects every other def/template whose
/// own `ParentName` attribute equals this value, which only a real
/// scan-wide index (`SourceIndex::children_by_template`) can resolve.
/// This module has no such index (see the crate's own top-of-file doc
/// comment: `extract/` is pure, no cross-mod knowledge at all) —
/// [`parent_name_predicate`] only ever locates and reports the query
/// itself; [`crate::analysis::edges::parent_name_targets`] is where it's
/// actually answered, against a real index, once one exists.
static HEAD_PARENT_NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used)]
    {
        Regex::new(
            r#"(?x)
            @ParentName \s* = \s*
                (?: "(?P<parent_name_dq>[^"]*)" | '(?P<parent_name_sq>[^']*)' )
        "#,
        )
        .expect("HEAD_PARENT_NAME_RE is a fixed, compile-time-checked pattern")
    }
});

/// A located xpath head: the def type it names, its bracketed predicate's
/// inner text, and where the whole head ends.
pub(super) struct HeadSpan<'a> {
    /// The def type between the root prefix and the `[`.
    pub def_type: &'a str,
    /// The head bracket's inner text (`Defs/ThingDef[` **here** `]`).
    pub predicate: &'a str,
    /// The byte index just past the head bracket's closing `]`.
    pub end: usize,
}

/// Locates `xpath`'s head, or `None` when it doesn't have the recognized
/// `<root>/<DefType>[...]` shape (or the bracket is unbalanced).
pub(super) fn locate_head(xpath: &str) -> Option<HeadSpan<'_>> {
    let captures = HEAD_RE.captures(xpath)?;
    let def_type = captures.name("def_type")?.as_str();
    // The pattern ends with the literal `[`, so the match ends one byte
    // past it.
    let bracket_start = captures.get(0)?.end() - 1;
    let close = find_matching_bracket(&xpath[bracket_start..])?;
    let end = bracket_start + close + 1;
    Some(HeadSpan {
        def_type,
        predicate: &xpath[bracket_start + 1..end - 1],
        end,
    })
}

/// Splits the text following a head into the extra `[...]` predicates
/// applying to the *def node itself* and whatever path follows them.
/// `None` when one of those brackets is unbalanced.
pub(super) fn root_predicate_brackets(after_head: &str) -> Option<(Vec<&str>, &str)> {
    let mut rest = after_head;
    let mut brackets = Vec::new();
    while rest.starts_with('[') {
        let close = find_matching_bracket(rest)?;
        brackets.push(&rest[1..close]);
        rest = &rest[close + 1..];
    }
    Some((brackets, rest))
}

/// The index, relative to `s`, of the `]` matching `s`'s leading `[`,
/// skipping any `[`/`]` characters that occur inside a quoted literal
/// (e.g. an attribute value containing a literal bracket). `s` must
/// start with `[`.
///
/// `pub(crate)`, not `pub(super)`, for the `PatchInvalidatesPredicate`
/// producer (`analysis::edges::predicate_invalidation_shape`): that function
/// walks a raw `sub_path`'s own `[...]` predicate steps with the identical
/// quote-aware bracket-matching rule this module already has, and a second
/// hand-rolled copy in `analysis::edges` would be exactly the kind of drift
/// this crate's own two-xpath-parsers-on-purpose doc comment
/// (`crates/rim-analyzer/CLAUDE.md`) warns against for the *parsing* rules,
/// even though the two modules solve different problems.
pub(crate) fn find_matching_bracket(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for (index, ch) in s.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// Every def a patch xpath's head names, in source order and
/// deduplicated: one target for an ordinary
/// `Defs/<Type>[defName="X"]` head, several for a disjunctive
/// `[defName="X" or defName="Y" or @Name="Z"]` one (RimWorld applies such
/// an op to *every* matching def, so the analyzer must index it under
/// each). Empty when the xpath doesn't have a recognized head at all.
///
/// Every returned target shares the same `sub_path`: whatever follows the
/// head *and* any extra predicates on the def node itself
/// (`[defName="X"][not(comps)]/comps` -> `comps`), so two mods patching
/// the same field collide on one key whether or not they guard it with a
/// root predicate.
#[must_use]
pub fn parse_all(xpath: &str) -> Vec<DefTarget> {
    let Some(head) = locate_head(xpath) else {
        return Vec::new();
    };
    let after_head = &xpath[head.end..];
    let remainder = root_predicate_brackets(after_head).map_or(after_head, |(_, rest)| rest);
    let sub_path = remainder.trim_start_matches('/');
    let sub_path = (!sub_path.is_empty()).then(|| sub_path.to_string());

    let mut targets: Vec<DefTarget> = Vec::new();
    for captures in HEAD_NAME_RE.captures_iter(head.predicate) {
        let def_name_match = captures
            .name("def_name_dq")
            .or_else(|| captures.name("def_name_sq"));
        let (def_name, selector) = match def_name_match {
            Some(m) => (m.as_str().to_string(), Selector::DefName),
            None => match captures
                .name("name_attr_dq")
                .or_else(|| captures.name("name_attr_sq"))
            {
                Some(m) => (m.as_str().to_string(), Selector::NameAttr),
                None => continue,
            },
        };
        let target = DefTarget {
            def_type: head.def_type.to_string(),
            def_name,
            selector,
            sub_path: sub_path.clone(),
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

/// The *first* def a patch xpath targets — [`parse_all`]'s head, kept for
/// callers (`PatchOp::target`, `analysis::edges`) that only need one
/// representative def.
#[must_use]
pub fn parse(xpath: &str) -> Option<DefTarget> {
    parse_all(xpath).into_iter().next()
}

/// A `[@ParentName="X"]`-headed xpath's own `(def_type, parent_name,
/// sub_path)`, when its head predicate carries that equality — `None`
/// for anything else, including a head that *also* names a `defName`/
/// `@Name` (that shape is already fully resolvable by [`parse_all`]
/// alone; this function exists only for the case that one can't touch
/// at all — see [`HEAD_PARENT_NAME_RE`]'s own doc comment for why
/// resolving *which* defs match is this module's caller's job, never
/// this module's own). `sub_path` is computed exactly the way
/// [`parse_all`] computes it — whatever follows the head and any extra
/// root-level predicates — so a `[@ParentName="X"][not(comps)]/comps`
/// head reports `sub_path: Some("comps")`, the identical convention
/// every returned child target then shares.
#[must_use]
pub fn parent_name_predicate(xpath: &str) -> Option<(String, String, Option<String>)> {
    let head = locate_head(xpath)?;
    let captures = HEAD_PARENT_NAME_RE.captures(head.predicate)?;
    let parent_name = captures
        .name("parent_name_dq")
        .or_else(|| captures.name("parent_name_sq"))?
        .as_str()
        .to_string();

    let after_head = &xpath[head.end..];
    let remainder = root_predicate_brackets(after_head).map_or(after_head, |(_, rest)| rest);
    let sub_path = remainder.trim_start_matches('/');
    let sub_path = (!sub_path.is_empty()).then(|| sub_path.to_string());

    Some((head.def_type.to_string(), parent_name, sub_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_def_name_target() {
        let target = parse(r#"Defs/ThingDef[defName="Wall"]"#).unwrap();
        assert_eq!(target.def_type, "ThingDef");
        assert_eq!(target.def_name, "Wall");
        assert_eq!(target.selector, Selector::DefName);
        assert_eq!(target.sub_path, None);
    }

    #[test]
    fn parses_target_with_sub_path() {
        let target = parse(r#"Defs/ThingDef[defName="Wall"]/statBases"#).unwrap();
        assert_eq!(target.def_type, "ThingDef");
        assert_eq!(target.def_name, "Wall");
        assert_eq!(target.sub_path.as_deref(), Some("statBases"));
    }

    #[test]
    fn parses_leading_slash_form() {
        let target = parse(r#"/Defs/ThingDef[defName="Wall"]"#).unwrap();
        assert_eq!(target.def_type, "ThingDef");
        assert_eq!(target.def_name, "Wall");
    }

    #[test]
    fn parses_wildcard_root_prefix() {
        let target = parse(r#"*/ThingDef[defName="Wall"]"#).unwrap();
        assert_eq!(target.def_type, "ThingDef");
        assert_eq!(target.def_name, "Wall");
    }

    #[test]
    fn parses_double_slash_prefix() {
        let target = parse(r#"//ThingDef[defName="Wall"]"#).unwrap();
        assert_eq!(target.def_type, "ThingDef");
        assert_eq!(target.def_name, "Wall");
    }

    #[test]
    fn parses_single_quoted_def_name() {
        let target = parse(r"Defs/ThingDef[defName='Wall']").unwrap();
        assert_eq!(target.def_name, "Wall");
    }

    #[test]
    fn parses_name_attribute_form() {
        let target = parse(r#"Defs/ThingDef[@Name="WallBase"]"#).unwrap();
        assert_eq!(target.def_type, "ThingDef");
        assert_eq!(target.def_name, "WallBase");
        assert_eq!(target.selector, Selector::NameAttr);
    }

    #[test]
    fn parses_single_quoted_name_attribute() {
        let target = parse(r"Defs/ThingDef[@Name='WallBase']").unwrap();
        assert_eq!(target.def_name, "WallBase");
        assert_eq!(target.selector, Selector::NameAttr);
    }

    #[test]
    fn parses_predicate_with_extra_surrounding_whitespace() {
        let target = parse(r#"Defs/ThingDef[ defName = "Wall" ]"#).unwrap();
        assert_eq!(target.def_name, "Wall");
    }

    #[test]
    fn parses_namespaced_def_type() {
        let target = parse(r#"Defs/example.PartDef[defName="Human_PartA"]"#).unwrap();
        assert_eq!(target.def_type, "example.PartDef");
    }

    #[test]
    fn free_form_xpath_yields_no_target() {
        assert_eq!(parse("Defs/ThingDef/statBases"), None);
        assert_eq!(parse(""), None);
        assert!(parse_all("Defs/ThingDef/statBases").is_empty());
    }

    // --- multi-def heads --------------------------------------------------

    #[test]
    fn parse_all_returns_every_def_an_or_predicate_names() {
        let targets = parse_all(r#"Defs/ThingDef[defName="X" or defName="Y"]/statBases"#);
        assert_eq!(
            targets
                .iter()
                .map(|t| t.def_name.as_str())
                .collect::<Vec<_>>(),
            vec!["X", "Y"]
        );
        assert!(
            targets
                .iter()
                .all(|t| t.sub_path.as_deref() == Some("statBases"))
        );
        // `parse` keeps the first, as every existing caller expects.
        assert_eq!(
            parse(r#"Defs/ThingDef[defName="X" or defName="Y"]"#)
                .unwrap()
                .def_name,
            "X"
        );
    }

    #[test]
    fn parse_all_mixes_def_name_and_name_attribute_leaves() {
        let targets = parse_all(r#"*/ThingDef[defName='X' or @Name = "B"]"#);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].selector, Selector::DefName);
        assert_eq!(targets[1].selector, Selector::NameAttr);
        assert_eq!(targets[1].def_name, "B");
    }

    #[test]
    fn parse_all_is_best_effort_across_and_composition_too() {
        // Real shape from workshop patches. `xpath_expr` refuses to
        // *replay* this (an `and` in the head), but collision detection
        // still wants both defs.
        let targets = parse_all(r#"*/ThingDef[(defName = "X" or defName = "Y") and not(comps)]"#);
        assert_eq!(
            targets
                .iter()
                .map(|t| t.def_name.as_str())
                .collect::<Vec<_>>(),
            vec!["X", "Y"]
        );
    }

    #[test]
    fn parse_all_deduplicates_a_repeated_name() {
        let targets = parse_all(r#"Defs/ThingDef[defName="X" or defName="X"]"#);
        assert_eq!(targets.len(), 1);
    }

    #[test]
    fn a_longer_identifier_ending_in_def_name_is_not_a_def_name() {
        assert!(parse_all(r#"Defs/ThingDef[myDefName="X"]"#).is_empty());
    }

    // --- root predicates --------------------------------------------------

    /// Extra predicates on the def node itself are not part of the
    /// `sub_path`: two mods patching `/comps`, one of them guarded by
    /// `[not(tags)]`, must land on the same collision key.
    #[test]
    fn root_predicates_are_excluded_from_the_sub_path() {
        let target = parse(r#"Defs/ThingDef[defName="X"][not(tags)][comps]/comps"#).unwrap();
        assert_eq!(target.sub_path.as_deref(), Some("comps"));
    }

    #[test]
    fn root_predicates_with_nothing_after_them_leave_no_sub_path() {
        let target = parse(r#"Defs/ThingDef[defName="X"][not(comps)]"#).unwrap();
        assert_eq!(target.sub_path, None);
    }

    /// A `]` inside a quoted value must not end the head early.
    #[test]
    fn a_bracket_inside_a_quoted_value_does_not_close_the_head() {
        let target = parse(r#"Defs/ThingDef[defName="a]b"]/statBases"#).unwrap();
        assert_eq!(target.def_name, "a]b");
        assert_eq!(target.sub_path.as_deref(), Some("statBases"));
    }

    #[test]
    fn an_unbalanced_head_bracket_yields_no_target() {
        assert!(parse_all(r#"Defs/ThingDef[defName="X""#).is_empty());
    }

    // --- @ParentName= heads ------------------------------------------------

    /// A `[@ParentName="X"]` head names no def directly — `parse_all`
    /// correctly finds nothing (it can only extract a name the xpath
    /// itself writes, and this predicate doesn't write one).
    #[test]
    fn parent_name_headed_ops_yield_no_ordinary_target() {
        assert!(parse_all(r#"Defs/ThingDef[@ParentName="WallBase"]"#).is_empty());
    }

    #[test]
    fn parent_name_predicate_finds_the_type_and_parent_name() {
        let (def_type, parent_name, sub_path) =
            parent_name_predicate(r#"Defs/ThingDef[@ParentName="WallBase"]"#).unwrap();
        assert_eq!(def_type, "ThingDef");
        assert_eq!(parent_name, "WallBase");
        assert_eq!(sub_path, None);
    }

    #[test]
    fn parent_name_predicate_computes_a_sub_path() {
        let (_, _, sub_path) =
            parent_name_predicate(r#"Defs/ThingDef[@ParentName="WallBase"]/statBases"#).unwrap();
        assert_eq!(sub_path.as_deref(), Some("statBases"));
    }

    #[test]
    fn parent_name_predicate_excludes_extra_root_predicates_from_the_sub_path() {
        let (_, _, sub_path) =
            parent_name_predicate(r#"Defs/ThingDef[@ParentName="WallBase"][not(comps)]/comps"#)
                .unwrap();
        assert_eq!(sub_path.as_deref(), Some("comps"));
    }

    #[test]
    fn parent_name_predicate_accepts_single_quotes() {
        let (_, parent_name, _) =
            parent_name_predicate(r"Defs/ThingDef[@ParentName='WallBase']").unwrap();
        assert_eq!(parent_name, "WallBase");
    }

    #[test]
    fn parent_name_predicate_is_none_for_an_ordinary_def_name_head() {
        assert!(parent_name_predicate(r#"Defs/ThingDef[defName="Wall"]"#).is_none());
    }

    #[test]
    fn parent_name_predicate_is_none_for_free_form_xpath() {
        assert!(parent_name_predicate("Defs/ThingDef/statBases").is_none());
    }
}
