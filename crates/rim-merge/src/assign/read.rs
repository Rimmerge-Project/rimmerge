//! Reading an assignment instance's fields, and the inference adapter over them.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{AssignmentSchema, Cardinality, FieldOccurrence, InstanceValues};

use crate::tree::{Content, FieldNode, FieldPath, FieldTree, PathSegment};

/// Whether `path` is the one leaf `read_instance` never records: a
/// top-level `defName` — an instance's identity, always rendered from
/// [`AssignmentRow::def_name`](rim_resolve::domain::AssignmentRow::def_name),
/// never a schema field.
fn is_def_name_field(path: &FieldPath) -> bool {
    matches!(path.segments(), [PathSegment::Child(tag)] if tag == "defName")
}

/// Reads one assignment instance's `FieldTree` into
/// `rim_resolve::domain::InstanceValues`: every leaf field
/// and every `li` item ([`FieldTree::leaves`]), grouped by the
/// *container* a list item lives under rather than by each item's own
/// identity-addressed path — see [`accumulate_leaf`] for the per-leaf
/// rule, and [`is_def_name_field`] for the one leaf this skips entirely.
#[must_use]
pub fn read_instance(tree: &FieldTree) -> InstanceValues {
    let mut cardinalities: BTreeMap<FieldPath, Cardinality> = BTreeMap::new();
    let mut values: BTreeMap<FieldPath, Vec<String>> = BTreeMap::new();

    for (path, node) in tree.leaves() {
        accumulate_leaf(&path, node, &mut cardinalities, &mut values);
    }

    cardinalities
        .into_iter()
        .map(|(path, cardinality)| {
            let values = values.remove(&path).unwrap_or_default();
            (
                path,
                FieldOccurrence {
                    cardinality,
                    values,
                },
            )
        })
        .collect()
}

/// Folds one [`FieldTree::leaves`] entry into `cardinalities`/`values`: a
/// list field like `speciesNames` becomes one entry (`Cardinality::List`,
/// one value per race name, in document order — the trailing `Item`
/// segment is dropped from the key), and a nested structured field like
/// `severityCurve/points` becomes one entry at that two-segment path, not
/// one per point. A `li` item with structured (non-text) content
/// contributes no value to its container's entry, but the container is
/// still present with an empty `values` list — see
/// [`rim_resolve::domain::FieldOccurrence`]'s own doc comment. Skips
/// `defName` entirely ([`is_def_name_field`]).
fn accumulate_leaf(
    path: &FieldPath,
    node: &FieldNode,
    cardinalities: &mut BTreeMap<FieldPath, Cardinality>,
    values: &mut BTreeMap<FieldPath, Vec<String>>,
) {
    // `split_last` (rather than `segments[..segments.len() - 1]`) makes
    // "an item's field key drops its own trailing `Item` segment" hold
    // by construction instead of by a separately-checked length — a
    // length guard living apart from the arithmetic it guards is a latent
    // subtract-overflow panic.
    let segments = path.segments();
    let (field_key, is_item) = match segments.split_last() {
        Some((PathSegment::Item(_), rest)) => (FieldPath::new(rest.to_vec()), true),
        _ => (path.clone(), false),
    };
    if is_def_name_field(&field_key) {
        return;
    }

    if is_item {
        cardinalities.insert(field_key.clone(), Cardinality::List);
    } else {
        cardinalities
            .entry(field_key.clone())
            .or_insert(Cardinality::Scalar);
    }
    let entry = values.entry(field_key).or_default();
    if let Content::Text(text) = &node.content {
        entry.push(text.clone());
    }
}

/// Infers an assignment schema over every active instance of one candidate
/// assignment def type: flattens nothing itself (that's [`read_instance`]'s
/// job, already done by the caller) — thin adapter over
/// `AssignmentSchema::infer_fields`, wrapping its field map with
/// `def_type` and an empty `target_shapes` (see this module's own doc
/// comment for why `target_shapes` is populated separately).
///
/// `resolve` is a closure over a def-name index (e.g.
/// `SourceIndex::defs_by_name`) so this engine never sees the index type;
/// `dll_owner` is the same shape for the classification rules'
/// DLL-namespace signal (e.g. `SourceIndex`'s exposure of `dll_owner_of`'s rule).
/// `existing_def_type` is the same shape again for `AssignmentSchema::
/// infer_fields`'s own tag-reconstruction tie discriminator:
/// whether a type name has at least one
/// active instance anywhere in the scanned corpus (e.g. built from
/// `SourceIndex::owners_by_def`'s own type component).
#[must_use]
pub fn infer(
    def_type: &str,
    instances: &[(ModId, InstanceValues)],
    resolve: &dyn Fn(&str) -> Vec<(String, ModId)>,
    dll_owner: &dyn Fn(&str) -> Option<ModId>,
    existing_def_type: &dyn Fn(&str) -> bool,
    refs: &BTreeSet<ModId>,
) -> AssignmentSchema {
    let fields =
        AssignmentSchema::infer_fields(instances, refs, resolve, dll_owner, existing_def_type);
    AssignmentSchema {
        def_type: def_type.to_string(),
        refs: refs.clone(),
        fields,
        target_shapes: BTreeMap::new(),
    }
}
