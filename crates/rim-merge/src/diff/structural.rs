//! Structural changes: an owner renaming, re-parenting, or re-classing a def.

use rim_analyzer::domain::ModId;

use super::field_diff::{OwnerVersion, ThreeWayDiff};
use crate::tree::{Content, FieldPath, FieldTree, PathSegment};

/// The four type-defining trigger fields: a change to any of these makes
/// composing this def override's other fields unsafe, since the
/// load-order winner's own class hierarchy may not carry whatever another
/// owner tried to add. Ordered by priority — [`structural_change`] checks
/// them in this order and reports the first one that fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StructuralField {
    /// The `thingClass` leaf field.
    ThingClass,
    /// The `ParentName` attribute — stripped onto [`FieldTree::parent_name`],
    /// so never part of [`ThreeWayDiff::fields`]; compared directly,
    /// owner to owner. A changed `ParentName` alone always triggers this — no
    /// template-equivalence check is performed, even when the two
    /// `ParentName`s happen to resolve to identical effective fields.
    ParentName,
    /// The def's own root element's `Class` attribute.
    RootClass,
    /// The `Class` attribute of an existing `comps/li` entry, matched by
    /// list position (see [`structural_change`]'s own doc comment for why
    /// position, not [`crate::tree::ItemIdentity`], is how "existing" is
    /// decided here). Only a position both owners' own `comps` list
    /// actually reaches counts — a comp only one owner has at all is an
    /// ordinary one-sided add or drop, not a class change, and never
    /// trips this.
    ///
    /// **Scoped to the literal `comps` path only, deliberately, not
    /// extended to cover `modExtensions/li[@Class=…]`** — structurally
    /// the identical failure mode (a `DefModExtension` subclass the
    /// winner's own `modExtensions` list can't carry), but the guard is
    /// defined over "an existing `comps/li`" only. A known,
    /// accepted gap, not a bug to fix quietly if rediscovered later: a
    /// def whose owners disagree only on a `modExtensions` entry's class
    /// currently field-merges without the guard firing at all.
    CompClass,
}

impl std::fmt::Display for StructuralField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ThingClass => "thingClass",
            Self::ParentName => "ParentName",
            Self::RootClass => "the def's root Class attribute",
            Self::CompClass => "a comps/li entry's Class attribute",
        })
    }
}

/// One owner's type-defining change relative to [`ThreeWayDiff::base`],
/// and which owner made it — [`structural_change`]'s own result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralChange {
    /// Which of [`StructuralField`]'s four fields triggered.
    pub field: StructuralField,
    /// The owner whose contribution differs from `diff`'s own base.
    pub by: ModId,
}

/// [`three_way`](crate::diff::field_diff::three_way)/[`structural_change`] couldn't produce an answer — `base`
/// isn't the `mod_id` of any entry in the owners given.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("base {0} is not among the given owners")]
pub struct BaseNotAnOwner(pub ModId);

/// The structural guard: whether any owner of a def override carries a
/// type-defining change relative to `diff`'s own base, and — if so — which
/// field and owner triggered it first.
///
/// **Pure and read-only, and never called by this crate itself** (never
/// wired into `plan_def_override`). `rim-session`'s `PlanMerge` is the
/// real caller, running it over the exact `ThreeWayDiff`/`OwnerVersion`s
/// it just built and forcing the preview's own `MergeState` to
/// `NeedsFieldInput` when it fires, rather than reclassifying
/// `diff.fields` here. It deliberately does not "reclassify every field
/// of a triggering def to `Conflict`": a `ParentName` difference is never
/// itself a `FieldDiff` (stripped onto `FieldTree::parent_name` before
/// `ThreeWayDiff::fields` exists at all), so a per-field rewrite has
/// nothing to reclassify for most real triggers (most real-install
/// triggers are `ParentName`), and a field nobody actually disagrees on
/// would need a lying `DiffClass::Conflict { by: {} }` to force one
/// anyway. See `rim-session`'s `state_from_plan` (`use_cases::
/// plan_merge`) for the real representation and its own justification.
/// The field/owner pair this function reports is exactly what
/// `rim-session` carries through as `MergePreview::structural_change`
/// for a caller to name; this crate makes no claim about what, if
/// anything, renders it.
///
/// Checked in the order [`StructuralField`] declares (`thingClass`,
/// `ParentName`, root `Class`, a comp's `Class`), `owners` scanned in the
/// order given (skipping `diff.base` itself) — so the result is
/// deterministic even when more than one owner or field would trigger.
/// `Ok(None)` when no owner differs from base in any of the four.
///
/// `thingClass` is read off `diff.fields` (the same three-way comparison
/// the whole preview is built from); `ParentName` and the root `Class`
/// attribute are read directly off each owner's own `raw` tree (neither
/// is a diffed field at all — see [`FieldTree`]'s own doc comment for why
/// `ParentName` never reaches [`ThreeWayDiff::fields`], and
/// [`crate::xml::parse`] never strips `Class`, so it survives on
/// `raw.root.attrs` unlike `ParentName`/`Name`/`Abstract`/`Inherit`). A
/// comp's own `Class` is read off each owner's `resolved` tree instead
/// (post-inheritance — an owner overriding the class of a comp it only
/// inherited, never declares itself, is exactly the shape this guard
/// exists to catch) and compared **by list position**, not by
/// [`crate::tree::ItemIdentity`]: `ItemIdentity` picks a `li`'s own
/// `Class` attribute as its primary identity, so an identity-addressed
/// diff can never distinguish "this existing comp's class changed" from
/// "this comp was removed and an unrelated one was added" — both produce
/// the identical shape (one path present-then-absent, a different path
/// absent-then-present). List position is the only cross-owner
/// correspondence left once identity itself is what changed.
///
/// **Disclosed limitation of positional comparison**: removing a
/// non-trailing comp shifts every later comp's own position by one, so a
/// clean removal (`[Foo, Bar, Baz]` -> `[Foo, Baz]`, `Bar` dropped) still
/// compares position 1 (`Baz` now sits where `Bar` did) as a class
/// *change* rather than the removal it really is — [`StructuralField::CompClass`]
/// fires the same way a genuine in-place class change would. This is a
/// deliberate, accepted false-attribution, not a bug to route around:
/// either shape means the two owners' `comps` lists no longer correspond
/// position-for-position, which is exactly the ambiguity this guard
/// exists to be conservative about — pinned by
/// `removing_a_non_trailing_comp_is_reported_as_a_comp_class_change`
/// below, not silently narrowed.
///
/// # Errors
///
/// [`BaseNotAnOwner`] when `diff.base` isn't the `mod_id` of any entry in
/// `owners` — the same precondition [`three_way`](crate::diff::field_diff::three_way) itself enforces. A
/// caller pairing a diff with a differently-filtered `owners` slice must
/// see this as a hard error, never a silent `Ok(None)`: in `rim-session`
/// this return value gates reclassification, and a diff whose base fell out of
/// `owners` failing open would disable the guard instead of reporting the
/// mismatch.
pub fn structural_change(
    diff: &ThreeWayDiff,
    owners: &[OwnerVersion],
) -> Result<Option<StructuralChange>, BaseNotAnOwner> {
    let base_owner = owners
        .iter()
        .find(|owner| owner.mod_id == diff.base)
        .ok_or_else(|| BaseNotAnOwner(diff.base.clone()))?;
    let others: Vec<&OwnerVersion> = owners
        .iter()
        .filter(|owner| owner.mod_id != diff.base)
        .collect();

    if let Some(field) = diff.fields.iter().find(
        |field| matches!(field.path.segments(), [PathSegment::Child(tag)] if tag == "thingClass"),
    ) {
        for owner in &others {
            if field
                .candidates
                .get(&owner.mod_id)
                .is_some_and(|value| value != &field.base)
            {
                return Ok(Some(StructuralChange {
                    field: StructuralField::ThingClass,
                    by: owner.mod_id.clone(),
                }));
            }
        }
    }

    for owner in &others {
        if owner.raw.parent_name != base_owner.raw.parent_name {
            return Ok(Some(StructuralChange {
                field: StructuralField::ParentName,
                by: owner.mod_id.clone(),
            }));
        }
    }

    let base_root_class = base_owner.raw.root.attrs.get("Class");
    for owner in &others {
        if owner.raw.root.attrs.get("Class") != base_root_class {
            return Ok(Some(StructuralChange {
                field: StructuralField::RootClass,
                by: owner.mod_id.clone(),
            }));
        }
    }

    let base_comp_classes = comp_classes(&base_owner.resolved);
    for owner in &others {
        let owner_comp_classes = comp_classes(&owner.resolved);
        if base_comp_classes
            .iter()
            .zip(owner_comp_classes.iter())
            .any(|(base_class, owner_class)| base_class != owner_class)
        {
            return Ok(Some(StructuralChange {
                field: StructuralField::CompClass,
                by: owner.mod_id.clone(),
            }));
        }
    }

    Ok(None)
}

/// Every `comps/li` entry's own `Class` attribute, in document order —
/// [`structural_change`]'s own positional comparison for
/// [`StructuralField::CompClass`]. Empty when the tree has no `comps`
/// field, or `comps` has no `li` children at all.
fn comp_classes(tree: &FieldTree) -> Vec<Option<&str>> {
    let comps_path = FieldPath::new(vec![PathSegment::Child("comps".to_string())]);
    let Some(node) = tree.get(&comps_path) else {
        return Vec::new();
    };
    let Content::Children(children) = &node.content else {
        return Vec::new();
    };
    children
        .iter()
        .filter(|child| child.tag == "li")
        .map(|child| child.attrs.get("Class").map(String::as_str))
        .collect()
}
