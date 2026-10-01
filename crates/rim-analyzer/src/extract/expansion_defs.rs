//! Parses `Defs/Misc/ExpansionDefs/ExpansionDefs.xml`, RimWorld's own
//! Core/DLC display-name source: each `<ExpansionDef>`'s `<label>` is the
//! name shown for its `<linkedMod>` packageId. Every DLC's `ExpansionDef`
//! ships in *Core's* `Defs/` folder, not in the DLC's own — this file is
//! read once from `Data/Core`.

use std::collections::HashMap;

use thiserror::Error;

use crate::domain::ModId;

use super::xml_util::{
    MAX_RAW_ELEMENT_DEPTH, child_text, decode_lossy, raw_element_nesting_exceeds,
};

#[derive(Debug, Error)]
pub enum ExpansionDefsError {
    #[error("failed to parse XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("root element is not <Defs>")]
    WrongRoot,
    #[error("raw element nesting exceeds {0} levels; refusing to parse")]
    TooDeep(usize),
}

/// One `<ExpansionDef>`'s display label and, for the mod info panel,
/// description — Core/DLC's own `<description>` doesn't live in their
/// `About.xml` (the engine reads `ExpansionDef.description` instead, the
/// same source as `label`), so both are read from this one file rather
/// than adding a second pass over it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpansionDefEntry {
    pub label: String,
    pub description: Option<String>,
}

/// Maps each `<ExpansionDef>`'s `<linkedMod>` packageId to its `<label>`/
/// `<description>`. An `ExpansionDef` missing `linkedMod` or `label` is
/// skipped rather than erroring the whole file; `description` is optional
/// on both the XML and this map's own value.
pub fn parse(bytes: &[u8]) -> Result<HashMap<ModId, ExpansionDefEntry>, ExpansionDefsError> {
    let text = decode_lossy(bytes);
    if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(ExpansionDefsError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("Defs") {
        return Err(ExpansionDefsError::WrongRoot);
    }

    let map = root
        .children()
        .filter(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case("ExpansionDef"))
        .filter_map(|def| {
            let linked_mod = child_text(def, "linkedMod")?;
            let label = child_text(def, "label")?;
            let description = child_text(def, "description");
            Some((
                ModId::new(linked_mod),
                ExpansionDefEntry { label, description },
            ))
        })
        .collect();
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_linked_mod_to_label() {
        let xml = br#"<Defs>
              <ExpansionDef>
                <defName>Core</defName>
                <label>Core</label>
                <linkedMod>Ludeon.RimWorld</linkedMod>
              </ExpansionDef>
              <ExpansionDef>
                <defName>Royalty</defName>
                <label>Royalty</label>
                <linkedMod>Ludeon.RimWorld.Royalty</linkedMod>
              </ExpansionDef>
            </Defs>"#;
        let map = parse(xml).unwrap();
        assert_eq!(
            map.get(&ModId::new("ludeon.rimworld"))
                .map(|e| e.label.as_str()),
            Some("Core")
        );
        assert_eq!(
            map.get(&ModId::new("ludeon.rimworld.royalty"))
                .map(|e| e.label.as_str()),
            Some("Royalty")
        );
    }

    #[test]
    fn maps_linked_mod_to_its_description_when_present() {
        let xml = br#"<Defs>
              <ExpansionDef>
                <defName>Core</defName>
                <label>Core</label>
                <description>The base game.</description>
                <linkedMod>Ludeon.RimWorld</linkedMod>
              </ExpansionDef>
              <ExpansionDef>
                <defName>Royalty</defName>
                <label>Royalty</label>
                <linkedMod>Ludeon.RimWorld.Royalty</linkedMod>
              </ExpansionDef>
            </Defs>"#;
        let map = parse(xml).unwrap();
        assert_eq!(
            map.get(&ModId::new("ludeon.rimworld"))
                .and_then(|e| e.description.as_deref()),
            Some("The base game.")
        );
        assert_eq!(
            map.get(&ModId::new("ludeon.rimworld.royalty"))
                .and_then(|e| e.description.as_deref()),
            None,
            "a def with no <description> maps to None, not an error"
        );
    }

    #[test]
    fn skips_expansion_defs_missing_linked_mod_or_label() {
        let xml = br#"<Defs>
              <ExpansionDef><defName>NoLabel</defName><linkedMod>a.b</linkedMod></ExpansionDef>
              <ExpansionDef><defName>NoLink</defName><label>Label</label></ExpansionDef>
            </Defs>"#;
        let map = parse(xml).unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn wrong_root_is_an_error() {
        assert!(matches!(
            parse(b"<NotDefs/>"),
            Err(ExpansionDefsError::WrongRoot)
        ));
    }
}
