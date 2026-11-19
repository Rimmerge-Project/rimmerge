//! [`FileModTimestampReader`]: when each mod was last updated, for the
//! Mods page's "last updated" display sort.
//!
//! - **Workshop** mods: `timeupdated` from Steam's own record,
//!   `<library>/steamapps/workshop/appworkshop_294100.acf` (the parent of
//!   the workshop `content` folder), under `WorkshopItemsInstalled`: when
//!   the installed version was published on the Workshop (not when it was
//!   downloaded).
//! - **Local** mods: the modification time of `About/About.xml`, "last
//!   modified on disk".
//! - **Core/DLC**: unavailable, they ship with the game.
//!
//! Every failure (no `.acf`, one that is oversized or malformed, no
//! `About.xml`, a time of zero or past the year 9999) answers `Unavailable(Unknown)`: never an
//! error, never an epoch guess. The `.acf` is untrusted input, so it is
//! read through a size bound and parsed by a small tolerant tokenizer
//! with a depth bound. Local only: nothing here touches the network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rim_analyzer::domain::{ModId, Source};
use rim_analyzer::infra::read_bounded_with_limit;
use rim_session::ModTimeSubject;
use rim_session::ports::ModTimestampReader;
use rim_session::{UnixSeconds, UpdateTime, UpdateUnavailable};

/// The `.acf` is a few hundred KB on a large install; 4 MiB is far past
/// any real file and still refuses a hostile one.
const MAX_ACF_BYTES: u64 = 4 * 1024 * 1024;

/// 9999-12-31T23:59:59Z, the last second a date formatter can show. A
/// larger `timeupdated` or mtime is corrupt, not a time.
const MAX_PLAUSIBLE_SECONDS: u64 = 253_402_300_799;

/// The `.acf` nests three levels deep; anything past this is not one.
const MAX_ACF_DEPTH: usize = 16;

const ACF_FILE_NAME: &str = "appworkshop_294100.acf";
const INSTALLED_SECTION: &str = "WorkshopItemsInstalled";
const UPDATED_KEY: &str = "timeupdated";

/// Reads update times from Steam's `.acf` and each local mod's
/// `About.xml`. Stateless: every call reads from disk.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileModTimestampReader;

impl FileModTimestampReader {
    /// Builds the reader.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ModTimestampReader for FileModTimestampReader {
    fn read_update_times(
        &self,
        workshop_dir: &Path,
        subjects: &[ModTimeSubject],
    ) -> BTreeMap<ModId, UpdateTime> {
        let steam_times = if subjects.iter().any(|s| s.source == Source::Workshop) {
            acf_path(workshop_dir)
                .map(|path| read_installed_times(&path, MAX_ACF_BYTES))
                .unwrap_or_default()
        } else {
            BTreeMap::new()
        };
        subjects
            .iter()
            .map(|subject| (subject.id.clone(), update_time(subject, &steam_times)))
            .collect()
    }
}

fn update_time(subject: &ModTimeSubject, steam_times: &BTreeMap<u64, u64>) -> UpdateTime {
    match subject.source {
        Source::Core | Source::Dlc => UpdateTime::Unavailable(UpdateUnavailable::ShippedWithGame),
        Source::Workshop => subject
            .workshop_id
            .and_then(|id| steam_times.get(&id))
            .map_or(UpdateTime::Unavailable(UpdateUnavailable::Unknown), |&s| {
                UpdateTime::Known(UnixSeconds::new(s))
            }),
        Source::Local => about_modified(&subject.path),
    }
}

fn about_modified(mod_root: &Path) -> UpdateTime {
    let seconds = std::fs::metadata(mod_root.join("About").join("About.xml"))
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs())
        .and_then(plausible);
    seconds.map_or(UpdateTime::Unavailable(UpdateUnavailable::Unknown), |s| {
        UpdateTime::Known(UnixSeconds::new(s))
    })
}

/// `<library>/steamapps/workshop/appworkshop_294100.acf` for a workshop
/// content folder `<library>/steamapps/workshop/content/294100`.
fn acf_path(workshop_dir: &Path) -> Option<PathBuf> {
    let workshop = workshop_dir.parent()?.parent()?;
    Some(workshop.join(ACF_FILE_NAME))
}

/// Workshop item id -> installed `timeupdated`, or empty when the file is
/// missing, over `limit` bytes, not UTF-8, or malformed.
fn read_installed_times(path: &Path, limit: u64) -> BTreeMap<u64, u64> {
    let Ok(bytes) = read_bounded_with_limit(path, limit) else {
        return BTreeMap::new();
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return BTreeMap::new();
    };
    parse_installed_times(&text).unwrap_or_default()
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    Text(String),
    Open,
    Close,
}

/// A stray character or an unterminated string.
#[derive(Debug, PartialEq, Eq)]
struct Malformed;

/// Streams Valve's key-value text as quoted strings and braces, one token
/// per `next`, so a hostile file never becomes a vector of tokens. Yields
/// `Err(Malformed)` once, on a stray character or an unterminated string,
/// and ends there.
struct Tokens<'a> {
    chars: std::iter::Peekable<std::str::Chars<'a>>,
    is_done: bool,
}

impl<'a> Tokens<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            chars: text.chars().peekable(),
            is_done: false,
        }
    }

    fn fail(&mut self) -> Option<Result<Token, Malformed>> {
        self.is_done = true;
        Some(Err(Malformed))
    }

    fn skip_comment(&mut self) {
        for skipped in self.chars.by_ref() {
            if skipped == '\n' {
                break;
            }
        }
    }

    fn read_quoted(&mut self) -> Option<String> {
        let mut value = String::new();
        loop {
            match self.chars.next()? {
                '"' => return Some(value),
                '\\' => value.push(match self.chars.next()? {
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                }),
                other => value.push(other),
            }
        }
    }
}

impl Iterator for Tokens<'_> {
    type Item = Result<Token, Malformed>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.is_done {
            return None;
        }
        while let Some(c) = self.chars.next() {
            match c {
                c if c.is_whitespace() => {}
                '{' => return Some(Ok(Token::Open)),
                '}' => return Some(Ok(Token::Close)),
                '/' if self.chars.peek() == Some(&'/') => self.skip_comment(),
                '"' => {
                    return match self.read_quoted() {
                        Some(text) => Some(Ok(Token::Text(text))),
                        None => self.fail(),
                    };
                }
                _ => return self.fail(),
            }
        }
        None
    }
}

/// A time past the year 9999 is not a Steam or file time: it would make
/// the display's date formatting throw. Zero is no time either.
fn plausible(seconds: u64) -> Option<u64> {
    (1..=MAX_PLAUSIBLE_SECONDS)
        .contains(&seconds)
        .then_some(seconds)
}

/// Walks the token stream, collecting `timeupdated` of each item directly
/// under `AppWorkshop/WorkshopItemsInstalled`. A time of zero or past
/// [`MAX_PLAUSIBLE_SECONDS`] is no time. Every token is read as it is
/// needed, so a malformed or too-deep file is refused at the first bad
/// token.
fn parse_installed_times(text: &str) -> Option<BTreeMap<u64, u64>> {
    parse_tokens(Tokens::new(text))
}

fn parse_tokens(
    mut tokens: impl Iterator<Item = Result<Token, Malformed>>,
) -> Option<BTreeMap<u64, u64>> {
    let mut times = BTreeMap::new();
    let mut path: Vec<String> = Vec::new();
    while let Some(token) = tokens.next() {
        match token.ok()? {
            Token::Text(key) => match tokens.next()?.ok()? {
                Token::Open => {
                    if path.len() >= MAX_ACF_DEPTH {
                        return None;
                    }
                    path.push(key);
                }
                Token::Text(value) => {
                    if let Some(item) = installed_item(&path)
                        && key.eq_ignore_ascii_case(UPDATED_KEY)
                        && let Some(seconds) = value.parse::<u64>().ok().and_then(plausible)
                    {
                        times.insert(item, seconds);
                    }
                }
                Token::Close => return None,
            },
            Token::Close => {
                path.pop()?;
            }
            Token::Open => return None,
        }
    }
    path.is_empty().then_some(times)
}

/// The item id when `path` is `AppWorkshop/WorkshopItemsInstalled/<id>`.
fn installed_item(path: &[String]) -> Option<u64> {
    match path {
        [_, section, id] if section.eq_ignore_ascii_case(INSTALLED_SECTION) => id.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, SystemTime};

    use tempfile::TempDir;

    use super::*;

    const VALID_ACF: &str = r#"
"AppWorkshop"
{
	"appid"		"294100"
	"WorkshopItemsInstalled"
	{
		"1111"
		{
			"size"		"10"
			"timeupdated"		"1700000000"
			"manifest"		"5"
		}
		"2222"
		{
			"timeupdated"		"1600000000"
		}
		"3333"
		{
			"timeupdated"		"0"
		}
	}
	"WorkshopItemDetails"
	{
		"4444"
		{
			"timeupdated"		"1800000000"
		}
	}
}
"#;

    fn subject(id: &str, source: Source, path: &Path, workshop_id: Option<u64>) -> ModTimeSubject {
        ModTimeSubject {
            id: ModId::new(id),
            source,
            path: path.to_path_buf(),
            workshop_id,
        }
    }

    /// `<library>/steamapps/workshop/content/294100`, with `acf` written
    /// beside `content` when given. Returns the temp root and the content
    /// folder.
    fn library(acf: Option<&str>) -> (TempDir, PathBuf) {
        let root = TempDir::new().expect("temp dir");
        let workshop = root.path().join("steamapps").join("workshop");
        let content = workshop.join("content").join("294100");
        fs::create_dir_all(&content).expect("create workshop content");
        if let Some(text) = acf {
            fs::write(workshop.join(ACF_FILE_NAME), text).expect("write acf");
        }
        (root, content)
    }

    fn read_one(workshop_dir: &Path, subject: ModTimeSubject) -> UpdateTime {
        let id = subject.id.clone();
        FileModTimestampReader::new()
            .read_update_times(workshop_dir, &[subject])
            .remove(&id)
            .expect("an answer for every subject")
    }

    fn known(seconds: u64) -> UpdateTime {
        UpdateTime::Known(UnixSeconds::new(seconds))
    }

    fn unknown() -> UpdateTime {
        UpdateTime::Unavailable(UpdateUnavailable::Unknown)
    }

    #[test]
    fn a_workshop_mod_reads_timeupdated_from_the_installed_section() {
        let (_root, content) = library(Some(VALID_ACF));

        let time = read_one(
            &content,
            subject("w", Source::Workshop, Path::new("x"), Some(1111)),
        );

        assert_eq!(time, known(1_700_000_000));
    }

    #[test]
    fn an_item_only_in_the_details_section_is_unavailable() {
        let (_root, content) = library(Some(VALID_ACF));

        let time = read_one(
            &content,
            subject("w", Source::Workshop, Path::new("x"), Some(4444)),
        );

        assert_eq!(time, unknown());
    }

    #[test]
    fn a_zero_timeupdated_is_unavailable_not_the_epoch() {
        let (_root, content) = library(Some(VALID_ACF));

        let time = read_one(
            &content,
            subject("w", Source::Workshop, Path::new("x"), Some(3333)),
        );

        assert_eq!(time, unknown());
    }

    #[test]
    fn a_workshop_mod_without_an_id_or_a_record_is_unavailable() {
        let (_root, content) = library(Some(VALID_ACF));

        assert_eq!(
            read_one(
                &content,
                subject("a", Source::Workshop, Path::new("x"), None)
            ),
            unknown()
        );
        assert_eq!(
            read_one(
                &content,
                subject("b", Source::Workshop, Path::new("x"), Some(9))
            ),
            unknown()
        );
    }

    #[test]
    fn a_missing_acf_degrades_every_workshop_mod_to_unavailable() {
        let (_root, content) = library(None);

        let time = read_one(
            &content,
            subject("w", Source::Workshop, Path::new("x"), Some(1111)),
        );

        assert_eq!(time, unknown());
    }

    #[test]
    fn a_workshop_dir_with_no_parents_degrades_instead_of_failing() {
        let time = read_one(
            Path::new("294100"),
            subject("w", Source::Workshop, Path::new("x"), Some(1111)),
        );

        assert_eq!(time, unknown());
    }

    #[test]
    fn malformed_acf_text_degrades_to_unavailable() {
        for broken in [
            "\"AppWorkshop\" { \"WorkshopItemsInstalled\" { \"1111\" { \"timeupdated\" \"5\" }",
            "\"AppWorkshop\" { \"unterminated",
            "not vdf at all",
            "\"AppWorkshop\" } }",
            "\"AppWorkshop\" \"x\" \"y\"",
        ] {
            let (_root, content) = library(Some(broken));

            let time = read_one(
                &content,
                subject("w", Source::Workshop, Path::new("x"), Some(1111)),
            );

            assert_eq!(time, unknown(), "input: {broken}");
        }
    }

    #[test]
    fn an_acf_nested_past_the_depth_bound_is_refused() {
        let nested = "\"k\" {".repeat(MAX_ACF_DEPTH + 1) + &"}".repeat(MAX_ACF_DEPTH + 1);

        assert_eq!(parse_installed_times(&nested), None);
    }

    #[test]
    fn a_pathological_input_is_refused_without_reading_every_token() {
        // An endless stream cannot be collected: only a parser that stops
        // at the depth bound returns.
        let endless = (0..).flat_map(|_| [Ok(Token::Text("k".to_owned())), Ok(Token::Open)]);

        assert_eq!(parse_tokens(endless), None);
    }

    #[test]
    fn a_long_run_of_open_braces_is_refused_at_the_first_one() {
        assert_eq!(parse_installed_times(&"{".repeat(1_000_000)), None);
    }

    #[test]
    fn an_out_of_range_timeupdated_is_unavailable() {
        for huge in ["18446744073709551615", "253402300800"] {
            let acf = format!(
                "\"AppWorkshop\" {{ \"WorkshopItemsInstalled\" {{ \"1111\" {{ \"timeupdated\" \"{huge}\" }} }} }}"
            );
            let (_root, content) = library(Some(&acf));

            let time = read_one(
                &content,
                subject("w", Source::Workshop, Path::new("x"), Some(1111)),
            );

            assert_eq!(time, unknown(), "timeupdated: {huge}");
        }
    }

    #[test]
    fn the_last_plausible_second_is_still_a_time() {
        assert_eq!(
            plausible(MAX_PLAUSIBLE_SECONDS),
            Some(MAX_PLAUSIBLE_SECONDS)
        );
        assert_eq!(plausible(MAX_PLAUSIBLE_SECONDS + 1), None);
        assert_eq!(plausible(0), None);
    }

    #[test]
    fn an_oversized_acf_is_refused_and_one_at_the_limit_is_read() {
        let (_root, content) = library(Some(VALID_ACF));
        let acf = acf_path(&content).expect("acf path");
        let size = fs::metadata(&acf).expect("acf metadata").len();

        assert_eq!(read_installed_times(&acf, size - 1), BTreeMap::new());
        assert_eq!(
            read_installed_times(&acf, size).get(&1111),
            Some(&1_700_000_000)
        );
    }

    #[test]
    fn an_acf_that_is_not_utf8_degrades_to_unavailable() {
        let (_root, content) = library(None);
        let acf = acf_path(&content).expect("acf path");
        fs::write(&acf, [0xff, 0xfe, 0xfd]).expect("write acf");

        assert_eq!(read_installed_times(&acf, MAX_ACF_BYTES), BTreeMap::new());
    }

    #[test]
    fn a_local_mod_reads_the_about_xml_modification_time() {
        let (_root, content) = library(None);
        let mod_root = content.parent().expect("parent").join("local_mod");
        fs::create_dir_all(mod_root.join("About")).expect("create About");
        let about = fs::File::create(mod_root.join("About").join("About.xml")).expect("About.xml");
        about
            .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_650_000_000))
            .expect("set mtime");

        let time = read_one(&content, subject("l", Source::Local, &mod_root, None));

        assert_eq!(time, known(1_650_000_000));
    }

    #[test]
    fn a_local_mod_without_about_xml_is_unavailable() {
        let (_root, content) = library(None);

        let time = read_one(&content, subject("l", Source::Local, &content, None));

        assert_eq!(time, unknown());
    }

    #[test]
    fn core_and_dlc_are_shipped_with_the_game_whatever_their_files_say() {
        let (_root, content) = library(None);
        let shipped = UpdateTime::Unavailable(UpdateUnavailable::ShippedWithGame);

        assert_eq!(
            read_one(&content, subject("core", Source::Core, &content, None)),
            shipped
        );
        assert_eq!(
            read_one(&content, subject("dlc", Source::Dlc, &content, Some(1111))),
            shipped
        );
    }

    #[test]
    fn every_subject_gets_an_answer() {
        let (_root, content) = library(Some(VALID_ACF));
        let subjects = [
            subject("w", Source::Workshop, Path::new("x"), Some(2222)),
            subject("c", Source::Core, Path::new("x"), None),
            subject("l", Source::Local, Path::new("nowhere"), None),
        ];

        let times = FileModTimestampReader::new().read_update_times(&content, &subjects);

        assert_eq!(times.len(), 3);
        assert_eq!(times.get(&ModId::new("w")), Some(&known(1_600_000_000)));
    }
}
