//! Where a Mods-list search matched inside a row's name and id.
//!
//! The list filter admits a row when its Unicode-lowercased id or name
//! contains the Unicode-lowercased search text. This module answers the
//! same question for display: *which parts* of the original text matched,
//! so an interface can mark them and never disagree with the filter.
//!
//! Ranges are **byte** ranges into the original (not lowercased) text, on
//! character boundaries, ascending and non-overlapping. Converting them to
//! another unit (the desktop's UTF-16 code units) is the interface's job,
//! done once at its boundary.

use std::ops::Range;

use super::ModListRow;

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

    /// The matches of `needle` (already Unicode-lowercased, as the filter
    /// holds it) in `row`'s name and id.
    pub(super) fn of(row: &ModListRow, needle: &str) -> Self {
        Self {
            name: match_ranges(&row.name, needle),
            id: match_ranges(row.mod_id.as_str(), needle),
        }
    }
}

/// One original character and where its lowercase form sits in the
/// lowercased text.
struct CharSpan {
    original: Range<usize>,
    folded: Range<usize>,
}

/// Every non-overlapping occurrence of `needle` in `text.to_lowercase()`,
/// mapped back to byte ranges of `text`.
///
/// Lowercasing can change a text's length (`İ` becomes `i` plus a
/// combining dot), so a hit in the folded text is widened to the whole
/// original characters it touches: a match that covers only part of what
/// one character folds to marks that character. Two hits that end up
/// sharing a character are merged; hits that merely touch stay separate.
///
/// The result is non-empty exactly when `text.to_lowercase()` contains the
/// non-empty `needle`: the list filter admits a row by asking this, so the
/// marks and the filter cannot disagree. The folded text is
/// `str::to_lowercase`. Per-character spans are measured with
/// `char::to_lowercase`; the two differ only for the final sigma (`Σ`
/// lowercases to `ς` at a word end and `σ` elsewhere), and both are two
/// bytes, so the spans line up.
pub(super) fn match_ranges(text: &str, needle: &str) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    let folded = text.to_lowercase();
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
/// which Unicode's lowercase rules never cause: the text matched (the
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
            folded_end += character.to_lowercase().map(char::len_utf8).sum::<usize>();
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
        match_ranges(text, &needle.to_lowercase())
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
        assert_eq!(text.to_lowercase().len(), text.len() + 1);

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
    fn a_final_sigma_is_matched_the_way_the_filter_folds_it() {
        // `str::to_lowercase` writes the word-final "Σ" as "ς", so that
        // is what the filter compares; the ranges must agree with it.
        let text = "ΟΔΟΣ mod";
        assert!(text.to_lowercase().contains("οδος"));
        assert_eq!(match_ranges(text, "οδος"), [span(0, 8)]);
        assert_eq!(match_ranges(text, "mod"), [span(9, 12)]);
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

    /// The bytes of `text.to_lowercase()` that the character starting at
    /// `original_byte` folds to.
    fn folded_span_of_char_at(text: &str, original_byte: usize) -> Range<usize> {
        let mut folded_start = 0;
        for (start, character) in text.char_indices() {
            let folded_len: usize = character.to_lowercase().map(char::len_utf8).sum();
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
            let needle = needle.to_lowercase();
            let hits: Vec<Range<usize>> = text
                .to_lowercase()
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
            let needle = needle.to_lowercase();
            let ranges = match_ranges(&text, &needle);

            // The filter and the marks agree: marks exist exactly when
            // the filter's own predicate admits the text.
            prop_assert_eq!(!ranges.is_empty(), text.to_lowercase().contains(&needle));
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
                // A lone character's fold equals its fold in context except
                // the final sigma, which the filter also folds to ς or σ.
                let marked = text[range.clone()].to_lowercase();
                let relaxed = marked.replace('ς', "σ");
                prop_assert!(
                    marked.contains(&needle) || relaxed.contains(&needle.replace('ς', "σ")),
                    "{marked:?} lacks {needle:?}"
                );
            }
        }
    }
}
