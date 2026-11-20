//! Parsing a mod-list document: the content-based format detection, the
//! XML safety rules, and the `.rml` / `ModsConfig.xml` readers.

use std::borrow::Cow;
use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_analyzer::extract::{MAX_RAW_ELEMENT_DEPTH, raw_element_nesting_exceeds};
use rim_session::mod_list::{
    ListAlignment, ListFormat, ListedGameVersion, ListedName, ListedPackageId, ModListLimits,
    ParsedModList, Rejection, SharedModEntry, SharedModList, SharedModListError, SkippedEntry,
    TruncatedText, WorkshopId, parse_text,
};
use roxmltree::{Document, Error as XmlError, Node, ParsingOptions};

const RML_ROOT: &str = "savedModList";
const MODS_CONFIG_ROOT: &str = "ModsConfigData";
const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const XML_STARTS: [&str; 5] = [
    "<?xml",
    "<!",
    "<savedModList",
    "<ModsConfigData",
    "<savegame",
];

/// Parses `bytes` as a mod list in whichever format its content shows.
pub(super) fn parse_bytes(bytes: &[u8]) -> Result<ParsedModList, Rejection> {
    if bytes.len() > ModListLimits::MAX_INPUT_BYTES {
        return Err(Rejection::TooLarge {
            limit_bytes: ModListLimits::MAX_INPUT_BYTES,
        });
    }
    let text = decode(bytes);
    let trimmed = text.trim_start();
    if is_xml(trimmed) {
        parse_xml(trimmed)
    } else {
        parse_text(&text)
    }
}

/// Lossy UTF-8 without a BOM. An invalid byte becomes a 3-byte U+FFFD, so
/// the text can outgrow input that was within the byte limit and make
/// `parse_text` report `TooLarge` for it; past the limit every U+FFFD,
/// decoded or original, becomes a 1-byte `?` instead, which cannot grow
/// the text (the byte bound is the reader's job, checked above).
pub(super) fn decode(bytes: &[u8]) -> Cow<'_, str> {
    let text = String::from_utf8_lossy(bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes));
    if text.len() > ModListLimits::MAX_INPUT_BYTES {
        Cow::Owned(text.replace(char::REPLACEMENT_CHARACTER, "?"))
    } else {
        text
    }
}

/// Whether trimmed `text` starts like one of the XML documents this reads
/// (a declaration, a `<!` such as a DOCTYPE or comment, or a known root).
/// Anything else, including a hand-typed list that begins with `<`, is
/// text.
fn is_xml(text: &str) -> bool {
    XML_STARTS.iter().any(|start| {
        text.get(..start.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(start))
    })
}

fn parse_xml(text: &str) -> Result<ParsedModList, Rejection> {
    let document = parse_document(text)?;
    let root = document.root_element();
    let root_name = root.tag_name().name();
    if root_name.eq_ignore_ascii_case(RML_ROOT) {
        read_rml(root)
    } else if root_name.eq_ignore_ascii_case(MODS_CONFIG_ROOT) {
        read_mods_config(root)
    } else {
        Err(Rejection::UnrecognizedFormat)
    }
}

/// The one XML safety sequence every shape shares: no DOCTYPE, bounded
/// nesting, bounded node count, no entity resolver.
pub(super) fn parse_document(text: &str) -> Result<Document<'_>, Rejection> {
    // roxmltree already refuses a non-empty DTD; this also refuses an
    // empty one, which it allows, so no DOCTYPE is ever accepted.
    if text.contains("<!DOCTYPE") {
        return Err(Rejection::DtdNotAllowed);
    }
    if raw_element_nesting_exceeds(text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(Rejection::TooDeep);
    }
    let options = ParsingOptions {
        allow_dtd: false,
        nodes_limit: u32::try_from(ModListLimits::MAX_XML_NODES).unwrap_or(u32::MAX),
        entity_resolver: None,
    };
    Document::parse_with_options(text, options).map_err(rejection_for)
}

fn rejection_for(error: XmlError) -> Rejection {
    match error {
        XmlError::DtdDetected => Rejection::DtdNotAllowed,
        // The node cap is the only thing that bounds a document of many
        // elements outside any list, so it reads as "too many entries".
        XmlError::NodesLimitReached => Rejection::TooManyEntries {
            limit: ModListLimits::MAX_ENTRIES,
        },
        _ => Rejection::MalformedXml,
    }
}

/// The first direct child element of `node` named `tag`, ignoring case.
/// (`rim_analyzer`'s own helper for this is crate-private.)
fn direct_child<'a, 'input>(node: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name().eq_ignore_ascii_case(tag))
}

/// The direct `<li>` children of `container`, in order.
fn list_items<'a, 'input>(container: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    container
        .children()
        .filter(|child| child.is_element() && child.tag_name().name().eq_ignore_ascii_case("li"))
}

/// The text of every `<li>` of `container`, one per item and borrowed
/// from the document. An item that is not plain text (it has an element
/// child) reads as empty, which no id or name accepts. The count is
/// checked first, so an oversized list never allocates.
fn list_texts<'a>(container: Node<'a, '_>) -> Result<Vec<&'a str>, Rejection> {
    if list_items(container).count() > ModListLimits::MAX_ENTRIES {
        return Err(Rejection::TooManyEntries {
            limit: ModListLimits::MAX_ENTRIES,
        });
    }
    Ok(list_items(container)
        .map(|item| {
            if item.children().any(|child| child.is_element()) {
                ""
            } else {
                item.text().unwrap_or("")
            }
        })
        .collect())
}

pub(super) fn child_texts<'a>(
    parent: Node<'a, '_>,
    tag: &str,
) -> Result<Option<Vec<&'a str>>, Rejection> {
    direct_child(parent, tag).map(list_texts).transpose()
}

pub(super) fn game_version_of(parent: Node, tag: &str) -> Option<ListedGameVersion> {
    direct_child(parent, tag)
        .and_then(|node| node.text())
        .and_then(ListedGameVersion::new)
}

fn read_rml(root: Node) -> Result<ParsedModList, Rejection> {
    let modlist = direct_child(root, "modList");
    let ids = modlist
        .map(|node| child_texts(node, "ids"))
        .transpose()?
        .flatten()
        .ok_or(Rejection::MissingModList)?;
    let names = modlist
        .map(|node| child_texts(node, "names"))
        .transpose()?
        .flatten();
    let meta = direct_child(root, "meta");
    let (workshop_ids, workshop_alignment) = match meta {
        Some(meta) => meta_workshop_ids(meta)?,
        None => (BTreeMap::new(), ListAlignment::Absent),
    };
    ListPieces {
        format: ListFormat::SavedModList,
        game_version: meta.and_then(|meta| game_version_of(meta, "gameVersion")),
        ids,
        names,
        workshop_ids,
        workshop_alignment,
    }
    .into_parsed()
}

fn read_mods_config(root: Node) -> Result<ParsedModList, Rejection> {
    let ids = child_texts(root, "activeMods")?.ok_or(Rejection::MissingModList)?;
    ListPieces {
        format: ListFormat::ModsConfig,
        game_version: game_version_of(root, "version"),
        ids,
        names: None,
        workshop_ids: BTreeMap::new(),
        workshop_alignment: ListAlignment::Absent,
    }
    .into_parsed()
}

/// The `meta` block's package id to Workshop id pairs. The two lists are
/// zipped by position only when their lengths agree; otherwise no Workshop
/// id is taken and the mismatch is reported. A `0` (no Workshop copy) and
/// anything non-numeric give no id, and the first pair for an id wins.
fn meta_workshop_ids(
    meta: Node,
) -> Result<(BTreeMap<ModId, WorkshopId>, ListAlignment), Rejection> {
    let package_ids = child_texts(meta, "modIds")?;
    let steam_ids = child_texts(meta, "modSteamIds")?;
    Ok(match (package_ids, steam_ids) {
        (Some(package_ids), Some(steam_ids)) => workshop_pairs(&package_ids, &steam_ids),
        _ => (BTreeMap::new(), ListAlignment::Absent),
    })
}

/// Pairs each package id with the Steam id at its position, when the two
/// lists have one value per id; a list of another length is dropped whole.
pub(super) fn workshop_pairs(
    package_ids: &[&str],
    steam_ids: &[&str],
) -> (BTreeMap<ModId, WorkshopId>, ListAlignment) {
    let alignment = ListAlignment::between(package_ids.len(), steam_ids.len());
    let mut pairs = BTreeMap::new();
    if alignment == ListAlignment::Aligned {
        for (package_id, steam_id) in package_ids.iter().zip(steam_ids) {
            if let Ok(workshop_id) = WorkshopId::try_from(steam_id.trim()) {
                pairs
                    .entry(ModId::new(package_id.trim()))
                    .or_insert(workshop_id);
            }
        }
    }
    (pairs, alignment)
}

/// The raw parts of one document, ready to become a [`ParsedModList`].
pub(super) struct ListPieces<'a> {
    pub(super) format: ListFormat,
    pub(super) game_version: Option<ListedGameVersion>,
    pub(super) ids: Vec<&'a str>,
    /// Kept only when it has one name per id.
    pub(super) names: Option<Vec<&'a str>>,
    pub(super) workshop_ids: BTreeMap<ModId, WorkshopId>,
    /// How the Steam-id list the pairs came from lined up with its ids.
    pub(super) workshop_alignment: ListAlignment,
}

impl ListPieces<'_> {
    pub(super) fn into_parsed(self) -> Result<ParsedModList, Rejection> {
        let names_alignment = self.names.as_ref().map_or(ListAlignment::Absent, |names| {
            ListAlignment::between(self.ids.len(), names.len())
        });
        let names = self
            .names
            .as_ref()
            .filter(|_| names_alignment == ListAlignment::Aligned);
        let mut entries = Vec::with_capacity(self.ids.len());
        let mut skipped = Vec::new();
        let mut omitted_skipped = 0;
        for (index, raw_id) in self.ids.iter().enumerate() {
            let Ok(id) = ListedPackageId::try_from(*raw_id) else {
                if skipped.len() < ModListLimits::MAX_SKIPPED_REPORTED {
                    skipped.push(SkippedEntry::MalformedId {
                        position: u32::try_from(index + 1).unwrap_or(u32::MAX),
                        text: TruncatedText::new(raw_id),
                    });
                } else {
                    omitted_skipped += 1;
                }
                continue;
            };
            // The app id a DLC row carries is not a Workshop id.
            let workshop_id = if id.is_official_content() {
                None
            } else {
                self.workshop_ids.get(id.as_mod_id()).copied()
            };
            let name = names
                .and_then(|names| names.get(index))
                .and_then(|name| ListedName::new(name));
            entries.push(SharedModEntry {
                id,
                name,
                workshop_id,
            });
        }
        let list = SharedModList::new(self.game_version, entries).map_err(|error| match error {
            SharedModListError::Empty => Rejection::NoEntries,
            SharedModListError::TooManyEntries { limit } => Rejection::TooManyEntries { limit },
        })?;
        Ok(ParsedModList {
            format: self.format,
            list,
            names: names_alignment,
            workshop_ids: self.workshop_alignment,
            skipped,
            omitted_skipped,
        })
    }
}
