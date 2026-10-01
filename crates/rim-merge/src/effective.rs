//! `rim_merge::effective`: the game's own pipeline over one contested
//! def, attributed field-by-field.
//!
//! Unlike [`crate::plan`], which folds a three-way diff plus a user's
//! *choices* into a patch plan (Rimmerge's proposal), this module makes no
//! choices at all: it replays every active mod's patch contributions
//! against the winning owner's raw node, in load order, then resolves
//! `ParentName` inheritance — exactly the sequence RimWorld itself runs
//! (last owner wins -> patches -> inheritance) — and records which stage
//! last touched each field of the result. Pure, like every other module
//! here: the caller (`rim-session`) hands in already-read XML text and an
//! already-built [`crate::inherit::TemplateSet`]; this module does no IO.
//!
//! Reuses [`crate::patch_eval::replay`] one contribution at a time
//! (folding, not a second applier) for the patch stage, and for the
//! inheritance stage folds [`crate::inherit::ancestor_chain`] the same way
//! [`crate::inherit::resolve`] does, calling its real
//! [`crate::inherit::merge_over`] for the tree it returns while tracking a
//! `Shadow` — a value-free mirror of the same tree, carrying a
//! [`Provenance`] at every leaf position instead of field data — in
//! lockstep, via its own `merge_shadow` (kept in sync with
//! [`crate::inherit::merge_over`]'s own branches by hand; see that
//! function's doc comment). A path-keyed diff of before/after leaf sets
//! cannot be made to agree with [`crate::tree::FieldTree::leaves`] in
//! general: it can't tell a same-valued redeclaration from "nothing
//! changed" (a middle template re-declaring a parent's field with the
//! identical value renders identically, so nothing "changed" to diff),
//! and it can't survive a `li` identity that only collides once a later
//! layer's own item is appended — a positional shadow, walked with the
//! exact same [`crate::tree::identify_all_li`] call
//! [`crate::tree::FieldTree::leaves`] itself uses, survives both because
//! it never depends on the *value* looking different, only on *which*
//! layer's node ended up at a given position. See [`fold_inheritance`]'s
//! doc comment for the fold in full.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::DefKey;

use crate::inherit::{InheritError, TemplateSet};
use crate::patch_eval::{self, PatchContribution, ReplayContext, ReplayError};
use crate::plan::Caveat;
use crate::tree::{FieldPath, FieldTree};
use fold::{fold_inheritance, owned_leaves, renamed_list_item_source};

mod blocks;
mod counterfactual;
mod fold;

pub use blocks::{ContributionBlock, contribution_blocks, duplicate_of_earlier_sibling};
pub use counterfactual::{CounterfactualFix, CounterfactualOutcome, counterfactual};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "effective/effective_tests.rs"]
mod tests;

/// Everything [`compute`] needs to run the game's pipeline over one
/// contested def.
#[derive(Debug, Clone)]
pub struct EffectiveInput<'a> {
    /// The winning owner: the last owner of this def/template in the
    /// selected order. RimWorld replaces a same-named def wholesale, so
    /// there is exactly one raw node to start from, never a merge of
    /// several owners' nodes.
    pub winner: &'a ModId,
    /// The winning owner's raw node, exactly as read from its file (not
    /// `ParentName`-resolved — that happens last, per this module's own
    /// pipeline order).
    pub raw: FieldTree,
    /// Every active mod's top-level patch operations aimed at this def,
    /// in the selected (load) order — the same slice shape
    /// [`crate::plan::plan_patch_collision`] takes, but *every*
    /// contribution on the def, not one collision's worth.
    pub contributions: &'a [PatchContribution<'a>],
    /// The active install, for gate evaluation, plus this def's own
    /// identity so a contribution aimed at a different def is recognized
    /// and skipped rather than mis-applied — see [`ReplayContext`].
    pub context: ReplayContext<'a>,
    /// Every template a `ParentName` chain from `raw` can reach.
    pub templates: &'a TemplateSet,
    /// Which mod registered each template `templates` holds, keyed the
    /// same way ([`(def_type, Name)`]) — [`TemplateSet`] itself is
    /// deliberately owner-agnostic (`crate::inherit`'s own doc comment:
    /// "doesn't care where the trees came from"), so [`Provenance::Inherited`]
    /// needs this alongside it. Built by the session from
    /// `SourceIndex::templates`' first owner under the selected order;
    /// a template not present here (should not happen once the
    /// session wires it, but this crate never guesses an owner it wasn't
    /// given) attributes that template's own fields to
    /// [`Provenance::UnattributedTemplate`] instead of silently leaving
    /// them out of [`EffectiveDef::provenance`].
    pub template_owners: &'a BTreeMap<(String, String), ModId>,
}

/// Which stage of the pipeline last set one field of [`EffectiveDef::resolved`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// Present in the winner's own raw node, untouched by any later stage.
    Owner(ModId),
    /// Last set by this mod's patch operation — `op_index` is the
    /// operation's own index in [`EffectiveInput::contributions`].
    Patch {
        /// The mod that shipped the operation.
        mod_id: ModId,
        /// The operation's index in [`EffectiveInput::contributions`].
        op_index: usize,
    },
    /// Came from a `ParentName` template, owned by this mod — neither the
    /// winner's raw node nor any patch overrode it.
    Inherited {
        /// The template that supplied this field (`(def_type, Name)`,
        /// carried as a [`DefKey`] since the two are addressed the same
        /// way).
        template: DefKey,
        /// The mod that registered that template.
        owner: ModId,
    },
    /// Came from a `ParentName` template like [`Self::Inherited`], but
    /// [`EffectiveInput::template_owners`] has no entry for it — the
    /// session should always wire one (see that field's own doc
    /// comment), but guessing an owner here would misattribute a real
    /// mod's own work, so the gap is named instead.
    UnattributedTemplate {
        /// The template that supplied this field.
        template: DefKey,
    },
}

/// Why a replay of every contribution, or the `ParentName` chain itself,
/// couldn't be carried all the way through — see [`Completeness::Partial`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stopper {
    /// A contribution's operation fell outside [`patch_eval`]'s replay
    /// semantics, or wasn't well-formed XML.
    Replay {
        /// The mod that shipped the operation.
        mod_id: ModId,
        /// The operation's index in [`EffectiveInput::contributions`].
        op_index: usize,
        /// Why [`patch_eval::replay`] stopped.
        error: ReplayError,
    },
    /// The winner's `ParentName` chain itself couldn't be resolved (a
    /// missing template, or a cycle) — recorded only when every
    /// contribution replayed cleanly; a chain this broken is the
    /// install's problem, not this replay's, but nothing past it is
    /// guessed either way.
    Inherit(InheritError),
}

impl std::fmt::Display for Stopper {
    /// Names which mod's own operation (or, for [`Self::Inherit`], the
    /// `ParentName` chain itself) stopped the pipeline, and why — the one
    /// canonical rendering every caller should use (`VerifyOrder`'s own
    /// `skipped` entries read this directly). `apps/cli`'s own
    /// `defs inspect` carries a hand-written `format_stopper` with
    /// identical wording.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Replay {
                mod_id,
                op_index,
                error,
            } => write!(f, "{mod_id}'s operation #{op_index} — {error}"),
            Self::Inherit(error) => write!(f, "inheritance — {error}"),
        }
    }
}

/// Whether [`compute`] made it through every stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Completeness {
    /// Every contribution replayed and the `ParentName` chain resolved.
    Complete,
    /// Stopped at [`Stopper`] — every stage before it is attributed in
    /// [`EffectiveDef::provenance`], and [`EffectiveDef::resolved`] is the tree
    /// as of the last good stage ("never skip a stopper and continue" — a later
    /// field's true owner can't be known without first knowing what the skipped
    /// contribution would have done to it).
    Partial {
        /// Where the pipeline stopped.
        stopped_at: Stopper,
    },
}

/// What running the game's own pipeline over one contested def produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveDef {
    /// The tree the game uses under the selected order: raw -> patched ->
    /// inherited. When [`Self::completeness`] is [`Completeness::Partial`],
    /// this is the tree as of the last stage that completed cleanly.
    pub resolved: FieldTree,
    /// Every leaf and list item of [`Self::resolved`], attributed to
    /// whichever stage last set it. A field [`crate::patch_eval`] removed,
    /// or that inheritance folded away (e.g. an empty leaf container
    /// replaced by the inherited children it never overrode — see
    /// [`compute`]'s doc comment), simply isn't a key here: this map
    /// mirrors `resolved`'s own leaves, not the pipeline's history.
    pub provenance: BTreeMap<FieldPath, Provenance>,
    /// Whether every stage completed.
    pub completeness: Completeness,
    /// Non-blocking issues collected from every contribution's replay,
    /// in the order they were raised — [`crate::patch_eval::replay`]'s own
    /// per-contribution caveats, concatenated.
    pub caveats: Vec<Caveat>,
    /// One [`patch_eval::TopLevelOutcome`] per contribution actually
    /// replayed, in the same order as [`EffectiveInput::contributions`] —
    /// [`compute`] replays one contribution at a time
    /// (`patch_eval::replay` with a single-element slice), so each call's
    /// own `ReplayOutcome::top_level_outcomes` always has exactly zero or
    /// one entry; this is those, concatenated, mirroring how
    /// [`Self::caveats`] is built. Read by `VerifyOrder` for success
    /// suppression — alongside `caveats`, never a replacement.
    pub top_level_outcomes: Vec<patch_eval::TopLevelOutcome>,
    /// Every contribution's own
    /// [`patch_eval::ReplayOutcome::suppressed_filter_head_ops`], summed —
    /// how many operations the replay refused to turn into a prediction
    /// because their *filter* head is a global query whose empty selection
    /// on this one def is no evidence at all. Additive, measurement-only:
    /// nothing in this crate branches on it.
    pub suppressed_filter_head_ops: usize,
}

/// Runs the game's own pipeline — last owner wins (already `input.raw`,
/// the caller's job) -> every contribution replayed in order -> `ParentName`
/// inheritance — and attributes every field of the result.
///
/// The fold, stage by stage:
///
/// 1. **Owner.** Every leaf/list item of `input.raw` starts attributed to
///    [`Provenance::Owner`].
/// 2. **Patch.** For each contribution in order: snapshot the current
///    tree's leaves, replay *that one* contribution
///    ([`patch_eval::replay`] with a single-element slice) over a clone of
///    the tree, and diff the snapshot against the outcome. Any leaf whose
///    whole node changed (content, tag, or attributes — see
///    [`owned_leaves`]) or newly appeared is attributed to
///    [`Provenance::Patch`]; any leaf the snapshot had that the outcome
///    doesn't is simply dropped from the provenance map (nothing here
///    represents "removed", it represents "not part of the
///    result"). A contribution whose own replay errors
///    ([`ReplayError`]) stops the fold immediately at
///    [`Completeness::Partial`] — the tree from *before* this
///    contribution (never its partial effect) becomes [`EffectiveDef::resolved`]'s
///    starting point for the (skipped) inheritance stage below: a
///    contribution this crate can't safely replay is never
///    partially applied.
/// 3. **Inherit.** Whatever the patch stage produced (complete or not —
///    inheritance is deterministic and needs no guessing about a stopped
///    contribution, so it always runs against the best tree available)
///    is folded through [`fold_inheritance`], which returns both the
///    resolved tree *and* its exact provenance in one pass — see that
///    function's doc comment for why the two can never drift apart. On
///    failure ([`InheritError`]), the pre-inheritance tree and its
///    already-correct provenance stand as-is, and — only if the patch
///    stage was itself [`Completeness::Complete`] (a patch-stage stopper
///    always takes priority; layering a second, unrelated failure reason
///    on top would only obscure the first) — the failure becomes the
///    [`Completeness::Partial`] reason.
#[must_use]
pub fn compute(input: EffectiveInput<'_>) -> EffectiveDef {
    let mut tree = input.raw;
    let mut provenance: BTreeMap<FieldPath, Provenance> = owned_leaves(&tree)
        .into_keys()
        .map(|path| (path, Provenance::Owner(input.winner.clone())))
        .collect();
    let mut caveats: Vec<Caveat> = Vec::new();
    let mut top_level_outcomes: Vec<patch_eval::TopLevelOutcome> = Vec::new();
    let mut suppressed_filter_head_ops = 0usize;
    let mut completeness = Completeness::Complete;
    // Rebuilt, never mutated through a shared reference — `patch_eval::replay` is
    // called fresh per contribution here (this function's own doc
    // comment), so `this_def_present` has to be threaded across *this*
    // loop explicitly; `patch_eval::replay`'s own internal loop does the
    // same thing for a multi-contribution call.
    let mut context = input.context;
    // Once true, the def is gone for the rest of this fold —
    // including inheritance (below). RimWorld deletes the whole element
    // from the document; a deleted node is never a `ParentName`
    // inheritance *target* either, so folding the ancestor chain over
    // the emptied tree (`fold_inheritance`'s ordinary behaviour) would
    // wrongly resurrect the def's own inherited fields — `tree.parent_name`
    // is untouched by the removal itself (only `tree.root.content` is),
    // so `fold_inheritance` has no way to know on its own that this
    // child no longer exists to inherit into.
    let mut def_removed = false;

    for (op_index, contribution) in input.contributions.iter().enumerate() {
        let before = owned_leaves(&tree);
        let outcome =
            patch_eval::replay(tree.clone(), std::slice::from_ref(contribution), &context);
        caveats.extend(outcome.caveats);
        top_level_outcomes.extend(outcome.top_level_outcomes);
        suppressed_filter_head_ops += outcome.suppressed_filter_head_ops;
        if outcome.def_removed_by.is_some() {
            // The def is now absent for every later contribution in this
            // fold — never for `outcome.tree` itself, which the error
            // check below may still discard wholesale.
            context.this_def_present = false;
            def_removed = true;
        }
        if let Some(error) = outcome.error {
            completeness = Completeness::Partial {
                stopped_at: Stopper::Replay {
                    mod_id: contribution.mod_id.clone(),
                    op_index,
                    error,
                },
            };
            break;
        }

        let after = owned_leaves(&outcome.tree);
        let mut claimed_stale: BTreeSet<FieldPath> = BTreeSet::new();
        for (path, node) in &after {
            if before.get(path) == Some(node) {
                continue;
            }
            // The "list case" attribution fix: a new sibling colliding with
            // an existing `li`'s own declared identity forces
            // `identify_all_li` to renumber *every* item sharing it to
            // `ItemId::Position`, including one this contribution never
            // touched — see `renamed_list_item_source`'s own doc comment.
            if let Some(stale_path) =
                renamed_list_item_source(&before, &after, path, node, &claimed_stale)
            {
                claimed_stale.insert(stale_path.clone());
                if let Some(carried) = provenance.get(stale_path).cloned() {
                    provenance.insert(path.clone(), carried);
                    continue;
                }
            }
            provenance.insert(
                path.clone(),
                Provenance::Patch {
                    mod_id: contribution.mod_id.clone(),
                    op_index,
                },
            );
        }
        provenance.retain(|path, _| after.contains_key(path));
        tree = outcome.tree;
    }

    // A removed def never reaches inheritance at all — see
    // `def_removed`'s own doc comment above. `provenance` already
    // reflects "nothing here" on its own (the patch loop's own
    // `provenance.retain` already dropped every leaf once `tree` emptied),
    // so skipping the fold needs no further cleanup.
    let resolved = if def_removed {
        tree
    } else {
        match fold_inheritance(
            &tree,
            input.templates,
            input.template_owners,
            &provenance,
            input.winner,
        ) {
            Ok(folded) => {
                provenance = folded.provenance;
                folded.resolved
            }
            Err(error) => {
                if completeness == Completeness::Complete {
                    completeness = Completeness::Partial {
                        stopped_at: Stopper::Inherit(error),
                    };
                }
                tree
            }
        }
    };

    EffectiveDef {
        resolved,
        provenance,
        completeness,
        caveats,
        top_level_outcomes,
        suppressed_filter_head_ops,
    }
}
