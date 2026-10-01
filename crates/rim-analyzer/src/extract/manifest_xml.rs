//! Parses a mod's `About/Manifest.xml` — the format community mod managers
//! use, sibling of `About.xml` — into raw, not-yet-resolved load-order
//! entries.
//!
//! Unlike `about_xml::DeclaredOrder`, whose lists are already `ModId`s
//! (About.xml's own `loadAfter`/`loadBefore`/`modDependencies` are always
//! packageIds), a Manifest.xml `<li>` may name another mod by its **display
//! name or its packageId** — such tooling accepts both — so this
//! module deliberately stops at the raw string. Resolving it needs the same
//! `build_name_map` that `analysis::edges::find_mod_edges` (FindMod) already
//! resolves against, which only exists once every mod's `About.xml` has been
//! read — long after `infra::mod_scan` reads this file — so resolution is
//! deferred to `analysis::edges::manifest_order_edges` (which also strips a
//! community-manager-style version bound, e.g. `"ExampleFramework >= 5.5.0"`,
//! before matching either form).
//!
//! **`<dependencies>` can carry either of two real shapes**: a plain
//! `<li>text</li>`, the same shape `loadAfter`/`loadBefore` always use, *or*
//! the nested `<li><packageId>…</packageId><displayName>…</displayName></li>`
//! shape `about_xml`'s own `modDependencies` uses — see
//! [`dependency_entries`].

use roxmltree::Node;
use thiserror::Error;

use crate::domain::ManifestOrder;

use super::xml_util::{
    MAX_RAW_ELEMENT_DEPTH, child_text, decode_lossy, direct_child, raw_element_nesting_exceeds,
    strings_from_container,
};

#[derive(Debug, Error)]
pub enum ManifestXmlError {
    #[error("failed to parse XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("root element is not <Manifest>")]
    WrongRoot,
    #[error("raw element nesting exceeds {0} levels; refusing to parse")]
    TooDeep(usize),
}

/// Parses `bytes` as a `Manifest.xml` document. Same skip-and-warn
/// convention as `about_xml::parse`/`load_folders::parse`: a malformed
/// file or the wrong root is the caller's to turn into a [`Warning`](crate::domain::Warning)
/// and treat as "no manifest" rather than aborting the scan — Manifest.xml
/// is optional and third-party-authored, unlike `About.xml`.
pub fn parse(bytes: &[u8]) -> Result<ManifestOrder, ManifestXmlError> {
    let text = decode_lossy(bytes);
    if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(ManifestXmlError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("Manifest") {
        return Err(ManifestXmlError::WrongRoot);
    }

    Ok(ManifestOrder {
        load_after: string_list(root, "loadAfter"),
        load_before: string_list(root, "loadBefore"),
        dependencies: direct_child(root, "dependencies")
            .map(dependency_entries)
            .unwrap_or_default(),
    })
}

fn string_list(node: Node, tag: &str) -> Vec<String> {
    direct_child(node, tag)
        .map(strings_from_container)
        .unwrap_or_default()
}

/// Like [`strings_from_container`], but for `<dependencies>` specifically: an
/// entry may be a plain `<li>text</li>` (the common shape,
/// `loadAfter`/`loadBefore`'s only shape), **or** the nested
/// `<li><packageId>…</packageId><displayName>…</displayName></li>` shape
/// `about_xml`'s own `modDependencies` uses (real manifests ship exactly this
/// shape). `strings_from_container`'s own `li.text()` reads the whitespace
/// *text node* preceding the nested `<packageId>` element, not the packageId
/// itself — trimmed to empty and silently dropped, the one class of failure
/// (an entry vanishing with no edge *and* no warning) this design says must
/// never happen. Falls back to the `<packageId>` child's own text when the
/// `<li>` itself has none.
fn dependency_entries(container: Node) -> Vec<String> {
    container
        .children()
        .filter(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case("li"))
        .filter_map(|li| {
            li.text()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| child_text(li, "packageId"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_load_after_load_before_and_dependencies() {
        let xml = br#"<Manifest>
              <identifier>some.mod</identifier>
              <dependencies><li>example.patchlib</li></dependencies>
              <loadBefore><li>ExamplePower.GridPrimary</li></loadBefore>
              <loadAfter>
                <li>example.patchlib</li>
                <li>Example.HookLib</li>
              </loadAfter>
            </Manifest>"#;
        let data = parse(xml).unwrap();
        assert_eq!(data.dependencies, vec!["example.patchlib".to_string()]);
        assert_eq!(
            data.load_before,
            vec!["ExamplePower.GridPrimary".to_string()]
        );
        assert_eq!(
            data.load_after,
            vec![
                "example.patchlib".to_string(),
                "Example.HookLib".to_string()
            ]
        );
    }

    /// A real, common shape on the install: every list present but empty
    /// (`<loadAfter></loadAfter>` or self-closed) — must parse to empty
    /// vectors, not an error.
    #[test]
    fn empty_lists_parse_as_empty_not_an_error() {
        let xml = br#"<Manifest>
              <dependencies />
              <incompatibleWith />
              <loadBefore />
              <loadAfter />
            </Manifest>"#;
        let data = parse(xml).unwrap();
        assert!(data.load_after.is_empty());
        assert!(data.load_before.is_empty());
        assert!(data.dependencies.is_empty());
    }

    /// A manifest naming no relevant tags at all (only `<version>`,
    /// `<manifestUri>`, etc.) — every list defaults empty.
    #[test]
    fn manifest_with_no_order_tags_at_all_parses_to_defaults() {
        let xml = br#"<Manifest>
              <version>1.0.0</version>
              <manifestUri>https://example.com/Manifest.xml</manifestUri>
            </Manifest>"#;
        let data = parse(xml).unwrap();
        assert_eq!(data, ManifestOrder::default());
    }

    #[test]
    fn wrong_root_element_is_an_error() {
        let xml = br#"<NotManifest/>"#;
        assert!(matches!(parse(xml), Err(ManifestXmlError::WrongRoot)));
    }

    #[test]
    fn malformed_xml_is_an_error_not_a_panic() {
        let xml = br#"<Manifest><loadAfter"#;
        assert!(parse(xml).is_err());
    }

    /// `<dependencies>` may use the nested
    /// `<li><packageId>…</packageId><displayName>…</displayName></li>` shape,
    /// as a real manifest does, not just the flat `<li>text</li>` one — read
    /// via `li.text()` alone, this entry would silently vanish (no text, no
    /// warning), since that reads the whitespace before `<packageId>`, not
    /// the packageId itself.
    #[test]
    fn dependencies_falls_back_to_a_nested_package_id_element() {
        let xml = br#"<Manifest>
              <dependencies>
                <li>
                  <packageId>example.patchlib</packageId>
                  <displayName>Example PatchLib</displayName>
                </li>
              </dependencies>
            </Manifest>"#;
        let data = parse(xml).unwrap();
        assert_eq!(data.dependencies, vec!["example.patchlib".to_string()]);
    }

    /// The two shapes can coexist across different entries in the same
    /// `<dependencies>` list.
    #[test]
    fn dependencies_mixes_flat_and_nested_entries() {
        let xml = br#"<Manifest>
              <dependencies>
                <li>plain.text.entry</li>
                <li>
                  <packageId>example.patchlib</packageId>
                </li>
              </dependencies>
            </Manifest>"#;
        let data = parse(xml).unwrap();
        assert_eq!(
            data.dependencies,
            vec![
                "plain.text.entry".to_string(),
                "example.patchlib".to_string()
            ]
        );
    }
}
