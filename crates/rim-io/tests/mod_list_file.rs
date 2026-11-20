//! `RmlFileStore` against real files in temp dirs: the `.rml` round trip,
//! the other accepted shapes, the writer's bytes, and the hostile inputs.
//! Every id and name is invented.

use std::fs;
use std::path::Path;

use rim_io::RmlFileStore;
use rim_session::mod_list::{
    ListAlignment, ListFormat, ListedGameVersion, ListedName, ListedPackageId, ModListLimits,
    ParsedModList, Rejection, SharedModEntry, SharedModList, SkippedEntry, WorkshopId,
};
use rim_session::ports::{ModListFileStore, ModListRead};
use tempfile::tempdir;

fn entry(id: &str, name: Option<&str>, workshop_id: Option<u64>) -> SharedModEntry {
    SharedModEntry {
        id: ListedPackageId::try_from(id).unwrap_or_else(|e| panic!("valid id {id:?}: {e}")),
        name: name.and_then(ListedName::new),
        workshop_id: workshop_id.and_then(WorkshopId::new),
    }
}

fn read_path(path: &Path) -> ModListRead {
    RmlFileStore::new()
        .read(path)
        .unwrap_or_else(|e| panic!("file is readable: {e}"))
}

fn read_bytes(bytes: &[u8]) -> ModListRead {
    let dir = tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let path = dir.path().join("input.bin");
    fs::write(&path, bytes).unwrap_or_else(|e| panic!("write input: {e}"));
    read_path(&path)
}

fn parsed(bytes: &[u8]) -> ParsedModList {
    match read_bytes(bytes) {
        ModListRead::Parsed(parsed) => parsed,
        ModListRead::Rejected(rejection) => panic!("unexpectedly rejected: {rejection:?}"),
    }
}

fn rejected(bytes: &[u8]) -> Rejection {
    match read_bytes(bytes) {
        ModListRead::Rejected(rejection) => rejection,
        ModListRead::Parsed(_) => panic!("unexpectedly parsed"),
    }
}

fn li(values: &[&str]) -> String {
    values.iter().map(|v| format!("<li>{v}</li>")).collect()
}

/// A compact `.rml` document from raw (already escaped) values.
fn rml(ids: &[&str], steam_ids: &[&str], names: &[&str]) -> String {
    format!(
        "<savedModList><meta><gameVersion>1.6.4871 rev591</gameVersion>\
         <modIds>{ids}</modIds><modSteamIds>{steam}</modSteamIds><modNames>{names_li}</modNames>\
         </meta><modList><ids>{ids}</ids><names>{names_li}</names></modList></savedModList>",
        ids = li(ids),
        steam = li(steam_ids),
        names_li = li(names),
    )
}

fn ids_of(parsed: &ParsedModList) -> Vec<&str> {
    parsed
        .list
        .entries()
        .iter()
        .map(|e| e.id.as_str())
        .collect()
}

fn workshop_of(parsed: &ParsedModList) -> Vec<Option<u64>> {
    parsed
        .list
        .entries()
        .iter()
        .map(|e| e.workshop_id.map(WorkshopId::get))
        .collect()
}

// ---- reading what is accepted -------------------------------------------

#[test]
fn a_written_rml_reads_back_to_the_same_list() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("list.rml");
    let list = SharedModList::new(
        ListedGameVersion::new("1.6.4871 rev591"),
        vec![
            entry("ludeon.rimworld", Some("Core"), None),
            entry("ludeon.rimworld.royalty", Some("Royalty"), None),
            entry(
                "example.framework",
                Some("Example Framework"),
                Some(1_234_567_890),
            ),
            entry("someone.localmod", Some("Some Local Mod"), None),
        ],
    )
    .expect("list");
    let store = RmlFileStore::new();

    store.write(&path, &list).expect("writes");
    let read = store.read(&path).expect("reads");

    assert_eq!(
        read,
        ModListRead::Parsed(ParsedModList {
            format: ListFormat::SavedModList,
            list,
            names: ListAlignment::Aligned,
            workshop_ids: ListAlignment::Aligned,
            skipped: Vec::new(),
            omitted_skipped: 0,
        })
    );
}

#[test]
fn a_game_shaped_file_reads_with_a_dlc_app_id_ignored_and_zero_meaning_none() {
    let mut text = String::from("\u{feff}");
    text.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n");
    text.push_str(&rml(
        &[
            "ludeon.rimworld",
            "ludeon.rimworld.royalty",
            "example.framework",
            "someone.localmod",
        ],
        &["0", "1149640", "1234567890", "0"],
        &["Core", "Royalty", "Example Framework", "Some Local Mod"],
    ));

    let parsed = parsed(text.as_bytes());

    assert_eq!(
        ids_of(&parsed),
        [
            "ludeon.rimworld",
            "ludeon.rimworld.royalty",
            "example.framework",
            "someone.localmod"
        ]
    );
    assert_eq!(
        workshop_of(&parsed),
        [None, None, Some(1_234_567_890), None]
    );
    assert_eq!(
        parsed.list.game_version().map(ListedGameVersion::as_str),
        Some("1.6.4871 rev591")
    );
    assert_eq!(
        parsed.list.entries()[2]
            .name
            .as_ref()
            .map(ListedName::as_str),
        Some("Example Framework")
    );
}

#[test]
fn a_dlc_app_id_is_never_a_workshop_id_even_for_a_dlc_the_writer_does_not_know() {
    let text = rml(
        &["ludeon.rimworld.futuredlc", "ludeon.rimworld"],
        &["424242", "424242"],
        &["Future", "Core"],
    );

    assert_eq!(workshop_of(&parsed(text.as_bytes())), [None, None]);
}

#[test]
fn a_mods_config_shaped_file_reads_ids_and_version_with_no_workshop_ids() {
    let text = "<?xml version=\"1.0\"?>\n<ModsConfigData><version>1.6.4871 rev591</version>\
                <activeMods><li>ludeon.rimworld</li><li>Example.Framework_steam</li></activeMods>\
                <knownExpansions><li>ludeon.rimworld.royalty</li></knownExpansions>\
                </ModsConfigData>";

    let parsed = parsed(text.as_bytes());

    assert_eq!(
        ids_of(&parsed),
        ["ludeon.rimworld", "example.framework_steam"]
    );
    assert_eq!(workshop_of(&parsed), [None, None]);
    assert!(parsed.list.entries().iter().all(|e| e.name.is_none()));
    assert_eq!(
        parsed.list.game_version().map(ListedGameVersion::as_str),
        Some("1.6.4871 rev591")
    );
}

#[test]
fn a_text_file_goes_to_the_text_parser_whatever_its_extension() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("list.xml");
    fs::write(
        &path,
        "# RimWorld 1.6.4871 rev591\n1. Core [ludeon.rimworld]\n2. Example [example.framework]\n",
    )
    .expect("write");

    let ModListRead::Parsed(parsed) = read_path(&path) else {
        panic!("a text list is importable");
    };

    assert_eq!(ids_of(&parsed), ["ludeon.rimworld", "example.framework"]);
}

#[test]
fn names_of_a_different_length_than_ids_are_ignored() {
    let text = rml(&["a.one", "b.two"], &["0", "0"], &["Only One"]);

    let parsed = parsed(text.as_bytes());

    assert_eq!(ids_of(&parsed), ["a.one", "b.two"]);
    assert!(parsed.list.entries().iter().all(|e| e.name.is_none()));
    assert_eq!(
        parsed.names,
        ListAlignment::Misaligned { ids: 2, values: 1 }
    );
    assert_eq!(parsed.workshop_ids, ListAlignment::Aligned);
}

#[test]
fn meta_lists_of_different_lengths_drop_the_workshop_ids_only() {
    let text = rml(&["a.one", "b.two"], &["111"], &["One", "Two"]);

    let parsed = parsed(text.as_bytes());

    assert_eq!(workshop_of(&parsed), [None, None]);
    assert_eq!(
        parsed.workshop_ids,
        ListAlignment::Misaligned { ids: 2, values: 1 }
    );
    assert_eq!(parsed.names, ListAlignment::Aligned);
    assert_eq!(
        parsed.list.entries()[1]
            .name
            .as_ref()
            .map(ListedName::as_str),
        Some("Two")
    );
}

#[test]
fn a_file_without_meta_still_reads_the_order() {
    let text =
        "<savedModList><modList><ids><li>a.one</li><li>b.two</li></ids></modList></savedModList>";

    let parsed = parsed(text.as_bytes());

    assert_eq!(ids_of(&parsed), ["a.one", "b.two"]);
    assert!(parsed.list.game_version().is_none());
}

#[test]
fn malformed_list_items_are_skipped_with_their_position_and_the_rest_kept() {
    let text = "<savedModList><modList><ids>\
                <li>a.one</li>\
                <li></li>\
                <li><x>nested.id</x></li>\
                <li>no spaces.allowed</li>\
                <li>b.two</li>\
                </ids></modList></savedModList>";

    let parsed = parsed(text.as_bytes());

    assert_eq!(ids_of(&parsed), ["a.one", "b.two"]);
    let positions: Vec<u32> = parsed
        .skipped
        .iter()
        .map(|skipped| match skipped {
            SkippedEntry::MalformedId { position, .. } => *position,
            SkippedEntry::NotAnEntry { .. } => panic!("an XML list has no lines"),
        })
        .collect();
    assert_eq!(positions, [2, 3, 4]);
}

#[test]
fn invalid_utf8_is_decoded_lossily_and_only_the_damaged_id_is_skipped() {
    let mut bytes = b"<savedModList><modList><ids><li>a.one</li><li>bad".to_vec();
    bytes.extend_from_slice(&[0xFF, 0xFE]);
    bytes.extend_from_slice(b".id</li><li>b.two</li></ids></modList></savedModList>");

    let parsed = parsed(&bytes);

    assert_eq!(ids_of(&parsed), ["a.one", "b.two"]);
    assert_eq!(parsed.skipped.len(), 1);
}

#[test]
fn a_full_size_list_the_writer_produced_reads_back_and_one_more_is_rejected() {
    let dir = tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let path = dir.path().join("big.rml");
    let entries = (0..ModListLimits::MAX_ENTRIES)
        .map(|n| {
            entry(
                &format!("example.mod{n}"),
                Some(&format!("Mod {n}")),
                Some(n as u64 + 1),
            )
        })
        .collect();
    let list = SharedModList::new(ListedGameVersion::new("1.6.4871 rev591"), entries)
        .unwrap_or_else(|e| panic!("list: {e}"));
    let store = RmlFileStore::new();
    store
        .write(&path, &list)
        .unwrap_or_else(|e| panic!("writes: {e}"));

    let ModListRead::Parsed(read) = read_path(&path) else {
        panic!("a full-size list must be accepted");
    };
    assert_eq!(read.list.entries().len(), ModListLimits::MAX_ENTRIES);
    assert_eq!(read.list, list);

    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read: {e}"));
    let one_more = text.replacen("<ids>\r\n", "<ids>\r\n\t\t\t<li>extra.mod</li>\r\n", 1);
    assert_eq!(
        rejected(one_more.as_bytes()),
        Rejection::TooManyEntries {
            limit: ModListLimits::MAX_ENTRIES
        }
    );
}

#[test]
fn leading_whitespace_before_the_declaration_is_tolerated() {
    let body = "<?xml version=\"1.0\"?><savedModList><modList><ids><li>a.one</li></ids></modList></savedModList>";
    for prefix in ["\r\n  ", "\n", "\u{feff} \n"] {
        let text = format!("{prefix}{body}");
        assert_eq!(ids_of(&parsed(text.as_bytes())), ["a.one"], "{prefix:?}");
    }
}

#[test]
fn a_text_list_that_starts_with_an_angle_bracket_is_text() {
    let url_first = "<https://steamcommunity.com/sharedfiles/filedetails/?id=1234567890>\na.one\n";
    let bracket_first = "<3 Some Mod [a.one]\nb.two\n";

    let from_url = parsed(url_first.as_bytes());
    let from_bracket = parsed(bracket_first.as_bytes());

    assert_eq!(ids_of(&from_bracket), ["a.one", "b.two"]);
    assert_eq!(ids_of(&from_url), ["a.one"]);
    assert_eq!(from_url.skipped, [SkippedEntry::NotAnEntry { line: 1 }]);
}

#[test]
fn invalid_bytes_cannot_push_an_in_limit_text_file_over_the_limit() {
    let mut bytes = b"a.one\n".to_vec();
    bytes.resize(ModListLimits::MAX_INPUT_BYTES, 0xFF);

    assert_eq!(ids_of(&parsed(&bytes)), ["a.one"]);
}

#[test]
fn leading_blank_lines_do_not_shift_a_text_lines_number() {
    let parsed = parsed(
        b"

junk line
a.one
",
    );

    assert_eq!(parsed.skipped, [SkippedEntry::NotAnEntry { line: 3 }]);
}

// ---- rejections ----------------------------------------------------------

#[test]
fn a_dtd_with_an_external_entity_is_rejected() {
    let text = "<?xml version=\"1.0\"?>\
                <!DOCTYPE savedModList [<!ENTITY e SYSTEM \"file:///nonexistent/secret\">]>\
                <savedModList><modList><ids><li>&e;</li></ids></modList></savedModList>";

    assert_eq!(rejected(text.as_bytes()), Rejection::DtdNotAllowed);
}

#[test]
fn a_billion_laughs_entity_bomb_is_rejected_without_expanding() {
    let text = "<?xml version=\"1.0\"?>\
                <!DOCTYPE lolz [\
                <!ENTITY lol \"lol\">\
                <!ENTITY lol2 \"&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;&lol;\">\
                <!ENTITY lol3 \"&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;&lol2;\">\
                ]>\
                <savedModList><modList><ids><li>&lol3;</li></ids></modList></savedModList>";

    assert_eq!(rejected(text.as_bytes()), Rejection::DtdNotAllowed);
}

#[test]
fn an_empty_doctype_is_rejected_too() {
    let text = "<!DOCTYPE savedModList><savedModList><modList><ids><li>a.one</li></ids></modList></savedModList>";

    assert_eq!(rejected(text.as_bytes()), Rejection::DtdNotAllowed);
}

#[test]
fn an_input_one_byte_over_the_limit_is_too_large() {
    let bytes = vec![b' '; ModListLimits::MAX_INPUT_BYTES + 1];

    assert_eq!(
        rejected(&bytes),
        Rejection::TooLarge {
            limit_bytes: ModListLimits::MAX_INPUT_BYTES
        }
    );
}

#[test]
fn one_entry_over_the_limit_is_too_many_entries() {
    let ids: Vec<String> = (0..=ModListLimits::MAX_ENTRIES)
        .map(|n| format!("example.mod{n}"))
        .collect();
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let text = format!(
        "<savedModList><modList><ids>{}</ids></modList></savedModList>",
        li(&refs)
    );

    assert_eq!(
        rejected(text.as_bytes()),
        Rejection::TooManyEntries {
            limit: ModListLimits::MAX_ENTRIES
        }
    );
}

#[test]
fn too_many_nodes_is_rejected_even_when_no_one_list_is_long() {
    let filler = "<x/>".repeat(ModListLimits::MAX_XML_NODES + 1);
    let text = format!(
        "<savedModList>{filler}<modList><ids><li>a.one</li></ids></modList></savedModList>"
    );

    assert_eq!(
        rejected(text.as_bytes()),
        Rejection::TooManyEntries {
            limit: ModListLimits::MAX_ENTRIES
        }
    );
}

#[test]
fn deep_nesting_is_too_deep() {
    let text = format!(
        "<savedModList>{}{}</savedModList>",
        "<a>".repeat(600),
        "</a>".repeat(600)
    );

    assert_eq!(rejected(text.as_bytes()), Rejection::TooDeep);
}

#[test]
fn an_unknown_root_is_an_unrecognized_format() {
    assert_eq!(
        rejected(b"<?xml version=\"1.0\"?><html><body>hi</body></html>"),
        Rejection::UnrecognizedFormat
    );
}

#[test]
fn an_rml_without_ids_is_a_missing_mod_list() {
    assert_eq!(
        rejected(b"<savedModList><meta><modIds><li>a.one</li></modIds></meta></savedModList>"),
        Rejection::MissingModList
    );
}

#[test]
fn a_mods_config_without_active_mods_is_a_missing_mod_list() {
    assert_eq!(
        rejected(b"<ModsConfigData><version>1</version></ModsConfigData>"),
        Rejection::MissingModList
    );
}

#[test]
fn broken_xml_is_malformed() {
    assert_eq!(
        rejected(b"<savedModList><modList><ids></modList>"),
        Rejection::MalformedXml
    );
}

#[test]
fn a_list_whose_every_id_is_malformed_names_no_entries() {
    assert_eq!(
        rejected(
            b"<savedModList><modList><ids><li>nodot</li><li></li></ids></modList></savedModList>"
        ),
        Rejection::NoEntries
    );
}

#[test]
fn an_empty_file_names_no_entries() {
    assert_eq!(rejected(b""), Rejection::NoEntries);
}

#[test]
fn a_missing_file_is_an_error_not_a_rejection() {
    let dir = tempdir().expect("tempdir");

    let result = RmlFileStore::new().read(&dir.path().join("absent.rml"));

    assert!(result.is_err());
}

// ---- writing --------------------------------------------------------------

#[test]
fn names_with_xml_special_characters_survive_a_round_trip_verbatim() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("list.rml");
    let name = "Tom & Jerry's <\"Best\"> Mod";
    let list =
        SharedModList::new(None, vec![entry("example.framework", Some(name), None)]).expect("list");
    let store = RmlFileStore::new();

    store.write(&path, &list).expect("writes");

    let ModListRead::Parsed(read) = store.read(&path).expect("reads") else {
        panic!("rejected");
    };
    assert_eq!(
        read.list.entries()[0].name.as_ref().map(ListedName::as_str),
        Some(name)
    );
    let on_disk = fs::read_to_string(&path).expect("utf-8");
    assert!(on_disk.contains("Tom &amp; Jerry's &lt;\"Best\"&gt; Mod"));
}

#[test]
fn the_written_file_has_the_bom_crlf_and_tabs_the_game_writes() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("list.rml");
    let list =
        SharedModList::new(None, vec![entry("example.framework", None, None)]).expect("list");

    RmlFileStore::new().write(&path, &list).expect("writes");

    let bytes = fs::read(&path).expect("read");
    assert!(bytes.starts_with(b"\xef\xbb\xbf<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n"));
    let text = String::from_utf8(bytes).expect("utf-8");
    assert!(text.contains("\r\n\t<meta>\r\n\t\t<modIds>\r\n\t\t\t<li>example.framework</li>\r\n"));
    assert!(!text.replace("\r\n", "").contains(['\r', '\n']));
    assert!(text.ends_with("</savedModList>"));
}

#[test]
fn writing_replaces_an_existing_file_and_leaves_no_temp_file() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("list.rml");
    fs::write(&path, "stale").expect("seed");
    let list =
        SharedModList::new(None, vec![entry("example.framework", None, None)]).expect("list");

    RmlFileStore::new().write(&path, &list).expect("writes");

    assert!(
        fs::read(&path)
            .expect("read")
            .starts_with(b"\xef\xbb\xbf<?xml")
    );
    let names: Vec<_> = fs::read_dir(dir.path())
        .expect("list dir")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(names.len(), 1, "only the list remains: {names:?}");
}

#[test]
fn read_mod_list_on_a_reader_matches_reading_the_same_bytes_from_a_file() {
    let text_list = b"1. Example [example.framework]\n2. Local [someone.localmod]\n".as_slice();
    let oversized = vec![b'a'; ModListLimits::MAX_INPUT_BYTES + 1];
    let save = save_file(&["example.alpha"], &["0"], &["Alpha"], "1.6.4871 rev591");

    for bytes in [
        text_list,
        b"<!DOCTYPE x>".as_slice(),
        oversized.as_slice(),
        save.as_bytes(),
    ] {
        assert_eq!(
            rim_io::read_mod_list(bytes).expect("reads"),
            read_bytes(bytes)
        );
    }
}

// ---- a RimWorld save: only its header is read ----------------------------

/// The header of a save shaped as the game writes it (BOM, declaration,
/// CRLF, tabs), up to and including `</meta>`.
fn save_header_text(ids: &[&str], steam_ids: &[&str], names: &[&str], version: &str) -> String {
    let lines = |values: &[&str]| -> String {
        values
            .iter()
            .map(|value| format!("\t\t\t<li>{value}</li>\r\n"))
            .collect()
    };
    format!(
        "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<savegame>\r\n\t<meta>\r\n\
         \t\t<gameVersion>{version}</gameVersion>\r\n\t\t<modIds>\r\n{ids}\t\t</modIds>\r\n\
         \t\t<modSteamIds>\r\n{steam}\t\t</modSteamIds>\r\n\
         \t\t<modNames>\r\n{names}\t\t</modNames>\r\n\t</meta>\r\n",
        ids = lines(ids),
        steam = lines(steam_ids),
        names = lines(names),
    )
}

/// A whole small save: the header and a body that is never read.
fn save_file(ids: &[&str], steam_ids: &[&str], names: &[&str], version: &str) -> String {
    let mut text = save_header_text(ids, steam_ids, names, version);
    text.push_str("\t<game>\r\n\t\t<colony>body</colony>\r\n\t</game>\r\n</savegame>");
    text
}

const SAVE_IDS: [&str; 5] = [
    "example.earlyhook",
    "ludeon.rimworld",
    "ludeon.rimworld.royalty",
    "example.alphathings_steam",
    "example.betatools",
];

fn standard_save() -> String {
    save_file(
        &SAVE_IDS,
        &["0", "0", "1149640", "1234567890", "0"],
        &[
            "Early Hook",
            "Core",
            "Royalty",
            "Alpha Things &amp; More",
            "Beta Tools",
        ],
        "1.6.4871 rev591",
    )
}

#[test]
fn a_game_shaped_save_reads_its_header_as_a_list() {
    let parsed = parsed(standard_save().as_bytes());

    assert_eq!(parsed.format, ListFormat::SaveHeader);
    assert_eq!(ids_of(&parsed), SAVE_IDS);
    assert_eq!(
        workshop_of(&parsed),
        [None, None, None, Some(1_234_567_890), None]
    );
    assert_eq!(parsed.names, ListAlignment::Aligned);
    assert_eq!(parsed.workshop_ids, ListAlignment::Aligned);
    assert_eq!(
        parsed.list.game_version().map(ListedGameVersion::as_str),
        Some("1.6.4871 rev591")
    );
    assert_eq!(
        parsed.list.entries()[3]
            .name
            .as_ref()
            .map(ListedName::as_str),
        Some("Alpha Things & More")
    );
}

#[test]
fn a_save_id_written_with_the_steam_suffix_is_kept_as_written() {
    let parsed = parsed(standard_save().as_bytes());

    assert_eq!(ids_of(&parsed)[3], "example.alphathings_steam");
}

#[test]
fn a_save_body_after_the_header_is_never_read() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("big.rws");
    let mut bytes =
        save_header_text(&["ludeon.rimworld"], &["0"], &["Core"], "1.6.4871").into_bytes();
    bytes.extend_from_slice(b"\t<game><!DOCTYPE x><unclosed>\xff\xfe");
    bytes.resize(bytes.len() + 16 * 1024 * 1024, 0xff);
    fs::write(&path, &bytes).expect("write");

    let ModListRead::Parsed(parsed) = read_path(&path) else {
        panic!("the header alone is a list");
    };

    assert_eq!(ids_of(&parsed), ["ludeon.rimworld"]);
}

#[test]
fn reading_a_save_leaves_its_bytes_and_modified_time_unchanged() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("autosave.rws");
    fs::write(&path, standard_save()).expect("write");
    let before_bytes = fs::read(&path).expect("read");
    let before_time = fs::metadata(&path)
        .expect("metadata")
        .modified()
        .expect("mtime");

    let _ = read_path(&path);

    assert_eq!(fs::read(&path).expect("read"), before_bytes);
    assert_eq!(
        fs::metadata(&path)
            .expect("metadata")
            .modified()
            .expect("mtime"),
        before_time
    );
}

#[test]
fn a_save_that_ends_inside_or_right_after_its_meta_is_truncated() {
    for text in [
        "<savegame><meta><gameVersion>1.6</gameVersion><modIds><li>example.a</li>",
        "<savegame><meta>",
    ] {
        assert_eq!(
            rejected(text.as_bytes()),
            Rejection::SaveHeaderTruncated,
            "{text}"
        );
    }
}

#[test]
fn a_save_whose_first_child_is_not_meta_has_no_header() {
    for text in [
        "<savegame><game><x/></game></savegame>",
        "<savegame><foo/><meta><modIds><li>example.a</li></modIds></meta>",
    ] {
        assert_eq!(
            rejected(text.as_bytes()),
            Rejection::SaveHeaderMissing,
            "{text}"
        );
    }
}

#[test]
fn a_save_meta_without_mod_ids_is_a_missing_mod_list() {
    let text = "<savegame><meta><gameVersion>1.6</gameVersion></meta><game/></savegame>";

    assert_eq!(rejected(text.as_bytes()), Rejection::MissingModList);
}

#[test]
fn a_save_header_beyond_the_budget_is_too_large() {
    let mut text = String::from("<savegame><meta>");
    text.push_str(&" ".repeat(ModListLimits::MAX_INPUT_BYTES));
    text.push_str("</meta>");

    assert_eq!(
        rejected(text.as_bytes()),
        Rejection::SaveHeaderTooLarge {
            limit_bytes: ModListLimits::MAX_INPUT_BYTES
        }
    );
}

#[test]
fn a_save_header_holds_the_same_bounds_as_every_other_list() {
    let too_many: Vec<String> = (0..=ModListLimits::MAX_ENTRIES)
        .map(|index| format!("example.m{index}"))
        .collect();
    let too_many_refs: Vec<&str> = too_many.iter().map(String::as_str).collect();
    let many = save_file(&too_many_refs, &[], &[], "1.6");
    let deep = format!(
        "<savegame><meta><modIds><li>{}{}</li></modIds></meta>",
        "<a>".repeat(600),
        "</a>".repeat(600)
    );

    assert_eq!(
        rejected(many.as_bytes()),
        Rejection::TooManyEntries {
            limit: ModListLimits::MAX_ENTRIES
        }
    );
    assert_eq!(rejected(deep.as_bytes()), Rejection::TooDeep);
    assert_eq!(
        rejected(b"<!DOCTYPE x [<!ENTITY a \"b\">]><savegame><meta>"),
        Rejection::DtdNotAllowed
    );
}

#[test]
fn a_meta_end_inside_a_comment_never_reads_as_a_shorter_list() {
    let text = "<savegame><meta><!-- </meta> --><modIds><li>example.a</li></modIds></meta>";

    assert_eq!(rejected(text.as_bytes()), Rejection::MalformedXml);
}

#[test]
fn a_save_skips_a_malformed_id_and_rejects_a_list_of_only_malformed_ids() {
    let some = save_file(
        &["example.ok", "not an id"],
        &["0", "0"],
        &["Ok", "Bad"],
        "1.6",
    );
    let none = save_file(&["not an id"], &["0"], &["Bad"], "1.6");

    let parsed = parsed(some.as_bytes());

    assert_eq!(ids_of(&parsed), ["example.ok"]);
    assert!(matches!(
        parsed.skipped.as_slice(),
        [SkippedEntry::MalformedId { position: 2, .. }]
    ));
    assert_eq!(rejected(none.as_bytes()), Rejection::NoEntries);
}

#[test]
fn a_save_with_one_name_too_few_drops_the_names_and_says_so() {
    let text = save_file(
        &["example.a", "example.b", "example.c", "example.d"],
        &["0", "0", "0", "0"],
        &["A", "B", "C"],
        "1.6",
    );

    let parsed = parsed(text.as_bytes());

    assert_eq!(parsed.list.entries().len(), 4);
    assert!(parsed.list.entries().iter().all(|e| e.name.is_none()));
    assert_eq!(
        parsed.names,
        ListAlignment::Misaligned { ids: 4, values: 3 }
    );
    assert_eq!(parsed.workshop_ids, ListAlignment::Aligned);
}

#[test]
fn a_save_with_one_steam_id_too_many_drops_the_links_and_says_so() {
    let text = save_file(
        &["example.a", "example.b"],
        &["1234567890", "2345678901", "3456789012"],
        &["A", "B"],
        "1.6",
    );

    let parsed = parsed(text.as_bytes());

    assert_eq!(workshop_of(&parsed), [None, None]);
    assert_eq!(
        parsed.workshop_ids,
        ListAlignment::Misaligned { ids: 2, values: 3 }
    );
    assert_eq!(parsed.names, ListAlignment::Aligned);
}

#[test]
fn only_a_root_named_savegame_is_a_save() {
    let lookalike = "<savegameX><meta><modIds><li>example.a</li></modIds></meta></savegameX>";
    let upper = "<SaveGame><meta><modIds><li>example.a</li></modIds></meta></SaveGame>";

    assert_eq!(
        rejected(lookalike.as_bytes()),
        Rejection::UnrecognizedFormat
    );
    assert_eq!(parsed(upper.as_bytes()).format, ListFormat::SaveHeader);
}

#[test]
fn a_save_with_whitespace_before_a_declaration_less_root_is_read() {
    let text = "  \n<savegame><meta><modIds><li>example.a</li></modIds></meta><game/>";

    assert_eq!(ids_of(&parsed(text.as_bytes())), ["example.a"]);
}

#[test]
fn the_other_shapes_report_their_format() {
    let rml_text = rml(&["a.one"], &["0"], &["One"]);

    assert_eq!(parsed(rml_text.as_bytes()).format, ListFormat::SavedModList);
    assert_eq!(parsed(b"a.one\n").format, ListFormat::Text);
    assert_eq!(
        parsed(b"<ModsConfigData><activeMods><li>a.one</li></activeMods></ModsConfigData>").format,
        ListFormat::ModsConfig
    );
}
