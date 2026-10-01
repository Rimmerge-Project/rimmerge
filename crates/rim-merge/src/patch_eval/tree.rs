//! Tree edits the operations share: replace/insert/ensure children at a path.

use std::collections::BTreeMap;

use roxmltree::Node;

use super::identity::direct_child;
use crate::tree::{Content, FieldNode, FieldPath, PathSegment, get_mut, identify_all_li};
use crate::xml;

pub(super) fn value_children(node: Node) -> Vec<FieldNode> {
    direct_child(node, "value")
        .map(|value| {
            value
                .children()
                .filter(Node::is_element)
                .filter_map(|child| xml::parse_element(child).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn split_parent(path: &FieldPath) -> (FieldPath, Option<PathSegment>) {
    let mut segments = path.segments().to_vec();
    let last = segments.pop();
    (FieldPath::new(segments), last)
}

fn find_index(children: &[FieldNode], segment: &PathSegment) -> Option<usize> {
    match segment {
        PathSegment::Child(tag) => children.iter().position(|child| &child.tag == tag),
        PathSegment::Item(item_id) => identify_all_li(children)
            .into_iter()
            .find(|(_, id)| id == item_id)
            .map(|(index, _)| index),
    }
}

pub(super) fn replace_at(root: &mut FieldNode, path: &FieldPath, values: &[FieldNode]) {
    let (parent_path, Some(last)) = split_parent(path) else {
        return;
    };
    let Some(parent) = get_mut(root, parent_path.segments()) else {
        return;
    };
    let Content::Children(children) = &mut parent.content else {
        return;
    };
    if let Some(index) = find_index(children, &last) {
        children.splice(index..=index, values.iter().cloned());
    }
}

pub(super) fn insert_sibling(
    root: &mut FieldNode,
    path: &FieldPath,
    values: &[FieldNode],
    prepend: bool,
) {
    let (parent_path, Some(last)) = split_parent(path) else {
        return;
    };
    let Some(parent) = get_mut(root, parent_path.segments()) else {
        return;
    };
    let Content::Children(children) = &mut parent.content else {
        return;
    };
    if let Some(index) = find_index(children, &last) {
        let insert_at = if prepend { index } else { index + 1 };
        for (offset, value) in values.iter().cloned().enumerate() {
            children.insert(insert_at + offset, value);
        }
    }
}

pub(super) fn ensure_children<'a>(node: &'a mut FieldNode, tag: &str) -> &'a mut Vec<FieldNode> {
    let exists = matches!(&node.content, Content::Children(children) if children.iter().any(|child| child.tag == tag));
    if !exists {
        let new_child = FieldNode {
            tag: tag.to_string(),
            attrs: BTreeMap::new(),
            content: Content::Children(Vec::new()),
        };
        match &mut node.content {
            Content::Children(children) => children.push(new_child),
            _ => node.content = Content::Children(vec![new_child]),
        }
    }
    let Content::Children(children) = &mut node.content else {
        unreachable!("just ensured Content::Children above")
    };
    let index = children
        .iter()
        .position(|child| child.tag == tag)
        .unwrap_or_else(|| unreachable!("just ensured a {tag} child above"));
    let Content::Children(inner) = &mut children[index].content else {
        unreachable!("just constructed this child as Content::Children")
    };
    inner
}
