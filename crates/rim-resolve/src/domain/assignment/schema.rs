//! [`AssignmentSchema`]: a learned, then user-confirmed, description of
//! one assignment def type. See
//! `crate::domain::assignment`'s own module doc comment for why
//! [`AssignmentSchema::infer_fields`] is hosted here rather than in
//! `rim-merge`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use rim_analyzer::domain::ModId;
use serde::{Deserialize, Serialize};

use crate::domain::merge::{FieldPath, PathSegment};

use super::shape::TargetShape;

/// A field is a reference field (the reference gate) when at least this
/// many *distinct* values resolve to the field's voted type. Chosen
/// against a real install so a field with only a handful of resolvable
/// values — too thin a sample to trust — falls through to
/// `Scalar`/`Opaque` instead. A field whose leaf tag ends in `Def`/`Defs`
/// is exempted from this floor down to `resolved >= 1`
/// (`has_def_suffixed_leaf_tag`) — see [`classify_reference`]'s own doc
/// comment; a `defaultToolDef`-shaped field does not fall through here.
///
/// Re-exported, not redefined: `rim-analyzer`'s own
/// `analysis::references` (the dangling-def-reference check) needs the
/// identical floor for its own vote, and the crate graph is
/// `rim-analyzer` -> `rim-resolve`, so the shared constant has to live at
/// the lower layer — the same pattern `rim_merge::patch_eval::identity::
/// toggle_default` already established for a rule both `rim-analyzer` and
/// `rim-merge` need.
pub use rim_analyzer::domain::MIN_RESOLVED_DISTINCT;

/// A reference field's resolved values must be at least this share of
/// its *resolvable* ones (the reference gate) — never of every value
/// observed: an unresolvable value (a race from an inactive mod) is
/// unknown, never evidence against.
pub const TYPE_COVERAGE_MIN: f64 = 0.8;

/// A reference field is an [`FieldRole::ItemSlot`] (rather than a
/// [`FieldRole::TargetKey`]) when at least this share of its resolved
/// values' majority owner sits inside R (the inside/outside split).
pub const INSIDE_MIN: f64 = 0.5;

/// The most distinct string values a non-reference scalar field may have
/// and still be classified [`ScalarKind::Enum`] rather than
/// [`ScalarKind::Text`] (scalar classification).
pub const ENUM_MAX_DISTINCT: usize = 8;

/// Whether a field was observed as a `<li>` list or a single scalar
/// element **in one instance** — decided at flattening time (`rim-merge`'s
/// own tree walk), carried here as plain data so
/// [`AssignmentSchema::infer_fields`] can classify without ever seeing a
/// tree. A field's schema-wide [`FieldSpec::cardinality`] is a separate,
/// aggregate decision over every instance's own occurrence — see that
/// field's own doc comment for RimWorld's bare-scalar `List<T>` shorthand,
/// which this per-occurrence value alone does not capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cardinality {
    /// A single leaf value.
    Scalar,
    /// A `<li>` list; [`FieldOccurrence::values`] holds one entry per
    /// item, in document order.
    List,
}

/// One field's occurrence in one instance: its [`Cardinality`] there and
/// its text values — the one leaf value for [`Cardinality::Scalar`], one
/// entry per `<li>` for [`Cardinality::List`]. What a `rim-merge`
/// `read_instance` (a later step) flattens a
/// [`crate::domain::merge::FieldPath`] down to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldOccurrence {
    /// Whether this occurrence was a list or a single scalar.
    pub cardinality: Cardinality,
    /// The occurrence's text values.
    pub values: Vec<String>,
}

/// One assignment instance, flattened: every field path it carried, with
/// its [`FieldOccurrence`]. What [`AssignmentSchema::infer_fields`] reads
/// per instance.
pub type InstanceValues = BTreeMap<FieldPath, FieldOccurrence>;

/// A resolved (non-reference) scalar field's type, from its observed
/// values.
///
/// Tagged `"type"`, not `"kind"`: [`FieldRole::Scalar`] already has its
/// own `kind: ScalarKind` field, and this enum's internal tag sharing
/// that field's own name would read as if `kind.kind` picked the
/// variant. Nothing persisted depends on the tag's name yet (no store
/// ships this crate's JSON directly), so renaming it costs nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScalarKind {
    /// Every value is `true`/`false` (case-insensitive).
    Bool,
    /// Every value parses as a number.
    Number,
    /// At most [`ENUM_MAX_DISTINCT`] distinct string values.
    Enum {
        /// The distinct values observed.
        values: BTreeSet<String>,
    },
    /// Free text: more than [`ENUM_MAX_DISTINCT`] distinct values, or
    /// values that don't parse as `Bool`/`Number`.
    Text,
}

impl fmt::Display for ScalarKind {
    /// A stable, bounded rendering: `"Bool"`/`"Number"`/`"Text"` verbatim,
    /// `"Enum(a|b|c)"` with its distinct values joined in their own
    /// (already-sorted, since [`Self::Enum::values`] is a [`BTreeSet`])
    /// order — never `{self:?}`'s derived `Debug`, which prints
    /// `Enum { values: {"a", "b", "c"} }` verbatim and would leak that
    /// shape into a table row or a `--json` field a caller might treat as
    /// stable text (`FieldRole`'s own `Display` gives the identical
    /// reasoning for the same problem one level up).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool => f.write_str("Bool"),
            Self::Number => f.write_str("Number"),
            Self::Enum { values } => {
                write!(f, "Enum(")?;
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        write!(f, "|")?;
                    }
                    write!(f, "{value}")?;
                }
                write!(f, ")")
            }
            Self::Text => f.write_str("Text"),
        }
    }
}

/// What one field of an assignment def type means.
///
/// Tagged `"type"`, not `"role"`: [`FieldSpec`] already has its own
/// `role: FieldRole` field, and this enum's internal tag sharing that
/// field's own name would read as if `role.role` picked the variant.
/// Nothing persisted depends on the tag's name yet, so renaming it costs
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldRole {
    /// Values resolve to defs owned outside R: `def_type` is the target
    /// granularity (`ThingDef` for `speciesNames`, `PawnKindDef` for
    /// `kindNames`).
    TargetKey {
        /// The target granularity.
        def_type: String,
    },
    /// Values resolve to defs owned inside R: `def_type` feeds the item
    /// picker (`example.PartDef`, `PartTagDef`).
    ItemSlot {
        /// The item type this slot picks from.
        def_type: String,
    },
    /// A float list paired with an item slot by name
    /// (`chanceprimaryTool` ↔ `primaryTool`): edited alongside it,
    /// same length enforced ([`crate::domain::AssignmentProject::set_row`]).
    Chances {
        /// The slot field this list is paired to.
        for_slot: FieldPath,
    },
    /// Typed from the observed values, with the most common value as the
    /// default.
    Scalar {
        /// The scalar's type.
        kind: ScalarKind,
        /// The most common observed value, if any were observed.
        default: Option<String>,
    },
    /// Values that resolve to nothing and are not scalar-typed
    /// (structured children, a non-`Chances` list): shown read-only,
    /// copied verbatim by "copy from", never edited in v1.
    Opaque,
}

impl fmt::Display for FieldRole {
    /// The variant's own stable name only (`"TargetKey"`, `"ItemSlot"`,
    /// ...) — never the field contents `{self:?}` would also print, so
    /// an error message built from this (e.g.
    /// [`crate::domain::AssignmentRowError::WrongValueShape`]) stays
    /// stable regardless of a `def_type`/`kind` string's own contents.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::TargetKey { .. } => "TargetKey",
            Self::ItemSlot { .. } => "ItemSlot",
            Self::Chances { .. } => "Chances",
            Self::Scalar { .. } => "Scalar",
            Self::Opaque => "Opaque",
        };
        f.write_str(name)
    }
}

/// Every field observed on at least one instance of an assignment def
/// type, plus how the user has (or hasn't) confirmed its role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSpec {
    /// The field's current role — the inferred one, or the user's
    /// override.
    pub role: FieldRole,
    /// Whether the field is a list or a scalar, aggregated across every
    /// instance that carried it: `List` the moment *any* instance shows it
    /// `li`-wrapped — RimWorld's own `List<T>` XML deserializer accepts an
    /// unwrapped scalar element as a one-element list shorthand, a
    /// legitimate authoring convention some fields use for their sparser
    /// occurrences (verified on a real install). `Scalar` only when the
    /// field was never once observed `li`-wrapped.
    pub cardinality: Cardinality,
    /// How many instances carried the field, out of how many were read.
    pub observed: (usize, usize),
    /// `Some` when `role` was changed by the user — the *original*
    /// inferred role, kept exactly as [`AssignmentSchema::infer_fields`]
    /// first produced it, so a later re-inference (a new R) can diff
    /// against what the user actually started from rather than against
    /// whatever they last set it to. `None` for a field the schema has
    /// never had reclassified (including one the user added by hand,
    /// which was never inferred at all).
    pub inferred_role: Option<FieldRole>,
}

/// [`AssignmentSchema::confirm_role`] was asked to reclassify a field the
/// schema doesn't know about.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} is not a field of this schema")]
pub struct UnknownFieldError(pub FieldPath);

/// [`AssignmentSchema::add_field`] was asked to add a field the schema
/// already has.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} is already a field of this schema")]
pub struct FieldAlreadyExistsError(pub FieldPath);

/// A learned, then user-confirmed, description of one assignment def
/// type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentSchema {
    /// The assignment def type, e.g. `example.PartAssignmentDef`.
    pub def_type: String,
    /// The reference set the schema was inferred against.
    pub refs: BTreeSet<ModId>,
    /// Every field observed on at least one instance, by path.
    pub fields: BTreeMap<FieldPath, FieldSpec>,
    /// Per target key: the shape a candidate target must have.
    /// Populated by a caller that reads referenced targets'
    /// resolved trees and calls [`TargetShape::infer`] per
    /// [`FieldRole::TargetKey`] field — see this module's own doc
    /// comment for why that's not done inside [`Self::infer_fields`].
    pub target_shapes: BTreeMap<FieldPath, TargetShape>,
}

/// The top-level tag of a single-`Child`-segment [`FieldPath`] — the
/// shape every field of the reference fixture (`PartAssignmentDef`)
/// actually has. A path with any other shape (an item, or a nested chain) never
/// matches the [`FieldRole::Chances`] name-pairing rule, since that rule
/// is defined over sibling *top-level* fields only.
fn top_level_tag(path: &FieldPath) -> Option<&str> {
    match path.segments() {
        [PathSegment::Child(tag)] => Some(tag.as_str()),
        _ => None,
    }
}

/// Picks the key with the highest value in `counts`, breaking a tie by
/// the lexically smaller key. `counts` iterated in ascending key order
/// (a `BTreeMap`) makes this correct with a plain "strictly greater"
/// comparison: the first key reaching a given count is already the
/// smallest among any later ties. `K` is always a cheap reference
/// (`&str`, `&ModId`) in this module, never an owned allocation, so
/// `Copy` costs nothing.
fn argmax<K: Ord + Copy>(counts: &BTreeMap<K, usize>) -> Option<K> {
    let mut best: Option<(K, usize)> = None;
    for (candidate, count) in counts {
        if best.is_none_or(|(_, best_count)| *count > best_count) {
            best = Some((*candidate, *count));
        }
    }
    best.map(|(candidate, _)| candidate)
}

/// The most frequent value in `values`, tie-broken the same way
/// [`argmax`] is (lexically smaller wins).
fn mode<'a>(values: &[&'a str]) -> Option<&'a str> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for value in values {
        *counts.entry(*value).or_default() += 1;
    }
    argmax(&counts)
}

/// The mode owner among `owners` (the "majority owner" the inside/outside
/// split votes on), tie-broken by the lexically smaller [`ModId`]. Borrowing
/// version of [`majority_owner`], used internally by
/// [`classify_reference`] to avoid cloning an owner per candidate.
fn majority_owner_ref<'a>(owners: &[&'a ModId]) -> Option<&'a ModId> {
    let mut counts: BTreeMap<&ModId, usize> = BTreeMap::new();
    for owner in owners {
        *counts.entry(*owner).or_default() += 1;
    }
    argmax(&counts)
}

/// The majority owner among `owners`: the count-based vote with a
/// lexically-smaller-[`ModId`] tie-break the inside/outside split defines
/// for "majority owner (by `ModId::base()`)" —
/// [`AssignmentSchema::infer_fields`]'s own inside/outside split applies
/// it. Exposed publicly (not only
/// used internally) because `rim-merge`'s emitter needs the identical
/// vote to pick one value's dependency owner among several owners of the
/// same `defName` — call this
/// with the owners you already resolved a value's `defName` to, filtered
/// to whichever type you care about; this function does no resolving of
/// its own.
///
/// Callers compare `ModId::base()` themselves if they need `_steam`-copy
/// deduplication first; this function's tie-break operates on whatever
/// `ModId`s it's given, unmodified.
#[must_use]
pub fn majority_owner(owners: &[ModId]) -> Option<ModId> {
    let refs: Vec<&ModId> = owners.iter().collect();
    majority_owner_ref(&refs).cloned()
}

fn classify_scalar(raw_values: &[&str]) -> (ScalarKind, Option<String>) {
    if raw_values.is_empty() {
        return (ScalarKind::Text, None);
    }
    let is_bool = raw_values
        .iter()
        .all(|v| v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("false"));
    if is_bool {
        let default = mode(raw_values).map(|v| {
            if v.eq_ignore_ascii_case("true") {
                "true".to_string()
            } else {
                "false".to_string()
            }
        });
        return (ScalarKind::Bool, default);
    }
    let is_number = raw_values.iter().all(|v| v.parse::<f64>().is_ok());
    if is_number {
        return (ScalarKind::Number, mode(raw_values).map(str::to_string));
    }
    let distinct: BTreeSet<&str> = raw_values.iter().copied().collect();
    if distinct.len() <= ENUM_MAX_DISTINCT {
        return (
            ScalarKind::Enum {
                values: distinct.into_iter().map(str::to_string).collect(),
            },
            mode(raw_values).map(str::to_string),
        );
    }
    (ScalarKind::Text, mode(raw_values).map(str::to_string))
}

/// Every field's cardinality, observed count, and raw values — computed
/// once per field before any classification, so [`classify_reference`]/
/// [`classify_remaining`] never re-scan `instances`.
struct Observation<'a> {
    cardinality: Cardinality,
    observed: (usize, usize),
    raw_values: Vec<&'a str>,
}

fn observe<'a>(
    paths: &BTreeSet<FieldPath>,
    instances: &'a [(ModId, InstanceValues)],
) -> BTreeMap<FieldPath, Observation<'a>> {
    let total_instances = instances.len();
    paths
        .iter()
        .map(|path| {
            let occurrences: Vec<&FieldOccurrence> = instances
                .iter()
                .filter_map(|(_, values)| values.get(path))
                .collect();
            // RimWorld's own `List<T>` XML deserializer accepts a bare,
            // unwrapped scalar element as a one-element list shorthand
            // (verified on a real install) — so a field
            // leans `List` once instances show it `li`-wrapped, even when
            // most instances write the unwrapped shorthand instead.
            //
            // **Minimum-evidence guard**: a bare "any at all"
            // rule has no defense against a single malformed instance
            // (one mod's typo, or a hand-edited fixture) `li`-wrapping an
            // otherwise always-scalar field like `label` and flipping the
            // whole field's classification off one data point. `List`
            // requires either a majority of this field's own occurrences
            // to be `li`-wrapped, or at least two of them — two
            // independent instances agreeing is no longer "one odd
            // instance", while a genuinely sparse but *plural* wrapped
            // minority (the real-install `chance...` fields this rule
            // exists for) still classifies `List`. `MIN_RESOLVED_DISTINCT`
            // (5) is not reused here: it is a *distinct-value* evidentiary
            // bar built for the unrelated reference gate, and the
            // real-install `chance...` fields have only "a handful" of
            // `li`-wrapped occurrences among many more unwrapped ones, with
            // nothing confirming 5 or more, so a floor of 5 risks the exact
            // `Scalar` misclassification this rule exists to prevent. The
            // lower "at least two" floor closes the single-outlier gap
            // without that risk; a real-install re-measurement against this
            // exact guard remains open work (see
            // `crates/rim-resolve/CLAUDE.md`), same as the `kindNames`-branch
            // unverified note beside it. Only a field never once observed
            // `li`-wrapped is unconditionally `Scalar`.
            let wrapped_count = occurrences
                .iter()
                .filter(|o| o.cardinality == Cardinality::List)
                .count();
            let is_majority = wrapped_count * 2 > occurrences.len();
            let cardinality = if wrapped_count >= 2 || is_majority {
                Cardinality::List
            } else {
                Cardinality::Scalar
            };
            let raw_values: Vec<&str> = occurrences
                .iter()
                .flat_map(|o| o.values.iter().map(String::as_str))
                .collect();
            (
                path.clone(),
                Observation {
                    cardinality,
                    observed: (occurrences.len(), total_instances),
                    raw_values,
                },
            )
        })
        .collect()
}

/// Whether `path`'s own leaf segment is a named child whose tag ends in
/// `Def` or `Defs` (case-sensitive, e.g. `defaultToolDef` qualifies,
/// `speciesNames` does not). A known, deliberate miss: vanilla RimWorld
/// itself has 1928 lowercase `<def>` and 21 `<defs>` tags that are
/// genuine references this predicate never catches — a conservative miss
/// (falling through to the ordinary reference gate) is the safer default
/// here, not an oversight. The
/// match also requires the leaf to be a named `Child`, never a list
/// `Item` — currently a no-op condition rather than a deliberate
/// scalar-only gate: no [`InstanceValues`] key this function ever sees
/// carries a trailing `Item` segment (`rim-merge`'s own `collect_leaves`
/// never descends into an `li`, and `accumulate_leaf` strips a trailing
/// `Item` before recording a field), so don't read this as excluding
/// list paths on purpose.
///
/// [`classify_reference`] uses this to exempt such a field from
/// [`MIN_RESOLVED_DISTINCT`]'s distinct-value floor in the reference
/// gate, requiring
/// only `resolved >= 1` (`TYPE_COVERAGE_MIN` still applies unchanged) —
/// for **either** [`Cardinality::Scalar`] or [`Cardinality::List`]: this
/// function inspects only the leaf tag, never the field's cardinality,
/// and [`classify_reference`]'s own rescue branch doesn't gate on it
/// either. **Disclosed blast radius**:
/// a `Defs`-suffixed *list* field is rescued exactly the same way a
/// `Def`-suffixed scalar one is — `hediffDefs` (21 distinct values, only
/// 1 resolving on a real install) is an editable `ItemSlot`/`List` (a
/// multi-select in the row editor) rather than `Opaque` (read-only,
/// copied verbatim by "copy from") — see `crates/rim-resolve/CLAUDE.md`'s
/// own `Def`/`Defs`-suffix exemption note for the same disclosure.
///
/// On a real install, `example.PartAssignmentDef.defaultToolDef` has 23
/// distinct values but only 1 resolves — the other 22 `defName`s belong
/// to races the user doesn't have active, so the plain count gate alone
/// would classify it `Scalar{Text}` (23 distinct values, over
/// [`ENUM_MAX_DISTINCT`]) instead of the `ThingDef` picker the field's
/// own name promises.
/// `path`'s own leaf segment's tag, when the leaf is a named `Child` —
/// `None` for a list `Item` leaf (see [`has_def_suffixed_leaf_tag`]'s own
/// doc comment for why no [`InstanceValues`] key this crate ever sees
/// carries one) or an empty path. Shared by [`has_def_suffixed_leaf_tag`]
/// and [`tag_reconstructed_type`] so both read the identical segment
/// rather than each re-matching `path.segments().last()` on its own.
fn leaf_child_tag(path: &FieldPath) -> Option<&str> {
    match path.segments().last() {
        Some(PathSegment::Child(tag)) => Some(tag.as_str()),
        _ => None,
    }
}

fn has_def_suffixed_leaf_tag(path: &FieldPath) -> bool {
    leaf_child_tag(path).is_some_and(|tag| tag.ends_with("Def") || tag.ends_with("Defs"))
}

/// The candidate def type `leaf_tag`'s own root would name if the tag
/// were itself a type name rather than a role name — e.g. `letterDef` ->
/// `LetterDef`, `thingDefs` -> `ThingDef`. `None` when the tag carries
/// neither the `Def` nor `Defs` suffix, or the root is empty.
///
/// Hosted in this crate's production classifier rather than as a second,
/// independently-maintained copy in
/// `apps/cli/tests/real_install_def_suffix_exemption.rs`: that file's own
/// module doc comment has the full inspection this reconstruction is
/// based on, and calls this function directly
/// (via [`AssignmentSchema::reference_field_diagnostics`]'s own
/// `voted_type`/`role` fields, or this function itself for its
/// broader "likely-wrong" measurement) rather than keeping a third copy
/// of the same string surgery. This is the narrow, defensible half of a
/// "leaf tag vs chosen type" heuristic — most leaf tags are *role*
/// names, not type names (`postExplosionSpawnThingDef`/`filthDef`/
/// `slagDef` all correctly resolve to `ThingDef` while "disagreeing"
/// with their own stripped root), so this function's own output is
/// never trusted as a real candidate on its own — [`tag_reconstructed_type`]
/// additionally requires the reconstructed name to exist in the corpus.
/// `pub` (like [`majority_owner`]) since a caller outside this crate
/// needs the identical reconstruction for its own measurement, not only
/// for the production classifier's own use above.
#[must_use]
pub fn reconstruct_candidate_type(leaf_tag: &str) -> Option<String> {
    let root = leaf_tag
        .strip_suffix("Defs")
        .or_else(|| leaf_tag.strip_suffix("Def"))?;
    let mut chars = root.chars();
    let first = chars.next()?.to_ascii_uppercase();
    Some(format!("{first}{}Def", chars.as_str()))
}

/// The `Def`-suffix tie discriminator: a positive rule, not a
/// fall-through. Falling through to `Scalar` on a tie is wrong (on a real
/// install it flips about 70 of 234 rescued fields, many of them
/// already-correct, and demotes a `List` field to a bare-text `Scalar` —
/// structurally invalid XML, since `render_leaf`
/// (`crates/rim-merge/src/assign.rs`) emits a `Scalar` role as a bare text
/// node regardless of cardinality). Instead:
/// when `path`'s own leaf tag, stripped of its `Def`/`Defs` suffix and
/// re-capitalized ([`reconstruct_candidate_type`]), actually names a type
/// `existing_def_type` recognizes as present somewhere in the scanned
/// corpus, that reconstructed type is trusted as the field's real
/// candidate — even when the field's own vote never gave it a single
/// vote at all (a value that happens to *also* be a valid `defName` of
/// the wrong tied type carries no evidence *for* the right one; the
/// clustered `mentalBreakDef`/`needDef` misclassifications this
/// discriminator fixes are exactly this shape). `None` — meaning the
/// caller keeps whatever type the vote itself picked, never a
/// fall-through to `Scalar` — when the tag carries no `Def`/`Defs`
/// suffix at all, or the
/// reconstructed name names no real corpus type (a role name like
/// `filthDef`/`slagDef`, which must not be flagged).
fn tag_reconstructed_type(
    path: &FieldPath,
    existing_def_type: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let tag = leaf_child_tag(path)?;
    let candidate = reconstruct_candidate_type(tag)?;
    existing_def_type(&candidate).then_some(candidate)
}

/// The field-wide type vote (rule 2's setup) plus everything downstream
/// of it needs: which type won, how many distinct values resolved to it,
/// how many were resolvable at all, and whether the leaf tag itself
/// qualifies for [`has_def_suffixed_leaf_tag`]'s exemption. Split out of
/// [`classify_reference`] so [`reference_field_diagnostics`] (real-install
/// measurement only) can read the same numbers `classify_reference`
/// itself decides on, through the identical computation — never a second,
/// separately-maintained copy of the vote.
struct ReferenceVote<'a> {
    // Owned, not `&'a str`: the winning type name is read off
    // `resolved_by_value`'s own `Vec<(String, ModId)>` entries (built
    // fresh from `resolve`'s return value each call), never off
    // `observation` itself — a borrow from a value this struct also
    // owns would be self-referential, which plain Rust structs can't
    // express.
    winning_type: String,
    resolved_by_value: BTreeMap<&'a str, Vec<(String, ModId)>>,
    resolved_values: Vec<&'a str>,
    resolvable_count: usize,
    resolved_count: usize,
    is_def_suffixed: bool,
    /// The second-most-voted type's own vote count among the same
    /// field-wide tally `winning_type` was picked from by [`argmax`] —
    /// `None` when the winning type is the *only* type any value
    /// resolved to (nothing to tie against). `resolved_count` above
    /// already *is* the winning type's own vote count (every distinct
    /// value whose resolved set contains `winning_type` — see
    /// `vote_reference_type`'s own computation, which this field reuses
    /// rather than re-tallying), so no separate `winning_vote_count`
    /// field is needed alongside it.
    ///
    /// Read by [`reference_field_diagnostics`] for real-install
    /// measurement and by [`classify_reference`] (via [`is_tied`]) for the
    /// tie discriminator: see [`tag_reconstructed_type`]'s own doc comment
    /// for why a tie never simply falls through to `Scalar`.
    runner_up_vote_count: Option<usize>,
}

/// Whether `vote`'s own field-wide type tally is an exact tie — the
/// winning type's vote count does not *strictly* exceed the runner-up's.
/// `resolved_count` is never *less* than `runner_up_vote_count`
/// ([`argmax`] can never pick a non-maximal winner), so `>=` here only
/// ever fires on a genuine tie, never an impossible "runner-up ahead"
/// reading.
fn is_tied(vote: &ReferenceVote<'_>) -> bool {
    vote.runner_up_vote_count
        .is_some_and(|runner_up| runner_up >= vote.resolved_count)
}

/// `None` when nothing about this field's values resolves to any type at
/// all ([`argmax`] over an empty vote tally) — never a reference field,
/// regardless of [`has_def_suffixed_leaf_tag`].
fn vote_reference_type<'a>(
    path: &FieldPath,
    observation: &'a Observation<'a>,
    resolve: &dyn Fn(&str) -> Vec<(String, ModId)>,
) -> Option<ReferenceVote<'a>> {
    let distinct_values: BTreeSet<&str> = observation.raw_values.iter().copied().collect();
    let resolved_by_value: BTreeMap<&str, Vec<(String, ModId)>> = distinct_values
        .into_iter()
        .map(|v| (v, resolve(v)))
        .collect();

    let mut votes: BTreeMap<&str, usize> = BTreeMap::new();
    for resolved in resolved_by_value.values() {
        let distinct_types: BTreeSet<&str> = resolved.iter().map(|(t, _)| t.as_str()).collect();
        for def_type in distinct_types {
            *votes.entry(def_type).or_default() += 1;
        }
    }
    let winning_type_ref = argmax(&votes)?;
    let runner_up_vote_count = votes
        .iter()
        .filter(|(candidate, _)| **candidate != winning_type_ref)
        .map(|(_, count)| *count)
        .max();
    let winning_type = winning_type_ref.to_string();

    let resolved_values: Vec<&str> = resolved_by_value
        .iter()
        .filter(|(_, resolved)| resolved.iter().any(|(t, _)| *t == winning_type))
        .map(|(v, _)| *v)
        .collect();
    let resolvable_count = resolved_by_value.values().filter(|r| !r.is_empty()).count();
    let resolved_count = resolved_values.len();

    Some(ReferenceVote {
        winning_type,
        resolved_by_value,
        resolved_values,
        resolvable_count,
        resolved_count,
        is_def_suffixed: has_def_suffixed_leaf_tag(path),
        runner_up_vote_count,
    })
}

/// Rule 3's inside/outside split, once a field has already cleared rule
/// 2's reference gate on a real (non-exempted) sample — [`classify_reference`]'s
/// own tail, extracted so [`reference_field_diagnostics`] can call it
/// too without re-deciding rule 2 differently.
fn classify_by_ownership_share(
    vote: &ReferenceVote<'_>,
    refs: &BTreeSet<ModId>,
    dll_owner: &dyn Fn(&str) -> Option<ModId>,
) -> FieldRole {
    let inside_count = vote
        .resolved_values
        .iter()
        .filter(|v| {
            let owners: Vec<&ModId> = vote.resolved_by_value[*v]
                .iter()
                .filter(|(t, _)| *t == vote.winning_type)
                .map(|(_, owner)| owner)
                .collect();
            majority_owner_ref(&owners).is_some_and(|owner| refs.contains(&owner.base()))
        })
        .count();
    let inside_share = inside_count as f64 / vote.resolved_count as f64;
    let dll_signal =
        dll_owner(&vote.winning_type).is_some_and(|owner| refs.contains(&owner.base()));

    if inside_share >= INSIDE_MIN || dll_signal {
        FieldRole::ItemSlot {
            def_type: vote.winning_type.clone(),
        }
    } else {
        FieldRole::TargetKey {
            def_type: vote.winning_type.clone(),
        }
    }
}

/// The reference rules for one field: field-wide type voting, the
/// count-based reference gate, and the inside/outside split. `None` when
/// the field isn't a reference field at all (nothing voted, or the gate
/// rejected the winning type) — the caller then falls through to
/// [`classify_remaining`].
///
/// Resolves each of the field's distinct values exactly once
/// (`resolved_by_value`), reused for the vote tally, the reference gate,
/// and the inside-share computation — `resolve` is a caller-supplied
/// closure that may do real work (a `defs_by_name` lookup), so calling it
/// three times per value would be wasted work, not just style.
///
/// A field rescued *only* by [`has_def_suffixed_leaf_tag`]'s exemption
/// (its own `resolved` count still sits below [`MIN_RESOLVED_DISTINCT`])
/// skips the inside/outside vote entirely and classifies
/// [`FieldRole::ItemSlot`] unconditionally — for a `Def`-suffixed
/// *scalar* field or a `Defs`-suffixed *list* one alike, since the
/// exemption itself doesn't gate on [`Cardinality`] (see
/// [`has_def_suffixed_leaf_tag`]'s own doc comment): a one-value vote is
/// too thin to trust for *which* type owns the field's namespace, but a
/// low-evidence `<fooDef>`/`<fooDefs>` field is still always a value (or
/// values) the user picks, never the row's identity — and a
/// [`FieldRole::TargetKey`] is list-shaped everywhere downstream (the row
/// editor, the merge emitter), so a stray `TargetKey` here would make the
/// field unreachable rather than merely
/// mis-scoped.
///
/// **Checked before that rescue branch**: when the field-wide vote is an
/// exact tie ([`is_tied`]) and [`tag_reconstructed_type`] recognizes the
/// leaf tag as naming a real corpus type, that reconstructed type wins
/// outright — the `Def`-suffix tie discriminator, a
/// positive rule, not a fall-through — regardless of whether the field
/// is only-rescued-by-the-exemption or already clears
/// [`MIN_RESOLVED_DISTINCT`] on its own: a tie carries no real evidence
/// either way, so the tag's own signal (when it has one) always beats an
/// arbitrary `argmax` pick. `existing_def_type` is the corpus-wide
/// existence oracle this needs; see [`tag_reconstructed_type`]'s own doc
/// comment for why a purely per-field signal (the vote tally alone)
/// cannot answer this — the correct type can have *zero* votes for this
/// field's own values.
fn classify_reference(
    path: &FieldPath,
    observation: &Observation<'_>,
    refs: &BTreeSet<ModId>,
    resolve: &dyn Fn(&str) -> Vec<(String, ModId)>,
    dll_owner: &dyn Fn(&str) -> Option<ModId>,
    existing_def_type: &dyn Fn(&str) -> bool,
) -> Option<FieldRole> {
    let vote = vote_reference_type(path, observation, resolve)?;
    let min_resolved = if vote.is_def_suffixed {
        1
    } else {
        MIN_RESOLVED_DISTINCT
    };
    let is_reference = vote.resolved_count >= min_resolved
        && vote.resolvable_count > 0
        && (vote.resolved_count as f64 / vote.resolvable_count as f64) >= TYPE_COVERAGE_MIN;
    if !is_reference {
        return None;
    }

    if is_tied(&vote)
        && let Some(reconstructed) = tag_reconstructed_type(path, existing_def_type)
    {
        return Some(FieldRole::ItemSlot {
            def_type: reconstructed,
        });
    }

    // Rescued only by the exemption above — see this function's own doc
    // comment for why rule 3 is skipped rather than evaluated on a
    // one-value sample.
    if vote.is_def_suffixed && vote.resolved_count < MIN_RESOLVED_DISTINCT {
        return Some(FieldRole::ItemSlot {
            def_type: vote.winning_type.clone(),
        });
    }

    Some(classify_by_ownership_share(&vote, refs, dll_owner))
}

/// Per-field diagnostics behind [`has_def_suffixed_leaf_tag`]'s exemption,
/// for measuring its real-install blast radius.
/// Computed through the exact same [`vote_reference_type`]/
/// [`classify_by_ownership_share`] helpers [`classify_reference`] itself
/// calls, so a caller measuring the exemption's real-install reach can
/// never drift from what the real classifier decides. `#[cfg(any(test,
/// feature = "test-support"))]` only — never called from production
/// code, never shipped into the default build.
#[cfg(any(test, feature = "test-support"))]
#[derive(Debug, Clone, PartialEq)]
pub struct ReferenceFieldDiagnostics {
    /// [`has_def_suffixed_leaf_tag`]'s own verdict for this field's leaf
    /// tag.
    pub is_def_suffixed: bool,
    /// How many distinct values resolved to the field-wide winning type
    /// — `0` when nothing resolved to any type at all (rule 2 never
    /// finds a winner to vote on).
    pub resolved_count: usize,
    /// Whether the field is a reference field at all, and if so, which
    /// role rules 2-3 gave it — `None` when rule 2's gate rejects the
    /// field even under the lowered floor (a `Def`-suffixed field can
    /// still fail the [`TYPE_COVERAGE_MIN`] share check, or resolve to
    /// nothing at all).
    pub role: Option<FieldRole>,
    /// The field's aggregate [`Cardinality`] — whether a rescued field is
    /// a `Defs`-suffixed *list* (an editable multi-select rather than
    /// `Opaque`) or a `Def`-suffixed scalar one, per
    /// [`has_def_suffixed_leaf_tag`]'s own disclosed blast radius.
    pub cardinality: Cardinality,
    /// The second-most-voted type's own vote count from the same
    /// field-wide tally that picked `voted_type` — `None` when nothing
    /// resolved at all, or when the winning type was the only type any
    /// value resolved to (nothing to tie against). `resolved_count` above
    /// *is* the winning type's own vote count (see [`ReferenceVote`]'s
    /// own doc comment), so a caller comparing the two directly can tell
    /// whether the field-wide vote was an exact tie
    /// (`runner_up_vote_count == Some(resolved_count)` — `resolved_count`
    /// is never *less* than the runner-up's count, [`argmax`] picked it
    /// precisely because nothing scored higher).
    pub runner_up_vote_count: Option<usize>,
    /// The field-wide vote's own winning type, **before**
    /// [`tag_reconstructed_type`]'s tie discriminator can override it —
    /// `None` only when nothing resolved to any type at all (mirrors
    /// `resolved_count == 0`). Kept alongside `role`'s own (possibly
    /// overridden) `def_type`/`for_slot` so a caller can tell whether the
    /// discriminator actually changed this field's classification — the
    /// "before/after" comparison own "re-score
    /// the rule" ask needs, without re-deriving `classify_reference`'s
    /// decision by hand.
    pub voted_type: Option<String>,
}

#[cfg(any(test, feature = "test-support"))]
impl AssignmentSchema {
    /// [`ReferenceFieldDiagnostics`] for every field [`Self::infer_fields`]
    /// would see, keyed the same way — including fields rule 2 rejects
    /// outright (`is_def_suffixed` still reports truthfully for those;
    /// `resolved_count` is `0` when nothing resolved to any type). A
    /// field rescued *only* by [`has_def_suffixed_leaf_tag`]'s exemption
    /// is exactly `is_def_suffixed && resolved_count < MIN_RESOLVED_DISTINCT
    /// && role.is_some()`.
    #[must_use]
    pub fn reference_field_diagnostics(
        instances: &[(ModId, InstanceValues)],
        refs: &BTreeSet<ModId>,
        resolve: &dyn Fn(&str) -> Vec<(String, ModId)>,
        dll_owner: &dyn Fn(&str) -> Option<ModId>,
        existing_def_type: &dyn Fn(&str) -> bool,
    ) -> BTreeMap<FieldPath, ReferenceFieldDiagnostics> {
        let refs: BTreeSet<ModId> = refs.iter().map(ModId::base).collect();

        let mut paths: BTreeSet<FieldPath> = BTreeSet::new();
        for (_, values) in instances {
            paths.extend(values.keys().cloned());
        }
        let observations = observe(&paths, instances);

        observations
            .iter()
            .map(|(path, observation)| {
                let diagnostics = match vote_reference_type(path, observation, resolve) {
                    Some(vote) => {
                        let min_resolved = if vote.is_def_suffixed {
                            1
                        } else {
                            MIN_RESOLVED_DISTINCT
                        };
                        let is_reference = vote.resolved_count >= min_resolved
                            && vote.resolvable_count > 0
                            && (vote.resolved_count as f64 / vote.resolvable_count as f64)
                                >= TYPE_COVERAGE_MIN;
                        // Mirrors `classify_reference`'s own branch order
                        // exactly (tie discriminator, then the exemption's
                        // rescue branch, then rule 3) — see that
                        // function's own doc comment for why.
                        let role = if !is_reference {
                            None
                        } else if let Some(reconstructed) = is_tied(&vote)
                            .then(|| tag_reconstructed_type(path, existing_def_type))
                            .flatten()
                        {
                            Some(FieldRole::ItemSlot {
                                def_type: reconstructed,
                            })
                        } else if vote.is_def_suffixed
                            && vote.resolved_count < MIN_RESOLVED_DISTINCT
                        {
                            Some(FieldRole::ItemSlot {
                                def_type: vote.winning_type.clone(),
                            })
                        } else {
                            Some(classify_by_ownership_share(&vote, &refs, dll_owner))
                        };
                        ReferenceFieldDiagnostics {
                            is_def_suffixed: vote.is_def_suffixed,
                            resolved_count: vote.resolved_count,
                            role,
                            cardinality: observation.cardinality,
                            runner_up_vote_count: vote.runner_up_vote_count,
                            voted_type: Some(vote.winning_type.clone()),
                        }
                    }
                    None => ReferenceFieldDiagnostics {
                        is_def_suffixed: has_def_suffixed_leaf_tag(path),
                        resolved_count: 0,
                        role: None,
                        cardinality: observation.cardinality,
                        runner_up_vote_count: None,
                        voted_type: None,
                    },
                };
                (path.clone(), diagnostics)
            })
            .collect()
    }
}

/// The non-reference rules for one field: `Chances` when
/// every raw value parses as a float and the name pairs to a sibling
/// [`FieldRole::ItemSlot`] (already classified, in `fields`);
/// `Scalar`/`Opaque` otherwise.
fn classify_remaining(
    path: &FieldPath,
    observation: &Observation<'_>,
    fields: &BTreeMap<FieldPath, FieldSpec>,
) -> FieldRole {
    match observation.cardinality {
        Cardinality::List => {
            // Rule 4 is a *float* list paired by name — a non-numeric
            // list (structured children, free text) is never a
            // candidate regardless of its name.
            let is_float_list = !observation.raw_values.is_empty()
                && observation
                    .raw_values
                    .iter()
                    .all(|v| v.parse::<f64>().is_ok());
            if !is_float_list {
                return FieldRole::Opaque;
            }
            top_level_tag(path)
                .and_then(|tag| {
                    let lower = tag.to_ascii_lowercase();
                    let suffix = lower.strip_prefix("chance")?;
                    fields.iter().find_map(|(slot_path, spec)| {
                        let is_slot = matches!(spec.role, FieldRole::ItemSlot { .. });
                        let slot_tag = top_level_tag(slot_path)?;
                        (is_slot && slot_tag.to_ascii_lowercase() == suffix)
                            .then(|| slot_path.clone())
                    })
                })
                .map_or(FieldRole::Opaque, |for_slot| FieldRole::Chances {
                    for_slot,
                })
        }
        Cardinality::Scalar => {
            let (kind, default) = classify_scalar(&observation.raw_values);
            FieldRole::Scalar { kind, default }
        }
    }
}

impl AssignmentSchema {
    /// Infers every field's role from a reference framework's own
    /// instances — five rules (field-wide type voting, the reference gate,
    /// the inside/outside split, `Chances` pairing, and scalar
    /// classification), where the reference gate is a *count* of resolved
    /// distinct
    /// values plus type coverage among only the *resolvable* ones (an
    /// unresolvable value is unknown, never evidence against), and the
    /// type is voted *field-wide* before any per-value resolution is
    /// attempted (so a `defName` that exists under several types is
    /// classified by which type explains the whole field, not by a
    /// per-value tie-break).
    ///
    /// `refs` must already be the *effective* reference set (the
    /// selected refs plus the transitive
    /// closure of their declared `modDependencies`, Core/DLC excluded) —
    /// this function only normalises each entry to [`ModId::base`], it
    /// does not compute the closure itself (that needs the report, IO
    /// this pure crate has no way to read). A `_steam`-suffixed entry in
    /// `refs` is normalised the same way every owner comparison in this
    /// function already is, so a ref selected via its Steam copy still
    /// classifies the types it owns as [`FieldRole::ItemSlot`], not
    /// [`FieldRole::TargetKey`]: a one-sided comparison (`owner.base()`
    /// against a possibly-unnormalised `refs`) would silently misclassify a
    /// `_steam`-selected framework's own types as targets.
    ///
    /// `resolve` stands in for `defs_by_name`: every def type and owner
    /// that defines the given value (a `defName`), across every def
    /// type — a value with no entry resolves to nothing at all.
    /// `dll_owner` stands in for the analyzer's `dll_owner_of`: the
    /// reference mod (if any) whose shipped DLL owns a given def type's
    /// namespace. `existing_def_type` stands in for whether a given type
    /// name has at least one active instance anywhere in the scanned
    /// corpus (e.g. `session.sources().owners_by_def`'s own type
    /// component) — needed only by [`classify_reference`]'s tag-
    /// reconstruction tie discriminator, never
    /// by the five rules otherwise.
    ///
    /// Returns only the field map — see [`AssignmentSchema`]'s own doc
    /// comment for why `target_shapes` is a separate, IO-dependent step.
    #[must_use]
    pub fn infer_fields(
        instances: &[(ModId, InstanceValues)],
        refs: &BTreeSet<ModId>,
        resolve: &dyn Fn(&str) -> Vec<(String, ModId)>,
        dll_owner: &dyn Fn(&str) -> Option<ModId>,
        existing_def_type: &dyn Fn(&str) -> bool,
    ) -> BTreeMap<FieldPath, FieldSpec> {
        let refs: BTreeSet<ModId> = refs.iter().map(ModId::base).collect();

        let mut paths: BTreeSet<FieldPath> = BTreeSet::new();
        for (_, values) in instances {
            paths.extend(values.keys().cloned());
        }
        let observations = observe(&paths, instances);

        let mut fields = BTreeMap::new();
        // First pass: every reference field's role (rules 1-3) — item
        // slots are needed up front so the second pass can pair a
        // `Chances` list to one by name.
        let mut pending: Vec<&FieldPath> = Vec::new();
        for path in &paths {
            let Some(observation) = observations.get(path) else {
                // Unreachable: `observations` is built from this same
                // `paths` set. Skipped rather than indexed/panicking so
                // a future refactor that breaks that invariant fails
                // loudly as a missing field, not a panic.
                continue;
            };
            match classify_reference(
                path,
                observation,
                &refs,
                resolve,
                dll_owner,
                existing_def_type,
            ) {
                Some(role) => {
                    fields.insert(
                        path.clone(),
                        FieldSpec {
                            role,
                            cardinality: observation.cardinality,
                            observed: observation.observed,
                            inferred_role: None,
                        },
                    );
                }
                None => pending.push(path),
            }
        }

        // Second pass: `Chances` (rule 4, needs the item slots just
        // classified above) and `Scalar`/`Opaque` (rule 5) for every
        // field that isn't a reference field.
        for path in pending {
            let Some(observation) = observations.get(path) else {
                continue;
            };
            let role = classify_remaining(path, observation, &fields);
            fields.insert(
                path.clone(),
                FieldSpec {
                    role,
                    cardinality: observation.cardinality,
                    observed: observation.observed,
                    inferred_role: None,
                },
            );
        }

        fields
    }

    /// Whether at least one field is a [`FieldRole::TargetKey`] — the
    /// gate a def type must pass to be offered as an assignment-def
    /// candidate at all.
    #[must_use]
    pub fn has_target_key(&self) -> bool {
        self.fields
            .values()
            .any(|spec| matches!(spec.role, FieldRole::TargetKey { .. }))
    }

    /// Reclassifies `path`'s role. The *first* time a given field is
    /// reclassified, its original inferred role is captured into
    /// [`FieldSpec::inferred_role`] and never overwritten again by a
    /// later reclassification — see that field's own doc comment.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownFieldError`] when `path` isn't a field of this
    /// schema.
    pub fn confirm_role(
        &mut self,
        path: &FieldPath,
        role: FieldRole,
    ) -> Result<(), UnknownFieldError> {
        let spec = self
            .fields
            .get_mut(path)
            .ok_or_else(|| UnknownFieldError(path.clone()))?;
        if role != spec.role && spec.inferred_role.is_none() {
            spec.inferred_role = Some(spec.role.clone());
        }
        spec.role = role;
        Ok(())
    }

    /// Adds a field the XML never showed, with an explicit role the user
    /// chose from scratch — `inferred_role` stays `None`: there is no
    /// inference to diff against.
    ///
    /// # Errors
    ///
    /// Returns [`FieldAlreadyExistsError`] (storing nothing) when `path`
    /// already names a field of this schema — use
    /// [`Self::confirm_role`] to reclassify an existing one instead.
    pub fn add_field(
        &mut self,
        path: FieldPath,
        role: FieldRole,
        cardinality: Cardinality,
    ) -> Result<(), FieldAlreadyExistsError> {
        match self.fields.entry(path) {
            std::collections::btree_map::Entry::Occupied(entry) => {
                Err(FieldAlreadyExistsError(entry.key().clone()))
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(FieldSpec {
                    role,
                    cardinality,
                    observed: (0, 0),
                    inferred_role: None,
                });
                Ok(())
            }
        }
    }

    /// Removes a field from the schema ("hide a field") —
    /// a hidden field is simply absent from [`Self::fields`], so it is
    /// never rendered or editable until a later re-inference or
    /// [`Self::add_field`] brings it back.
    pub fn remove_field(&mut self, path: &FieldPath) -> Option<FieldSpec> {
        self.fields.remove(path)
    }
}
