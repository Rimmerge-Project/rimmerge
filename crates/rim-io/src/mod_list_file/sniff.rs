//! Telling a RimWorld save from every other mod-list shape by looking at
//! the first bytes: the root element and the first element inside it.
//!
//! A save is hundreds of megabytes, so the reader decides *before* it has
//! the whole input whether it is looking at one (see `stream.rs`). The
//! decision is made again from the start of the buffer each time more
//! bytes arrive, so it never depends on where a read happened to end; the
//! reader bounds the cost of that by only sniffing within a fixed window.
//! The root is matched without regard to case, like every other shape; the
//! first child must be exactly `meta`, which is how the game itself reads
//! a save.

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const SAVE_ROOT: &[u8] = b"savegame";
const META: &[u8] = b"meta";
const DOCTYPE: &[u8] = b"<!DOCTYPE";
const COMMENT_START: &[u8] = b"<!--";
const COMMENT_END: &[u8] = b"-->";
const PI_START: &[u8] = b"<?";
const PI_END: &[u8] = b"?>";

/// What the leading bytes of an input show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Sniff {
    /// The bytes so far do not settle it.
    NeedMore,
    /// Not a save: one of the other shapes, parsed whole.
    NotSave,
    /// A DOCTYPE before the root, refused whatever the root is.
    DocType,
    /// A `<savegame>` root is seen but its first child is not decided yet.
    /// Input that ends here is a truncated save, not a malformed list.
    SavePending,
    /// A `<savegame>` whose first child is `<meta>`. `root_at` is the byte
    /// offset of the root element's `<`; the root name follows it.
    SaveHeader { root_at: usize },
    /// A `<savegame>` whose first child is something else, or none.
    SaveWithoutHeader,
}

/// Classifies `bytes`, the start of an input. `NeedMore` means a longer
/// prefix could change the answer.
pub(super) fn sniff(bytes: &[u8]) -> Sniff {
    let mut at = if bytes.starts_with(UTF8_BOM) {
        UTF8_BOM.len()
    } else {
        0
    };
    loop {
        at += leading_whitespace(&bytes[at..]);
        let rest = &bytes[at..];
        match markup_at(rest) {
            Markup::NeedMore => return Sniff::NeedMore,
            Markup::Text => return Sniff::NotSave,
            Markup::DocType => return Sniff::DocType,
            Markup::Skippable(length) => at += length,
            Markup::Element => return sniff_root(rest, at),
        }
    }
}

/// `rest` starts with the root element's `<`, at byte `root_at` of the input.
fn sniff_root(rest: &[u8], root_at: usize) -> Sniff {
    match element_name(rest) {
        Name::Incomplete => return Sniff::NeedMore,
        Name::Found(name) if !name.eq_ignore_ascii_case(SAVE_ROOT) => return Sniff::NotSave,
        Name::Found(_) => {}
    }
    // The root's start tag ends at its first `>`. An attribute value
    // holding one can only make this misjudge a crafted file, which is
    // then refused, never read as a shorter list.
    let Some(tag_end) = rest.iter().position(|byte| *byte == b'>') else {
        return Sniff::SavePending;
    };
    if rest[..tag_end].ends_with(b"/") {
        return Sniff::SaveWithoutHeader;
    }
    sniff_first_child(&rest[tag_end + 1..], root_at)
}

fn sniff_first_child(mut inside: &[u8], root_at: usize) -> Sniff {
    loop {
        let Some(next) = inside.iter().position(|byte| *byte == b'<') else {
            return Sniff::SavePending;
        };
        inside = &inside[next..];
        match markup_at(inside) {
            Markup::NeedMore => return Sniff::SavePending,
            Markup::Skippable(length) => inside = &inside[length..],
            Markup::DocType | Markup::Text => return Sniff::SaveWithoutHeader,
            Markup::Element => {
                return match element_name(inside) {
                    Name::Incomplete => Sniff::SavePending,
                    Name::Found(name) if name == META => Sniff::SaveHeader { root_at },
                    Name::Found(_) => Sniff::SaveWithoutHeader,
                };
            }
        }
    }
}

enum Markup {
    NeedMore,
    /// Does not start with `<`, or is a closing tag or other `<!` markup: no
    /// element starts here.
    Text,
    DocType,
    /// A declaration, processing instruction or comment of this length.
    Skippable(usize),
    /// An element start tag.
    Element,
}

fn markup_at(rest: &[u8]) -> Markup {
    if rest.is_empty() {
        return Markup::NeedMore;
    }
    if rest[0] != b'<' {
        return Markup::Text;
    }
    if [DOCTYPE, COMMENT_START, PI_START]
        .iter()
        .any(|marker| rest.len() < marker.len() && marker.starts_with(rest))
    {
        return Markup::NeedMore;
    }
    if rest.starts_with(DOCTYPE) {
        return Markup::DocType;
    }
    if rest.starts_with(PI_START) {
        return skip_to(rest, PI_END);
    }
    if rest.starts_with(COMMENT_START) {
        return skip_to(rest, COMMENT_END);
    }
    if rest[1] == b'!' || rest[1] == b'/' {
        return Markup::Text;
    }
    Markup::Element
}

fn skip_to(rest: &[u8], end: &[u8]) -> Markup {
    match find(rest, end) {
        Some(at) => Markup::Skippable(at + end.len()),
        None => Markup::NeedMore,
    }
}

enum Name<'a> {
    /// The buffer ends inside the name.
    Incomplete,
    Found(&'a [u8]),
}

/// The name of the element whose `<` starts `rest`.
fn element_name(rest: &[u8]) -> Name<'_> {
    let body = &rest[1..];
    match body
        .iter()
        .position(|byte| byte.is_ascii_whitespace() || matches!(byte, b'>' | b'/'))
    {
        Some(end) => Name::Found(&body[..end]),
        None => Name::Incomplete,
    }
}

fn leading_whitespace(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count()
}

/// The offset of the first occurrence of `needle` in `haystack`.
pub(super) fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_with_a_meta_first_child_is_a_save_header() {
        let bytes = b"\xEF\xBB\xBF<?xml version=\"1.0\"?>\r\n<savegame>\r\n\t<meta>";

        assert_eq!(sniff(bytes), Sniff::SaveHeader { root_at: 26 });
    }

    #[test]
    fn the_root_name_is_matched_without_regard_to_case_but_meta_exactly() {
        assert_eq!(sniff(b"<SaveGame><meta>"), Sniff::SaveHeader { root_at: 0 });
        assert_eq!(sniff(b"<savegame><Meta>"), Sniff::SaveWithoutHeader);
    }

    #[test]
    fn the_root_offset_skips_a_leading_comment_that_names_the_root() {
        let bytes = b"<!-- <SAVEGAME --><savegame><meta>";

        assert_eq!(sniff(bytes), Sniff::SaveHeader { root_at: 18 });
    }

    #[test]
    fn a_save_root_with_no_child_yet_is_pending() {
        assert_eq!(
            sniff(
                b"<savegame>
  "
            ),
            Sniff::SavePending
        );
        assert_eq!(sniff(b"<savegame"), Sniff::NeedMore);
        assert_eq!(sniff(b"<savegame attr=\"x\""), Sniff::SavePending);
    }

    #[test]
    fn a_root_that_only_starts_with_the_save_name_is_not_a_save() {
        assert_eq!(sniff(b"<savegameX><meta>"), Sniff::NotSave);
    }

    #[test]
    fn a_first_child_other_than_meta_is_a_save_without_a_header() {
        assert_eq!(sniff(b"<savegame><game>"), Sniff::SaveWithoutHeader);
        assert_eq!(sniff(b"<savegame><foo/><meta>"), Sniff::SaveWithoutHeader);
        assert_eq!(sniff(b"<savegame/>"), Sniff::SaveWithoutHeader);
        assert_eq!(sniff(b"<savegame></savegame>"), Sniff::SaveWithoutHeader);
    }

    #[test]
    fn a_comment_before_the_first_child_is_skipped() {
        assert_eq!(
            sniff(b"<!-- note --><savegame><!-- a --><meta>"),
            Sniff::SaveHeader { root_at: 13 }
        );
    }

    #[test]
    fn a_doctype_before_the_root_is_reported_before_the_root_is_known() {
        assert_eq!(
            sniff(b"<!DOCTYPE x [<!ENTITY a \"b\">]><savegame>"),
            Sniff::DocType
        );
    }

    #[test]
    fn other_documents_are_not_saves() {
        assert_eq!(sniff(b"<savedModList><meta>"), Sniff::NotSave);
        assert_eq!(sniff(b"ludeon.rimworld\n"), Sniff::NotSave);
        assert_eq!(sniff(b"<ModsConfigData><version>"), Sniff::NotSave);
    }

    #[test]
    fn every_proper_prefix_of_a_header_needs_more_or_agrees() {
        let full = b"<?xml version=\"1.0\"?>\r\n<savegame>\r\n\t<meta>\r\n";
        for cut in 0..full.len() {
            let answer = sniff(&full[..cut]);
            assert!(
                matches!(
                    answer,
                    Sniff::NeedMore | Sniff::SavePending | Sniff::SaveHeader { .. }
                ),
                "prefix of {cut} bytes answered {answer:?}"
            );
        }
        assert_eq!(sniff(full), Sniff::SaveHeader { root_at: 23 });
    }
}
