//! Rendering confirmed sections back into `Defs/` XML.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    AssignmentRow, AssignmentSchema, Cardinality, FieldRole, FieldSpec, RowKey, RowValue, Section,
    TargetRef,
};

use crate::emit::{RenderedDefsFile, defs_file_path};
use crate::tree::{Content, FieldNode, FieldPath, PathSegment};
use crate::xml;

/// A field this render skipped rather than approximate: a field path
/// with a list-item segment among its ancestors (this engine only
/// renders a plain nested `Child` chain), an `ItemSlot` field whose
/// `Cardinality::Scalar` value carries more than one name (only one
/// name has a sensible scalar rendering), or a row with no [`TargetGate`]
/// decision at all. **Never an own-instance guard**: that check belongs
/// to the caller alone — see [`render_rows`]'s doc comment for why.
/// Surfaced by the caller as one of a later export step's own `skipped`
/// entries — never silently dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedField {
    /// The section (assignment def type) this field belongs to, since one
    /// render can cover several sections.
    pub def_type: String,
    /// The row this field belongs to, by its own [`RowKey`] (a
    /// target-keyed row's [`TargetRef`], or a free-standing row's own
    /// `defName`).
    pub row: RowKey,
    /// The field's own path — the zero-segment path for a whole-row skip
    /// (a missing [`TargetGate`] decision).
    pub path: FieldPath,
    /// Why it was skipped.
    pub reason: String,
}

/// Whether an emitted instance is gated behind `MayRequire` naming a mod,
/// or ungated (a Core target). Not a bare `Option<ModId>`: a
/// target with no entry at all in [`render_rows`]'s gate map is skipped
/// (as a [`SkippedField`]) rather than silently rendered ungated, since a
/// missing decision is not the same fact as "this target is Core".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetGate {
    /// No `MayRequire` — the target is Core.
    Core,
    /// `MayRequire` names this mod.
    Mod(ModId),
}

fn li_text(text: &str) -> FieldNode {
    FieldNode {
        tag: "li".to_string(),
        attrs: BTreeMap::new(),
        content: Content::Text(text.to_string()),
    }
}

fn list_node(tag: &str, items: Vec<FieldNode>) -> FieldNode {
    FieldNode {
        tag: tag.to_string(),
        attrs: BTreeMap::new(),
        content: Content::Children(items),
    }
}

fn text_node(tag: &str, text: &str) -> FieldNode {
    FieldNode {
        tag: tag.to_string(),
        attrs: BTreeMap::new(),
        content: Content::Text(text.to_string()),
    }
}

/// `path`'s own leaf tag plus its ancestor tags, root first — `None` if
/// the leaf or any ancestor is a list item rather than a named child:
/// this engine only renders a field path as nested plain `Child`
/// elements, never approximating an item-addressed segment by dropping
/// or flattening it.
fn split_path_tags(path: &FieldPath) -> Option<(&str, Vec<&str>)> {
    let segments = path.segments();
    let (last, ancestors) = segments.split_last()?;
    let PathSegment::Child(leaf_tag) = last else {
        return None;
    };
    let ancestor_tags = ancestors
        .iter()
        .map(|segment| match segment {
            PathSegment::Child(tag) => Some(tag.as_str()),
            PathSegment::Item(_) => None,
        })
        .collect::<Option<Vec<&str>>>()?;
    Some((leaf_tag.as_str(), ancestor_tags))
}

/// Wraps `leaf` (already tagged with a field path's own last segment)
/// inside every ancestor `Child` element the path names, innermost
/// first — a nested field's counterpart of a top-level field's own bare
/// node, e.g. `statBases/MoveSpeed` wraps a `<MoveSpeed>` leaf
/// inside a `<statBases>` element rather than rendering it at the top
/// level.
fn wrap_in_ancestors(ancestors: &[&str], leaf: FieldNode) -> FieldNode {
    ancestors.iter().rev().fold(leaf, |inner, tag| FieldNode {
        tag: (*tag).to_string(),
        attrs: BTreeMap::new(),
        content: Content::Children(vec![inner]),
    })
}

/// A short, stable name for a [`FieldRole`] variant — used only to name a
/// role/value mismatch in a [`SkippedField::reason`], never to drive
/// behaviour.
fn role_name(role: &FieldRole) -> &'static str {
    match role {
        FieldRole::TargetKey { .. } => "TargetKey",
        FieldRole::ItemSlot { .. } => "ItemSlot",
        FieldRole::Chances { .. } => "Chances",
        FieldRole::Scalar { .. } => "Scalar",
        FieldRole::Opaque => "Opaque",
    }
}

/// A short, stable name for a [`RowValue`] variant — the same purpose as
/// [`role_name`].
fn value_name(value: &RowValue) -> &'static str {
    match value {
        RowValue::Names(_) => "Names",
        RowValue::Numbers(_) => "Numbers",
        RowValue::Text(_) => "Text",
        RowValue::Omit => "Omit",
    }
}

/// Shared, per-row context [`render_field`]/[`render_leaf`] need for
/// every field of one row: which section (`def_type`, for
/// [`SkippedField::def_type`]), which row (`row_key`, for
/// [`SkippedField::row`]), and — for a target-keyed row only — the
/// [`TargetRef`] whose own key field always renders from
/// `target.def.def_name` rather than the row's own value. `None`
/// for a free-standing ([`RowKey::Own`]) row, which has no target at all.
struct FieldCtx<'a> {
    def_type: &'a str,
    row_key: &'a RowKey,
    target: Option<&'a TargetRef>,
}

/// Builds one [`SkippedField`] naming `ctx`'s own section and row.
fn skip_field(ctx: &FieldCtx, path: &FieldPath, reason: String) -> SkippedField {
    SkippedField {
        def_type: ctx.def_type.to_string(),
        row: ctx.row_key.clone(),
        path: path.clone(),
        reason,
    }
}

/// Renders one field's own (unwrapped) node from its role, cardinality,
/// and the row's value — `None` (with a [`SkippedField`] recorded) for a
/// `Cardinality::Scalar` `ItemSlot` carrying anything but exactly one
/// name, or any other role/value combination this engine doesn't
/// know how to render (never approximated) — e.g. a row's value stored
/// before the field's role was later reclassified, reachable through
/// `AssignmentProject::from_stored`, which trusts a stored row completely
/// rather than re-validating it against the current schema. `RowValue::Omit`
/// is the one value that is never a mismatch for any role: it's always an
/// intentional "use the default / omit this element". **No own-instance
/// check of any kind here**: an
/// `ItemSlot` value's names are rendered exactly as given — validating
/// whether a name is still active, or still a current own instance of
/// some other section, is `rim-session`'s `validate_and_clean_sections`
/// job alone (see [`render_rows`]'s doc comment).
fn render_leaf(
    ctx: &FieldCtx,
    path: &FieldPath,
    tag: &str,
    spec: &FieldSpec,
    value: &RowValue,
    skipped: &mut Vec<SkippedField>,
) -> Option<FieldNode> {
    match (&spec.role, value) {
        (FieldRole::ItemSlot { .. }, RowValue::Names(names)) => match spec.cardinality {
            Cardinality::List => Some(list_node(tag, names.iter().map(|n| li_text(n)).collect())),
            Cardinality::Scalar => match names.as_slice() {
                [name] => Some(text_node(tag, name)),
                _ => {
                    skipped.push(skip_field(
                        ctx,
                        path,
                        format!(
                            "scalar item slot has {} names; expected exactly one",
                            names.len()
                        ),
                    ));
                    None
                }
            },
        },
        (FieldRole::TargetKey { .. }, RowValue::Names(names)) => {
            Some(list_node(tag, names.iter().map(|n| li_text(n)).collect()))
        }
        (FieldRole::Chances { .. }, RowValue::Numbers(numbers)) => Some(list_node(
            tag,
            numbers.iter().map(|n| li_text(&n.to_string())).collect(),
        )),
        (FieldRole::Scalar { .. }, RowValue::Text(text)) => Some(text_node(tag, text)),
        (_, RowValue::Omit) => None,
        (role, value) => {
            skipped.push(skip_field(
                ctx,
                path,
                format!(
                    "{} field has a mismatched {} value",
                    role_name(role),
                    value_name(value)
                ),
            ));
            None
        }
    }
}

/// Renders one field of one row into its own (ancestor-wrapped) node, or
/// records why it was skipped instead — never both, and never
/// an approximate node. For a target-keyed row (`ctx.target` is `Some`),
/// `target.key_field` is always rendered from `target.def.def_name`,
/// overriding whatever `row.values` itself carries at that path —
/// `set_row` neither requires nor fills a row's own key value, so the
/// target's own identity is the only value this engine trusts for it. A
/// free-standing row (`ctx.target` is `None`) has no key field at all;
/// every one of its fields goes through [`render_leaf`] like any other.
fn render_field(
    ctx: &FieldCtx,
    path: &FieldPath,
    spec: &FieldSpec,
    row: &AssignmentRow,
    skipped: &mut Vec<SkippedField>,
) -> Option<FieldNode> {
    let Some((tag, ancestors)) = split_path_tags(path) else {
        skipped.push(skip_field(
            ctx,
            path,
            "field path addresses a list item, not a named child; cannot render".to_string(),
        ));
        return None;
    };

    if let Some(target) = ctx.target
        && path == &target.key_field
    {
        let leaf = list_node(tag, vec![li_text(&target.def.def_name)]);
        return Some(wrap_in_ancestors(&ancestors, leaf));
    }

    let value = row.values.get(path)?;
    let leaf = render_leaf(ctx, path, tag, spec, value, skipped)?;
    Some(wrap_in_ancestors(&ancestors, leaf))
}

/// One row's own `<D>...</D>` element: `defName` first, then every field
/// `schema` knows in schema field order (each via [`render_field`]) —
/// `attrs` carries `MayRequire` naming `target_mod`'s base id, when
/// given (never, for a free-standing row: `ctx.target`/`target_mod` are
/// both `None` there).
fn render_row_node(
    ctx: &FieldCtx,
    schema: &AssignmentSchema,
    row: &AssignmentRow,
    target_mod: Option<&ModId>,
    skipped: &mut Vec<SkippedField>,
) -> FieldNode {
    let mut attrs = BTreeMap::new();
    if let Some(mod_id) = target_mod {
        attrs.insert("MayRequire".to_string(), mod_id.base().as_str().to_string());
    }

    let mut children = vec![text_node("defName", &row.def_name)];
    for (path, spec) in &schema.fields {
        if let Some(node) = render_field(ctx, path, spec, row, skipped) {
            children.push(node);
        }
    }

    FieldNode {
        tag: schema.def_type.clone(),
        attrs,
        content: Content::Children(children),
    }
}

/// Renders one [`Section`]'s confirmed schema and rows into one
/// `Defs/rimmerge_<DefType>.xml` file, instances sorted by `def_name`
/// (deterministic regardless of `section.rows`' own order). Dispatches
/// per row on its own [`RowKey`] — a [`RowKey::Target`] row renders its
/// key field from the target and carries `MayRequire` from `gates`; a
/// [`RowKey::Own`] (free-standing) row has no key field and is never
/// gated at all (a standalone row has no target to gate against). No
/// file — but any [`SkippedField`]s are still returned — when
/// `section.rows` is empty, or when every row ended up skipped (the
/// missing-gate case, most notably): no file for a section with nothing
/// to emit, mirroring `emit`'s own `render_patches_file` convention of
/// producing no file for zero blocks.
///
/// `gates` maps each target-keyed row's own [`TargetRef`] to its
/// [`TargetGate`]; a target with no entry is skipped entirely
/// rather than rendered as though it were Core — free-standing rows
/// never consult `gates` at all. A field this engine can't safely render
/// is skipped rather than approximated. Every skip is returned
/// alongside the rendered files for the caller to fold into a later
/// export step's own `skipped` list.
///
/// **This function performs no own-instance/active-def validation of any
/// `ItemSlot` value.** The sole real caller (`rim-session`'s
/// `ExportAssignment::render_defs`) runs `validate_and_clean_sections`
/// first: that function strips any `ItemSlot` name that is neither a
/// currently active def (`SessionKnownDefs::contains`) nor a current own
/// instance of the referenced type (`SessionKnownDefs::own_instances`)
/// *before* this function ever sees the row. Every name that survives
/// that check is, by construction, either active or a current own
/// instance. A guard here could only ever list own-instance names (this
/// pure engine has no active-install index to know about the other
/// half), so it would reject the wholly legitimate "active but not own"
/// case as a false "dangling own reference" — breaking a core promise of
/// assignment projects: referencing a project's own new instance
/// *alongside* external instances of the same type. It would also catch
/// nothing `validate_and_clean_sections` doesn't already catch: both read
/// the identical `Section::own_instance_names()` for the "own" half, and
/// the "active" half only `rim-session` can know at all. The
/// force-removed-section case is proven by `rim-session`'s own
/// `a_reference_to_a_force_removed_sections_own_row_is_skipped_with_a_reason`
/// test.
#[must_use]
pub fn render_rows(
    section: &Section,
    gates: &BTreeMap<TargetRef, TargetGate>,
) -> (Vec<RenderedDefsFile>, Vec<SkippedField>) {
    if section.rows.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let def_type = &section.schema.def_type;
    let mut sorted: Vec<(&RowKey, &AssignmentRow)> = section.rows.iter().collect();
    sorted.sort_by(|(_, a), (_, b)| a.def_name.cmp(&b.def_name));

    let mut skipped = Vec::new();
    let mut rendered_any = false;
    let mut text = String::new();
    text.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    text.push_str("<Defs>\n");
    for (key, row) in sorted {
        let Some(node) = render_one_row(def_type, &section.schema, key, row, gates, &mut skipped)
        else {
            continue;
        };
        text.push_str(&xml::render_node(&node, 1));
        rendered_any = true;
    }
    text.push_str("</Defs>\n");

    if !rendered_any {
        return (Vec::new(), skipped);
    }

    let files = vec![RenderedDefsFile {
        relative_path: defs_file_path(def_type),
        content: text,
    }];
    (files, skipped)
}

/// One row's own dispatch on its [`RowKey`] — [`render_rows`]'s per-row step,
/// extracted so that function stays single-level like
/// [`render_field`]/[`render_leaf`]. `None` when a [`RowKey::Target`] row has
/// no [`TargetGate`] decision at all — recorded as a whole-row
/// [`SkippedField`] rather than rendered ungated; a [`RowKey::Own`]
/// (free-standing) row never consults `gates` and always renders.
fn render_one_row(
    def_type: &str,
    schema: &AssignmentSchema,
    key: &RowKey,
    row: &AssignmentRow,
    gates: &BTreeMap<TargetRef, TargetGate>,
    skipped: &mut Vec<SkippedField>,
) -> Option<FieldNode> {
    match key {
        RowKey::Target(target) => {
            let Some(gate) = gates.get(target) else {
                skipped.push(SkippedField {
                    def_type: def_type.to_string(),
                    row: key.clone(),
                    path: FieldPath::new(Vec::new()),
                    reason: "no MayRequire gate decision for this target".to_string(),
                });
                return None;
            };
            let target_mod = match gate {
                TargetGate::Core => None,
                TargetGate::Mod(mod_id) => Some(mod_id),
            };
            let ctx = FieldCtx {
                def_type,
                row_key: key,
                target: Some(target),
            };
            Some(render_row_node(&ctx, schema, row, target_mod, skipped))
        }
        RowKey::Own(_) => {
            let ctx = FieldCtx {
                def_type,
                row_key: key,
                target: None,
            };
            Some(render_row_node(&ctx, schema, row, None, skipped))
        }
    }
}

/// Renders every section of a multi-section assignment project
/// (typically `AssignmentProject::sections()`) into its own `Defs/<DefType>.xml`
/// file: one [`render_rows`] call per [`Section`], in the map's own
/// `BTreeMap` order (by def type), folding every section's own
/// [`SkippedField`]s into one list. `gates` is shared unchanged across
/// every section — an `ItemSlot` value in any one section can name
/// another section's own free-standing row (the point of sections), so
/// there is exactly one gate map for the whole render, never one per
/// section.
#[must_use]
pub fn render_sections(
    sections: &BTreeMap<String, Section>,
    gates: &BTreeMap<TargetRef, TargetGate>,
) -> (Vec<RenderedDefsFile>, Vec<SkippedField>) {
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for section in sections.values() {
        let (section_files, section_skipped) = render_rows(section, gates);
        files.extend(section_files);
        skipped.extend(section_skipped);
    }
    (files, skipped)
}
