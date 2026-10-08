//! Rendering a `.rml`, byte for byte as RimWorld's "Save list" writes it
//! (verified against a list the game saved): a UTF-8 BOM, the
//! `<?xml version="1.0" encoding="utf-8"?>` declaration, CRLF line
//! endings, tab indentation, `<meta>` then `<modList>`, the lists
//! duplicated in both, and no line ending after the root's end tag.

use rim_session::mod_list::{SharedModEntry, SharedModList};

use crate::xml_text::xml_escape_text;

const BOM: &[u8] = b"\xef\xbb\xbf";
const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>";
const EOL: &str = "\r\n";

/// The Steam app ids the game writes in `modSteamIds` for its own DLC
/// (vanilla facts: the id a DLC row carries instead of a Workshop id).
const DLC_APP_IDS: [(&str, u64); 5] = [
    ("ludeon.rimworld.royalty", 1_149_640),
    ("ludeon.rimworld.ideology", 1_392_840),
    ("ludeon.rimworld.biotech", 1_826_140),
    ("ludeon.rimworld.anomaly", 2_380_740),
    ("ludeon.rimworld.odyssey", 3_022_790),
];

/// What the game writes in `modSteamIds` for `entry`: the DLC's app id,
/// else the Workshop id, else `0` ("no link").
fn steam_id_for(entry: &SharedModEntry) -> u64 {
    let dlc = DLC_APP_IDS
        .iter()
        .find(|(id, _)| *id == entry.id.as_mod_id().base().as_str())
        .map(|&(_, app_id)| app_id);
    dlc.or(entry.workshop_id.map(|id| id.get())).unwrap_or(0)
}

/// Appends one tab-indented line.
fn push_line(out: &mut String, depth: usize, content: &str) {
    for _ in 0..depth {
        out.push('\t');
    }
    out.push_str(content);
    out.push_str(EOL);
}

/// Appends `<tag>` with one `<li>` per value, then `</tag>`, at `depth`.
fn push_list<'a>(out: &mut String, depth: usize, tag: &str, values: impl Iterator<Item = &'a str>) {
    push_line(out, depth, &format!("<{tag}>"));
    for value in values {
        push_line(
            out,
            depth + 1,
            &format!("<li>{}</li>", xml_escape_text(value)),
        );
    }
    push_line(out, depth, &format!("</{tag}>"));
}

/// The name written for `entry`: its own, else its id (every list keeps
/// one name per id, so positions stay aligned).
fn name_of(entry: &SharedModEntry) -> &str {
    entry
        .name
        .as_ref()
        .map_or_else(|| entry.id.as_str(), |name| name.as_str())
}

/// Renders `list` as the bytes of a `.rml` file.
pub(super) fn render_rml(list: &SharedModList) -> Vec<u8> {
    let entries = list.entries();
    let ids = || entries.iter().map(|entry| entry.id.as_str());
    let names = || entries.iter().map(name_of);
    let steam_ids: Vec<String> = entries
        .iter()
        .map(|entry| steam_id_for(entry).to_string())
        .collect();

    let mut out = String::new();
    push_line(&mut out, 0, DECLARATION);
    push_line(&mut out, 0, "<savedModList>");
    push_line(&mut out, 1, "<meta>");
    if let Some(version) = list.game_version() {
        let line = format!(
            "<gameVersion>{}</gameVersion>",
            xml_escape_text(version.as_str())
        );
        push_line(&mut out, 2, &line);
    }
    push_list(&mut out, 2, "modIds", ids());
    push_list(
        &mut out,
        2,
        "modSteamIds",
        steam_ids.iter().map(String::as_str),
    );
    push_list(&mut out, 2, "modNames", names());
    push_line(&mut out, 1, "</meta>");
    push_line(&mut out, 1, "<modList>");
    push_list(&mut out, 2, "ids", ids());
    push_list(&mut out, 2, "names", names());
    push_line(&mut out, 1, "</modList>");
    out.push_str("</savedModList>");

    let mut bytes = BOM.to_vec();
    bytes.extend_from_slice(out.as_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use rim_session::mod_list::{ListedGameVersion, ListedName, ListedPackageId, WorkshopId};

    use super::*;

    fn entry(id: &str, name: Option<&str>, workshop_id: Option<u64>) -> SharedModEntry {
        SharedModEntry {
            id: ListedPackageId::try_from(id).expect("valid id"),
            name: name.and_then(ListedName::new),
            workshop_id: workshop_id.and_then(WorkshopId::new),
        }
    }

    fn list(version: Option<&str>, entries: Vec<SharedModEntry>) -> SharedModList {
        SharedModList::new(version.and_then(ListedGameVersion::new), entries).expect("list")
    }

    #[test]
    fn output_is_byte_for_byte_the_shape_the_game_saves() {
        let list = list(
            Some("1.6.4871 rev590"),
            vec![
                entry("ludeon.rimworld", Some("Core"), None),
                entry("ludeon.rimworld.royalty", Some("Royalty"), None),
                entry(
                    "example.framework",
                    Some("Example & Framework"),
                    Some(1_234_567_890),
                ),
                entry("someone.localmod", None, None),
            ],
        );
        let expected = [
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>",
            "<savedModList>",
            "\t<meta>",
            "\t\t<gameVersion>1.6.4871 rev590</gameVersion>",
            "\t\t<modIds>",
            "\t\t\t<li>ludeon.rimworld</li>",
            "\t\t\t<li>ludeon.rimworld.royalty</li>",
            "\t\t\t<li>example.framework</li>",
            "\t\t\t<li>someone.localmod</li>",
            "\t\t</modIds>",
            "\t\t<modSteamIds>",
            "\t\t\t<li>0</li>",
            "\t\t\t<li>1149640</li>",
            "\t\t\t<li>1234567890</li>",
            "\t\t\t<li>0</li>",
            "\t\t</modSteamIds>",
            "\t\t<modNames>",
            "\t\t\t<li>Core</li>",
            "\t\t\t<li>Royalty</li>",
            "\t\t\t<li>Example &amp; Framework</li>",
            "\t\t\t<li>someone.localmod</li>",
            "\t\t</modNames>",
            "\t</meta>",
            "\t<modList>",
            "\t\t<ids>",
            "\t\t\t<li>ludeon.rimworld</li>",
            "\t\t\t<li>ludeon.rimworld.royalty</li>",
            "\t\t\t<li>example.framework</li>",
            "\t\t\t<li>someone.localmod</li>",
            "\t\t</ids>",
            "\t\t<names>",
            "\t\t\t<li>Core</li>",
            "\t\t\t<li>Royalty</li>",
            "\t\t\t<li>Example &amp; Framework</li>",
            "\t\t\t<li>someone.localmod</li>",
            "\t\t</names>",
            "\t</modList>",
            "</savedModList>",
        ]
        .join("\r\n");
        let mut expected_bytes = b"\xef\xbb\xbf".to_vec();
        expected_bytes.extend_from_slice(expected.as_bytes());

        assert_eq!(render_rml(&list), expected_bytes);
    }

    #[test]
    fn a_dlc_row_carries_its_app_id_even_when_a_workshop_id_is_present() {
        let dlc = entry("ludeon.rimworld.ideology", None, Some(42));

        assert_eq!(steam_id_for(&dlc), 1_392_840);
    }

    #[test]
    fn an_unlisted_official_row_and_a_local_mod_write_zero() {
        assert_eq!(
            steam_id_for(&entry("ludeon.rimworld.unknown", None, None)),
            0
        );
        assert_eq!(steam_id_for(&entry("ludeon.rimworld", None, None)), 0);
        assert_eq!(steam_id_for(&entry("someone.localmod", None, None)), 0);
    }

    #[test]
    fn an_unknown_game_version_omits_the_element() {
        let bytes = render_rml(&list(None, vec![entry("example.framework", None, None)]));
        let text = String::from_utf8(bytes).expect("utf-8");

        assert!(!text.contains("gameVersion"));
    }
}
