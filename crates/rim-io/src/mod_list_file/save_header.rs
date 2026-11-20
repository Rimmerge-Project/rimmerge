//! Reading the `<meta>` header of a RimWorld save.
//!
//! The caller (`stream.rs`) hands over the bytes of the save up to and
//! including its first `</meta>`; the rest of the file is never read. The
//! prefix is closed with a synthetic `</savegame>` and goes through the
//! same XML safety sequence as every other shape, then through the shared
//! [`ListPieces`] tail, so the id grammar, the skipped-entry report and the
//! official-content rule are written once.

use std::collections::BTreeMap;

use rim_session::mod_list::{ListAlignment, ListFormat, ParsedModList, Rejection};

use super::read::{
    ListPieces, child_texts, decode, game_version_of, parse_document, workshop_pairs,
};

const SAVE_ROOT: &str = "savegame";
const SYNTHETIC_END: &str = "</savegame>";

/// Parses `prefix`, a save up to and including its header's `</meta>`.
/// `root_at` is where the sniff found the root element's `<`; anything
/// before it (a declaration, comments) is not part of the document read.
pub(super) fn parse_save_header(prefix: &[u8], root_at: usize) -> Result<ParsedModList, Rejection> {
    let from_root = prefix.get(root_at..).ok_or(Rejection::UnrecognizedFormat)?;
    let decoded = decode(from_root);
    // The root is matched without regard to case, like every other shape,
    // so the closing tag copies the casing the file used. The sniff has
    // checked those bytes are the root name, so they are ASCII.
    let root_name = decoded
        .get(1..=SAVE_ROOT.len())
        .filter(|name| name.eq_ignore_ascii_case(SAVE_ROOT))
        .ok_or(Rejection::UnrecognizedFormat)?;
    let mut text = String::with_capacity(decoded.len() + SYNTHETIC_END.len());
    text.push_str(&decoded);
    text.push_str("</");
    text.push_str(root_name);
    text.push('>');
    let document = parse_document(&text)?;
    let root = document.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case(SAVE_ROOT) {
        return Err(Rejection::UnrecognizedFormat);
    }
    // The game itself expects `meta` as the first element after the root.
    let meta = root
        .children()
        .find(|child| child.is_element())
        .filter(|child| child.tag_name().name() == "meta")
        .ok_or(Rejection::SaveHeaderMissing)?;
    let ids = child_texts(meta, "modIds")?.ok_or(Rejection::MissingModList)?;
    let steam_ids = child_texts(meta, "modSteamIds")?;
    let names = child_texts(meta, "modNames")?;
    let (workshop_ids, workshop_alignment) = match steam_ids {
        Some(steam_ids) => workshop_pairs(&ids, &steam_ids),
        None => (BTreeMap::new(), ListAlignment::Absent),
    };
    ListPieces {
        format: ListFormat::SaveHeader,
        game_version: game_version_of(meta, "gameVersion"),
        ids,
        names,
        workshop_ids,
        workshop_alignment,
    }
    .into_parsed()
}
