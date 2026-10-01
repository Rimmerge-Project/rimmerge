//! Parses a mod's `About/About.xml` into merged, version-resolved data.
//!
//! `ByVersion` elements (e.g. `loadAfterByVersion/v1.6/li`) are merged with
//! their base counterpart here so the rest of the system never has to
//! think about version-specific overrides again.

use roxmltree::Node;
use thiserror::Error;

use crate::domain::{DeclaredOrder, GameVersion, ModDependency, ModId};

use super::xml_util::{
    MAX_RAW_ELEMENT_DEPTH, child_text, decode_lossy, direct_child, raw_element_nesting_exceeds,
    split_csv, strings_from_container,
};

/// The merged, version-resolved contents of one `About.xml`.
#[derive(Debug, Clone)]
pub struct AboutXmlData {
    pub id: ModId,
    pub name: String,
    pub authors: Vec<String>,
    pub url: Option<String>,
    pub supported_versions: Vec<String>,
    pub declared: DeclaredOrder,
}

#[derive(Debug, Error)]
pub enum AboutXmlError {
    #[error("failed to parse XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("root element is not <ModMetaData>")]
    WrongRoot,
    #[error("missing required <packageId> element")]
    MissingPackageId,
    #[error("raw element nesting exceeds {0} levels; refusing to parse")]
    TooDeep(usize),
}

/// Parses `bytes` as an `About.xml` document, merging `ByVersion` variants
/// for `game_version`. Never panics on malformed input: XML errors, a
/// missing root, or a missing `packageId` all surface as
/// [`AboutXmlError`] for the caller to turn into a skip-and-warn.
pub fn parse(bytes: &[u8], game_version: GameVersion) -> Result<AboutXmlData, AboutXmlError> {
    let text = decode_lossy(bytes);
    if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(AboutXmlError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("ModMetaData") {
        return Err(AboutXmlError::WrongRoot);
    }

    let package_id = child_text(root, "packageId").ok_or(AboutXmlError::MissingPackageId)?;
    let name = child_text(root, "name").unwrap_or_default();
    let authors = extract_authors(root);
    let url = child_text(root, "url");
    let supported_versions = direct_child(root, "supportedVersions")
        .map(strings_from_container)
        .unwrap_or_default();

    let declared = DeclaredOrder {
        load_after: merged_id_list(root, "loadAfter", game_version),
        load_before: merged_id_list(root, "loadBefore", game_version),
        force_load_after: merged_id_list(root, "forceLoadAfter", game_version),
        force_load_before: merged_id_list(root, "forceLoadBefore", game_version),
        dependencies: merged_dependencies(root, game_version),
        incompatible_with: merged_id_list(root, "incompatibleWith", game_version),
    };

    Ok(AboutXmlData {
        id: ModId::new(package_id),
        name,
        authors,
        url,
        supported_versions,
        declared,
    })
}

fn dependencies_from_container(container: Node) -> Vec<ModDependency> {
    container
        .children()
        .filter(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case("li"))
        .filter_map(|li| {
            let id = ModId::new(child_text(li, "packageId")?);
            let display_name = child_text(li, "displayName");
            Some(ModDependency { id, display_name })
        })
        .collect()
}

/// The base `<tag><li>..</li></tag>` list, or — per the RimWorld wiki,
/// "overrides for that version" — the
/// `<tagByVersion><v{version}><li>..</li></v{version}></tagByVersion>`
/// list in its entirety when one exists for `version`. `ByVersion` entries
/// replace the base list rather than merging with it; a mod that wants
/// both must repeat the base entries under its `ByVersion` block.
fn merged_id_list(node: Node, tag: &str, version: GameVersion) -> Vec<ModId> {
    let by_version_tag = format!("{tag}ByVersion");
    let versioned = direct_child(node, &by_version_tag)
        .and_then(|wrapper| direct_child(wrapper, &version.tag_name()))
        .map(strings_from_container);

    let raw = versioned.unwrap_or_else(|| {
        direct_child(node, tag)
            .map(strings_from_container)
            .unwrap_or_default()
    });
    raw.into_iter().map(ModId::new).collect()
}

/// Same override-not-merge rule as [`merged_id_list`], for
/// `modDependencies`/`modDependenciesByVersion`.
fn merged_dependencies(node: Node, version: GameVersion) -> Vec<ModDependency> {
    let versioned = direct_child(node, "modDependenciesByVersion")
        .and_then(|wrapper| direct_child(wrapper, &version.tag_name()))
        .map(dependencies_from_container);

    versioned.unwrap_or_else(|| {
        direct_child(node, "modDependencies")
            .map(dependencies_from_container)
            .unwrap_or_default()
    })
}

/// `<authors><li>..</li></authors>`, falling back to `<authors>Name</authors>`
/// (no `<li>` wrapper) and then to the singular `<author>Name</author>`.
/// RimWorld treats a comma-separated author string (either form) as
/// multiple authors, so every source is split on `,` here.
fn extract_authors(node: Node) -> Vec<String> {
    if let Some(authors_node) = direct_child(node, "authors") {
        let via_li = strings_from_container(authors_node);
        if !via_li.is_empty() {
            return via_li.iter().flat_map(|a| split_csv(a)).collect();
        }
        if let Some(text) = authors_node.text().map(str::trim).filter(|s| !s.is_empty()) {
            return split_csv(text);
        }
    }
    child_text(node, "author")
        .map(|a| split_csv(&a))
        .unwrap_or_default()
}

/// About.xml details read lazily, per mod selection, for the mod info
/// panel — never during a scan and never stored on [`crate::domain::Report`]
/// (see [`parse_details`]'s own doc comment for why).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AboutDetails {
    /// The mod's own packageId, read straight from this same document —
    /// callers compare it against a scan-time id to detect a folder that
    /// changed since the scan (`rim-session`'s `ReadModAbout`'s `Changed`
    /// outcome).
    pub package_id: ModId,
    /// `<description>`, resolved through the same `descriptionsByVersion`
    /// override rule [`parse`]'s declared-order lists use (the base value
    /// replaced in full when a block for `game_version` exists), with the
    /// engine's own literal-`\n`-to-newline decoding applied
    /// (`ParseHelper.ParseString`, ground-truthed from the decompiled
    /// engine: `str.Replace("\\n", "\n")`). `None` when neither the base
    /// nor a matching versioned block is present.
    pub description: Option<String>,
    /// `<modVersion>`, verbatim.
    pub mod_version: Option<String>,
    /// `<modIconPath>`, verbatim — a `ContentFinder`-resolved texture key,
    /// not a file path; see the engine order in the mod info panel's own
    /// design notes for how this, `About/ModIcon.png`, and the default
    /// icon are tried in turn.
    pub mod_icon_path: Option<String>,
    /// `<url>`, verbatim. Re-read here (rather than trusted from the
    /// scan-time [`AboutXmlData::url`]) so an inactive mod — which
    /// carries no `url` field of its own, see
    /// [`crate::domain::InactiveMod`]'s own doc comment — can still show
    /// and open its homepage.
    pub url: Option<String>,
}

/// Parses just the fields `About.xml` carries that the scan path never
/// needs: the description, mod version, and icon path. Kept separate from
/// [`parse`]/[`AboutXmlData`] — re-reading `About.xml` per mod selection
/// (`rim-session`'s `ReadModAbout` use case) instead of caching these
/// fields on every scanned [`crate::domain::Report`] avoids growing every
/// `analyze --json` by the full text of every mod's description for a
/// field only the mod info panel reads.
///
/// # Errors
///
/// Same failure modes as [`parse`]: [`AboutXmlError::Xml`] for malformed
/// XML, [`AboutXmlError::WrongRoot`] when the root isn't `<ModMetaData>`,
/// and [`AboutXmlError::MissingPackageId`] when `<packageId>` is absent.
pub fn parse_details(
    bytes: &[u8],
    game_version: GameVersion,
) -> Result<AboutDetails, AboutXmlError> {
    let text = decode_lossy(bytes);
    if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(AboutXmlError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("ModMetaData") {
        return Err(AboutXmlError::WrongRoot);
    }

    let package_id = child_text(root, "packageId").ok_or(AboutXmlError::MissingPackageId)?;
    let description = versioned_description(root, game_version)
        .or_else(|| child_text(root, "description"))
        .map(|raw| decode_engine_string(&raw));
    let mod_version = child_text(root, "modVersion");
    let mod_icon_path = child_text(root, "modIconPath");
    let url = child_text(root, "url");

    Ok(AboutDetails {
        package_id: ModId::new(package_id),
        description,
        mod_version,
        mod_icon_path,
        url,
    })
}

/// The `<descriptionsByVersion><v{version}>text</v{version}></descriptionsByVersion>`
/// override for `game_version`, when present — replaces `<description>` in
/// full, the same override-not-merge rule [`merged_id_list`] applies to
/// the declared-order lists. Named `descriptionsByVersion` (plural),
/// unlike every `{tag}ByVersion` pair [`merged_id_list`]/
/// [`merged_dependencies`] handle — the engine's own field name, not a
/// pattern this crate chose.
fn versioned_description(node: Node, version: GameVersion) -> Option<String> {
    direct_child(node, "descriptionsByVersion")
        .and_then(|wrapper| direct_child(wrapper, &version.tag_name()))
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// `ParseHelper.ParseString`'s own decoding, ground-truthed from the
/// decompiled engine: a literal two-character `\n` (backslash, `n`) an
/// author typed becomes a real newline. Applied only to the description —
/// [`parse`]'s other string fields (`name`, `url`, ...) never carry this
/// escape in practice and reading it back verbatim there matches the
/// engine's own untouched fields.
fn decode_engine_string(raw: &str) -> String {
    raw.replace("\\n", "\n")
}

/// Fixed fallback for Core/DLC display names when
/// `Defs/Misc/ExpansionDefs/ExpansionDefs.xml` doesn't resolve one (e.g.
/// the file couldn't be read, or a future DLC's `<linkedMod>` isn't
/// recognized yet).
#[must_use]
pub fn known_dlc_display_name(id: &ModId) -> Option<&'static str> {
    match id.as_str() {
        "ludeon.rimworld" => Some("Core"),
        "ludeon.rimworld.royalty" => Some("Royalty"),
        "ludeon.rimworld.ideology" => Some("Ideology"),
        "ludeon.rimworld.biotech" => Some("Biotech"),
        "ludeon.rimworld.anomaly" => Some("Anomaly"),
        "ludeon.rimworld.odyssey" => Some("Odyssey"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v16() -> GameVersion {
        GameVersion::new(1, 6)
    }

    #[test]
    fn parses_minimal_about_xml() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
            <ModMetaData>
              <packageId>Author.ModName</packageId>
              <name>Mod Name</name>
              <author>Author</author>
              <url>https://example.com</url>
              <supportedVersions><li>1.6</li></supportedVersions>
            </ModMetaData>"#;
        let data = parse(xml, v16()).unwrap();
        assert_eq!(data.id, ModId::new("author.modname"));
        assert_eq!(data.name, "Mod Name");
        assert_eq!(data.authors, vec!["Author".to_string()]);
        assert_eq!(data.url.as_deref(), Some("https://example.com"));
        assert_eq!(data.supported_versions, vec!["1.6".to_string()]);
    }

    #[test]
    fn parses_authors_li_list() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <authors><li>Alice</li><li>Bob</li></authors>
            </ModMetaData>"#;
        let data = parse(xml, v16()).unwrap();
        assert_eq!(data.authors, vec!["Alice".to_string(), "Bob".to_string()]);
    }

    #[test]
    fn splits_comma_separated_author_into_multiple_authors() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <author>Alice, Bob</author>
            </ModMetaData>"#;
        let data = parse(xml, v16()).unwrap();
        assert_eq!(data.authors, vec!["Alice".to_string(), "Bob".to_string()]);
    }

    #[test]
    fn known_dlc_display_name_covers_core_and_every_dlc() {
        assert_eq!(
            known_dlc_display_name(&ModId::new("Ludeon.RimWorld")),
            Some("Core")
        );
        assert_eq!(
            known_dlc_display_name(&ModId::new("Ludeon.RimWorld.Odyssey")),
            Some("Odyssey")
        );
        assert_eq!(known_dlc_display_name(&ModId::new("some.other.mod")), None);
    }

    #[test]
    fn by_version_list_overrides_the_base_list_for_the_matching_version() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <loadAfter><li>base.mod</li></loadAfter>
              <loadAfterByVersion>
                <v1.6><li>versioned.mod</li></v1.6>
                <v1.5><li>only.for.1.5</li></v1.5>
              </loadAfterByVersion>
            </ModMetaData>"#;
        let data = parse(xml, v16()).unwrap();
        assert_eq!(data.declared.load_after, vec![ModId::new("versioned.mod")]);
    }

    #[test]
    fn base_list_is_used_when_no_by_version_entry_matches() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <loadAfter><li>base.mod</li></loadAfter>
              <loadAfterByVersion>
                <v1.5><li>only.for.1.5</li></v1.5>
              </loadAfterByVersion>
            </ModMetaData>"#;
        let data = parse(xml, v16()).unwrap();
        assert_eq!(data.declared.load_after, vec![ModId::new("base.mod")]);
    }

    #[test]
    fn mod_dependencies_by_version_overrides_the_base_list() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <modDependencies>
                <li>
                  <packageId>example.patchlib</packageId>
                  <displayName>Example PatchLib</displayName>
                </li>
              </modDependencies>
              <modDependenciesByVersion>
                <v1.6><li><packageId>extra.dep</packageId></li></v1.6>
              </modDependenciesByVersion>
            </ModMetaData>"#;
        let data = parse(xml, v16()).unwrap();
        assert_eq!(data.declared.dependencies.len(), 1);
        assert_eq!(data.declared.dependencies[0].id, ModId::new("extra.dep"));
        assert_eq!(data.declared.dependencies[0].display_name, None);
    }

    #[test]
    fn missing_package_id_is_an_error() {
        let xml = br#"<ModMetaData><name>No Id</name></ModMetaData>"#;
        assert!(matches!(
            parse(xml, v16()),
            Err(AboutXmlError::MissingPackageId)
        ));
    }

    #[test]
    fn wrong_root_element_is_an_error() {
        let xml = br#"<NotModMetaData/>"#;
        assert!(matches!(parse(xml, v16()), Err(AboutXmlError::WrongRoot)));
    }

    #[test]
    fn malformed_xml_is_an_error_not_a_panic() {
        let xml = br#"<ModMetaData><packageId>a.b</packageId"#;
        assert!(parse(xml, v16()).is_err());
    }

    #[test]
    fn strips_bom_before_parsing() {
        let mut xml = vec![0xEF, 0xBB, 0xBF];
        xml.extend_from_slice(b"<ModMetaData><packageId>a.b</packageId></ModMetaData>");
        assert!(parse(&xml, v16()).is_ok());
    }

    #[test]
    fn parse_details_reads_the_base_description() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <description>A short description.</description>
            </ModMetaData>"#;
        let details = parse_details(xml, v16()).unwrap();
        assert_eq!(details.package_id, ModId::new("a.b"));
        assert_eq!(details.description.as_deref(), Some("A short description."));
    }

    #[test]
    fn parse_details_descriptions_by_version_replaces_the_base_for_the_matching_version() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <description>base</description>
              <descriptionsByVersion>
                <v1.6>versioned for 1.6</v1.6>
                <v1.5>versioned for 1.5</v1.5>
              </descriptionsByVersion>
            </ModMetaData>"#;
        let details = parse_details(xml, v16()).unwrap();
        assert_eq!(details.description.as_deref(), Some("versioned for 1.6"));
    }

    #[test]
    fn parse_details_leaves_the_base_description_when_no_by_version_entry_matches() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <description>base</description>
              <descriptionsByVersion>
                <v1.5>versioned for 1.5</v1.5>
              </descriptionsByVersion>
            </ModMetaData>"#;
        let details = parse_details(xml, v16()).unwrap();
        assert_eq!(details.description.as_deref(), Some("base"));
    }

    #[test]
    fn parse_details_decodes_literal_backslash_n_as_a_newline() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <description>line one\nline two</description>
            </ModMetaData>"#;
        let details = parse_details(xml, v16()).unwrap();
        assert_eq!(details.description.as_deref(), Some("line one\nline two"));
    }

    #[test]
    fn parse_details_absent_description_is_none() {
        let xml = br#"<ModMetaData><packageId>a.b</packageId></ModMetaData>"#;
        let details = parse_details(xml, v16()).unwrap();
        assert_eq!(details.description, None);
    }

    #[test]
    fn parse_details_reads_mod_version_and_mod_icon_path() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <modVersion>1.2.3</modVersion>
              <modIconPath>UI/Icons/MyIcon</modIconPath>
            </ModMetaData>"#;
        let details = parse_details(xml, v16()).unwrap();
        assert_eq!(details.mod_version.as_deref(), Some("1.2.3"));
        assert_eq!(details.mod_icon_path.as_deref(), Some("UI/Icons/MyIcon"));
    }

    #[test]
    fn parse_details_reads_url() {
        let xml = br#"<ModMetaData>
              <packageId>a.b</packageId>
              <url>https://example.com</url>
            </ModMetaData>"#;
        let details = parse_details(xml, v16()).unwrap();
        assert_eq!(details.url.as_deref(), Some("https://example.com"));
    }

    #[test]
    fn parse_details_absent_url_is_none() {
        let xml = br#"<ModMetaData><packageId>a.b</packageId></ModMetaData>"#;
        let details = parse_details(xml, v16()).unwrap();
        assert_eq!(details.url, None);
    }

    #[test]
    fn parse_details_missing_package_id_is_an_error() {
        let xml = br#"<ModMetaData><description>x</description></ModMetaData>"#;
        assert!(matches!(
            parse_details(xml, v16()),
            Err(AboutXmlError::MissingPackageId)
        ));
    }

    #[test]
    fn parse_details_malformed_xml_is_an_error_not_a_panic() {
        let xml = br#"<ModMetaData><packageId>a.b</packageId"#;
        assert!(parse_details(xml, v16()).is_err());
    }
}
