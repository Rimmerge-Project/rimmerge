//! The "Copy as text" format and its parser.
//!
//! ```text
//! # RimWorld 1.6.4871 rev590
//! 1. Core [ludeon.rimworld]
//! 2. Example Framework [example.framework] <https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890>
//! 3. Some Local Mod [someone.localmod]
//! ```
//!
//! The text is language-neutral (shared content, like the English rule
//! comments). Names are sanitised on the way out so a name can never fake
//! an id or a link on the way back in.

use super::{
    ListedGameVersion, ListedName, ListedPackageId, ModListLimits, ParsedModList, Rejection,
    SharedModEntry, SharedModList, SharedModListError, SkippedEntry, TruncatedText, WorkshopId,
    count_as_u32,
};
use crate::mod_info::workshop_url;

const COMMENT_MARKER: char = '#';
const VERSION_COMMENT_PREFIX: &str = "RimWorld ";
const URL_PREFIXES: [&str; 2] = [
    "https://steamcommunity.com/sharedfiles/filedetails/?id=",
    "https://steamcommunity.com/workshop/filedetails/?id=",
];
/// Characters Discord would render as markdown, and the backslash that
/// escapes them: each is written after a backslash.
const MARKDOWN_ESCAPED: [char; 6] = ['*', '_', '~', '`', '|', '\\'];
/// Written after every `@` so `@everyone` and `@here` are not mentions
/// (Discord matches the literal text). A word joiner is invisible, is not
/// one of the characters a name is stripped of, and is removed again on
/// parse.
const MENTION_BREAK: char = '\u{2060}';

/// Renders `list` as the shareable text: a `# RimWorld <version>` comment
/// when the version is known, then one numbered line per entry.
#[must_use]
pub fn render_text(list: &SharedModList) -> String {
    let mut out = String::new();
    if let Some(version) = list.game_version() {
        out.push_str(&format!("# {VERSION_COMMENT_PREFIX}{version}\n"));
    }
    for (index, entry) in list.entries().iter().enumerate() {
        out.push_str(&render_line(index + 1, entry));
        out.push('\n');
    }
    out
}

fn render_line(number: usize, entry: &SharedModEntry) -> String {
    let mut line = format!("{number}. ");
    if let Some(name) = &entry.name {
        line.push_str(&escape_name(name.as_str()));
        line.push(' ');
    }
    line.push_str(&format!("[{}]", entry.id));
    if let Some(workshop_id) = entry.workshop_id {
        line.push_str(&format!(" <{}>", workshop_url(workshop_id.get()).as_str()));
    }
    line
}

/// Escapes Discord markdown, breaks `@` mentions, and replaces the
/// characters that delimit an id or a link with parentheses.
fn escape_name(name: &str) -> String {
    let mut escaped = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '[' | '<' => escaped.push('('),
            ']' | '>' => escaped.push(')'),
            '@' => {
                escaped.push('@');
                escaped.push(MENTION_BREAK);
            }
            c if MARKDOWN_ESCAPED.contains(&c) => {
                escaped.push('\\');
                escaped.push(c);
            }
            c => escaped.push(c),
        }
    }
    escaped
}

fn unescape_name(escaped: &str) -> String {
    let mut name = String::with_capacity(escaped.len());
    let mut chars = escaped.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\'
            && let Some(&next) = chars.peek()
            && MARKDOWN_ESCAPED.contains(&next)
        {
            chars.next();
            name.push(next);
        } else if c == '@' && chars.peek() == Some(&MENTION_BREAK) {
            chars.next();
            name.push(c);
        } else {
            name.push(c);
        }
    }
    name
}

/// What one line of text means.
enum Line {
    Ignored,
    Version(ListedGameVersion),
    Entry(SharedModEntry),
    MalformedId(TruncatedText),
    NotAnEntry,
}

/// Accumulates what a parse finds, bounded.
#[derive(Default)]
struct Collected {
    version: Option<ListedGameVersion>,
    entries: Vec<SharedModEntry>,
    skipped: Vec<SkippedEntry>,
    omitted_skipped: usize,
    entry_like_lines: usize,
}

impl Collected {
    fn skip(&mut self, skipped: SkippedEntry) {
        if self.skipped.len() < ModListLimits::MAX_SKIPPED_REPORTED {
            self.skipped.push(skipped);
        } else {
            self.omitted_skipped += 1;
        }
    }
}

/// Parses the text format, or a plain list of package ids, one per line.
/// A line that is not an entry is skipped and reported; only a document
/// that is too big or names no mod is rejected.
pub fn parse_text(input: &str) -> Result<ParsedModList, Rejection> {
    if input.len() > ModListLimits::MAX_INPUT_BYTES {
        return Err(Rejection::TooLarge {
            limit_bytes: ModListLimits::MAX_INPUT_BYTES,
        });
    }
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut collected = Collected::default();
    for (index, raw_line) in input.lines().enumerate() {
        let line_number = count_as_u32(index + 1);
        match classify(raw_line) {
            Line::Ignored => {}
            Line::Version(version) => {
                collected.version.get_or_insert(version);
            }
            Line::Entry(entry) => {
                collected.entry_like_lines += 1;
                collected.entries.push(entry);
                if collected.entries.len() > ModListLimits::MAX_ENTRIES {
                    return Err(Rejection::TooManyEntries {
                        limit: ModListLimits::MAX_ENTRIES,
                    });
                }
            }
            Line::MalformedId(text) => {
                collected.entry_like_lines += 1;
                collected.skip(SkippedEntry::MalformedId {
                    position: count_as_u32(collected.entry_like_lines),
                    text,
                });
            }
            Line::NotAnEntry => collected.skip(SkippedEntry::NotAnEntry { line: line_number }),
        }
    }
    let list =
        SharedModList::new(collected.version, collected.entries).map_err(|error| match error {
            SharedModListError::Empty => Rejection::NoEntries,
            SharedModListError::TooManyEntries { limit } => Rejection::TooManyEntries { limit },
        })?;
    Ok(ParsedModList {
        list,
        skipped: collected.skipped,
        omitted_skipped: collected.omitted_skipped,
    })
}

fn classify(raw_line: &str) -> Line {
    let line = raw_line.trim();
    if line.is_empty() {
        return Line::Ignored;
    }
    if line.len() > ModListLimits::MAX_LINE_BYTES {
        return Line::NotAnEntry;
    }
    if let Some(comment) = line.strip_prefix(COMMENT_MARKER) {
        return version_comment(comment).map_or(Line::Ignored, Line::Version);
    }
    let (body, workshop_id) = split_workshop_url(strip_numbering(line));
    match split_id(body) {
        Some(IdPart::Bracketed { name, id }) => entry_line(name, id, workshop_id),
        Some(IdPart::Bare(id)) => bare_entry_line(id, workshop_id),
        None => Line::NotAnEntry,
    }
}

fn version_comment(comment: &str) -> Option<ListedGameVersion> {
    let version = comment.trim().strip_prefix(VERSION_COMMENT_PREFIX)?;
    ListedGameVersion::new(version)
}

/// Drops a leading `N.` or `N)` when whitespace (or the end) follows it.
fn strip_numbering(line: &str) -> &str {
    let after_digits = line.trim_start_matches(|c: char| c.is_ascii_digit());
    if after_digits.len() == line.len() {
        return line;
    }
    let Some(rest) = after_digits
        .strip_prefix('.')
        .or_else(|| after_digits.strip_prefix(')'))
    else {
        return line;
    };
    if rest.is_empty() || rest.starts_with(char::is_whitespace) {
        rest.trim_start()
    } else {
        line
    }
}

/// Splits a trailing URL, with or without `<>`; the Workshop id is `Some`
/// only for a Steam Workshop item URL.
fn split_workshop_url(body: &str) -> (&str, Option<WorkshopId>) {
    let body = body.trim_end();
    if let Some(without_close) = body.strip_suffix('>')
        && let Some(open) = without_close.rfind('<')
    {
        let url = &without_close[open + 1..];
        return (without_close[..open].trim_end(), workshop_id_of_url(url));
    }
    match body.rsplit_once(char::is_whitespace) {
        Some((rest, last)) if last.starts_with("https://") => {
            (rest.trim_end(), workshop_id_of_url(last))
        }
        _ => (body, None),
    }
}

fn workshop_id_of_url(url: &str) -> Option<WorkshopId> {
    let digits_and_rest = URL_PREFIXES
        .iter()
        .find_map(|prefix| url.strip_prefix(prefix))?;
    let digit_count = digits_and_rest
        .bytes()
        .take_while(u8::is_ascii_digit)
        .count();
    let (digits, rest) = digits_and_rest.split_at(digit_count);
    if !(rest.is_empty() || rest.starts_with('&')) {
        return None;
    }
    WorkshopId::try_from(digits).ok()
}

enum IdPart<'a> {
    Bracketed { name: &'a str, id: &'a str },
    Bare(&'a str),
}

fn split_id(body: &str) -> Option<IdPart<'_>> {
    if let Some(without_close) = body.strip_suffix(']') {
        let open = without_close.rfind('[')?;
        return Some(IdPart::Bracketed {
            name: without_close[..open].trim(),
            id: &without_close[open + 1..],
        });
    }
    let is_bare_token = !body.is_empty()
        && !body.contains(char::is_whitespace)
        && !body.contains(['[', ']', '<', '>']);
    is_bare_token.then_some(IdPart::Bare(body))
}

fn entry_line(name: &str, id: &str, workshop_id: Option<WorkshopId>) -> Line {
    match ListedPackageId::try_from(id) {
        Ok(id) => Line::Entry(SharedModEntry {
            id,
            name: ListedName::new(&unescape_name(name)),
            workshop_id,
        }),
        Err(_) => Line::MalformedId(TruncatedText::new(id)),
    }
}

/// A bare token that fails the id grammar is junk, not a malformed entry:
/// nothing marks it as an attempt at an id.
fn bare_entry_line(id: &str, workshop_id: Option<WorkshopId>) -> Line {
    match ListedPackageId::try_from(id) {
        Ok(id) => Line::Entry(SharedModEntry {
            id,
            name: None,
            workshop_id,
        }),
        Err(_) => Line::NotAnEntry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_list::test_fixtures::{entry, named_entry};

    fn ids(parsed: &ParsedModList) -> Vec<&str> {
        parsed
            .list
            .entries()
            .iter()
            .map(|e| e.id.as_str())
            .collect()
    }

    fn version(text: &str) -> Option<ListedGameVersion> {
        ListedGameVersion::new(text)
    }

    #[test]
    fn render_matches_the_documented_shape() {
        let list = SharedModList::new(
            version("1.6.4871 rev590"),
            vec![
                named_entry("ludeon.rimworld", "Core", None),
                named_entry(
                    "example.framework",
                    "Example Framework",
                    Some(1_234_567_890),
                ),
                entry("someone.localmod"),
            ],
        )
        .expect("list");

        assert_eq!(
            render_text(&list),
            "# RimWorld 1.6.4871 rev590\n\
             1. Core [ludeon.rimworld]\n\
             2. Example Framework [example.framework] \
             <https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890>\n\
             3. [someone.localmod]\n"
        );
    }

    /// The desktop's "Copy the missing list" is built client-side in
    /// `apps/desktop/src/utils/orderShare.ts` (`missingListText`). Its vitest reads this same
    /// fixture file and asserts the same text, so changing either escaper without the other
    /// (and the fixture) fails one of the two suites.
    #[test]
    fn render_matches_the_desktop_client_parity_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/client_text_parity.json"))
                .expect("fixture is valid JSON");
        let entries = fixture["entries"]
            .as_array()
            .expect("entries array")
            .iter()
            .map(|row| SharedModEntry {
                id: ListedPackageId::try_from(row["id"].as_str().expect("id")).expect("valid id"),
                name: row["name"].as_str().and_then(ListedName::new),
                workshop_id: row["workshopId"].as_u64().and_then(WorkshopId::new),
            })
            .collect();
        let list = SharedModList::new(None, entries).expect("list");

        assert_eq!(
            render_text(&list),
            fixture["expected"].as_str().expect("expected text")
        );
    }

    #[test]
    fn round_trip_keeps_ids_workshop_ids_order_and_version() {
        let list = SharedModList::new(
            version("1.6.4871 rev590"),
            vec![
                named_entry("a.first", "First", Some(11)),
                entry("b.second"),
                named_entry("c.third_steam", "Third", None),
            ],
        )
        .expect("list");

        let parsed = parse_text(&render_text(&list)).expect("parses");

        assert_eq!(parsed.list, list);
        assert!(parsed.skipped.is_empty());
    }

    fn parsed_names(parsed: &ParsedModList) -> Vec<&str> {
        parsed
            .list
            .entries()
            .iter()
            .filter_map(|e| e.name.as_ref().map(ListedName::as_str))
            .collect()
    }

    #[test]
    fn a_hostile_name_cannot_change_the_parsed_id_or_link() {
        let list = SharedModList::new(
            None,
            vec![
                named_entry("real.one", "Evil [evil.id] <https://x> *bold*", None),
                named_entry(
                    "real.two",
                    "Trick <https://steamcommunity.com/sharedfiles/filedetails/?id=999>",
                    None,
                ),
            ],
        )
        .expect("list");

        let rendered = render_text(&list);
        let parsed = parse_text(&rendered).expect("parses");

        assert_eq!(ids(&parsed), vec!["real.one", "real.two"]);
        assert!(
            parsed
                .list
                .entries()
                .iter()
                .all(|e| e.workshop_id.is_none())
        );
        assert!(rendered.contains(r"\*bold\*"));
        assert!(parsed.skipped.is_empty());
        // Brackets become parentheses; everything else survives.
        assert_eq!(
            parsed_names(&parsed),
            vec![
                "Evil (evil.id) (https://x) *bold*",
                "Trick (https://steamcommunity.com/sharedfiles/filedetails/?id=999)",
            ]
        );
    }

    #[test]
    fn markdown_and_backslashes_in_a_name_round_trip() {
        let name = r"Tom_s *Mod* ~x~ `y` |z| \w";
        let list =
            SharedModList::new(None, vec![named_entry("real.one", name, None)]).expect("list");

        let rendered = render_text(&list);
        let parsed = parse_text(&rendered).expect("parses");

        assert_eq!(parsed_names(&parsed), vec![name]);
        assert!(rendered.contains(r"Tom\_s \*Mod\* \~x\~ \`y\` \|z\| \\w"));
    }

    #[test]
    fn a_mention_in_a_name_is_broken_in_the_text_and_restored_on_parse() {
        let name = "@everyone and @here";
        let list =
            SharedModList::new(None, vec![named_entry("real.one", name, None)]).expect("list");

        let rendered = render_text(&list);
        let parsed = parse_text(&rendered).expect("parses");

        assert!(!rendered.contains("@everyone") && !rendered.contains("@here"));
        assert_eq!(parsed_names(&parsed), vec![name]);
    }

    #[test]
    fn a_word_joiner_after_an_at_sign_in_a_name_does_not_leak_through_the_round_trip() {
        let list = SharedModList::new(
            None,
            vec![named_entry("real.one", "@\u{2060}everyone", None)],
        )
        .expect("list");

        let parsed = parse_text(&render_text(&list)).expect("parses");

        assert_eq!(parsed_names(&parsed), vec!["@everyone"]);
    }

    #[test]
    fn a_bare_id_list_parses() {
        let parsed = parse_text("example.framework\nsomeone.localmod\n").expect("parses");

        assert_eq!(ids(&parsed), vec!["example.framework", "someone.localmod"]);
        assert!(parsed.list.entries().iter().all(|e| e.name.is_none()));
    }

    #[test]
    fn dot_and_paren_numbering_both_parse() {
        let parsed =
            parse_text("1. First [a.one]\n2) Second [b.two]\n3 Third [c.three]\n").expect("parses");

        assert_eq!(ids(&parsed), vec!["a.one", "b.two", "c.three"]);
        let names: Vec<_> = parsed
            .list
            .entries()
            .iter()
            .filter_map(|e| e.name.as_ref().map(ListedName::as_str))
            .collect();
        assert_eq!(names, vec!["First", "Second", "3 Third"]);
    }

    #[test]
    fn a_numbered_bare_id_is_not_mistaken_for_numbering() {
        let parsed = parse_text("1.foo.bar\n").expect("parses");

        assert_eq!(ids(&parsed), vec!["1.foo.bar"]);
    }

    #[test]
    fn only_steam_workshop_urls_give_a_workshop_id() {
        let parsed = parse_text(
            "A [a.one] <https://example.org/sharedfiles/filedetails/?id=5>\n\
             B [b.two] https://steamcommunity.com/workshop/filedetails/?id=77\n\
             C [c.three] <https://steamcommunity.com/workshop/filedetails/?id=88&x=1>\n\
             D [d.four] <https://steamcommunity.com/sharedfiles/filedetails/?id=0>\n",
        )
        .expect("parses");

        let workshop: Vec<_> = parsed
            .list
            .entries()
            .iter()
            .map(|e| e.workshop_id.map(WorkshopId::get))
            .collect();
        assert_eq!(workshop, vec![None, Some(77), Some(88), None]);
    }

    #[test]
    fn comment_lines_and_blank_lines_are_ignored() {
        let parsed = parse_text("# a comment\n\n   \n# RimWorld 1.5\nfoo.bar\n").expect("parses");

        assert_eq!(ids(&parsed), vec!["foo.bar"]);
        assert!(parsed.skipped.is_empty());
        assert_eq!(
            parsed.list.game_version().map(ListedGameVersion::as_str),
            Some("1.5")
        );
    }

    #[test]
    fn a_junk_line_is_reported_with_its_one_based_line_number() {
        let parsed = parse_text("foo.bar\nthis is not a mod\n\nnodot\nbaz.qux\n").expect("parses");

        assert_eq!(ids(&parsed), vec!["foo.bar", "baz.qux"]);
        assert_eq!(
            parsed.skipped,
            vec![
                SkippedEntry::NotAnEntry { line: 2 },
                SkippedEntry::NotAnEntry { line: 4 },
            ]
        );
    }

    #[test]
    fn a_bracketed_id_that_fails_the_grammar_is_malformed_not_fatal() {
        let parsed = parse_text("Good [a.one]\nBad [../x]\nGood [b.two]\n").expect("parses");

        assert_eq!(ids(&parsed), vec!["a.one", "b.two"]);
        assert_eq!(
            parsed.skipped,
            vec![SkippedEntry::MalformedId {
                position: 2,
                text: TruncatedText::new("../x"),
            }]
        );
    }

    #[test]
    fn crlf_and_lf_and_a_bom_are_handled() {
        let crlf = parse_text("\u{feff}# RimWorld 1.6\r\n1. A [a.one]\r\n2. B [b.two]\r\n")
            .expect("parses");
        let lf = parse_text("# RimWorld 1.6\n1. A [a.one]\n2. B [b.two]\n").expect("parses");

        assert_eq!(crlf, lf);
        assert_eq!(ids(&crlf), vec!["a.one", "b.two"]);
    }

    #[test]
    fn a_line_over_the_byte_limit_is_skipped() {
        let long_name = "n".repeat(ModListLimits::MAX_LINE_BYTES);
        let input = format!("1. {long_name} [a.one]\n2. Short [b.two]\n");

        let parsed = parse_text(&input).expect("parses");

        assert_eq!(ids(&parsed), vec!["b.two"]);
        assert_eq!(parsed.skipped, vec![SkippedEntry::NotAnEntry { line: 1 }]);
    }

    #[test]
    fn empty_and_comment_only_input_has_no_entries() {
        assert_eq!(parse_text(""), Err(Rejection::NoEntries));
        assert_eq!(parse_text("# RimWorld 1.6\n\n"), Err(Rejection::NoEntries));
        assert_eq!(parse_text("junk only\n"), Err(Rejection::NoEntries));
    }

    #[test]
    fn input_over_the_byte_limit_is_too_large() {
        let input = "a.b\n".repeat(ModListLimits::MAX_INPUT_BYTES / 4 + 1);

        assert_eq!(
            parse_text(&input),
            Err(Rejection::TooLarge {
                limit_bytes: ModListLimits::MAX_INPUT_BYTES
            })
        );
    }

    #[test]
    fn more_entries_than_the_limit_is_rejected_and_the_limit_itself_is_fine() {
        let at_limit = "a.b\n".repeat(ModListLimits::MAX_ENTRIES);
        assert!(parse_text(&at_limit).is_ok());

        let over = "a.b\n".repeat(ModListLimits::MAX_ENTRIES + 1);
        assert_eq!(
            parse_text(&over),
            Err(Rejection::TooManyEntries {
                limit: ModListLimits::MAX_ENTRIES
            })
        );
    }

    #[test]
    fn skipped_entries_are_capped_and_the_rest_counted() {
        let junk_lines = ModListLimits::MAX_SKIPPED_REPORTED + 25;
        let input = format!("{}real.one\n", "junk\n".repeat(junk_lines));

        let parsed = parse_text(&input).expect("parses");

        assert_eq!(parsed.skipped.len(), ModListLimits::MAX_SKIPPED_REPORTED);
        assert_eq!(parsed.omitted_skipped, 25);
    }
}
