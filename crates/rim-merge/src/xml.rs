//! `roxmltree` document text <-> [`FieldTree`]. Parsing is pure (no IO):
//! callers hand this module XML text they already have (a def's own
//! element, a template's, or a `<value>` fragment) — see this crate's
//! top-level doc comment for why the boundary sits there.

use std::collections::BTreeMap;

use roxmltree::Node;

use crate::error::MergeError;
use crate::tree::{Content, FieldNode, FieldTree};

/// The deepest element nesting [`parse`]/[`parse_element`] will follow —
/// bounds recursion against hostile or corrupt input (a security
/// baseline, not a realistic RimWorld def shape: even a deeply nested
/// `comps`/`modExtensions` chain stays well under this).
pub const MAX_DEPTH: usize = 64;

/// The deepest **raw** element nesting this module (or
/// [`crate::patch_eval::replay`], which parses one contribution's own
/// operation XML the same way) will hand to `roxmltree::Document::parse`
/// at all — a much larger, purely defensive bound than [`MAX_DEPTH`]
/// above, which only ever runs once a document has already parsed.
/// `roxmltree`'s own recursive-descent parser recurses over raw element
/// nesting depth and can overflow the stack **before** [`MAX_DEPTH`]'s own
/// check in [`parse_content`] ever runs — measured directly on a 1 MiB
/// stack (the CLI's own main thread size): 1,000 levels of bare element
/// nesting parses fine, 5,000 overflows it. [`raw_element_nesting_exceeds`]
/// rejects a document past this bound with its own linear, non-recursive
/// scan, specifically so the one call neither `MAX_DEPTH` nor anything
/// else in this crate bounds never runs on pathological input at all.
pub(crate) const MAX_RAW_ELEMENT_DEPTH: usize = 512;

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
pub(crate) fn raw_element_nesting_exceeds(text: &str, max_depth: usize) -> bool {
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

/// Parses `xml_text` (one def or template element, e.g.
/// `<HediffDef ParentName="...">...</HediffDef>`) into a [`FieldTree`],
/// stripping `Name`/`ParentName`/`Abstract`/`Inherit` off the root: the
/// first two move into [`FieldTree::name`]/[`FieldTree::parent_name`];
/// `Abstract`/`Inherit` are simply discarded — see [`FieldTree`]'s own
/// doc comment for why neither is meaningful once a tree exists as data.
///
/// # Errors
///
/// [`MergeError::Xml`] on malformed XML, [`MergeError::EmptyDocument`] on
/// a document with no root element, [`MergeError::MixedContent`] on an
/// element mixing element children with non-whitespace text,
/// [`MergeError::TooDeep`] past [`MAX_DEPTH`] levels of nesting.
pub fn parse(xml_text: &str) -> Result<FieldTree, MergeError> {
    if raw_element_nesting_exceeds(xml_text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(MergeError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(xml_text)?;
    let root = doc.root_element();
    if !root.is_element() {
        return Err(MergeError::EmptyDocument);
    }

    let mut attrs: BTreeMap<String, String> = root
        .attributes()
        .map(|attr| (attr.name().to_string(), attr.value().to_string()))
        .collect();
    let name = attrs.remove("Name");
    let parent_name = attrs.remove("ParentName");
    attrs.remove("Abstract");
    attrs.remove("Inherit");

    let content = parse_content(root, 0)?;
    Ok(FieldTree {
        root: FieldNode {
            tag: root.tag_name().name().to_string(),
            attrs,
            content,
        },
        parent_name,
        name,
    })
}

/// Parses one standalone element (e.g. a `<value>` fragment's own child)
/// into a [`FieldNode`] — `pub(crate)` for [`crate::patch_eval`], which
/// needs to turn a `<value>`'s children into [`FieldNode`]s the same way
/// [`parse`] turns a def's own children into them.
///
/// # Errors
///
/// See [`parse`].
pub(crate) fn parse_element(node: Node) -> Result<FieldNode, MergeError> {
    parse_node(node, 0)
}

fn parse_node(node: Node, depth: usize) -> Result<FieldNode, MergeError> {
    let attrs = node
        .attributes()
        .map(|attr| (attr.name().to_string(), attr.value().to_string()))
        .collect();
    Ok(FieldNode {
        tag: node.tag_name().name().to_string(),
        attrs,
        content: parse_content(node, depth)?,
    })
}

fn parse_content(node: Node, depth: usize) -> Result<Content, MergeError> {
    if depth > MAX_DEPTH {
        return Err(MergeError::TooDeep(MAX_DEPTH));
    }
    let element_children: Vec<Node> = node.children().filter(Node::is_element).collect();
    let text: String = node
        .children()
        .filter(Node::is_text)
        .filter_map(|child| child.text())
        .collect();
    let has_text = !text.trim().is_empty();

    if !element_children.is_empty() {
        if has_text {
            return Err(MergeError::MixedContent {
                tag: node.tag_name().name().to_string(),
            });
        }
        let children = element_children
            .into_iter()
            .map(|child| parse_node(child, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(Content::Children(children));
    }

    if has_text {
        Ok(Content::Text(text.trim().to_string()))
    } else {
        Ok(Content::Empty)
    }
}

/// Renders `tree` back to XML text: 2-space indent, one element per line,
/// deterministic attribute order (`Name`/`ParentName` first when present,
/// then the node's own attributes alphabetically — a [`BTreeMap`] already
/// iterates that way), every value escaped. No XML declaration: this is
/// the element's own text, not a file — [`crate::emit`] adds the
/// declaration when it writes a whole `Patches/*.xml` file.
#[must_use]
pub fn render(tree: &FieldTree) -> String {
    let mut extra = Vec::new();
    if let Some(name) = &tree.name {
        extra.push(("Name", name.as_str()));
    }
    if let Some(parent) = &tree.parent_name {
        extra.push(("ParentName", parent.as_str()));
    }
    render_element(&tree.root, &extra, 0)
}

/// Renders a single [`FieldNode`] (and its descendants) with no root-only
/// attributes folded in — the form used for a `<value>` fragment, which
/// is never itself a def/template root.
#[must_use]
pub fn render_node(node: &FieldNode, depth: usize) -> String {
    render_element(node, &[], depth)
}

fn render_element(node: &FieldNode, extra_attrs: &[(&str, &str)], depth: usize) -> String {
    let indent = "  ".repeat(depth);
    let mut attr_text = String::new();
    for (name, value) in extra_attrs {
        attr_text.push_str(&format!(" {name}=\"{}\"", escape_attr(value)));
    }
    for (name, value) in &node.attrs {
        attr_text.push_str(&format!(" {name}=\"{}\"", escape_attr(value)));
    }

    match &node.content {
        Content::Empty => format!("{indent}<{}{attr_text} />\n", node.tag),
        Content::Text(text) => {
            format!(
                "{indent}<{}{attr_text}>{}</{}>\n",
                node.tag,
                escape_text(text),
                node.tag
            )
        }
        Content::Children(children) => {
            let mut out = format!("{indent}<{}{attr_text}>\n", node.tag);
            for child in children {
                out.push_str(&render_node(child, depth + 1));
            }
            out.push_str(&format!("{indent}</{}>\n", node.tag));
            out
        }
    }
}

/// Escapes `&`, `<`, `>` for XML text content — `pub(crate)` since
/// [`crate::emit`] renders `About.xml`/`Patches/*.xml` text by hand
/// (outside a [`FieldNode`] tree) and needs the exact same rule.
pub(crate) fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// [`escape_text`] plus `"`, for XML attribute values.
pub(crate) fn escape_attr(text: &str) -> String {
    escape_text(text).replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_name_parent_name_abstract_and_inherit_off_the_root() {
        let tree = parse(
            r#"<HediffDef Name="addedPartExampleSynth" ParentName="AddedBodyPartBase" Abstract="True">
                 <defaultLabelColor>(188,39,242)</defaultLabelColor>
               </HediffDef>"#,
        )
        .unwrap();
        assert_eq!(tree.name.as_deref(), Some("addedPartExampleSynth"));
        assert_eq!(tree.parent_name.as_deref(), Some("AddedBodyPartBase"));
        assert!(!tree.root.attrs.contains_key("Name"));
        assert!(!tree.root.attrs.contains_key("ParentName"));
        assert!(!tree.root.attrs.contains_key("Abstract"));
    }

    #[test]
    fn parses_a_leaf_text_child() {
        let tree = parse("<HediffDef><label>bionic heart</label></HediffDef>").unwrap();
        let label = tree.get(&"label".parse().unwrap()).expect("label present");
        assert_eq!(label.content, Content::Text("bionic heart".to_string()));
    }

    #[test]
    fn parses_a_self_closing_element_as_empty() {
        let tree = parse(r#"<ThingDef><graphicData Inherit="False"/></ThingDef>"#).unwrap();
        let graphic_data = tree.get(&"graphicData".parse().unwrap()).unwrap();
        assert_eq!(graphic_data.content, Content::Empty);
        assert_eq!(
            graphic_data.attrs.get("Inherit").map(String::as_str),
            Some("False")
        );
    }

    #[test]
    fn mixed_content_is_a_parse_error() {
        let result = parse("<HediffDef>text<label>x</label></HediffDef>");
        assert!(matches!(result, Err(MergeError::MixedContent { tag }) if tag == "HediffDef"));
    }

    #[test]
    fn malformed_xml_is_a_parse_error() {
        assert!(matches!(parse("<Unclosed>"), Err(MergeError::Xml(_))));
    }

    #[test]
    fn nesting_past_max_depth_is_rejected() {
        let mut xml = "<Root>".to_string();
        for i in 0..(MAX_DEPTH + 2) {
            xml.push_str(&format!("<n{i}>"));
        }
        xml.push_str("leaf");
        for i in (0..(MAX_DEPTH + 2)).rev() {
            xml.push_str(&format!("</n{i}>"));
        }
        xml.push_str("</Root>");

        assert!(matches!(parse(&xml), Err(MergeError::TooDeep(MAX_DEPTH))));
    }

    /// The actual attack shape — a ~300 KB Workshop patch with 50,000
    /// nested elements — must never even reach
    /// `roxmltree::Document::parse`, whose own recursive-descent parser
    /// overflows a 1 MiB stack well before [`MAX_DEPTH`]'s own, later
    /// check would ever get a chance to run — measured directly: bare
    /// element nesting this deep overflows the parse call itself. Run on
    /// a 1 MiB stack (the CLI's own main thread size); without
    /// [`raw_element_nesting_exceeds`]'s pre-parse scan, this test aborts
    /// the whole test process rather than failing an assertion.
    #[test]
    fn pathological_nesting_is_rejected_before_parsing_not_crashed() {
        let mut xml = "<Root>".to_string();
        for _ in 0..50_000 {
            xml.push_str("<a>");
        }
        xml.push_str("leaf");
        for _ in 0..50_000 {
            xml.push_str("</a>");
        }
        xml.push_str("</Root>");

        std::thread::Builder::new()
            .stack_size(1024 * 1024)
            .spawn(move || {
                assert!(matches!(
                    parse(&xml),
                    Err(MergeError::TooDeep(depth)) if depth == MAX_RAW_ELEMENT_DEPTH
                ));
            })
            .expect("spawning the probe thread")
            .join()
            .expect(
                "a pathologically deep document overflowed a 1 MiB stack instead of being \
                 rejected cleanly",
            );
    }

    #[test]
    fn nesting_within_max_depth_parses_fine() {
        let mut xml = "<Root>".to_string();
        for i in 0..(MAX_DEPTH - 1) {
            xml.push_str(&format!("<n{i}>"));
        }
        xml.push_str("leaf");
        for i in (0..(MAX_DEPTH - 1)).rev() {
            xml.push_str(&format!("</n{i}>"));
        }
        xml.push_str("</Root>");

        assert!(parse(&xml).is_ok());
    }

    #[test]
    fn render_escapes_ampersand_and_angle_brackets() {
        let tree = parse("<HediffDef><label>A &amp; B &lt; C</label></HediffDef>").unwrap();
        let text = render(&tree);
        assert!(text.contains("A &amp; B &lt; C"));
    }

    #[test]
    fn parse_render_parse_render_is_stable() {
        let original = r#"<HediffDef ParentName="AddedBodyPartBase">
              <defName>BionicHeart</defName>
              <label>bionic heart</label>
              <addedPartProps>
                <solid>true</solid>
                <partEfficiency>1.25</partEfficiency>
              </addedPartProps>
            </HediffDef>"#;
        let first_tree = parse(original).unwrap();
        let first_text = render(&first_tree);
        let second_tree = parse(&first_text).unwrap();
        let second_text = render(&second_tree);
        assert_eq!(first_text, second_text);
        assert_eq!(first_tree, second_tree);
    }

    #[test]
    fn render_renders_two_space_indentation_and_name_parent_name_first() {
        let tree =
            parse(r#"<HediffDef Name="X" ParentName="Y" MayRequire="z.mod"><a>1</a></HediffDef>"#)
                .unwrap();
        let text = render(&tree);
        assert_eq!(
            text,
            "<HediffDef Name=\"X\" ParentName=\"Y\" MayRequire=\"z.mod\">\n  <a>1</a>\n</HediffDef>\n"
        );
    }
}
