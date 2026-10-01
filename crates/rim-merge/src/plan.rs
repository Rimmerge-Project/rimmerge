//! Folds a [`ThreeWayDiff`] (or a patch collision's single-field outcome)
//! plus the user's stored [`MergeChoice`]s into a [`MergePlan`] of ops
//! against the *raw* winner node.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{ModId, Selector};
pub use rim_resolve::domain::{DefKey, MergeChoice};

use crate::diff::{
    EntryKind, FieldDiff, OwnerVersion, ThreeWayDiff, Value, collision_fields, field_value,
    is_confirmed_keyed_map, is_keyed_map_at,
};
use crate::patch_behaviours::PatchOperationBehaviours;
use crate::patch_eval::{self, DefExists, PatchContribution, ReplayContext, ReplayError};
use crate::tree::{FieldNode, FieldPath, FieldTree};
use choices::{
    ChoiceOutcome, MoveLastReplay, contributions_of, credited_contributor, has_failed_op,
    move_mod_last, move_mod_last_trees, resolve_choice, resolve_collision_choice,
};
use fields::{DropOutcome, def_key_of, depends_on_for, owners_from_diff, plan_drop};
use rendering::{
    attrs_for_entry, build_node_for, ends_in_position, path_is_xpath_safe, replace_or_add,
};

mod choices;
mod fields;
mod rendering;

pub use choices::resolved_field_values;
pub use rendering::build_resolved_node;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "plan/plan_tests.rs"]
mod tests;

/// The `packageId` RimWorld's own game content ships under — always
/// active, so it's never worth naming in a `MayRequire` gate.
/// `pub(crate)` so `assign.rs`'s own dependency computation can exclude
/// it too, the same way this module's own `depends_on`
/// already does.
pub(crate) const CORE_MOD_ID: &str = "ludeon.rimworld";

/// One mutation against the raw winner node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanOp {
    /// Replaces the node already at `path` (it exists in the raw winner)
    /// with `node`.
    Replace {
        /// The node's address.
        path: FieldPath,
        /// Its replacement.
        node: FieldNode,
    },
    /// Adds `node` as a new child under `parent` (the longest existing
    /// raw prefix of the intended path) — used both for an ordinary
    /// missing field and, with `Inherit="False"` baked into `node`'s own
    /// attributes, for dropping an item the raw winner doesn't itself
    /// define (see [`plan_def_override`]'s doc comment, rule 4).
    Add {
        /// The existing parent to add under.
        parent: FieldPath,
        /// The node to add.
        node: FieldNode,
    },
    /// Removes the raw node at `path` outright — used when the raw
    /// winner itself defines the node being dropped, so removing it
    /// leaves nothing behind for inheritance to resurface.
    Remove {
        /// The node's address.
        path: FieldPath,
    },
    /// Replaces the raw container at `path` with `node` (which carries
    /// `Inherit="False"`) — the only way to drop an item the container
    /// inherits, when the raw winner already defines that same
    /// container (see [`plan_def_override`]'s doc comment, rule 4).
    ReplaceInheritFalse {
        /// The container's address.
        path: FieldPath,
        /// Its full replacement content, `Inherit="False"` included.
        node: FieldNode,
    },
    /// Sets one attribute on the raw node at `path`.
    SetAttribute {
        /// The node's address.
        path: FieldPath,
        /// The attribute's name.
        name: String,
        /// The attribute's new value.
        value: String,
    },
}

/// One [`PlanOp`] plus the mods whose values it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedOp {
    /// The mutation itself.
    pub op: PlanOp,
    /// Mods whose values this op carries — rendered as `MayRequire` by
    /// `emit`. Base ids; Core is never included (always active); DLCs
    /// are kept (not always active).
    pub depends_on: BTreeSet<ModId>,
}

/// A caveat surfaced to the user alongside a [`MergePlan`] — something
/// they should know about even though it didn't block planning (or, for
/// several variants below, exactly *why* a field had to go to
/// [`MergePlan::unresolved`] instead of getting an op).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Caveat {
    /// More than one active mod registered a template of this `Name`.
    ///
    /// **Not "the earliest owner won"** (ground-truthed against the
    /// decompiled `Verse.XmlInheritance.GetBestParentFor`): every registration
    /// coexists, and each child independently resolves to whichever
    /// registration belongs to the nearest mod loading at or before it
    /// (falling back to a vanilla registration, or the lowest-loaded
    /// mod's, only when none qualifies) — there is no single winner this
    /// caveat could ever correctly name. Currently never actually raised
    /// by this crate: nothing in `inherit.rs`/`def_sources`-style
    /// resolution detects the ambiguity and surfaces it during a real
    /// merge/effective walk (a known, disclosed gap).
    DuplicateTemplate {
        /// The contested template name.
        name: String,
        /// Every mod that registered it, in load order.
        owners: Vec<ModId>,
    },
    /// A custom sequence-like operation class was replayed at its
    /// declared default toggle (`<enabled>`/`<defaultValue>`, true when
    /// absent) rather than whatever the user's own game has it set to.
    ModSettingDefault {
        /// The mod that shipped the operation.
        mod_id: ModId,
        /// The operation's `Class`.
        class: String,
    },
    /// A `Replace`/`Remove`/`Add`/`Insert`/`Attribute*`/
    /// `AddModExtension` operation matched zero nodes, or a
    /// `PatchOperationTest`'s xpath didn't match — RimWorld logs an error
    /// (or silently stops) and moves on; so does this replay.
    FailedOp {
        /// The mod that shipped the operation.
        mod_id: ModId,
        /// The operation's xpath.
        xpath: String,
    },
    /// This mod has patch operations whose xpath didn't parse to a def
    /// target at all — they can't be scoped, so they can't be replayed.
    UnscopedOps {
        /// The mod.
        mod_id: ModId,
        /// How many such operations it has.
        count: usize,
    },
    /// A mod's patch contribution wasn't well-formed XML.
    MalformedOperation {
        /// The mod that shipped it.
        mod_id: ModId,
    },
    /// A `PatchOperationRemove` whose resolved selection was exactly the
    /// def root emptied the def's own tree instead of being refused:
    /// RimWorld really does delete the node, so a replay can model it, but the def
    /// is thereafter absent for the rest of this fold and every later
    /// contribution on it replays against nothing. Disclosed so a merge
    /// preview never shows an emptied def with no explanation.
    DefRemoved {
        /// The mod whose `Remove` emptied the def.
        mod_id: ModId,
    },
    /// A `li` item could only be identified by its position among
    /// siblings — the weakest identity, worth flagging since a later,
    /// unrelated reorder would silently repoint this choice. Also raised
    /// when two or more `li` siblings in the same container computed the
    /// same identity and had to fall back to position to stay
    /// distinguishable at all (see [`crate::tree::identify_all_li`]).
    PositionalItem {
        /// The item's address.
        path: FieldPath,
    },
    /// A field's [`FieldPath`] carries an identity value (a `li`'s
    /// `Class`, key-child value, or text) that can't be safely embedded
    /// in an xpath predicate: either it contains both `"` and `'` (no
    /// quote character is left to delimit it), or it contains a `..` or
    /// `|` substring that `rim_analyzer::extract::xpath_expr`'s own
    /// (non-quote-aware) unsupported-xpath scan would reject even though
    /// it's safely inside quotes. Left unresolved rather than emitting an
    /// xpath this crate's own [`crate::patch_eval`] — or the analyzer —
    /// couldn't parse back.
    UnsafeXpathValue {
        /// The field's address.
        path: FieldPath,
    },
    /// A `MergeChoice::From` named a mod that isn't one of this field's
    /// owners/candidates.
    UnknownOwnerChoice {
        /// The field's address.
        path: FieldPath,
        /// The unrecognized mod.
        mod_id: ModId,
    },
    /// A `MergeChoice::Value` free-text fragment failed to parse (or, for
    /// a list item, didn't parse to a single `<li>` root).
    InvalidValueFragment {
        /// The field's address.
        path: FieldPath,
    },
    /// A `MergeChoice::Drop` on a patch-collision field: RimWorld's patch
    /// language has no operation that "drops" a value a replay already
    /// produced — dropping only makes sense against a def-override's own
    /// raw/inherited structure (see [`plan_def_override`]'s rule 4).
    UnsupportedDrop {
        /// The field's address.
        path: FieldPath,
    },
    /// The chosen value needed to be added under a raw parent the winner
    /// doesn't have, through a chain of missing intermediate elements
    /// this crate couldn't safely reconstruct (an ancestor identified by
    /// something other than a `Class` attribute — see `wrap`'s doc
    /// comment).
    UnreconstructableChain {
        /// The field's address.
        path: FieldPath,
    },
    /// A top-level (single-segment) field is purely inherited and the
    /// user chose to drop it: RimWorld's patch language can unset a `li`
    /// item (via `Inherit="False"` on its container) but has no way to
    /// unset an inherited *leaf* back to nothing — there's no container
    /// to reconstruct minus the leaf, since the leaf itself is the whole
    /// field.
    UnsettableLeaf {
        /// The field's address.
        path: FieldPath,
    },
    /// This plan's target def has an owner outside a compat patch's
    /// declared scope: in an install where one of `mods` loads after the
    /// scope's own winner, the raw node these ops target may be theirs
    /// instead — a field they also define is silently overwritten by this
    /// patch's `Replace`, and a `Replace` on a field they don't define
    /// fails at the game's patch stage with no change. Never attached by
    /// this crate's own planning functions (`plan_def_override`/
    /// `plan_patch_collision` know nothing about scopes); `rim-session`
    /// attaches it after restricting a plan's participants to a patch's
    /// scope, the only layer that knows what the scope is.
    OutOfScopeOwners {
        /// The owners outside the patch's scope that could still own the
        /// raw node this plan's ops target, under some other install's
        /// load order.
        mods: Vec<ModId>,
    },
    /// A keyed-map entry that one mod's own `PatchOperationAdd` independently
    /// contributed was dropped by a *later* mod's wholesale replace of the
    /// whole container — the entry still auto-resolved (`DiffClass::OneSided`,
    /// no stored choice needed), but the real, full-order replay no longer has
    /// it, so the merge mod restores it with an `Add`. Never raised for an
    /// explicit stored choice (a user choosing `From(m)` on an entry the real
    /// order already dropped is just an ordinary, un-caveated `Add`).
    ClobberedMapEntry {
        /// The dropped entry's own address.
        path: FieldPath,
        /// The mod whose contribution was clobbered.
        by: ModId,
    },
}

/// A short, human-readable rendering of a [`Caveat`] — additive
/// before this,
/// `apps/desktop/src-tauri/src/dto/merge.rs`'s own `caveat_text` free
/// function was the only place this text existed, and `apps/cli` printed
/// `{caveat:?}` (`Debug`) straight into user-facing output instead. This
/// impl's wording deliberately matches `caveat_text`'s own, mod-for-mod —
/// `caveat_text` itself is untouched (it has its own tests and callers in
/// that crate); a later step should switch it to call `Display` instead
/// of keeping two copies of the same text in sync by hand.
impl std::fmt::Display for Caveat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn join_ids<'a>(ids: impl IntoIterator<Item = &'a ModId>) -> String {
            ids.into_iter()
                .map(ModId::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        }
        match self {
            Caveat::DuplicateTemplate { name, owners } => write!(
                f,
                "template \"{name}\" is registered by multiple mods ({}); each child independently inherits from whichever registration loads nearest at or before it, so different children can resolve to different mods",
                join_ids(owners)
            ),
            Caveat::ModSettingDefault { mod_id, class } => write!(
                f,
                "{mod_id}'s {class} was replayed at its default setting, not necessarily your game's"
            ),
            Caveat::FailedOp { mod_id, xpath } => {
                write!(f, "{mod_id}'s patch operation on {xpath} matched nothing")
            }
            Caveat::UnscopedOps { mod_id, count } => write!(
                f,
                "{mod_id} has {count} patch operation(s) that couldn't be scoped to a def"
            ),
            Caveat::MalformedOperation { mod_id } => {
                write!(f, "{mod_id}'s patch operation wasn't well-formed XML")
            }
            Caveat::DefRemoved { mod_id } => {
                write!(f, "this def is removed by {mod_id}")
            }
            Caveat::PositionalItem { path } => write!(
                f,
                "{path} is identified only by its position; a later reorder could repoint this choice"
            ),
            Caveat::UnsafeXpathValue { path } => write!(
                f,
                "{path}'s value can't be safely written as an xpath and was left unresolved"
            ),
            Caveat::UnknownOwnerChoice { path, mod_id } => write!(
                f,
                "{path}: the chosen owner {mod_id} isn't a candidate for this field"
            ),
            Caveat::InvalidValueFragment { path } => {
                write!(f, "{path}: the entered value couldn't be parsed")
            }
            Caveat::UnsupportedDrop { path } => {
                write!(f, "{path}: dropping isn't supported for a patch collision")
            }
            Caveat::UnreconstructableChain { path } => {
                write!(
                    f,
                    "{path}: the containing structure couldn't be reconstructed"
                )
            }
            Caveat::UnsettableLeaf { path } => {
                write!(
                    f,
                    "{path}: an inherited leaf can't be unset back to nothing"
                )
            }
            Caveat::OutOfScopeOwners { mods } => write!(
                f,
                "in an install where {} loads after this patch's scope, this patch may target or overwrite their def instead",
                join_ids(mods)
            ),
            Caveat::ClobberedMapEntry { path, by } => write!(
                f,
                "{path} was added by {by} but a later patch replaced the whole map, dropping it; restored"
            ),
        }
    }
}

/// One contested def's patch: every op needed to turn the raw winner node
/// into the user's intended merge, plus what still needs input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergePlan {
    /// The contested def.
    pub key: DefKey,
    /// `DefName` for an ordinary def override, `NameAttr` when merging a
    /// patch collision on a template.
    pub selector: Selector,
    /// Whose raw node the emitted xpaths target under the selected order.
    pub winner: ModId,
    /// Every contributing owner (Core included), sorted — display only
    /// (the `Patches/*.xml` header comment); derived from the diff's own
    /// candidate set (or, for a collision, `def_owner` plus every
    /// colliding mod), not necessarily the selected load order.
    pub owners: Vec<ModId>,
    /// Every op, in path order.
    pub ops: Vec<PlannedOp>,
    /// Conflict fields with no stored choice yet, or fields that had to
    /// be abandoned for one of the reasons named in `caveats` — a
    /// `Caveat::UnsafeXpathValue`/`UnsettableLeaf`/`UnsupportedDrop`/
    /// `UnreconstructableChain`/`InvalidValueFragment`/
    /// `UnknownOwnerChoice` always accompanies the matching path here.
    pub unresolved: Vec<FieldPath>,
    /// Non-blocking issues surfaced alongside the plan.
    pub caveats: Vec<Caveat>,
}

/// Everything [`plan_patch_collision`] produced: the plan itself, plus
/// the very [`FieldDiff`]s it was planned from.
///
/// `fields` is [`crate::diff::collision_fields`]' own result over the
/// per-mod candidate trees this crate built internally — one entry per
/// key for a confirmed [`crate::tree::ContainerKind::KeyedMap`], the
/// single whole-subtree field otherwise. It is returned rather than
/// discarded so a caller that also *displays* the collision
/// (`rim-session`'s merge preview, and through it the def-conflict view)
/// reads the identical classification the plan was built from, instead
/// of rebuilding the candidate trees itself — a mirror that can drift (a
/// fix reaching the plan but not the display).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchCollisionOutcome {
    /// Every op needed, plus what still needs input.
    pub plan: MergePlan,
    /// The classified fields `plan` was built from, in
    /// [`crate::diff::collision_fields`]' own order.
    pub fields: Vec<FieldDiff>,
    /// Each field's own value in
    /// `final_outcome.tree` — the full-order replay's outcome, i.e. what
    /// RimWorld's own sequential patch application already produces here,
    /// independent of any stored choice or whether an op was needed for
    /// it. One entry per field in `fields` that reached the replay-value
    /// read (every one except a field dropped for an unsafe xpath before
    /// that point). This is the outcome `merge plan`'s own "final" column
    /// shows — not `FieldDiff::candidates`' *winner-only* entry: `winner`
    /// is always `def_owner` for a collision, which never itself
    /// contributes a patch op, so that entry reads `<absent>`/`?` for
    /// almost every field. Surfaced here (computed once, alongside `fields`) rather
    /// than recomputed by a caller, per this crate's own single-public-
    /// surface rule for a collision's per-mod candidate trees
    /// (`crates/rim-merge/CLAUDE.md`).
    pub final_values: BTreeMap<FieldPath, Value>,
}

/// Folds `diff` and `choices` into a [`MergePlan`] against `winner`'s raw
/// node — see this module's doc comment and [`plan_drop`] for the
/// per-rule op selection.
#[must_use]
pub fn plan_def_override(
    diff: &ThreeWayDiff,
    winner: &OwnerVersion,
    choices: &BTreeMap<FieldPath, MergeChoice>,
) -> MergePlan {
    let mut ops = Vec::new();
    let mut unresolved = Vec::new();
    let mut caveats = Vec::new();

    for field in &diff.fields {
        if !path_is_xpath_safe(&field.path) {
            unresolved.push(field.path.clone());
            caveats.push(Caveat::UnsafeXpathValue {
                path: field.path.clone(),
            });
            continue;
        }

        let choice = choices.get(&field.path);
        let chosen = match resolve_choice(field, choice) {
            ChoiceOutcome::NoChoice => {
                unresolved.push(field.path.clone());
                continue;
            }
            ChoiceOutcome::Invalid(caveat) => {
                unresolved.push(field.path.clone());
                caveats.push(caveat);
                continue;
            }
            ChoiceOutcome::Resolved(value) => value,
        };

        let current = field_value(&winner.resolved, &field.path, &field.entry);
        if chosen == current {
            continue;
        }

        let depends_on = depends_on_for(field, choice, &winner.mod_id);

        if chosen == Value::Absent {
            match plan_drop(&field.path, winner, depends_on) {
                DropOutcome::Op(op) => {
                    if ends_in_position(&field.path) {
                        caveats.push(Caveat::PositionalItem {
                            path: field.path.clone(),
                        });
                    }
                    ops.push(op);
                }
                DropOutcome::Blocked(caveat) => {
                    unresolved.push(field.path.clone());
                    caveats.push(caveat);
                }
            }
            continue;
        }

        let Some(node) = build_node_for(&field.path, &chosen, &attrs_for_entry(field)) else {
            continue;
        };
        match replace_or_add(&field.path, &winner.raw, node) {
            Some(op) => {
                if ends_in_position(&field.path) {
                    caveats.push(Caveat::PositionalItem {
                        path: field.path.clone(),
                    });
                }
                ops.push(PlannedOp { op, depends_on });
            }
            None => {
                unresolved.push(field.path.clone());
                caveats.push(Caveat::UnreconstructableChain {
                    path: field.path.clone(),
                });
            }
        }
    }

    MergePlan {
        key: def_key_of(&winner.raw),
        selector: Selector::DefName,
        winner: winner.mod_id.clone(),
        owners: owners_from_diff(diff, &winner.mod_id),
        ops,
        unresolved,
        caveats,
    }
}

/// Everything [`plan_patch_collision`] needs — grouped into one struct
/// since the function otherwise takes more independent parameters than
/// is readable at a call site.
#[derive(Clone)]
pub struct PatchCollisionInput<'a> {
    /// The patched def.
    pub key: DefKey,
    /// Which attribute the collision's predicate matched on.
    pub selector: Selector,
    /// The contested field under the def, or `None` for the def root
    /// itself.
    pub sub_path: Option<FieldPath>,
    /// The mod whose raw node the emitted xpaths target (the def's own
    /// owner under the selected order — distinct from `mods`, the
    /// *patching* mods).
    pub def_owner: ModId,
    /// The def owner's raw node.
    pub target_raw: &'a FieldTree,
    /// Every mod whose op(s) touch this exact target, in the selected
    /// order.
    pub mods: &'a [ModId],
    /// Every mutating op the selected order would apply to this def (any
    /// sub-path), in replay order.
    pub contributions: &'a [PatchContribution<'a>],
    /// Every currently active mod, for gate evaluation.
    pub active_mods: &'a BTreeSet<ModId>,
    /// `PatchOperationFindMod` display name -> the [`ModId`] it names.
    pub mod_names_by_display: &'a BTreeMap<String, ModId>,
    /// Answers a bare existence test on another def — see
    /// [`crate::patch_eval::DefExists`]. Pass
    /// [`crate::patch_eval::def_existence_unknown`] when no def index is
    /// at hand.
    pub def_exists: DefExists<'a>,
    /// The user's stored choices for this collision, keyed by each
    /// entry's own [`FieldPath`] — a `sub_path`-keyed entry for an
    /// ordinary field, one entry per key (`sub_path` plus that
    /// key's own tag) for a keyed-map collision. "Take the whole map from
    /// M" is simply `MergeChoice::From(M)` stored under every one of that
    /// map's own entry paths — there is no separate "whole map" choice
    /// shape.
    pub choices: &'a BTreeMap<FieldPath, MergeChoice>,
    /// Which third-party operation classes the replay knows by name —
    /// see [`crate::patch_eval::ReplayContext::behaviours`]. Pass
    /// [`crate::patch_behaviours::PatchOperationBehaviours::none`] when
    /// no loaded data is at hand.
    pub behaviours: &'a PatchOperationBehaviours,
}

impl std::fmt::Debug for PatchCollisionInput<'_> {
    /// Hand-written because [`DefExists`] is a `&dyn Fn`, which has no
    /// `Debug` — every other field prints as it would from a derive.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PatchCollisionInput")
            .field("key", &self.key)
            .field("selector", &self.selector)
            .field("sub_path", &self.sub_path)
            .field("def_owner", &self.def_owner)
            .field("target_raw", &self.target_raw)
            .field("mods", &self.mods)
            .field("contributions", &self.contributions)
            .field("active_mods", &self.active_mods)
            .field("mod_names_by_display", &self.mod_names_by_display)
            .field("def_exists", &"<fn>")
            .field("choices", &self.choices)
            .field("behaviours", &self.behaviours)
            .finish()
    }
}

/// Plans one patch collision's contested field: `sub_path` (or the whole
/// def root, when `None`) under a def that more than one mod patches —
/// or, when `sub_path`'s node is a [`crate::tree::ContainerKind::KeyedMap`],
/// every one of its keys, planned independently. `contributions` is
/// every mutating op
/// the selected order would apply to this def — the same replay-ordered
/// list [`crate::patch_eval::replay`] consumes — and `mods` is every mod
/// whose op(s) touch this exact target, in the selected order.
///
/// `final_under_order` (`final_outcome` below) is the replay of every
/// contribution as given — [`crate::diff::collision_fields`]'s own
/// `final_tree`, and also what an entry's chosen value is compared
/// against to decide whether an op is needed at all. Per-mod candidates
/// come from one replay per member of `mods`: when `sub_path` is a keyed
/// map in `final_outcome.tree`, each is an *isolated* replay of that
/// mod's own contributions alone (the disjoint-key union
/// `collision_fields` needs — see its own doc comment for why
/// `move_mod_last` alone misclassifies a clean disjoint add); otherwise
/// each is `candidate(m)` — the same replay with `m`'s own contributions
/// moved last (so `m` wins the collision — see [`move_mod_last`]'s doc
/// comment for exactly what "moved" means here), preserving this
/// non-map path's own long-tested semantics exactly. Every entry's
/// `DiffClass` resolves the same way a def-override field does
/// (`resolve_choice`, via [`resolve_collision_choice`]):
/// `Unchanged`/`OneSided`/`Agreeing` auto-resolve to the union, `Conflict`
/// needs an explicit per-entry `From`/`Value` choice (never `Drop` — see
/// [`resolve_collision_choice`]'s own doc comment). An entry whose
/// resolved value already matches `final_outcome.tree`'s own gets no
/// op — including every entry of an all-auto-resolved keyed map, which
/// leaves `ops` empty (`MergeState::Complete { op_count: 0 }`: the game
/// already unions the keys, nothing to emit). One exception:
/// [`Caveat::ClobberedMapEntry`] when an auto-resolved (no stored choice)
/// `OneSided` entry's chosen value differs from a *absent*
/// `final_outcome` value — a later mod's own wholesale replace of the
/// whole map dropped an earlier mod's independent add, restored here
/// with an `Add`.
///
/// Returns the plan *and* the [`FieldDiff`]s it was planned from (see
/// [`PatchCollisionOutcome`]) — the per-mod candidate construction
/// described above is this function's own, and a caller that needs the
/// same classification for display must read it from here rather than
/// rebuild it.
///
/// # Errors
///
/// The [`ReplayError`] from [`crate::patch_eval::replay`] when any
/// contribution's xpath falls outside the replay's xpath grammar or fails
/// to parse. The caller (`rim-session`, which alone can turn this into
/// `rim_resolve::domain::MergeState::CannotMerge`) must not present a
/// partial result when this happens.
pub fn plan_patch_collision(
    input: PatchCollisionInput<'_>,
) -> Result<PatchCollisionOutcome, ReplayError> {
    let PatchCollisionInput {
        key,
        selector,
        sub_path,
        def_owner,
        target_raw,
        mods,
        contributions,
        active_mods,
        mod_names_by_display,
        def_exists,
        choices,
        behaviours,
    } = input;

    let context = ReplayContext {
        active_mods,
        mod_names_by_display,
        def_type: &key.def_type,
        def_name: &key.def_name,
        selector,
        def_exists,
        this_def_present: true,
        behaviours,
    };

    let path = sub_path.unwrap_or_else(|| FieldPath::new(vec![]));

    let final_outcome = patch_eval::replay(target_raw.clone(), contributions, &context);
    if let Some(error) = final_outcome.error {
        return Err(error);
    }
    let looks_like_map = is_keyed_map_at(&final_outcome.tree, &path);
    let move_last = MoveLastReplay {
        mods,
        contributions,
        target_raw,
        contested: &path,
    };

    let per_mod_trees = if looks_like_map {
        let mut isolated_trees: Vec<(ModId, FieldTree)> = Vec::new();
        for mod_id in mods {
            let only_this_mod = contributions_of(contributions, mod_id);
            let mut outcome = patch_eval::replay(target_raw.clone(), &only_this_mod, &context);
            if let Some(error) = outcome.error {
                return Err(error);
            }
            // This mod's own op(s), replayed alone,
            // matched nothing (a `Caveat::FailedOp` on this exact mod) —
            // a plausible, ordinary shape (a `Replace` of a key only
            // *another* mod's earlier `Add` created). An isolated
            // candidate that never actually ran can't attribute this mod
            // correctly (it would read `Value::Absent` regardless of
            // what this mod's op really does), so it can't be trusted to
            // decide "clean disjoint add vs. genuinely order-dependent
            // change" the way a candidate that *did* run can. Fall back
            // to the real, already-patched tree via `move_mod_last`
            // instead — this mod's own ops moved last, so it still wins
            // the collision the way every non-map collision does, but
            // sees every other contribution's own structural change
            // first.
            if has_failed_op(&outcome.caveats, mod_id) {
                let reordered = move_mod_last(contributions, mod_id);
                outcome = patch_eval::replay(target_raw.clone(), &reordered, &context);
                if let Some(error) = outcome.error {
                    return Err(error);
                }
            }
            isolated_trees.push((mod_id.clone(), outcome.tree));
        }
        // `looks_like_map` is decided from `final_outcome.tree` alone,
        // but the isolated candidates just
        // built might themselves disagree (e.g. one mod's own isolated
        // replay removes the container's only key, leaving an ordinary
        // `Record` behind, not a `KeyedMap`) — exactly the disagreement
        // `collision_fields`'s own second check exists to catch, except
        // by then these isolated trees would be the *only* candidates it
        // has to fall back with (neither the isolated algorithm nor
        // `move_mod_last`). Reusing the identical confirmation check here
        // means whatever `collision_fields` ends up deciding, the trees
        // it was handed already match that decision.
        if is_confirmed_keyed_map(&final_outcome.tree, &isolated_trees, &path) {
            isolated_trees
        } else {
            move_mod_last_trees(&move_last, &context)?
        }
    } else {
        move_mod_last_trees(&move_last, &context)?
    };

    let fields = collision_fields(target_raw, &final_outcome.tree, &per_mod_trees, &path);

    let mut ops = Vec::new();
    let mut unresolved = Vec::new();
    let mut caveats = final_outcome.caveats;
    // Every field's own full-order-replay value, keyed for a caller
    // (`rim-session`'s merge preview, and through it `merge plan`'s own
    // `final` column) that needs to show what RimWorld's own sequential patch
    // application already produces here — distinct from `field.candidates`,
    // which never carries `def_owner` at all (a patch collision's `winner` is
    // always the def's own owner, never one of `mods`), and distinct from
    // whichever candidate this loop ends up choosing for `ops` (a
    // `NoChoice`/`Invalid` field contributes no op, but the game still
    // produces *some* value here). Built unconditionally, one entry per
    // field, regardless of how that field's own choice resolves below.
    let mut final_values = BTreeMap::new();

    for field in &fields {
        if !path_is_xpath_safe(&field.path) {
            unresolved.push(field.path.clone());
            caveats.push(Caveat::UnsafeXpathValue {
                path: field.path.clone(),
            });
            continue;
        }

        let choice = choices.get(&field.path);
        let final_value = field_value(&final_outcome.tree, &field.path, &field.entry);
        final_values.insert(field.path.clone(), final_value.clone());

        match resolve_collision_choice(field, choice) {
            ChoiceOutcome::NoChoice => unresolved.push(field.path.clone()),
            ChoiceOutcome::Invalid(caveat) => {
                unresolved.push(field.path.clone());
                caveats.push(caveat);
            }
            ChoiceOutcome::Resolved(chosen) if chosen == final_value => {}
            // An automatic resolution (no stored choice) landed on "nothing
            // contributed" while the real, full-order replay already has a value
            // here — isolated per-mod attribution couldn't explain it (the
            // "isolated replay for attribution" risk: a mod's own op that depends
            // on another's earlier structural change fails when replayed alone).
            // Trust the real replay rather than reporting a spurious gap with
            // nothing meaningful to pick.
            // Scoped to `EntryKind::MapEntry`: the
            // isolated-replay attribution risk this exists for only ever
            // applies to a keyed map's own per-key candidates; an
            // ordinary single-field collision's candidates always come
            // from a full, `move_mod_last`-reordered replay, so a
            // `Resolved(Value::Absent)` there means the credited mod's
            // own change genuinely lands on nothing (e.g. mod A removes
            // a field, mod B re-adds it, and something about the
            // resolved order still nets out absent) — a real gap the
            // user needs to see, not a spurious one to hide. Without this
            // guard the same silent no-op would fire on every ordinary
            // field too, breaking the rule that every non-map case plans
            // byte-identically.
            ChoiceOutcome::Resolved(Value::Absent)
                if choice.is_none() && matches!(field.entry, EntryKind::MapEntry { .. }) => {}
            ChoiceOutcome::Resolved(chosen) => {
                let Some(node) = build_node_for(&field.path, &chosen, &attrs_for_entry(field))
                else {
                    unresolved.push(field.path.clone());
                    continue;
                };
                // A later mod's wholesale replace of the whole map
                // dropped an earlier, independent contribution this
                // entry still auto-resolves to (any class
                // `resolve_choice`'s own automatic result credits a
                // single contributor for — `Agreeing`'s earliest member
                // is exactly who `resolve_choice` itself picked).
                if choice.is_none()
                    && final_value == Value::Absent
                    && matches!(field.entry, EntryKind::MapEntry { .. })
                    && let Some(by) = credited_contributor(&field.class)
                {
                    caveats.push(Caveat::ClobberedMapEntry {
                        path: field.path.clone(),
                        by: by.clone(),
                    });
                }
                let depends_on = depends_on_for(field, choice, &def_owner);
                match replace_or_add(&field.path, &final_outcome.tree, node) {
                    Some(op) => {
                        if ends_in_position(&field.path) {
                            caveats.push(Caveat::PositionalItem {
                                path: field.path.clone(),
                            });
                        }
                        ops.push(PlannedOp { op, depends_on });
                    }
                    None => {
                        unresolved.push(field.path.clone());
                        caveats.push(Caveat::UnreconstructableChain {
                            path: field.path.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(PatchCollisionOutcome {
        plan: MergePlan {
            key,
            selector,
            winner: def_owner.clone(),
            owners: owners_from_mods(&def_owner, mods),
            ops,
            unresolved,
            caveats,
        },
        fields,
        final_values,
    })
}

fn owners_from_mods(def_owner: &ModId, mods: &[ModId]) -> Vec<ModId> {
    let mut owners: BTreeSet<ModId> = mods.iter().cloned().collect();
    owners.insert(def_owner.clone());
    owners.into_iter().collect()
}
