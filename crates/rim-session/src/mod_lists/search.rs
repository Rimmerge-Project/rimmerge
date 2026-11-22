//! Where a Mods-list search matched inside a row's name and id.
//!
//! The list filter admits a row when its folded id or name contains the
//! folded search text ([`search_fold`]: Unicode lowercase, one character at a
//! time, final sigma written as `σ`). This module answers the
//! same question for display: *which parts* of the original text matched,
//! so an interface can mark them and never disagree with the filter.
//!
//! Ranges are **byte** ranges into the original (not lowercased) text, on
//! character boundaries, ascending and non-overlapping. Converting them to
//! another unit (the desktop's UTF-16 code units) is the interface's job,
//! done once at its boundary.

use std::ops::Range;

use super::ModListRow;
use crate::search_fold::{fold_char, search_fold};

/// The matched parts of one row's two searchable texts. Both are empty
/// when no search ran; under a search a row is listed only when at least
/// one is not (it can match in one field only).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchHighlight {
    /// Matched byte ranges of the row's display name.
    pub name: Vec<Range<usize>>,
    /// Matched byte ranges of the row's exact mod id.
    pub id: Vec<Range<usize>>,
}

impl SearchHighlight {
    /// Whether nothing matched in either text.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.name.is_empty() && self.id.is_empty()
    }

    /// The matches of `needle` (already [`search_fold`]ed, as the filter holds it)
    /// in `row`'s name and id.
    pub(super) fn of(row: &ModListRow, needle: &str) -> Self {
        Self {
            name: match_ranges(&row.name, needle),
            id: match_ranges(row.mod_id.as_str(), needle),
        }
    }
}

/// One original character and where its lowercase form sits in the
/// folded text.
struct CharSpan {
    original: Range<usize>,
    folded: Range<usize>,
}

/// Every non-overlapping occurrence of `needle` in [`search_fold`]`(text)`, mapped
/// back to byte ranges of `text`.
///
/// Lowercasing can change a text's length (`İ` becomes `i` plus a
/// combining dot), so a hit in the folded text is widened to the whole
/// original characters it touches: a match that covers only part of what
/// one character folds to marks that character. Two hits that end up
/// sharing a character are merged; hits that merely touch stay separate.
///
/// The result is non-empty exactly when `search_fold(text)` contains the non-empty
/// `needle` (itself folded): the list filter admits a row by asking this, so
/// the marks and the filter cannot disagree.
pub(super) fn match_ranges(text: &str, needle: &str) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    let folded = search_fold(text);
    let mut hits = folded.match_indices(needle).peekable();
    if hits.peek().is_none() {
        return Vec::new();
    }
    let spans = char_spans(text);
    if spans.last().map_or(0, |span| span.folded.end) != folded.len() {
        return whole_text_fallback(text, "per-character folds do not add up to the text's fold");
    }
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (start, hit) in hits {
        let end = start + hit.len();
        let first = spans.partition_point(|span| span.folded.end <= start);
        let after_last = spans.partition_point(|span| span.folded.start < end);
        let (Some(head), Some(tail)) = (
            spans.get(first),
            after_last.checked_sub(1).and_then(|i| spans.get(i)),
        ) else {
            return whole_text_fallback(text, "a hit lies outside every character's fold");
        };
        let range = head.original.start..tail.original.end;
        match ranges.last_mut() {
            Some(previous) if range.start < previous.end => {
                previous.end = previous.end.max(range.end)
            }
            _ => ranges.push(range),
        }
    }
    ranges
}

/// The answer when the folded text and the per-character spans disagree,
/// which folding character by character never causes: the text matched (the
/// filter's verdict must not change), so it is marked whole rather than
/// left unmarked and wrongly dropped from the list. Tests fail loudly.
fn whole_text_fallback(text: &str, why: &str) -> Vec<Range<usize>> {
    debug_assert!(false, "{why}: {text:?}");
    Vec::from([Range {
        start: 0,
        end: text.len(),
    }])
}

fn char_spans(text: &str) -> Vec<CharSpan> {
    let mut folded_end = 0;
    text.char_indices()
        .map(|(start, character)| {
            let folded_start = folded_end;
            folded_end += fold_char(character).map(char::len_utf8).sum::<usize>();
            CharSpan {
                original: start..start + character.len_utf8(),
                folded: folded_start..folded_end,
            }
        })
        .collect()
}

/// A one-range expectation for the tests of this module and its siblings (a
/// bare `[a..b]` array trips `single_range_in_vec_init`).
#[cfg(test)]
pub(super) fn span(start: usize, end: usize) -> Range<usize> {
    start..end
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// The matched pieces of `text`, as a reader of the highlight sees them.
    fn pieces<'a>(text: &'a str, needle: &str) -> Vec<&'a str> {
        match_ranges(text, &search_fold(needle))
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn an_ascii_match_ignores_case_and_keeps_the_original_spelling() {
        assert_eq!(pieces("Combat Extended", "EXT"), ["Ext"]);
        assert_eq!(match_ranges("Combat Extended", "ext"), [span(7, 10)]);
    }

    #[test]
    fn every_occurrence_is_marked_not_just_the_first() {
        assert_eq!(match_ranges("abcabcab", "ab"), [0..2, 3..5, 6..8]);
    }

    #[test]
    fn touching_matches_stay_separate_ranges() {
        assert_eq!(match_ranges("aaaa", "aa"), [0..2, 2..4]);
    }

    #[test]
    fn no_match_and_an_empty_needle_give_no_ranges() {
        assert!(match_ranges("Alpha", "zz").is_empty());
        assert!(match_ranges("Alpha", "").is_empty());
        assert!(match_ranges("", "a").is_empty());
    }

    #[test]
    fn a_multibyte_character_before_the_match_shifts_the_byte_range() {
        // "é" is two bytes, "日本" three each.
        assert_eq!(match_ranges("é日本mod", "mod"), [span(8, 11)]);
        assert_eq!(pieces("é日本mod", "MOD"), ["mod"]);
    }

    #[test]
    fn a_non_ascii_needle_matches_through_case_folding() {
        assert_eq!(pieces("Ärger mit ÄRGER", "ärger"), ["Ärger", "ÄRGER"]);
    }

    #[test]
    fn a_length_changing_fold_marks_the_whole_original_character() {
        // 'İ' (2 bytes) lowercases to "i" + U+0307 (3 bytes): the folded
        // text is longer than the original.
        let text = "xİy";
        assert_eq!(search_fold(text).len(), text.len() + 1);

        // The needle covers the "i" only, the dot only, and both: each
        // marks the one original character.
        assert_eq!(match_ranges(text, "i"), [span(1, 3)]);
        assert_eq!(match_ranges(text, "\u{307}"), [span(1, 3)]);
        assert_eq!(match_ranges(text, "i\u{307}y"), [span(1, 4)]);
        assert_eq!(pieces(text, "XİY"), ["xİy"]);
    }

    #[test]
    fn a_match_after_a_length_changing_character_lands_on_the_right_bytes() {
        let text = "İİ name";
        // Two 2-byte characters precede "name", which starts at byte 5.
        assert_eq!(match_ranges(text, "name"), [span(5, 9)]);
        assert_eq!(pieces(text, "NAME"), ["name"]);
    }

    #[test]
    fn hits_that_share_an_expanding_character_merge_into_one_range() {
        // Folded: U+0307 a i U+0307 a i U+0307. The needle "\u{307}ai"
        // hits twice, and both hits touch the first "İ" (the first ends in
        // its "i", the second starts at its dot): one range, not two
        // overlapping ones.
        let text = "\u{307}aİaİ";
        assert_eq!(match_ranges(text, "\u{307}ai"), [span(0, 8)]);
    }

    #[test]
    fn a_final_sigma_folds_like_any_other_sigma() {
        // `str::to_lowercase` would write the word-final "Σ" as "ς"; the
        // search folds it to "σ" everywhere, so a needle ending in "Σ" finds
        // the letter at a word end and inside a word alike.
        let text = "ΟΔΟΣ mod";
        assert_eq!(search_fold("ΟΔΟΣ"), "οδοσ");
        assert_eq!(match_ranges(text, &search_fold("ΟΔΟΣ")), [span(0, 8)]);
        assert_eq!(match_ranges(text, "mod"), [span(9, 12)]);
    }

    #[test]
    fn a_needle_ending_in_sigma_matches_inside_a_word() {
        // The bug: "ΑΣ" folded to "ας" and never matched "ΑΣΑ" ("ασα").
        assert_eq!(match_ranges("ΑΣΑ", &search_fold("ΑΣ")), [span(0, 4)]);
        assert_eq!(pieces("Mod ΑΣΑ", "ΑΣ"), ["ΑΣ"]);
        assert_eq!(match_ranges("Mod ΑΣΑ", &search_fold("ΑΣ")), [span(4, 8)]);
    }

    #[test]
    fn a_sigma_in_the_middle_of_a_haystack_matches_a_lowercase_needle() {
        assert_eq!(pieces("ΑΣΑ ΑΣ", "ασ"), ["ΑΣ", "ΑΣ"]);
        assert_eq!(pieces("ΑΣΑ", "σα"), ["ΣΑ"]);
    }

    #[test]
    fn a_typed_final_sigma_finds_a_capital_sigma_and_the_other_way_round() {
        // Greek is typed with "ς" at a word end; the capital has one form, "Σ".
        assert_eq!(pieces("ΚΟΣΜΟΣ", "κοσμος"), ["ΚΟΣΜΟΣ"]);
        assert_eq!(pieces("Mod ΟΔΟΣ", "οδος"), ["ΟΔΟΣ"]);
        assert_eq!(pieces("οδος", "ΟΔΟΣ"), ["οδος"]);
        assert_eq!(pieces("ασα", "ας"), ["ασ"]);
        // "ς" and "σ" are one letter to the search, wherever either stands.
        assert_eq!(pieces("οδοσ", "ς"), ["σ"]);
        assert_eq!(pieces("οδος", "σ"), ["ς"]);
    }

    #[test]
    fn a_match_covering_the_whole_text_is_one_range() {
        assert_eq!(match_ranges("Alpha", "alpha"), [span(0, 5)]);
    }

    #[test]
    fn astral_characters_are_whole_units() {
        // U+1D7D8 is four bytes in UTF-8.
        assert_eq!(match_ranges("\u{1D7D8}ab", "ab"), [span(4, 6)]);
    }

    /// The bytes of `search_fold(text)` that the character starting at
    /// `original_byte` folds to.
    fn folded_span_of_char_at(text: &str, original_byte: usize) -> Range<usize> {
        let mut folded_start = 0;
        for (start, character) in text.char_indices() {
            let folded_len: usize = fold_char(character).map(char::len_utf8).sum();
            if start == original_byte {
                return folded_start..folded_start + folded_len;
            }
            folded_start += folded_len;
        }
        panic!("{original_byte} is not the start of a character in {text:?}");
    }

    proptest! {
        #[test]
        fn widening_never_marks_more_than_the_hits_touch(
            text in "[a-zA-ZİΣσςéÉ日 ._0-9]{0,24}",
            needle in "[a-zA-ZİΣσςéÉ日._0-9]{1,3}",
        ) {
            let needle = search_fold(&needle);
            let hits: Vec<Range<usize>> = search_fold(&text)
                .match_indices(&needle)
                .map(|(start, hit)| start..start + hit.len())
                .collect();
            let overlaps_a_hit = |folded: &Range<usize>| {
                hits.iter().any(|hit| folded.start < hit.end && hit.start < folded.end)
            };

            let ranges = match_ranges(&text, &needle);

            prop_assert!(ranges.len() <= hits.len());
            for range in &ranges {
                // The first and the last marked character each fold to bytes
                // a hit covers: a range never starts or ends on a character
                // the match did not touch, so "mark the whole text" fails.
                let first = folded_span_of_char_at(&text, range.start);
                let last_start = text[..range.end]
                    .char_indices()
                    .next_back()
                    .map(|(index, _)| index)
                    .expect("a range is not empty");
                let last = folded_span_of_char_at(&text, last_start);
                prop_assert!(overlaps_a_hit(&first), "{range:?} starts off the hits in {text:?}");
                prop_assert!(overlaps_a_hit(&last), "{range:?} ends off the hits in {text:?}");
            }
        }
    }

    proptest! {
        #[test]
        fn every_range_folds_to_text_that_holds_the_needle(
            text in "[a-zA-ZİΣσςéÉ日 ._0-9]{0,24}",
            needle in "[a-zA-ZİΣσςéÉ日._0-9]{1,3}",
        ) {
            let needle = search_fold(&needle);
            let ranges = match_ranges(&text, &needle);

            // The filter and the marks agree: marks exist exactly when
            // the filter's own predicate admits the text.
            prop_assert_eq!(!ranges.is_empty(), search_fold(&text).contains(&needle));
            let mut previous_end = 0;
            for range in &ranges {
                // On character boundaries, ascending, never overlapping.
                prop_assert!(text.is_char_boundary(range.start));
                prop_assert!(text.is_char_boundary(range.end));
                prop_assert!(range.start >= previous_end);
                prop_assert!(range.start < range.end);
                previous_end = range.end;
                // Folded on its own, the marked text holds the needle (the
                // range was widened to whole characters, never narrowed).
                // A character folds the same alone as in context.
                let marked = search_fold(&text[range.clone()]);
                prop_assert!(marked.contains(&needle), "{marked:?} lacks {needle:?}");
            }
        }
    }
}
