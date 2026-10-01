//! Field rows: sub-path parsing, row classification, preference, and building the row set for each
//! conflict kind.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_analyzer::extract::xpath_expr::{self, Predicate, Step, XPathExpr};
use rim_merge::diff::{DiffClass, EntryKind, FieldDiff, ThreeWayDiff, Value};
use rim_merge::effective::{EffectiveDef, Provenance};
use rim_merge::tree::{Content, FieldPath, FieldTree, ItemId, PathSegment};
use rim_resolve::domain::{
    Action, Decision, DecisionSet, DefKey, FindingKey, MergeChoice, MergeState,
};

use super::grouping::{
    after_merge_for, after_merge_tree, drop_container_conflicts_explained_by_list_children,
    fold_duplicate_list_entries,
};
use super::{FieldRow, FieldRowKind, Preference};
use crate::merge_workspace::MergePreview;
use crate::use_cases::{DefInspection, stored_choices};

/// One `/`-separated xpath step, converted to a [`PathSegment`] —
/// duplicated from `crate::use_cases::plan_merge`'s own private
/// `segment_from_step` (the same "duplicate a small private helper
/// rather than expose it for one caller" call [`after_merge_mod`] makes:
/// the two live in different modules of this crate and neither's own
/// error type/dependencies are worth threading across for ~15 lines).
fn segment_from_step(step: &Step) -> Result<PathSegment, String> {
    if step.name != "li" {
        if !step.predicates.is_empty() {
            return Err(format!(
                "cannot represent a predicate on step '{}' as a field path",
                step.name
            ));
        }
        return Ok(PathSegment::Child(step.name.clone()));
    }
    match step.predicates.as_slice() {
        [Predicate::Attr(name, value)] if name == "Class" => {
            Ok(PathSegment::Item(ItemId::Class(value.clone())))
        }
        [Predicate::ChildText(name, value)] => Ok(PathSegment::Item(ItemId::Key {
            child: name.clone(),
            value: value.clone(),
        })),
        [Predicate::Text(value)] => Ok(PathSegment::Item(ItemId::Text(value.clone()))),
        [Predicate::Position(position)] => Ok(PathSegment::Item(ItemId::Position(
            position.saturating_sub(1),
        ))),
        other => Err(format!(
            "cannot represent li predicate {other:?} as a field path"
        )),
    }
}

/// Parses a [`FindingKey::PatchCollision`]'s own `sub_path` text into a
/// [`FieldPath`] — duplicated from `crate::use_cases::plan_merge`'s own
/// private `field_path_from_sub_path` (see [`segment_from_step`]'s doc
/// comment for why). `Err` for a sub_path outside the grammar — the same
/// case that already makes the cached [`MergePreview`] `CannotMerge`, so
/// callers here degrade to "no contested-field row from the plan", not a
/// hard failure.
pub(super) fn parse_sub_path(sub_path: &str) -> Result<FieldPath, String> {
    let synthetic = format!(r#"Defs/X[defName="x"]/{sub_path}"#);
    match xpath_expr::parse(&synthetic) {
        XPathExpr::Supported { steps, .. } => {
            let segments = steps
                .iter()
                .map(segment_from_step)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FieldPath::new(segments))
        }
        XPathExpr::DocumentRoot => Err(format!(
            "synthetic sub_path xpath unexpectedly resolved to the document root: '{sub_path}'"
        )),
        XPathExpr::Unsupported { reason } => Err(reason),
    }
}

pub(super) fn is_list_item(path: &FieldPath) -> bool {
    matches!(path.segments().last(), Some(PathSegment::Item(_)))
}

/// Reads `tree`'s value at `path` — duplicated from
/// `crate::use_cases::plan_merge`'s own private `value_of` (same
/// rationale as [`segment_from_step`]).
pub(super) fn value_at(tree: &FieldTree, path: &FieldPath) -> Value {
    match tree.get(path) {
        None => Value::Absent,
        Some(node) if is_list_item(path) => Value::Item(node.clone()),
        Some(node) => match &node.content {
            Content::Text(text) => Value::Leaf(text.clone()),
            Content::Empty => Value::Leaf(String::new()),
            Content::Children(_) => Value::Item(node.clone()),
        },
    }
}

/// Whether `path` sits strictly beneath `ancestor` — used to fall a
/// container path's own attribution back onto whatever
/// [`EffectiveDef::provenance`] actually recorded underneath it, since
/// that map is leaf-only (see its own doc comment) and never carries a
/// direct entry for a container itself.
fn is_descendant_of(path: &FieldPath, ancestor: &FieldPath) -> bool {
    let ancestor_segments = ancestor.segments();
    path.segments().len() > ancestor_segments.len()
        && path.segments()[..ancestor_segments.len()] == *ancestor_segments
}

/// Who a container path's own value should be credited to when
/// [`EffectiveDef::provenance`] has no direct entry for it: the latest
/// (`op_index`-wise) [`Provenance::Patch`] among its descendants, or else
/// whichever [`Provenance::Owner`]/[`Provenance::Inherited`] descendant
/// supplied one — so a patched-but-never-directly-keyed container
/// (`comps`, `statBases`, ...) still names *someone* instead of
/// [`in_game_at`] falling through to `None` for every container-shaped
/// path.
fn container_mod(effective: &EffectiveDef, path: &FieldPath) -> Option<ModId> {
    let mut latest_patch: Option<(usize, &ModId)> = None;
    let mut fallback_owner: Option<&ModId> = None;
    for (descendant, provenance) in &effective.provenance {
        if !is_descendant_of(descendant, path) {
            continue;
        }
        match provenance {
            Provenance::Patch { mod_id, op_index } => {
                if latest_patch.is_none_or(|(current, _)| *op_index > current) {
                    latest_patch = Some((*op_index, mod_id));
                }
            }
            Provenance::Owner(id) => {
                fallback_owner.get_or_insert(id);
            }
            Provenance::Inherited { owner, .. } => {
                fallback_owner.get_or_insert(owner);
            }
            Provenance::UnattributedTemplate { .. } => {}
        }
    }
    latest_patch
        .map(|(_, id)| id.clone())
        .or_else(|| fallback_owner.cloned())
}

/// Who, per the effective def's own provenance, last set `path` — `None`
/// when `path` isn't in the (possibly partial) resolved tree at all, or
/// when it's [`Provenance::UnattributedTemplate`] (no mod to name). A
/// container path (`comps`, `statBases`, ...) never has a direct entry of
/// its own — [`EffectiveDef::provenance`] is leaf-only — so it falls back
/// to [`container_mod`] over its own descendants instead.
fn in_game_at(effective: &EffectiveDef, path: &FieldPath) -> Option<(ModId, Value)> {
    if let Some(provenance) = effective.provenance.get(path) {
        let mod_id = match provenance {
            Provenance::Owner(id) | Provenance::Patch { mod_id: id, .. } => id.clone(),
            Provenance::Inherited { owner, .. } => owner.clone(),
            Provenance::UnattributedTemplate { .. } => return None,
        };
        return Some((mod_id, value_at(&effective.resolved, path)));
    }
    let value = value_at(&effective.resolved, path);
    if value == Value::Absent {
        return None;
    }
    let mod_id = container_mod(effective, path)?;
    Some((mod_id, value))
}

/// Every mod a [`DiffClass`] actually credits with a differing value —
/// [`FieldRow::values`]' own membership rule ("who changed what", not
/// every owner regardless of whether it differs from the base).
fn differing_members(class: &DiffClass) -> BTreeSet<ModId> {
    match class {
        DiffClass::Unchanged => BTreeSet::new(),
        DiffClass::OneSided { by } => [by.clone()].into_iter().collect(),
        DiffClass::Agreeing { by } | DiffClass::Conflict { by } => by.clone(),
    }
}

/// A [`DiffClass::Conflict`] always wins regardless of [`FieldDiff::entry`]
/// (a keyed map's own contested key stays a `Conflict` row, never
/// `MapEntry` — a keyed map with three disjoint adds and one contested
/// key yields 3 `MapEntry` rows plus 1 `Conflict` row, not 4 `MapEntry`
/// rows); otherwise `entry`'s own shape decides
/// [`FieldRowKind::MapEntry`]/[`FieldRowKind::ListEntry`]/[`FieldRowKind::CleanMerge`].
fn classify_row(field: &FieldDiff) -> FieldRowKind {
    match &field.class {
        DiffClass::Unchanged => FieldRowKind::Unchanged,
        DiffClass::Conflict { .. } => FieldRowKind::Conflict,
        DiffClass::OneSided { .. } | DiffClass::Agreeing { .. } => {
            if matches!(field.entry, EntryKind::MapEntry { .. }) {
                FieldRowKind::MapEntry
            } else if field.is_list_item {
                FieldRowKind::ListEntry
            } else {
                FieldRowKind::CleanMerge
            }
        }
    }
}

/// [`FieldRow::values`] for a [`FieldDiff`]-backed row: every differing
/// toucher's own value, in `owners_in_order`'s order (already the
/// selected order — see [`MergePreview::owners`]).
fn ordered_values(field: &FieldDiff, owners_in_order: &[ModId]) -> Vec<(ModId, Value)> {
    let members = differing_members(&field.class);
    owners_in_order
        .iter()
        .filter(|id| members.contains(id))
        .filter_map(|id| field.candidates.get(id).cloned().map(|v| (id.clone(), v)))
        .collect()
}

/// [`FieldRow::values`]/[`FieldRow::agreed_by`] for one [`FieldDiff`]-backed
/// row, splitting an [`EntryKind::MapEntry`]'s [`DiffClass::Agreeing`]
/// case per the "also added by" fold: `values` keeps only the
/// earliest contributor in `owners_in_order` (matching every other
/// `Agreeing` row's single-value display), the rest go to `agreed_by` —
/// the same "who else agrees" shape [`fold_duplicate_list_entries`]
/// already gives a `ListEntry` row, extended to a map's own key
/// agreement. Every other row (a plain leaf/list field, or a `MapEntry`
/// that isn't `Agreeing`) keeps [`ordered_values`]'s full member list in
/// `values` and an empty `agreed_by`.
fn map_entry_values_and_agreed_by(
    field: &FieldDiff,
    owners_in_order: &[ModId],
) -> (Vec<(ModId, Value)>, Vec<ModId>) {
    let values = ordered_values(field, owners_in_order);
    if !matches!(field.entry, EntryKind::MapEntry { .. })
        || !matches!(field.class, DiffClass::Agreeing { .. })
    {
        return (values, Vec::new());
    }
    let mut agreeing = values.into_iter();
    let Some(earliest) = agreeing.next() else {
        return (Vec::new(), Vec::new());
    };
    let agreed_by = agreeing.map(|(mod_id, _)| mod_id).collect();
    (vec![earliest], agreed_by)
}

fn preference_for(
    path: &FieldPath,
    winner: &ModId,
    key: &FindingKey,
    choices: &BTreeMap<FieldPath, MergeChoice>,
    decisions: &DecisionSet,
) -> Preference {
    if let Some(choice) = choices.get(path) {
        return Preference::MergeChoice {
            choice: choice.clone(),
        };
    }
    if let Some(Decision {
        action: Action::PreferWinner {
            winner: pref_winner,
            ..
        },
        ..
    }) = decisions.get(key)
    {
        return Preference::Decision {
            winner: pref_winner.clone(),
        };
    }
    Preference::LoadOrder {
        winner: winner.clone(),
    }
}

/// Every field a [`FieldDiff`]-backed diff produces (`DefOverride`, or a
/// `PatchCollision` whose own plan succeeded), in the diff's own order —
/// the shared half of [`build_def_override_fields`]/
/// [`build_patch_collision_fields`]. `owners_in_order` is
/// [`MergePreview::owners`] — already the selected order, whichever
/// participant set (every owner, or the colliding mods) this preview was
/// built from.
#[allow(
    clippy::too_many_arguments,
    reason = "each parameter is one distinct piece of the cached preview/inspection/decisions this shared row-builder joins; grouping them would just rename this same list one level down"
)]
fn rows_from_diff(
    key: &FindingKey,
    diff: &ThreeWayDiff,
    owners_in_order: &[ModId],
    winner: &ModId,
    effective: &EffectiveDef,
    choices: &BTreeMap<FieldPath, MergeChoice>,
    decisions: &DecisionSet,
    after_tree: Option<&FieldTree>,
) -> Vec<FieldRow> {
    diff.fields
        .iter()
        .map(|field| {
            let kind = classify_row(field);
            let (values, agreed_by) = map_entry_values_and_agreed_by(field, owners_in_order);
            let in_game = in_game_at(effective, &field.path);
            let after_merge = after_tree.and_then(|tree| {
                after_merge_for(diff, field, choices.get(&field.path), winner, tree)
            });
            let preference = if matches!(kind, FieldRowKind::Conflict) {
                preference_for(&field.path, winner, key, choices, decisions)
            } else {
                Preference::None
            };
            FieldRow {
                path: field.path.clone(),
                kind,
                values,
                // Non-empty only for an `Agreeing` `MapEntry` row — see
                // `map_entry_values_and_agreed_by`'s own doc comment.
                // Every other `FieldDiff`-backed row already lists every
                // agreeing toucher in `values` itself (two owners setting
                // one named field to the same value never collides at the
                // path level the way a shared map key does).
                agreed_by,
                in_game,
                after_merge,
                preference,
            }
        })
        .collect()
}

/// Context rows for a `PatchCollision` beyond its own contested
/// `sub_path` (every field any *of this collision's own* contributors'
/// ops touch, not just the contested one)
/// — read straight off the effective def's own provenance, no new IO:
/// every path attributed to a [`Provenance::Patch`] whose mod is one of
/// `mods` and isn't already covered. Each becomes a single-value row
/// (there is no per-candidate diff for a field the plan never scoped) —
/// [`FieldRowKind::ListEntry`] for a `li` item, [`FieldRowKind::CleanMerge`]
/// otherwise; `in_game` always reads the effective value, but
/// `after_merge` is `None` unless `after_tree` is `Some` (the cached
/// preview is [`MergeState::Complete`]; a `NeedsFieldInput`/`CannotMerge`
/// preview was never actually built, so claiming a merge result for it
/// would be wrong). When it *is* `Some`, the value is the same
/// effective one: nothing about a merge on the contested field changes
/// any other field. A `li` item that's a byte-identical duplicate of an
/// earlier sibling under a colliding identity gets no row of its own at
/// all — [`fold_duplicate_list_entries`] folds it into that earlier row's
/// own [`FieldRow::agreed_by`] instead (the "list case" dedup).
fn context_rows_from_provenance(
    effective: &EffectiveDef,
    mods: &BTreeSet<ModId>,
    covered: &BTreeSet<FieldPath>,
    after_tree: Option<&FieldTree>,
) -> Vec<FieldRow> {
    let (duplicate_paths, mut agreed_by) =
        fold_duplicate_list_entries(effective, covered, |provenance| match provenance {
            Provenance::Patch { mod_id, .. } if mods.contains(mod_id) => Some(mod_id.clone()),
            _ => None,
        });

    effective
        .provenance
        .iter()
        .filter(|(path, _)| !covered.contains(*path) && !duplicate_paths.contains(*path))
        .filter_map(|(path, provenance)| {
            let Provenance::Patch { mod_id, .. } = provenance else {
                return None;
            };
            if !mods.contains(mod_id) {
                return None;
            }
            let value = value_at(&effective.resolved, path);
            if value == Value::Absent {
                return None;
            }
            let kind = if is_list_item(path) {
                FieldRowKind::ListEntry
            } else {
                FieldRowKind::CleanMerge
            };
            Some(FieldRow {
                path: path.clone(),
                kind,
                values: vec![(mod_id.clone(), value.clone())],
                agreed_by: agreed_by.remove(path).unwrap_or_default(),
                in_game: Some((mod_id.clone(), value.clone())),
                after_merge: after_tree.is_some().then(|| (mod_id.clone(), value)),
                preference: Preference::None,
            })
        })
        .collect()
}

/// Fallback fields for a `DefOverride` with no [`MergePreview`] at all
/// — `PlanMerge::execute`'s own `plan_def_override` hard-errors
/// (`PlanMergeError::MissingSource`/`Inherit`) rather than caching
/// anything when an owner's own `ParentName` chain can't be resolved, and
/// an empty panel is never acceptable. The gap itself is surfaced as
/// `Problem::MissingTemplate` by [`problems_from_completeness`] regardless
/// (the
/// *same* gap `effective::compute`'s own inheritance stage independently
/// discovers in the necessarily-incomplete `TemplateSet` `InspectDef`'s
/// own, more lenient `template_chain` produces). One single-value row per
/// leaf/list item [`EffectiveDef::provenance`] actually has — the same
/// shape [`context_rows_from_provenance`] falls back to for a `CannotMerge`
/// `PatchCollision`, but sourced from *every* provenance entry (`Owner`/
/// `Inherited` included, not just `Patch`), since a `DefOverride`'s own
/// content is mostly owner-declared, never patched. No diff exists to
/// compare owners against, so every row is `CleanMerge`/`ListEntry`, never
/// `Conflict` — there's no way to tell a genuine multi-owner disagreement
/// from a single owner's own value without one. A `li` item byte-identical
/// to an earlier sibling under a colliding identity is folded into that
/// sibling's own row (see [`fold_duplicate_list_entries`]), same as
/// [`context_rows_from_provenance`]'s own dedup.
fn fields_from_provenance_only(effective: &EffectiveDef) -> Vec<FieldRow> {
    let no_coverage = BTreeSet::new();
    let (duplicate_paths, mut agreed_by) =
        fold_duplicate_list_entries(effective, &no_coverage, |provenance| match provenance {
            Provenance::Owner(id) | Provenance::Patch { mod_id: id, .. } => Some(id.clone()),
            Provenance::Inherited { owner, .. } => Some(owner.clone()),
            Provenance::UnattributedTemplate { .. } => None,
        });

    let mut rows: Vec<FieldRow> = effective
        .provenance
        .iter()
        .filter(|(path, _)| !duplicate_paths.contains(*path))
        .filter_map(|(path, provenance)| {
            let mod_id = match provenance {
                Provenance::Owner(id) | Provenance::Patch { mod_id: id, .. } => id.clone(),
                Provenance::Inherited { owner, .. } => owner.clone(),
                Provenance::UnattributedTemplate { .. } => return None,
            };
            let value = value_at(&effective.resolved, path);
            if value == Value::Absent {
                return None;
            }
            let kind = if is_list_item(path) {
                FieldRowKind::ListEntry
            } else {
                FieldRowKind::CleanMerge
            };
            Some(FieldRow {
                path: path.clone(),
                kind,
                values: vec![(mod_id.clone(), value.clone())],
                agreed_by: agreed_by.remove(path).unwrap_or_default(),
                in_game: Some((mod_id, value)),
                after_merge: None,
                preference: Preference::None,
            })
        })
        .collect();
    sort_field_rows(&mut rows, effective);
    rows
}

/// [`DefConflictKind::DefOverride`]'s own fields: entirely from the
/// cached preview's diff when one exists — a `DefOverride` *planned*
/// (`scope: None`, always true for [`Session::def_conflict_view`]'s own
/// slot) never comes back `CannotMerge` (see `crate::use_cases::plan_merge`'s
/// own `plan_def_override`: that state only arises from a patch's own
/// scope restricting participants below two, which never applies here),
/// so there is always a full diff to build every row from. `preview` is
/// `None` when planning itself couldn't even be attempted — see
/// [`fields_from_provenance_only`].
pub(super) fn build_def_override_fields(
    key: &FindingKey,
    preview: Option<&MergePreview>,
    inspection: &DefInspection,
    decisions: &DecisionSet,
) -> Vec<FieldRow> {
    let Some(preview) = preview else {
        return fields_from_provenance_only(&inspection.effective);
    };
    let choices = stored_choices(decisions, key);
    let after_tree = matches!(preview.state, MergeState::Complete { .. })
        .then(|| after_merge_tree(&preview.plan.key.def_type, &preview.diff, &choices));
    let mut rows = rows_from_diff(
        key,
        &preview.diff,
        &preview.owners,
        &preview.winner,
        &inspection.effective,
        &choices,
        decisions,
        after_tree.as_ref(),
    );
    drop_container_conflicts_explained_by_list_children(&mut rows, &inspection.effective);
    sort_field_rows(&mut rows, &inspection.effective);
    rows
}

/// [`DefConflictKind::PatchCollision`]'s own fields: the contested
/// `sub_path` from the cached preview's diff when planning succeeded
/// (`preview.diff.fields` has exactly one entry, unless `sub_path`'s own
/// node is a [`rim_merge::tree::ContainerKind::KeyedMap`], in which case
/// `rim_merge::diff::collision_fields` has already expanded it into one
/// entry per key, each its own row below, and no row for `sub_path`
/// itself exists at all; see
/// `crate::use_cases::plan_merge::plan_patch_collision`), plus every
/// other field the collision's own `mods` touch
/// ([`context_rows_from_provenance`]). When planning itself is
/// `CannotMerge` (the def's *full* active-patcher fold — every patcher,
/// not just `mods` — hit a stopper somewhere; see
/// `crate::use_cases::plan_merge::plan_patch_collision`'s own `contributions`,
/// which are never scoped to `mods` at the profile level), `preview.diff`
/// is empty, so the contested field is *always* still surfaced — with
/// `in_game: None` when the fold never reached it — since an empty panel
/// is never acceptable.
#[allow(
    clippy::too_many_arguments,
    reason = "each parameter is one distinct piece of a PatchCollision key plus the cached preview/inspection/decisions; grouping them would just rename this same list one level down"
)]
pub(super) fn build_patch_collision_fields(
    key: &FindingKey,
    preview: &MergePreview,
    inspection: &DefInspection,
    def_key: &DefKey,
    sub_path: Option<&FieldPath>,
    mods: &BTreeSet<ModId>,
    decisions: &DecisionSet,
) -> Vec<FieldRow> {
    let choices = stored_choices(decisions, key);
    let after_tree = matches!(preview.state, MergeState::Complete { .. })
        .then(|| after_merge_tree(&def_key.def_type, &preview.diff, &choices));

    let mut rows = rows_from_diff(
        key,
        &preview.diff,
        &preview.owners,
        &preview.winner,
        &inspection.effective,
        &choices,
        decisions,
        after_tree.as_ref(),
    );
    // `covered` is captured from the diff's own rows *before* the container
    // demotion below drops any of them: a dropped container-conflict row
    // is still explained (its own list children cover it), so its own
    // path must not be reconsidered by the contested-field/context logic
    // that follows — see `is_container_conflict_explained_by_list_children`'s
    // own doc comment.
    let mut covered: BTreeSet<FieldPath> = rows.iter().map(|row| row.path.clone()).collect();
    drop_container_conflicts_explained_by_list_children(&mut rows, &inspection.effective);

    // Once `collision_fields` has expanded a keyed-map
    // collision, `rows` above already carries one row per key (path
    // `sub_path/key`, tagged `EntryKind::MapEntry { container: sub_path }`
    // — see `classify_row`) and no row for `sub_path` itself; `covered`
    // alone can't see that (it holds per-key paths, never `sub_path`), so
    // the contested-field fallback below must not manufacture a spurious container-level
    // row just because `sub_path` isn't literally one of them.
    let sub_path_was_expanded = sub_path.is_some_and(|sub_path| {
        preview.diff.fields.iter().any(|field| {
            matches!(&field.entry, EntryKind::MapEntry { container } if container == sub_path)
        })
    });

    // The collision's own contested
    // field always gets its own `Conflict` row, built (and marked
    // `covered`) *before* the context rows below — never left to
    // `context_rows_from_provenance` to mis-kind as a clean, single-mod
    // context row just because one contributor's own op happened to reach
    // this exact path before some *other* mod's stopper poisoned the
    // whole preview.
    if let Some(sub_path) = sub_path
        && !covered.contains(sub_path)
        && !sub_path_was_expanded
    {
        let in_game = in_game_at(&inspection.effective, sub_path);
        rows.push(FieldRow {
            path: sub_path.clone(),
            kind: FieldRowKind::Conflict,
            values: in_game.clone().into_iter().collect(),
            agreed_by: Vec::new(),
            in_game,
            after_merge: None,
            preference: preference_for(sub_path, &preview.winner, key, &choices, decisions),
        });
        covered.insert(sub_path.clone());
    }

    rows.extend(context_rows_from_provenance(
        &inspection.effective,
        mods,
        &covered,
        after_tree.as_ref(),
    ));

    sort_field_rows(&mut rows, &inspection.effective);
    rows
}

/// "Conflicts first, then by path" — deterministic since [`FieldPath`]
/// is `Ord`. Every row within a rank is then ordered by its own position
/// in `effective.resolved`'s document order, not just `ListEntry` rows:
/// keying only `ListEntry` rows by `Option<usize>` position and leaving
/// every other kind at `None` would — since `None < Some(_)` — bunch
/// *every* list entry after *every* other row regardless of where its own
/// container actually sits, detaching it from its sibling scalar fields
/// once a page split them apart. `FieldPath`'s own `Ord` alone has the
/// same problem one level up: it compares `ItemId`s lexically
/// (`Class`/`Key`/`Text` name, then `Position`), which needn't agree with
/// where an item actually landed — two mods loading in one order can add
/// items whose `Class` names sort the other way.
pub(super) fn sort_field_rows(rows: &mut [FieldRow], effective: &EffectiveDef) {
    fn rank(kind: FieldRowKind) -> u8 {
        match kind {
            FieldRowKind::Conflict => 0,
            FieldRowKind::CleanMerge
            | FieldRowKind::ListEntry
            | FieldRowKind::MapEntry
            | FieldRowKind::Unchanged => 1,
        }
    }
    // `FieldTree::leaves()` walks `resolved` in document order, which for
    // a `li` item *is* its final list position — a ready-made position
    // index, no separate walk of each list's own children needed.
    let resolved_order: BTreeMap<FieldPath, usize> = effective
        .resolved
        .leaves()
        .enumerate()
        .map(|(index, (path, _))| (path, index))
        .collect();
    // A row whose own path isn't itself a leaf/li (a container-shaped
    // `CleanMerge`/`Conflict` row, or the `PatchCollision` contested-field
    // fallback row) uses its *first* descendant's own position instead,
    // so it still sorts alongside its own children rather than always
    // landing wherever `None`/`usize::MAX` happens to fall; a row with no
    // descendant in `resolved` at all (the real fold never reached it, or
    // it was clobbered away) sorts last, not first.
    let document_position = |row: &FieldRow| -> usize {
        if let Some(&position) = resolved_order.get(&row.path) {
            return position;
        }
        resolved_order
            .iter()
            .filter(|(path, _)| is_descendant_of(path, &row.path))
            .map(|(_, &position)| position)
            .min()
            .unwrap_or(usize::MAX)
    };
    rows.sort_by(|a, b| {
        rank(a.kind)
            .cmp(&rank(b.kind))
            .then_with(|| document_position(a).cmp(&document_position(b)))
            .then_with(|| a.path.cmp(&b.path))
    });
}
