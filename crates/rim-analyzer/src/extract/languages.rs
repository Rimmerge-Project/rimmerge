//! Indexes a `Languages/<lang>/Keyed/**/*.xml` file: every direct child
//! element of the root is one keyed translation key, whatever the root
//! element is named. RimWorld's `LoadedLanguage` keyed loader reads the
//! root's children without checking the root's own tag, so real installs ship
//! roots named `<LanguageData>`, `<Keyed>`, `<LangaugesData>`, and
//! `<LanguagData>` (all load fine in-game) — indexing must be at least as
//! lenient. Data only — which key collides across mods, and which owner's
//! value wins (last loaded), is `analysis::conflicts`' job.

use thiserror::Error;

use super::xml_util::{MAX_RAW_ELEMENT_DEPTH, decode_lossy, raw_element_nesting_exceeds};

#[derive(Debug, Error)]
pub enum LanguagesError {
    #[error("skipped: {} ({source}) at {}", xml_error_kind(.source), .source.pos())]
    Xml {
        #[from]
        source: roxmltree::Error,
    },
    #[error("skipped: raw element nesting exceeds {0} levels; refusing to parse")]
    TooDeep(usize),
}

/// Names what went wrong for the "skipped: <kind> (<parser message>) at
/// <line>:<col>" wording: a mismatched closing tag is common enough in
/// real-world (RimWorld-rejected) mods to call out by name, everything
/// else is just "malformed XML". The parser's own message stays in the
/// note verbatim either way — this only supplies the human-readable
/// headline in front of it.
fn xml_error_kind(error: &roxmltree::Error) -> &'static str {
    match error {
        roxmltree::Error::UnexpectedCloseTag(..) => "mismatched closing tag",
        _ => "malformed XML",
    }
}

/// Every keyed translation key one `Keyed/**/*.xml` file defines: its root
/// element's direct children's tag names, e.g. `ThingDef_Wall.label` in
/// `<ThingDef_Wall.label>A wall</ThingDef_Wall.label>`. The root's own tag
/// name is never checked (see the module doc comment) — only a genuine XML
/// parse failure is reported.
pub fn index(bytes: &[u8]) -> Result<Vec<String>, LanguagesError> {
    let text = decode_lossy(bytes);
    if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(LanguagesError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();

    Ok(root
        .children()
        .filter(roxmltree::Node::is_element)
        .map(|element| element.tag_name().name().to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_every_direct_child_tag_as_a_key() {
        let xml = br#"<LanguageData>
              <ThingDef_Wall.label>a wall</ThingDef_Wall.label>
              <SomeKey>Some Text</SomeKey>
            </LanguageData>"#;
        let keys = index(xml).unwrap();
        assert_eq!(
            keys,
            vec!["ThingDef_Wall.label".to_string(), "SomeKey".to_string()]
        );
    }

    #[test]
    fn empty_language_data_yields_no_keys() {
        let xml = b"<LanguageData></LanguageData>";
        assert!(index(xml).unwrap().is_empty());
    }

    #[test]
    fn accepts_any_root_element_name() {
        // RimWorld's `LoadedLanguage` keyed loader reads the root's
        // children without checking the root's own tag name, so real
        // installs ship `<Keyed>`, `<LangaugesData>`, and `<LanguagData>`
        // roots (workshop 3000000015, 3000000016, 3000000017) that load
        // fine in-game. No warning, no `WrongRoot` variant any more.
        for root_tag in ["LanguageData", "Keyed", "LangaugesData", "LanguagData"] {
            let xml = format!("<{root_tag}><SomeKey>Some Text</SomeKey></{root_tag}>");
            let keys = index(xml.as_bytes()).unwrap();
            assert_eq!(keys, vec!["SomeKey".to_string()], "root <{root_tag}>");
        }
    }

    #[test]
    fn malformed_xml_is_an_error_worded_as_skipped() {
        // A genuinely malformed file (a bad attribute, as seen in workshop
        // 3000000018's `Keys.xml`) still fails, and is worded to match
        // `infra::mod_scan`'s other file-level "skipped: ..." errors —
        // the parser's own message survives verbatim inside the
        // parentheses so the user can still find the spot.
        let err = index(br#"<Keyed><A key=value>text</A></Keyed>"#).unwrap_err();
        let message = err.to_string();
        assert!(
            message.starts_with("skipped: malformed XML ("),
            "unexpected message: {message}"
        );
        assert!(
            message.contains("expected a quote"),
            "parser's own message should survive verbatim: {message}"
        );
    }

    #[test]
    fn mismatched_closing_tag_is_worded_as_such_not_generic_malformed_xml() {
        // A mismatched closing tag (as seen in workshop 3000000018's
        // `Keys.xml`) is common enough among real-world (RimWorld-rejected)
        // mods to name specifically, rather than the generic "malformed
        // XML" every other parse failure gets.
        let err = index(b"<Keyed><A>text</B></Keyed>").unwrap_err();
        let message = err.to_string();
        assert!(
            message.starts_with("skipped: mismatched closing tag ("),
            "unexpected message: {message}"
        );
        assert!(
            message.contains("expected 'A' tag, not 'B'"),
            "parser's own message should survive verbatim: {message}"
        );
        assert!(
            message.ends_with(" at 1:15"),
            "expected a trailing line:col, got: {message}"
        );
    }

    #[test]
    fn unclosed_tag_is_an_error_not_a_panic() {
        assert!(index(b"<LanguageData><Unclosed>").is_err());
    }
}
