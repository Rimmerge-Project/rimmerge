//! Shared byte-to-text decoding and small `roxmltree` traversal helpers
//! used by every extractor. The mod XML this reads is untrusted and
//! sometimes carries a BOM, inconsistent encodings, or unexpected shapes,
//! so every helper here returns `Option`/iterators rather than panicking.

use std::collections::BTreeSet;

use roxmltree::Node;

/// The shortest a `*Class` element's tag name can be: a one-character
/// prefix plus `Class` (e.g. a hypothetical `xClass`) — used only to rule
/// out a bare `<Class>` element (no prefix), which is not one of the
/// `thingClass`/`compClass`/`workerClass`/... shapes this collects.
const MIN_STAR_CLASS_TAG_LEN: usize = "Class".len() + 1;

/// Every fully-qualified type string named under `root` (`root` itself
/// included) as either an element's `Class` attribute or the text of an
/// element whose tag name ends with `Class` (`thingClass`, `compClass`,
/// `workerClass`, ...) — the two shapes RimWorld uses to name a C# type in
/// XML. Exact strings, case-sensitive, no namespace collapsing (shared by
/// [`super::patches`]'s per-operation `<value>` scan and [`super::defs`]'s
/// per-file inline scan, so a type injected by a patch and one written inline
/// in a def compare equal).
pub fn collect_class_strings(root: Node, out: &mut BTreeSet<String>) {
    for element in root.descendants().filter(Node::is_element) {
        if let Some(class) = element.attribute("Class") {
            let class = class.trim();
            if !class.is_empty() {
                out.insert(class.to_string());
            }
        }
        let tag = element.tag_name().name();
        if tag.len() >= MIN_STAR_CLASS_TAG_LEN && tag.ends_with("Class") {
            let text = element.text().map(str::trim).filter(|s| !s.is_empty());
            if let Some(text) = text {
                out.insert(text.to_string());
            }
        }
    }
}

/// Decodes `bytes` as UTF-8, replacing invalid sequences and stripping a
/// leading UTF-8 BOM if present. RimWorld mod XML is nominally UTF-8; this
/// never fails, matching the "record a warning and skip" policy at the
/// call site instead of aborting the whole run on one bad file.
#[must_use]
pub fn decode_lossy(bytes: &[u8]) -> String {
    const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
    let bytes = bytes.strip_prefix(&UTF8_BOM).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// The first direct child element of `node` named `tag` (case-insensitive).
pub fn direct_child<'a, 'input>(node: Node<'a, 'input>, tag: &str) -> Option<Node<'a, 'input>> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case(tag))
}

/// Every direct child element of `node`, in document order — the same
/// `.children().filter(Node::is_element)` shape several extractors already
/// repeat inline (`extract::patches::walk`/`walk_operation`), pulled out here
/// only for [`super::patches`]'s top-level `<value>` scan, which needs it in
/// two places (the injected-element and injected-def shapes) and, unlike
/// those existing call sites, has no ordinal index of its own to track
/// alongside it.
pub fn direct_element_children<'a, 'input>(
    node: Node<'a, 'input>,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

/// The trimmed, non-empty text of the direct child element named `tag`.
pub fn child_text(node: Node, tag: &str) -> Option<String> {
    direct_child(node, tag)
        .and_then(|n| n.text())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Trimmed, non-empty text from every direct `<li>` child of `container`.
pub fn strings_from_container(container: Node) -> Vec<String> {
    container
        .children()
        .filter(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case("li"))
        .filter_map(|li| li.text())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Splits a comma-separated string into trimmed, non-empty pieces — the
/// shape shared by `MayRequire`/`MayRequireAnyOf`/`IfModActive`/
/// `IfModNotActive` attribute values and comma-joined author lists. Case
/// is left as-is; callers needing case-insensitive comparison normalize
/// it themselves (e.g. via `ModId::new`).
pub fn split_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// [`split_csv`] applied to a node attribute value; a missing attribute
/// yields an empty list.
pub fn csv_attr(node: Node, attr: &str) -> Vec<String> {
    node.attribute(attr).map(split_csv).unwrap_or_default()
}

/// The deepest raw element nesting any extractor will hand to
/// `roxmltree::Document::parse` at all — a much larger, purely defensive
/// bound than any of this crate's own *post-parse* depth caps (e.g.
/// `patches::MAX_PATCH_TREE_DEPTH`, `defs::MAX_INLINE_NODE_PATH_DEPTH`),
/// which only ever run once a document has already parsed. `roxmltree`'s
/// own recursive-descent parser recurses over raw element nesting depth
/// and can overflow the stack **before** any of those checks ever run —
/// measured directly on a 1 MiB stack (the CLI's own main thread size):
/// 1,000 levels of bare element nesting parses fine, 5,000 overflows it.
/// [`raw_element_nesting_exceeds`] rejects a document past this bound
/// with its own linear, non-recursive scan, specifically so the one call
/// nothing else in this crate bounds never runs on pathological input at
/// all. Matches `rim_merge::xml::MAX_RAW_ELEMENT_DEPTH`.
pub const MAX_RAW_ELEMENT_DEPTH: usize = 512;

/// A fast, purely iterative (never recursive) approximate scan for
/// whether `text`'s own raw element nesting ever exceeds `max_depth` —
/// not a real XML parser: it tracks `<tag>`/`</tag>`/`<tag/>` boundaries
/// (skipping `<!--...-->`/`<![CDATA[...]]>`/`<?...?>` content, and
/// quoted attribute values so an embedded `>` can't truncate a tag early)
/// well enough to bound nesting depth conservatively, but validates
/// nothing else about the document. It exists only to decide "is this
/// safe to hand to a real, recursive-descent XML parser" —
/// `roxmltree::Document::parse` still does the real parse and still
/// rejects genuinely malformed input on its own terms.
pub fn raw_element_nesting_exceeds(text: &str, max_depth: usize) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut depth = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"<!--") {
            i = find_bytes(&bytes[i..], b"-->").map_or(bytes.len(), |end| i + end + 3);
            continue;
        }
        if bytes[i..].starts_with(b"<![CDATA[") {
            i = find_bytes(&bytes[i..], b"]]>").map_or(bytes.len(), |end| i + end + 3);
            continue;
        }
        if bytes[i..].starts_with(b"<?") {
            i = find_bytes(&bytes[i..], b"?>").map_or(bytes.len(), |end| i + end + 2);
            continue;
        }
        let Some(tag_end) = quote_aware_tag_end(bytes, i) else {
            break;
        };
        let is_closing = bytes.get(i + 1) == Some(&b'/');
        let is_self_closing = tag_end > i && bytes[tag_end - 1] == b'/';
        if is_closing {
            depth = depth.saturating_sub(1);
        } else if !is_self_closing {
            depth += 1;
            if depth > max_depth {
                return true;
            }
        }
        i = tag_end + 1;
    }
    false
}

/// The index of the `>` that closes the tag starting at `bytes[start]`
/// (which must be `<`), skipping any `>` inside a single- or
/// double-quoted attribute value so a literal `>` in an attribute can't
/// truncate the tag early.
fn quote_aware_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) if b == q => quote = None,
            Some(_) => {}
            None if b == b'"' || b == b'\'' => quote = Some(b),
            None if b == b'>' => return Some(i),
            None => {}
        }
        i += 1;
    }
    None
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_utf8_bom() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"<a/>");
        assert_eq!(decode_lossy(&bytes), "<a/>");
    }

    #[test]
    fn replaces_invalid_sequences_instead_of_failing() {
        let bytes = [b'<', b'a', 0xFF, b'>'];
        let text = decode_lossy(&bytes);
        assert!(text.starts_with("<a"));
        assert!(text.contains('>'));
    }

    #[test]
    fn split_csv_trims_and_drops_empty_entries() {
        assert_eq!(
            split_csv(" a.b ,, c.d"),
            vec!["a.b".to_string(), "c.d".to_string()]
        );
    }

    #[test]
    fn split_csv_of_empty_string_is_empty() {
        assert!(split_csv("").is_empty());
    }

    fn parse(xml: &str) -> roxmltree::Document<'_> {
        roxmltree::Document::parse(xml).unwrap()
    }

    #[test]
    fn collect_class_strings_reads_a_class_attribute() {
        let doc = parse(r#"<root><li Class="Example.Weapons.HeavyWeapon"/></root>"#);
        let mut out = BTreeSet::new();
        collect_class_strings(doc.root_element(), &mut out);
        assert_eq!(
            out,
            BTreeSet::from(["Example.Weapons.HeavyWeapon".to_string()])
        );
    }

    #[test]
    fn collect_class_strings_reads_a_star_class_elements_text() {
        let doc = parse("<root><thingClass>Some.Namespace.Thing</thingClass></root>");
        let mut out = BTreeSet::new();
        collect_class_strings(doc.root_element(), &mut out);
        assert_eq!(out, BTreeSet::from(["Some.Namespace.Thing".to_string()]));
    }

    #[test]
    fn collect_class_strings_ignores_a_bare_class_element_with_no_prefix() {
        let doc = parse("<root><Class>NotAPrefixedClassElement</Class></root>");
        let mut out = BTreeSet::new();
        collect_class_strings(doc.root_element(), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn direct_element_children_skips_text_nodes_and_descendants() {
        let doc = parse("<root>text<a/><b><c/></b></root>");
        let tags: Vec<&str> = direct_element_children(doc.root_element())
            .map(|n| n.tag_name().name())
            .collect();
        assert_eq!(tags, vec!["a", "b"]);
    }

    /// Every extractor that parses mod-provided (or hand-edited) XML must
    /// refuse pathological nesting before `roxmltree`'s recursive-descent
    /// parser sees it: 50,000 levels overflow a 1 MiB stack (the CLI's own
    /// main thread size), which aborts the whole process rather than
    /// failing an assertion, so this runs every parser on such a stack.
    #[test]
    fn every_untrusted_xml_extractor_refuses_pathologically_deep_nesting_on_a_small_stack() {
        use std::collections::HashSet;

        use crate::domain::GameVersion;
        use crate::extract::{
            about_xml, expansion_defs, languages, load_folders, manifest_xml, mods_config,
        };

        let mut inner = String::from("<leaf/>");
        for _ in 0..50_000 {
            inner = format!("<a>{inner}</a>");
        }
        let xml = format!("<Root>{inner}</Root>").into_bytes();
        let version = GameVersion::new(1, 6);

        let refusals: Vec<(&str, String)> = std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || {
                let active = HashSet::new();
                vec![
                    (
                        "about_xml::parse",
                        about_xml::parse(&xml, version).err().map(|e| e.to_string()),
                    ),
                    (
                        "about_xml::parse_details",
                        about_xml::parse_details(&xml, version)
                            .err()
                            .map(|e| e.to_string()),
                    ),
                    (
                        "expansion_defs::parse",
                        expansion_defs::parse(&xml).err().map(|e| e.to_string()),
                    ),
                    (
                        "languages::index",
                        languages::index(&xml).err().map(|e| e.to_string()),
                    ),
                    (
                        "load_folders::resolve_version_entry",
                        load_folders::resolve_version_entry(&xml, version, &active)
                            .err()
                            .map(|e| e.to_string()),
                    ),
                    (
                        "load_folders::has_exact_version_block",
                        load_folders::has_exact_version_block(&xml, version)
                            .err()
                            .map(|e| e.to_string()),
                    ),
                    (
                        "manifest_xml::parse",
                        manifest_xml::parse(&xml).err().map(|e| e.to_string()),
                    ),
                    (
                        "mods_config::parse_mods_config",
                        mods_config::parse_mods_config(&xml)
                            .err()
                            .map(|e| e.to_string()),
                    ),
                ]
                .into_iter()
                .map(|(name, error)| {
                    (
                        name,
                        error.unwrap_or_else(|| "accepted 50,000 levels".to_string()),
                    )
                })
                .collect()
            })
            .expect("spawning the probe thread")
            .join()
            .expect(
                "a pathologically deep XML file overflowed a 1 MiB stack instead of being refused",
            );

        for (name, message) in refusals {
            assert!(
                message.contains("nesting exceeds"),
                "{name} did not refuse deep nesting cleanly: {message}"
            );
        }
    }

    #[test]
    fn collect_class_strings_walks_every_descendant() {
        let doc = parse(
            r#"<root><a><b><compClass>Deep.Comp</compClass></b></a><li Class="X.Y"/></root>"#,
        );
        let mut out = BTreeSet::new();
        collect_class_strings(doc.root_element(), &mut out);
        assert_eq!(
            out,
            BTreeSet::from(["Deep.Comp".to_string(), "X.Y".to_string()])
        );
    }
}
