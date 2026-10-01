//! Turns a mod's free-text `About.xml` `<description>` into styled text
//! runs the UI can render safely — never HTML, so the description can
//! never reach the DOM as markup (see the mod info panel's own security
//! notes). Recognizes Unity's own rich-text tag set (`<b>`, `<i>`,
//! `<color=…>`, `<size=…>`, `<material=…>`, `<quad …>`); everything else,
//! including Steam BBCode (`[b]`, `[url]`, ...) and any HTML-looking
//! construct (`<script>`, `<img …>`), is left as plain literal text —
//! exactly how RimWorld itself displays a description, since the engine's
//! own rich-text renderer recognizes the identical tag set and nothing
//! else.

/// One contiguous run of text sharing the same style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RichRun {
    pub text: String,
    pub style: RunStyle,
}

/// Bold/italic are independent flags — Unity's `<b>`/`<i>` nest freely, so
/// every combination (including both at once) is legal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunStyle {
    pub bold: bool,
    pub italic: bool,
}

/// [`parse`]'s result: the styled runs, plus whether the input was
/// truncated before parsing (see `MAX_INPUT_BYTES`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedRichText {
    pub runs: Vec<RichRun>,
    pub truncated: bool,
}

/// More than twice the measured real-install maximum description length
/// (~14 KiB) — a defensive bound against a hostile or pathological
/// description, not a measured typical size.
const MAX_INPUT_BYTES: usize = 32 * 1024;

/// How many *matched* recognized-tag levels deep [`parse`] will recurse.
/// Spent per matched tag (an `or`-flat sequence of tags at the same level
/// costs nothing extra) — a tag opened once this budget is spent is left
/// literal rather than recursed into, so a pathologically deep input
/// can't blow the stack.
const MAX_TAG_DEPTH: usize = 8;

/// The longest a recognized tag's own `<...>` text (name plus attributes)
/// is allowed to be before this parser gives up looking for its closing
/// `>` and treats the `<` as a literal character instead — generous over
/// any real Unity rich-text tag (`<color=#ffffffff>` and friends), and
/// small enough that a hostile `<` followed by megabytes of `>`-free text
/// costs a single bounded scan, not a document-length one, per `<`.
const MAX_TAG_SCAN_BYTES: usize = 256;

/// The total bytes [`find_matching_close`] may examine across one whole
/// [`parse`] call — a generous multiple of [`MAX_INPUT_BYTES`], well past
/// what any real description's own tags need (each successful match scans
/// only as far as its own nearby closing tag), but a hard bound on the
/// pathological case: many same-named opening tags with no closing tag at
/// all, each otherwise scanning all the way to the end of the (already
/// shrinking) remaining text — quadratic in the number of such tags with
/// no bound at all. Once exhausted, every further "no matching close"
/// answer is `None` at zero cost, and [`ParsedRichText::truncated`] is set
/// — a genuine "might have missed a real closing tag past this point",
/// not merely "this input was oversized" (`truncate`'s own reason for
/// that field), but the same signal either way: don't trust this result
/// to be the *complete* parse of the input.
const MAX_MATCH_SCAN_BYTES: usize = 8 * MAX_INPUT_BYTES;

/// The six tag names this parser recognizes, and whether a matched pair
/// keeps its inner text (`Bold`/`Italic`/`Color`/`Size`) or drops it
/// wholesale (`Material`/`Quad`, an image/material reference with no text
/// of its own to keep).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecognizedTag {
    Bold,
    Italic,
    Color,
    Size,
    Material,
    Quad,
}

impl RecognizedTag {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "b" => Some(Self::Bold),
            "i" => Some(Self::Italic),
            "color" => Some(Self::Color),
            "size" => Some(Self::Size),
            "material" => Some(Self::Material),
            "quad" => Some(Self::Quad),
            _ => None,
        }
    }

    /// Whether a matched pair's inner content is kept as text (with a
    /// possibly-changed style) or dropped in full.
    fn keeps_inner_text(self) -> bool {
        !matches!(self, Self::Material | Self::Quad)
    }
}

/// Parses `input` into styled runs. Input over `MAX_INPUT_BYTES` is
/// truncated (at a char boundary) before parsing, with
/// [`ParsedRichText::truncated`] set.
#[must_use]
pub fn parse(input: &str) -> ParsedRichText {
    let (text, truncated) = truncate(input);
    let mut runs = Vec::new();
    let mut match_scan_budget = MAX_MATCH_SCAN_BYTES;
    let mut budget_exhausted = false;
    parse_segment(
        text,
        RunStyle::default(),
        0,
        &mut runs,
        &mut match_scan_budget,
        &mut budget_exhausted,
    );
    ParsedRichText {
        runs,
        truncated: truncated || budget_exhausted,
    }
}

/// Truncates `input` to at most [`MAX_INPUT_BYTES`], at the nearest
/// preceding char boundary.
fn truncate(input: &str) -> (&str, bool) {
    if input.len() <= MAX_INPUT_BYTES {
        return (input, false);
    }
    let mut end = MAX_INPUT_BYTES;
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    (&input[..end], true)
}

/// Parses `text` (a slice with no enclosing recognized tag consumed by a
/// caller) into runs of `style`, appending them to `out`. `depth` is how
/// many matched recognized tags already enclose `text`. `match_scan_budget`/
/// `budget_exhausted` are shared across the whole [`parse`] call, including
/// every recursive descent into a matched pair's own inner text — see
/// [`MAX_MATCH_SCAN_BYTES`].
fn parse_segment(
    text: &str,
    style: RunStyle,
    depth: usize,
    out: &mut Vec<RichRun>,
    match_scan_budget: &mut usize,
    budget_exhausted: &mut bool,
) {
    let bytes = text.as_bytes();
    let mut literal_start = 0usize;
    let mut i = 0usize;
    let flush = |out: &mut Vec<RichRun>, start: usize, end: usize| {
        if end > start {
            out.push(RichRun {
                text: text[start..end].to_string(),
                style,
            });
        }
    };

    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let Some(parsed) = parse_tag_at(text, i) else {
            // Doesn't look like a tag at all (e.g. a bare `<` followed by
            // a digit, or no `>` within the scan window) — one literal
            // character, keep scanning.
            i += 1;
            continue;
        };
        if parsed.closing {
            // A closing tag with no open counterpart in scope (we only
            // ever recurse past a matched pair as a whole, so a stray
            // closer here is always unmatched) — literal, like any other
            // unrecognized construct.
            i = parsed.tag_end;
            continue;
        }
        let Some(recognized) = RecognizedTag::from_name(parsed.name) else {
            // An unrecognized tag (`<script>`, `<img ...>`, ...) is
            // always literal, matched close or not.
            i = parsed.tag_end;
            continue;
        };
        if depth >= MAX_TAG_DEPTH {
            i = parsed.tag_end;
            continue;
        }
        let Some(close_start) = find_matching_close(
            text,
            parsed.name,
            parsed.tag_end,
            match_scan_budget,
            budget_exhausted,
        ) else {
            // No matching close anywhere in the rest of this segment (or
            // the scan budget ran out first) — Unity (and this parser)
            // leaves the opening tag literal.
            i = parsed.tag_end;
            continue;
        };
        let close_end = close_start + "</".len() + parsed.name.len() + ">".len();

        flush(out, literal_start, i);
        if recognized.keeps_inner_text() {
            let inner_style = match recognized {
                RecognizedTag::Bold => RunStyle {
                    bold: true,
                    ..style
                },
                RecognizedTag::Italic => RunStyle {
                    italic: true,
                    ..style
                },
                _ => style,
            };
            parse_segment(
                &text[parsed.tag_end..close_start],
                inner_style,
                depth + 1,
                out,
                match_scan_budget,
                budget_exhausted,
            );
        }
        // `Material`/`Quad`: the whole span between the tags is an
        // image/material reference, not text — dropped in full, no
        // recursion, nothing pushed.
        i = close_end;
        literal_start = i;
    }
    flush(out, literal_start, bytes.len());
}

/// One tag-like construct found at `start` (the index of its `<`):
/// whether it's a closer (`</name...>`), its name, and the index right
/// after its closing `>`.
struct ParsedTag<'a> {
    closing: bool,
    name: &'a str,
    tag_end: usize,
}

/// Recognizes a tag-shaped construct starting at `text[start]` (which
/// must be `<`): optionally `/`, then an ASCII-alphanumeric name, then
/// anything up to the next `>` within [`MAX_TAG_SCAN_BYTES`]. Returns
/// `None` when there's no letter right after `<`/`</`, or no `>` within
/// the scan window — in either case the caller treats `<` as one literal
/// character.
fn parse_tag_at(text: &str, start: usize) -> Option<ParsedTag<'_>> {
    let bytes = text.as_bytes();
    debug_assert_eq!(bytes[start], b'<');
    let mut cursor = start + 1;
    let closing = bytes.get(cursor) == Some(&b'/');
    if closing {
        cursor += 1;
    }
    let name_start = cursor;
    while bytes.get(cursor).is_some_and(|b| b.is_ascii_alphanumeric()) {
        cursor += 1;
    }
    if cursor == name_start {
        return None;
    }
    let name = &text[name_start..cursor];

    // Byte-level search, not `text[cursor..scan_limit].find('>')`:
    // `scan_limit` is an arbitrary byte offset (`start + MAX_TAG_SCAN_BYTES`
    // clamped to the string's own length) with no guarantee of landing on
    // a char boundary, and slicing `text` there would panic on non-ASCII
    // content past the window's start. `>` is itself always a single ASCII
    // byte, so searching for it in `bytes` directly is exact regardless.
    let scan_limit = (start + MAX_TAG_SCAN_BYTES).min(bytes.len());
    let close_angle = bytes[cursor..scan_limit].iter().position(|&b| b == b'>')?;
    let tag_end = cursor + close_angle + 1;
    Some(ParsedTag {
        closing,
        name,
        tag_end,
    })
}

/// The byte index of the `<` of the next `</tag_name>` in `text[from..]`
/// that closes the tag opened just before `from`, accounting for the same
/// tag name nesting inside (an inner `<tag_name>...</tag_name>` pair is
/// skipped as a unit) — `None` when no matching close exists at all, or
/// when `match_scan_budget` (see [`MAX_MATCH_SCAN_BYTES`]) runs out first,
/// in which case `budget_exhausted` is also set. Only same-name nesting
/// affects the count; any other content (text, other tag names) is
/// scanned over without effect.
///
/// Operates on `bytes`, never re-slicing `text` as `&str`: `from` and
/// every position this function itself advances to are already
/// byte-exact tag boundaries, but the plain `i += 1` fallback below walks
/// one raw byte at a time and can land mid-character on non-ASCII text —
/// slicing `text` there (`&str` indexing requires a char boundary) would
/// panic on a mod description containing so much as one accented letter
/// ahead of an unmatched tag. `[u8]::starts_with` has no such requirement.
fn find_matching_close(
    text: &str,
    tag_name: &str,
    from: usize,
    match_scan_budget: &mut usize,
    budget_exhausted: &mut bool,
) -> Option<usize> {
    let open = format!("<{tag_name}");
    let close = format!("</{tag_name}>");
    let open_bytes = open.as_bytes();
    let close_bytes = close.as_bytes();
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = from;
    while i < bytes.len() {
        if *match_scan_budget == 0 {
            *budget_exhausted = true;
            return None;
        }
        *match_scan_budget -= 1;
        if bytes[i..].starts_with(close_bytes) {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
            i += close_bytes.len();
            continue;
        }
        if bytes[i..].starts_with(open_bytes) {
            // Only count it as a nested *open* of the same tag when it's
            // genuinely a tag start, not just a name that happens to be a
            // prefix of a longer one (`<b` inside `<big`, which isn't a
            // recognized name anyway, but keep this precise regardless).
            let after = i + open_bytes.len();
            let boundary = bytes.get(after).is_none_or(|b| !b.is_ascii_alphanumeric());
            if boundary {
                depth += 1;
                i = after;
                continue;
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn run(text: &str, bold: bool, italic: bool) -> RichRun {
        RichRun {
            text: text.to_string(),
            style: RunStyle { bold, italic },
        }
    }

    #[test]
    fn bold_tag_produces_one_bold_run() {
        let parsed = parse("<b>x</b>");
        assert_eq!(parsed.runs, vec![run("x", true, false)]);
        assert!(!parsed.truncated);
    }

    #[test]
    fn nested_bold_and_italic_produce_one_run_with_both_flags() {
        let parsed = parse("<b><i>x</i></b>");
        assert_eq!(parsed.runs, vec![run("x", true, true)]);
    }

    #[test]
    fn color_and_size_keep_the_text_but_drop_the_style() {
        assert_eq!(
            parse("<color=#f00>x</color>").runs,
            vec![run("x", false, false)]
        );
        assert_eq!(
            parse("<size=20>x</size>").runs,
            vec![run("x", false, false)]
        );
    }

    #[test]
    fn quad_is_removed_entirely_tag_and_content() {
        let parsed = parse("before<quad material=\"X\" size=\"20\">ignored</quad>after");
        assert_eq!(
            parsed.runs,
            vec![run("before", false, false), run("after", false, false)]
        );
    }

    #[test]
    fn material_is_removed_entirely_tag_and_content() {
        let parsed = parse("before<material=\"X\">ignored</material>after");
        assert_eq!(
            parsed.runs,
            vec![run("before", false, false), run("after", false, false)]
        );
    }

    #[test]
    fn an_unmatched_bold_tag_stays_literal() {
        let parsed = parse("<b>no closer here");
        assert_eq!(parsed.runs, vec![run("<b>no closer here", false, false)]);
    }

    #[test]
    fn a_script_tag_comes_back_as_literal_text() {
        let parsed = parse("<script>alert(1)</script>");
        assert_eq!(
            parsed.runs,
            vec![run("<script>alert(1)</script>", false, false)]
        );
    }

    #[test]
    fn an_img_tag_comes_back_as_literal_text() {
        let parsed = parse("<img src=x onerror=alert(1)>");
        assert_eq!(
            parsed.runs,
            vec![run("<img src=x onerror=alert(1)>", false, false)]
        );
    }

    #[test]
    fn bbcode_stays_literal() {
        let parsed = parse("[b]Bold BBCode[/b] and [url=https://example.com]a link[/url]");
        assert_eq!(
            parsed.runs,
            vec![run(
                "[b]Bold BBCode[/b] and [url=https://example.com]a link[/url]",
                false,
                false
            )]
        );
    }

    #[test]
    fn depth_over_the_cap_stays_literal() {
        let opens = "<b>".repeat(MAX_TAG_DEPTH + 1);
        let closes = "</b>".repeat(MAX_TAG_DEPTH + 1);
        let input = format!("{opens}x{closes}");

        let parsed = parse(&input);

        // The innermost (9th) `<b>` is past the depth budget, so it's
        // left literal rather than recursed into — the reconstructed
        // text must still contain the raw 9th `<b>` tag somewhere.
        let joined: String = parsed.runs.iter().map(|r| r.text.as_str()).collect();
        assert!(joined.contains("<b>"), "joined = {joined:?}");
    }

    #[test]
    fn input_over_the_cap_is_truncated_and_flagged() {
        let input = "a".repeat(MAX_INPUT_BYTES + 1_000);
        let parsed = parse(&input);
        let total_len: usize = parsed.runs.iter().map(|r| r.text.len()).sum();
        assert!(parsed.truncated);
        assert_eq!(total_len, MAX_INPUT_BYTES);
    }

    /// Regression: `find_matching_close`'s fallback byte-at-a-time scan can
    /// land mid-character on non-ASCII text between an opening tag and its
    /// (missing) close — re-slicing `text` there panicked
    /// ("byte index N is not a char boundary") before this function moved
    /// to purely byte-level (`[u8]`) comparisons. No amount of accented or
    /// non-Latin text ahead of an unmatched tag should ever panic.
    #[test]
    fn non_ascii_text_before_an_unmatched_tags_end_never_panics() {
        let input = "<b>h\u{e9}llo world with no closing tag, caf\u{e9} au lait, \u{4f60}\u{597d}";
        let parsed = parse(input);
        // Unmatched, so the whole thing stays literal.
        let joined: String = parsed.runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(joined, input);
    }

    /// Regression: `parse_tag_at`'s `MAX_TAG_SCAN_BYTES` window's own end
    /// (`start + MAX_TAG_SCAN_BYTES`, clamped to the string's length) is an
    /// arbitrary byte offset with no guarantee of landing on a char
    /// boundary — re-slicing `text` there to search for `>` panicked
    /// before this function moved to a byte-level (`[u8]`) search. Placed
    /// so the offset lands inside a 2-byte character.
    #[test]
    fn non_ascii_text_straddling_the_tag_scan_window_never_panics() {
        let mut input = String::from("<b");
        for _ in 0..(MAX_TAG_SCAN_BYTES - 2) {
            input.push('a');
        }
        input.push('\u{e9}'); // straddles the MAX_TAG_SCAN_BYTES boundary
        input.push_str(" and no angle bracket anywhere near here at all");
        let parsed = parse(&input);
        let joined: String = parsed.runs.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(joined, input);
    }

    /// Regression for the quadratic case: many same-named opening tags
    /// with no closing tag at all, each of which used to make
    /// `find_matching_close` scan all the way to the end of the
    /// (shrinking) remaining text before this function gained a shared,
    /// bounded [`MAX_MATCH_SCAN_BYTES`] scan budget. Bounded by wall clock
    /// (a generous multiple of what the fixed implementation actually
    /// takes) rather than an internal step count, since no step counter is
    /// exposed publicly — the point is that this completes at all quickly,
    /// not a specific step total.
    #[test]
    fn many_unmatched_same_named_tags_stay_fast() {
        // "<b>" is 3 bytes; fills the whole MAX_INPUT_BYTES budget with no
        // closing tag anywhere, the worst case for the old, unbounded scan.
        let input = "<b>".repeat(MAX_INPUT_BYTES / 3 + 10);

        let started = std::time::Instant::now();
        let parsed = parse(&input);
        let elapsed = started.elapsed();

        assert!(
            elapsed < std::time::Duration::from_millis(500),
            "parsing {} unmatched tags took {elapsed:?}, expected well under 500ms",
            MAX_INPUT_BYTES / 3
        );
        // The scan budget is far smaller than the total unmatched-tag
        // count, so it must have been exhausted at least once.
        assert!(parsed.truncated);
    }

    #[test]
    fn plain_text_with_no_tags_is_one_default_styled_run() {
        let parsed = parse("just plain text");
        assert_eq!(parsed.runs, vec![run("just plain text", false, false)]);
    }

    #[test]
    fn empty_input_yields_no_runs() {
        assert_eq!(parse("").runs, Vec::new());
    }

    // -- Property tests -----------------------------------------------

    /// Builds nested `<b>`/`<i>`/`<color=#000>`/`<size=1>` markup, at most
    /// [`MAX_TAG_DEPTH`] deep, around plain leaf text drawn from a small
    /// tag-free alphabet.
    fn arb_tagged(depth: u32) -> BoxedStrategy<String> {
        let leaf = "[a-zA-Z0-9 ]{0,12}".prop_map(|s| s);
        if depth == 0 {
            return leaf.boxed();
        }
        leaf.prop_recursive(depth, 64, 8, |inner| {
            prop_oneof![
                inner.clone().prop_map(|s| format!("<b>{s}</b>")),
                inner.clone().prop_map(|s| format!("<i>{s}</i>")),
                inner
                    .clone()
                    .prop_map(|s| format!("<color=#000000>{s}</color>")),
                inner.prop_map(|s| format!("<size=12>{s}</size>")),
            ]
        })
        .boxed()
    }

    proptest! {
        /// Every matched b/i/color/size tag pair's own markup is consumed
        /// — it never survives into the joined run text.
        #[test]
        fn matched_tag_markup_never_survives_into_run_text(input in arb_tagged(MAX_TAG_DEPTH as u32)) {
            let parsed = parse(&input);
            let joined: String = parsed.runs.iter().map(|r| r.text.as_str()).collect();
            prop_assert!(!joined.contains("<b>"));
            prop_assert!(!joined.contains("</b>"));
            prop_assert!(!joined.contains("<i>"));
            prop_assert!(!joined.contains("</i>"));
            prop_assert!(!joined.contains("<color=#000000>"));
            prop_assert!(!joined.contains("</color>"));
            prop_assert!(!joined.contains("<size=12>"));
            prop_assert!(!joined.contains("</size>"));
        }

        /// Input with no `<` at all round-trips unchanged, as one
        /// default-styled run.
        #[test]
        fn tag_free_input_round_trips_unchanged(input in "[^<]{0,500}") {
            let parsed = parse(&input);
            prop_assert!(!parsed.truncated);
            let joined: String = parsed.runs.iter().map(|r| r.text.as_str()).collect();
            prop_assert_eq!(joined, input);
            prop_assert!(parsed.runs.iter().all(|r| r.style == RunStyle::default()));
        }
    }
}
