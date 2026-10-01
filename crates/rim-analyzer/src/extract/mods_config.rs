//! Parses `ModsConfig.xml`'s active mod list.

use thiserror::Error;

use crate::domain::ModId;

use super::xml_util::{
    MAX_RAW_ELEMENT_DEPTH, child_text, decode_lossy, direct_child, raw_element_nesting_exceeds,
    strings_from_container,
};

#[derive(Debug, Error)]
pub enum ModsConfigError {
    #[error("failed to parse XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("root element is not <ModsConfigData>")]
    WrongRoot,
    #[error("missing required <activeMods> element")]
    MissingActiveMods,
    #[error("raw element nesting exceeds {0} levels; refusing to parse")]
    TooDeep(usize),
}

/// The full, structured contents of one `ModsConfig.xml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModsConfigDocument {
    /// The `<version>` element's text (e.g. `"1.6.4871 rev590"`), verbatim
    /// — never parsed into a [`crate::domain::GameVersion`]: this is the
    /// version RimWorld last *saved* the config under, not necessarily the
    /// one a scan is run against (the caller resolves that separately; see
    /// `rim_analyzer::infra::paths::default_game_version`). Empty when the
    /// element is missing, which older or hand-edited configs sometimes
    /// omit — tolerated rather than treated as an error, unlike a missing
    /// `<activeMods>`.
    pub version: String,
    /// The ordered, active mod list — `<activeMods><li>packageId</li>...</activeMods>`.
    pub active_mods: Vec<ModId>,
    /// The DLC packageIds RimWorld's own launcher has enabled —
    /// `<knownExpansions><li>packageId</li>...</knownExpansions>`. Empty
    /// (not an error) when the element is missing.
    pub known_expansions: Vec<ModId>,
}

/// Parses the full `<ModsConfigData>` document: the game version it was
/// last saved under, the ordered active mod list, and the known-expansions
/// list. Only `<activeMods>` is required — see
/// [`ModsConfigDocument::version`]/[`ModsConfigDocument::known_expansions`]'s
/// own doc comments for why the other two tolerate being absent.
pub fn parse_mods_config(bytes: &[u8]) -> Result<ModsConfigDocument, ModsConfigError> {
    let text = decode_lossy(bytes);
    if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(ModsConfigError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();
    if !root
        .tag_name()
        .name()
        .eq_ignore_ascii_case("ModsConfigData")
    {
        return Err(ModsConfigError::WrongRoot);
    }
    let active_mods_node =
        direct_child(root, "activeMods").ok_or(ModsConfigError::MissingActiveMods)?;
    let active_mods = strings_from_container(active_mods_node)
        .into_iter()
        .map(ModId::new)
        .collect();
    let version = child_text(root, "version").unwrap_or_default();
    let known_expansions = direct_child(root, "knownExpansions")
        .map(strings_from_container)
        .unwrap_or_default()
        .into_iter()
        .map(ModId::new)
        .collect();

    Ok(ModsConfigDocument {
        version,
        active_mods,
        known_expansions,
    })
}

/// Parses the ordered, active mod list out of `ModsConfig.xml`'s
/// `<activeMods><li>packageId</li>...</activeMods>`. A thin projection of
/// [`parse_mods_config`] for callers that only need the active list.
pub fn parse_active_mods(bytes: &[u8]) -> Result<Vec<ModId>, ModsConfigError> {
    parse_mods_config(bytes).map(|doc| doc.active_mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_active_mods_in_order() {
        let xml = br#"<ModsConfigData>
              <version>1.6.4871 rev590</version>
              <activeMods>
                <li>ludeon.rimworld</li>
                <li>Example.PatchLib</li>
              </activeMods>
              <knownExpansions><li>ludeon.rimworld.royalty</li></knownExpansions>
            </ModsConfigData>"#;
        let mods = parse_active_mods(xml).unwrap();
        assert_eq!(
            mods,
            vec![
                ModId::new("ludeon.rimworld"),
                ModId::new("example.patchlib")
            ]
        );
    }

    #[test]
    fn parse_mods_config_reads_version_and_known_expansions() {
        let xml = br#"<ModsConfigData>
              <version>1.6.4871 rev590</version>
              <activeMods>
                <li>ludeon.rimworld</li>
                <li>Example.PatchLib</li>
              </activeMods>
              <knownExpansions><li>ludeon.rimworld.royalty</li></knownExpansions>
            </ModsConfigData>"#;

        let doc = parse_mods_config(xml).unwrap();

        assert_eq!(doc.version, "1.6.4871 rev590");
        assert_eq!(
            doc.active_mods,
            vec![
                ModId::new("ludeon.rimworld"),
                ModId::new("example.patchlib")
            ]
        );
        assert_eq!(
            doc.known_expansions,
            vec![ModId::new("ludeon.rimworld.royalty")]
        );
    }

    #[test]
    fn parse_active_mods_delegates_to_parse_mods_config() {
        let xml = br#"<ModsConfigData>
              <version>1.6</version>
              <activeMods><li>a.mod</li></activeMods>
            </ModsConfigData>"#;

        assert_eq!(
            parse_active_mods(xml).unwrap(),
            parse_mods_config(xml).unwrap().active_mods
        );
    }

    #[test]
    fn missing_version_and_known_expansions_are_tolerated_not_errors() {
        let xml = br#"<ModsConfigData><activeMods><li>a.mod</li></activeMods></ModsConfigData>"#;

        let doc = parse_mods_config(xml).unwrap();

        assert_eq!(doc.version, "");
        assert!(doc.known_expansions.is_empty());
        assert_eq!(doc.active_mods, vec![ModId::new("a.mod")]);
    }

    #[test]
    fn missing_active_mods_is_an_error() {
        let xml = br#"<ModsConfigData><version>1.6</version></ModsConfigData>"#;
        assert!(matches!(
            parse_active_mods(xml),
            Err(ModsConfigError::MissingActiveMods)
        ));
    }

    #[test]
    fn wrong_root_is_an_error() {
        assert!(matches!(
            parse_active_mods(b"<Nope/>"),
            Err(ModsConfigError::WrongRoot)
        ));
    }
}
