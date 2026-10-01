//! Applying stored choices: resolving a field's chosen value and crediting the contributor.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_analyzer::extract::xpath_target;
pub use rim_resolve::domain::MergeChoice;
use rim_resolve::domain::PathSegment;

use super::Caveat;
use super::fields::{DropOutcome, plan_drop};
use super::rendering::{attrs_for_entry, build_node_for, path_is_xpath_safe, replace_or_add};
use crate::diff::{DiffClass, FieldDiff, OwnerVersion, ThreeWayDiff, Value, field_value};
use crate::patch_eval::{self, PatchContribution, ReplayContext, ReplayError};
use crate::tree::{Content, FieldPath, FieldTree};

/// What resolving one field's chosen value produced.
pub(super) enum ChoiceOutcome {
    /// A usable value — compare against the field's current natural
    /// result; if it already matches, no op is needed.
    Resolved(Value),
    /// A choice was stored, but it can't be turned into a value — the
    /// field goes to `unresolved` and `caveat` is recorded.
    Invalid(Caveat),
    /// No stored choice, and the diff class has no automatic result
    /// (`Conflict`) — the field goes to `unresolved` with no caveat (this
    /// is the ordinary "needs input" case, not a problem).
    NoChoice,
}

/// Resolves one field's chosen value: a stored choice if present,
/// otherwise the diff class's automatic result.
pub(super) fn resolve_choice(field: &FieldDiff, choice: Option<&MergeChoice>) -> ChoiceOutcome {
    if let Some(choice) = choice {
        return match choice {
            MergeChoice::From { mod_id } => match field.candidates.get(mod_id) {
                Some(value) => ChoiceOutcome::Resolved(value.clone()),
                None => ChoiceOutcome::Invalid(Caveat::UnknownOwnerChoice {
                    path: field.path.clone(),
                    mod_id: mod_id.clone(),
                }),
            },
            MergeChoice::Value { text } => match value_from_free_text(field.is_list_item, text) {
                Ok(value) => ChoiceOutcome::Resolved(value),
                Err(()) => ChoiceOutcome::Invalid(Caveat::InvalidValueFragment {
                    path: field.path.clone(),
                }),
            },
            MergeChoice::Drop => ChoiceOutcome::Resolved(Value::Absent),
        };
    }
    match &field.class {
        DiffClass::Unchanged => ChoiceOutcome::Resolved(field.base.clone()),
        DiffClass::OneSided { by } => field
            .candidates
            .get(by)
            .cloned()
            .map_or(ChoiceOutcome::NoChoice, ChoiceOutcome::Resolved),
        DiffClass::Agreeing { by } => by
            .iter()
            .next()
            .and_then(|id| field.candidates.get(id).cloned())
            .map_or(ChoiceOutcome::NoChoice, ChoiceOutcome::Resolved),
        DiffClass::Conflict { .. } => ChoiceOutcome::NoChoice,
    }
}

/// A `MergeChoice::Value` free-text choice: plain text for a leaf, an XML
/// fragment (`<li>...</li>`) for a list item.
///
/// # Errors
///
/// `Err(())` when the text isn't a leaf's plain text situation it should
/// be, or (for a list item) doesn't parse to exactly one `<li>`-tagged
/// root element.
fn value_from_free_text(is_list_item: bool, text: &str) -> Result<Value, ()> {
    if !is_list_item {
        return Ok(Value::Leaf(text.to_string()));
    }
    match crate::xml::parse(&format!("<root>{text}</root>")) {
        Ok(wrapper) => match &wrapper.root.content {
            Content::Children(children) => match children.as_slice() {
                [item] if item.tag == "li" => Ok(Value::Item(item.clone())),
                _ => Err(()),
            },
            _ => Err(()),
        },
        Err(_) => Err(()),
    }
}

/// The "final" value for a `DefOverride`: the merge's own resolved value per
/// field — [`resolve_choice`]'s result, the exact same per-field value
/// [`plan_def_override`](super::plan_def_override) itself folds into ops
/// from. A field with no resolution at all is simply
/// absent from the returned map — the caller's own "clearly-empty marker"
/// case, never [`Value::Absent`], which already means something else (a
/// field explicitly resolving to "not present"). That absence covers
/// **every** path [`plan_def_override`](super::plan_def_override) itself would abandon into
/// `unresolved` after `resolve_choice` succeeds, not just the two
/// [`ChoiceOutcome`] failure arms (checking only those two would report a
/// confident `final` for a field `merge plan` also prints as
/// `UNRESOLVED` two lines below, the exact self-contradiction this
/// column exists to remove): an unsafe xpath
/// (`!path_is_xpath_safe`, checked first, exactly as `plan_def_override`
/// checks it before ever calling `resolve_choice`), a drop
/// [`plan_drop`] can't represent ([`DropOutcome::Blocked`] —
/// [`Caveat::UnsettableLeaf`], a one-segment leaf with no container to
/// reconstruct minus itself), and a chosen value [`build_node_for`]/
/// [`replace_or_add`] can't turn into a real op
/// ([`Caveat::UnreconstructableChain`]). The last two need `winner`'s own
/// raw/resolved tree — the same one [`plan_def_override`](super::plan_def_override) itself folds
/// ops against — which is why this function takes it too, rather than
/// `diff`/`choices` alone.
///
/// A `DefOverride` has no patch-collision-style full-order replay to read
/// a "what does the game actually produce" value from at all — RimWorld
/// itself never composes two def copies, it takes the load-order winner's
/// wholesale, so the only meaningful "what would this field actually end
/// up as" answer is "whatever the completed merge decides", i.e. this
/// function's own result — see this module's own doc comment for why
/// the two kinds' "final" columns mean genuinely different things.
///
/// Deliberately a small, standalone function rather than folding this
/// into [`plan_def_override`](super::plan_def_override)'s own return type: that function has ~30
/// existing callers across this crate's own test suite alone, none of
/// which need this value, and its own [`MergePlan`](super::MergePlan) already omits a
/// field the instant its resolved value equals what's already on disk
/// (`op_count == 0` skips it entirely) — this function is the one place
/// that still reports every field's resolved value regardless of whether
/// an op was needed for it.
#[must_use]
pub fn resolved_field_values(
    diff: &ThreeWayDiff,
    winner: &OwnerVersion,
    choices: &BTreeMap<FieldPath, MergeChoice>,
) -> BTreeMap<FieldPath, Value> {
    diff.fields
        .iter()
        .filter_map(|field| {
            if !path_is_xpath_safe(&field.path) {
                return None;
            }
            let choice = choices.get(&field.path);
            let chosen = match resolve_choice(field, choice) {
                ChoiceOutcome::Resolved(value) => value,
                ChoiceOutcome::NoChoice | ChoiceOutcome::Invalid(_) => return None,
            };
            // Mirrors `plan_def_override`'s own control flow exactly, in
            // the same order, so this function abandons a field if and
            // only if that one would: a value already matching the raw
            // winner's own resolved content needs no op at all and is
            // never even offered to `plan_drop`/`replace_or_add` below —
            // reordering this check after them could report `None` for a
            // field `plan_def_override` never abandons (e.g. an inherited
            // value `replace_or_add` can't reconstruct a fresh chain for,
            // but never has to, since nothing needs writing).
            let current = field_value(&winner.resolved, &field.path, &field.entry);
            if chosen == current {
                return Some((field.path.clone(), chosen));
            }
            if chosen == Value::Absent {
                return match plan_drop(&field.path, winner, BTreeSet::new()) {
                    DropOutcome::Op(_) => Some((field.path.clone(), chosen)),
                    DropOutcome::Blocked(_) => None,
                };
            }
            let node = build_node_for(&field.path, &chosen, &attrs_for_entry(field))?;
            replace_or_add(&field.path, &winner.raw, node).map(|_op| (field.path.clone(), chosen))
        })
        .collect()
}

/// Moves `target`'s own contributions to the end of `contributions`,
/// preserving every other contribution's relative order — the
/// `candidate(target)` replay of the patch-collision rules.
///
/// This moves **every** contribution `target` has on the def being
/// replayed, not only ones that happen to touch the exact contested
/// sub-path — "M's ops on this exact target moved to the end" could be
/// read either way, and narrowing it to just
/// the matching sub-path would need each contribution's xpath parsed
/// *before* replay just to filter, which [`plan_patch_collision`]'s
/// caller (`contributions` is raw XML text, not yet parsed) can't do
/// cheaply. Reordering `target`'s unrelated ops has no effect on the
/// contested sub-path's own value unless one of those ops has a
/// structural side effect the sub-path depends on (e.g. adding the
/// container the contested field lives in) — a real but narrow edge case
/// this crate accepts rather than re-parsing every contribution twice.
pub(super) fn move_mod_last<'a>(
    contributions: &[PatchContribution<'a>],
    target: &ModId,
) -> Vec<PatchContribution<'a>> {
    let mut others = Vec::new();
    let mut mine = Vec::new();
    for contribution in contributions.iter().copied() {
        if contribution.mod_id == target {
            mine.push(contribution);
        } else {
            others.push(contribution);
        }
    }
    others.extend(mine);
    others
}

/// `mod_id`'s own contributions alone, in their given order — the input of
/// an *isolated* per-mod replay.
pub(super) fn contributions_of<'a>(
    contributions: &[PatchContribution<'a>],
    mod_id: &ModId,
) -> Vec<PatchContribution<'a>> {
    contributions
        .iter()
        .copied()
        .filter(|contribution| contribution.mod_id == mod_id)
        .collect()
}

/// Whether a replay's `caveats` record an operation of `mod_id` that
/// matched nothing.
pub(super) fn has_failed_op(caveats: &[Caveat], mod_id: &ModId) -> bool {
    caveats
        .iter()
        .any(|caveat| matches!(caveat, Caveat::FailedOp { mod_id: failed, .. } if failed == mod_id))
}

/// Whether `xpath` can reach `contested` or something beneath it: its
/// segments after the def head run through every segment of `contested`
/// (a `li[...]` step stands for any item). An xpath that names no def
/// target, or the def node itself (which contains the field), cannot be
/// proven unrelated and counts.
fn xpath_reaches(xpath: &str, contested: &FieldPath) -> bool {
    let Some(target) = xpath_target::parse_all(xpath).into_iter().next() else {
        return true;
    };
    let Some(sub_path) = target.sub_path else {
        return true;
    };
    let steps: Vec<&str> = sub_path
        .split('/')
        .map(|step| step.split('[').next().unwrap_or(step))
        .collect();
    contested.segments().len() <= steps.len()
        && contested
            .segments()
            .iter()
            .zip(&steps)
            .all(|(segment, step)| match segment {
                PathSegment::Child(tag) => tag == step,
                PathSegment::Item(_) => *step == "li",
            })
}

/// Whether a replay's `caveats` record an operation of `mod_id` that
/// matched nothing at or beneath `contested`. A failure elsewhere in the
/// def says nothing about the contested field.
pub(super) fn has_failed_op_within(
    caveats: &[Caveat],
    mod_id: &ModId,
    contested: &FieldPath,
) -> bool {
    caveats.iter().any(|caveat| {
        matches!(caveat, Caveat::FailedOp { mod_id: failed, xpath }
            if failed == mod_id && xpath_reaches(xpath, contested))
    })
}

/// What [`move_mod_last_trees`] replays: the colliding `mods`' contributions
/// against `target_raw`, judged at the `contested` field.
pub(super) struct MoveLastReplay<'a, 'c> {
    pub(super) mods: &'a [ModId],
    pub(super) contributions: &'a [PatchContribution<'c>],
    pub(super) target_raw: &'a FieldTree,
    pub(super) contested: &'a FieldPath,
}

/// Every member of `mods`' own [`move_mod_last`]-reordered full replay —
/// [`plan_patch_collision`]'s per-mod candidate strategy for an ordinary
/// (non-map) collision, and its fallback once a keyed-map candidate set
/// turns out not to be confirmed one after all (see that function's own
/// doc comment).
///
/// A member whose own operation on the contested field fails in its
/// reordered replay (another mod removed the node it targets before it
/// ran; a failure elsewhere in the def does not count) has no candidate of its
/// own there: the tree reads exactly like the remover's, and the field
/// would classify as the two *agreeing*. Its candidate is then its own
/// contributions replayed alone against the def — what its author wrote.
/// If that fails too the operation is a dead target in every order, and
/// the reordered tree stays.
///
/// # Errors
///
/// The [`ReplayError`] any member's own replay fails with.
pub(super) fn move_mod_last_trees(
    replay: &MoveLastReplay<'_, '_>,
    context: &ReplayContext<'_>,
) -> Result<Vec<(ModId, FieldTree)>, ReplayError> {
    let MoveLastReplay {
        mods,
        contributions,
        target_raw,
        contested,
    } = *replay;
    let mut trees = Vec::new();
    for mod_id in mods {
        let reordered = move_mod_last(contributions, mod_id);
        let mut outcome = patch_eval::replay(target_raw.clone(), &reordered, context);
        if let Some(error) = outcome.error {
            return Err(error);
        }
        if has_failed_op_within(&outcome.caveats, mod_id, contested) {
            let alone = patch_eval::replay(
                target_raw.clone(),
                &contributions_of(contributions, mod_id),
                context,
            );
            if let Some(error) = alone.error {
                return Err(error);
            }
            if !has_failed_op_within(&alone.caveats, mod_id, contested) {
                outcome = alone;
            }
        }
        trees.push((mod_id.clone(), outcome.tree));
    }
    Ok(trees)
}

/// The one mod [`resolve_choice`]'s own automatic result credits for
/// `class`, when it has one — [`DiffClass::OneSided`]'s own contributor,
/// or [`DiffClass::Agreeing`]'s earliest (the same one `resolve_choice`
/// itself reads via `by.iter().next()`). `None` for `Unchanged`/`Conflict`,
/// neither of which ever auto-resolves to a single contributor at all.
pub(super) fn credited_contributor(class: &DiffClass) -> Option<&ModId> {
    match class {
        DiffClass::OneSided { by } => Some(by),
        DiffClass::Agreeing { by } => by.iter().next(),
        DiffClass::Unchanged | DiffClass::Conflict { .. } => None,
    }
}

/// Resolves one patch-collision entry's chosen value: an explicit
/// per-entry choice if stored, otherwise [`resolve_choice`]'s own
/// `DiffClass`-driven automatic result (`Unchanged`/`OneSided`/`Agreeing`
/// resolve to the union, `Conflict` needs the explicit choice) —
/// identical to a def-override field in every case but one:
/// [`MergeChoice::Drop`] is always [`ChoiceOutcome::Invalid`] here
/// ([`Caveat::UnsupportedDrop`]), never
/// [`ChoiceOutcome::Resolved(Value::Absent)`] the way [`resolve_choice`]
/// treats it for a def override — RimWorld's patch language has no
/// operation that unsets a value a replay already produced, only a def
/// override's own raw/inherited structure supports that
/// (see [`plan_def_override`]'s rule 4).
pub(super) fn resolve_collision_choice(
    field: &FieldDiff,
    choice: Option<&MergeChoice>,
) -> ChoiceOutcome {
    if matches!(choice, Some(MergeChoice::Drop)) {
        return ChoiceOutcome::Invalid(Caveat::UnsupportedDrop {
            path: field.path.clone(),
        });
    }
    resolve_choice(field, choice)
}
