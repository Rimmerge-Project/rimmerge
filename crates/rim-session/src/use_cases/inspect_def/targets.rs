//! Resolving the inspected def: name-only lookups, owners in load order, and child references.

use std::collections::BTreeSet;

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{LoadOrder, ModId, Selector};
use rim_resolve::domain::{DefKey, DefRef};

use super::inspection::InspectDefError;
use crate::use_cases::def_sources::DefSourceLookupError;

/// Maps [`DefSourceLookupError`] onto [`InspectDefError`] for one call
/// site — `MissingSource` becomes [`InspectDefError::NotFound`] rather
/// than a panic: every call site here has already checked `def_ref`
/// exists in [`SourceIndex`] and intersects the selected order, so this
/// arm should be unreachable in practice, but a fallible external
/// condition is never the place for `unreachable!` (the "never panic
/// across API boundaries" rule) — `NotFound` is the closest honest
/// answer if it somehow still fires.
pub(super) fn map_lookup_error(def_ref: &DefRef, error: DefSourceLookupError) -> InspectDefError {
    match error {
        DefSourceLookupError::Source(source) => InspectDefError::Source(source),
        DefSourceLookupError::Xml(message) => InspectDefError::Xml(message),
        DefSourceLookupError::MissingSource(_) => InspectDefError::NotFound(def_ref.clone()),
    }
}

/// Whether two [`DefRef`]s name the same real template registration —
/// plain `==` alone misses the case where one is [`DefRef::name_only`]
/// (`key.def_type` empty — `FindingKey::DuplicateTemplateName::def_ref()`'s
/// own shape, since `rim_analyzer`'s `Indices::template_owners` has no
/// `def_type` to give it) and the other is an ordinary, typed
/// `Selector::NameAttr` ref for the *same* `Name` (e.g. a `PatchCollision`/
/// `DefOverride` finding on that same template, which does know its own
/// `def_type`).
/// [`InspectDef::resolve_target`] already treats these two forms as the
/// same target when *resolving* it (see [`resolve_name_only_def_type`]'s
/// own doc comment) — [`current_findings`](crate::use_cases::inspect_def::current_findings) must agree, or a finding
/// attached under whichever form the analyzer happened to produce would
/// silently never show up on an inspection built from the other form. A
/// `Selector::DefName` ref is only ever compared by plain equality: a
/// name-only ref is always `Selector::NameAttr` by construction
/// (`DefRef::is_name_only`'s own doc comment).
pub(super) fn same_def_ref(a: &DefRef, b: &DefRef) -> bool {
    if a == b {
        return true;
    }
    let (named, typed) = if a.is_name_only() {
        (a, b)
    } else if b.is_name_only() {
        (b, a)
    } else {
        return false;
    };
    typed.selector == named.selector && typed.key.def_name == named.key.def_name
}

/// Which `def_type` a [`DefRef::name_only`] ref resolves to: among every
/// `(def_type, Name)` template entry sharing this `Name`, whichever one's
/// earliest active owner sits soonest in `order` overall. **This is this
/// crate's own bucketing heuristic, not a real `XmlInheritance` rule
/// applied to content resolution** (contrast [`def_sources::nearest_owner`](crate::use_cases::def_sources::nearest_owner)'s
/// own doc comment, which *is* ground-truthed against the decompiled
/// `GetBestParentFor`): the real engine's own dictionary is genuinely keyed
/// by `Name` alone, ignoring the enclosing tag (see
/// `crates/rim-analyzer/CLAUDE.md`'s and this crate's own notes on
/// `ParentName` resolution) — that half is real — but it has no `def_type`
/// concept at this stage *at all*, so "which type should our own scan treat an untyped
/// ref as" is a question the real engine never asks, and "earliest owner"
/// is simply this crate's own arbitrary, deterministic answer to it
/// (see the tie-break note below), not a claim about who wins the
/// template's own content. `None` when no def type registered this
/// `Name` at all.
///
/// **Tie-break**: two entries can share the same earliest-owner position
/// only when the *same mod* registers this `Name` under more than one
/// `def_type` (a real but rare authoring mistake) — `Iterator::min_by_key`
/// keeps the first-encountered entry on a tie, and `SourceIndex::templates`
/// is a `BTreeMap<(String, String), _>`, so iteration for a fixed `Name`
/// visits every tied `def_type` in ascending alphabetical order. The tie
/// therefore breaks alphabetically by `def_type` — an arbitrary but
/// deterministic choice for a case RimWorld's own engine can't actually
/// hit (a `Name` is looked up with no `def_type` at all, so two entries at
/// the same position are indistinguishable to it too).
fn resolve_name_only_def_type(
    sources: &SourceIndex,
    order: &LoadOrder,
    name: &str,
) -> Option<String> {
    let position_of = |id: &ModId| order.as_slice().iter().position(|o| o == id);
    sources
        .templates
        .iter()
        .filter(|((_, n), _)| n == name)
        .filter_map(|((def_type, _), owners)| {
            owners
                .iter()
                .filter_map(|(owner, _)| position_of(owner))
                .min()
                .map(|position| (position, def_type.clone()))
        })
        .min_by_key(|(position, _)| *position)
        .map(|(_, def_type)| def_type)
}

/// Every mod registering *any* `(def_type, Name)` template sharing `name`
/// — the name-only ref's own owner set, merged across every def type this
/// scan bucketed a registration of that `Name` under (see
/// [`resolve_name_only_def_type`]'s own doc comment for why merging
/// across types is the right, `DuplicateTemplateName`-consistent
/// behaviour here), ordered by `order`.
pub(super) fn name_only_owners_in_order(
    sources: &SourceIndex,
    order: &LoadOrder,
    name: &str,
) -> Vec<ModId> {
    let merged: BTreeSet<ModId> = sources
        .templates
        .iter()
        .filter(|((_, n), _)| n == name)
        .flat_map(|(_, owners)| owners.iter().map(|(owner, _)| owner.clone()))
        .collect();
    order
        .as_slice()
        .iter()
        .filter(|id| merged.contains(id))
        .cloned()
        .collect()
}

/// The [`DefRef`] for one `children_by_template` entry's `(def_type,
/// name)` key: `owners_by_def` carrying this key means some active mod
/// registers it as a concrete def ([`Selector::DefName`]); otherwise
/// it's a bare, `Name`-only template child with no `defName` of its own
/// ([`Selector::NameAttr`]) — a large share of Core's `Name` declarations
/// are themselves exactly this, and typing such a child `DefName` would
/// make it an unresolvable, dead link.
pub(super) fn child_ref(sources: &SourceIndex, key: &(String, String)) -> DefRef {
    let def_key = DefKey {
        def_type: key.0.clone(),
        def_name: key.1.clone(),
    };
    let selector = if sources.owners_by_def.contains_key(key) {
        Selector::DefName
    } else {
        Selector::NameAttr
    };
    DefRef::new(def_key, selector)
}

/// [`name_only_owners_in_order`]'s children-list sibling: every direct
/// child of *any* `(def_type, Name)` entry sharing `name`, merged and
/// deduplicated (a def could in principle appear as a child of more than
/// one same-`Name` entry — vanishingly unlikely, but the dedup costs
/// nothing), in load order.
pub(super) fn name_only_children(sources: &SourceIndex, name: &str) -> Vec<(ModId, DefRef)> {
    let mut seen: BTreeSet<(ModId, String, String)> = BTreeSet::new();
    let mut children = Vec::new();
    for ((_, n), kids) in &sources.children_by_template {
        if n != name {
            continue;
        }
        for (owner, child_key) in kids {
            let dedup_key = (owner.clone(), child_key.0.clone(), child_key.1.clone());
            if seen.insert(dedup_key) {
                children.push((owner.clone(), child_ref(sources, child_key)));
            }
        }
    }
    children
}

/// Every mod owning `(def_type, def_name)` under `selector`, in `order`.
pub(super) fn owners_in_order(
    sources: &SourceIndex,
    order: &LoadOrder,
    def_type: &str,
    def_name: &str,
    selector: Selector,
) -> Vec<ModId> {
    match selector {
        Selector::DefName => {
            let key = (def_type.to_string(), def_name.to_string());
            order
                .as_slice()
                .iter()
                .filter(|id| sources.defs.contains_key(&((*id).clone(), key.clone())))
                .cloned()
                .collect()
        }
        Selector::NameAttr => {
            let key = (def_type.to_string(), def_name.to_string());
            let Some(registrants) = sources.templates.get(&key) else {
                return Vec::new();
            };
            order
                .as_slice()
                .iter()
                .filter(|id| registrants.iter().any(|(owner, _)| owner == *id))
                .cloned()
                .collect()
        }
    }
}

/// The `(def_type, def_name, selector)` this call should actually work
/// against, plus whether it exists at all — `None` when `def_ref` names
/// nothing this scan indexed (`InspectDefError::NotFound`).
pub(super) fn resolve_target(
    sources: &SourceIndex,
    order: &LoadOrder,
    def_ref: &DefRef,
) -> Option<(String, String, Selector)> {
    if def_ref.is_name_only() {
        let def_type = resolve_name_only_def_type(sources, order, &def_ref.key.def_name)?;
        return Some((def_type, def_ref.key.def_name.clone(), Selector::NameAttr));
    }
    let key = (def_ref.key.def_type.clone(), def_ref.key.def_name.clone());
    // `owners_by_def`/`templates` are the O(log n) "does this exist at
    // all" answer `rim_analyzer::analysis::source_index::build` always
    // populates alongside the forward `defs` map a real scan produces.
    // A `Selector::DefName` ref missing from `owners_by_def` falls back
    // to `templates`: a typed ref built without checking which map
    // actually holds the key — e.g. a `children` entry naming an abstract
    // `ParentName` target, or any other caller doing the same — still
    // resolves, as a
    // `NameAttr` target, instead of `NotFound`. `Selector::NameAttr`
    // never falls back the other way: a template ref naming no concrete
    // def is expected, not a bug to route around.
    match def_ref.selector {
        Selector::DefName if sources.owners_by_def.contains_key(&key) => {
            Some((key.0, key.1, Selector::DefName))
        }
        Selector::DefName | Selector::NameAttr if sources.templates.contains_key(&key) => {
            Some((key.0, key.1, Selector::NameAttr))
        }
        Selector::DefName | Selector::NameAttr => None,
    }
}

// `representative_op` lives in `def_sources.rs` so `verify_order.rs`
// shares it — imported below as `def_sources::representative_op`.
