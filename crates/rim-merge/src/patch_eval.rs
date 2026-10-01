//! The xpath-subset replay evaluator: applies a contested def's patch
//! operations, in order, to a raw [`FieldTree`], following RimWorld's own
//! patch replay semantics.
//!
//! Each contribution is handed in as the raw XML text of one *top-level*
//! `<Operation>` a mod's patch file aims at this def (any sub-path); this
//! module parses that text itself (a pure, in-memory operation — no
//! filesystem access) and walks its `PatchOperationSequence`/
//! `PatchOperationConditional`/`PatchOperationFindMod` structure directly,
//! rather than consuming `rim-analyzer`'s already-flattened `PatchOp`
//! list. That list (built by `SourceIndex`, see the analyzer's own
//! `patches::walk`) exists for *finding* collisions — it flattens control
//! flow into gates on each leaf op, which is right for "does a collision
//! exist" but loses two things a *replay* needs: which sibling ops
//! belonged to the same `PatchOperationSequence` (needed to abort the
//! rest of it on a failure) and whether a `PatchOperationConditional`'s
//! branch actually matched the *current* document state (which only a
//! replay, not a static scan, can know). Reusing the raw operation text
//! keeps this module self-sufficient for both.
//!
//! [`ReplayContext`] carries the identity of the def actually being
//! replayed (`def_type`/`def_name`/`selector`): one `<Operation>` a mod
//! ships may (inside a `PatchOperationSequence`, say) contain ops aimed
//! at several *different* defs — the analyzer's `SourceIndex` indexes
//! every op whose xpath parses to a target, keyed per target def, but a
//! contribution handed to [`replay`] is the whole top-level `<Operation>`
//! text, unfiltered. An op whose own xpath targets a different def is
//! simply not ours to run (`Ok(true)`: it "succeeded elsewhere", not a
//! failure here); a `PatchOperationConditional`/`PatchOperationTest`
//! aimed at a different def can't be evaluated at all without that def's
//! document (a patch collision whose xpath needs the full merged document
//! is out of scope), so it's `Unsupported`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use rim_analyzer::domain::{ModId, Selector};
use roxmltree::Node;

use crate::patch_behaviours::PatchOperationBehaviours;
use crate::plan::Caveat;
use crate::tree::FieldTree;
use dispatch::apply_operation;
use identity::{child_text, operation_identity};

mod custom_ops;
mod dispatch;
mod identity;
mod select;
mod standard_ops;
mod tree;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "patch_eval/patch_eval_tests.rs"]
mod tests;

/// One mod's contribution: the raw XML text of one top-level `<Operation>`
/// its patch file aims at this def.
#[derive(Debug, Clone, Copy)]
pub struct PatchContribution<'a> {
    /// The mod that shipped this operation.
    pub mod_id: &'a ModId,
    /// The `<Operation>` element's own XML text.
    pub operation_xml: &'a str,
}

/// Answers "does `(def_type, def_name)` exist in the active install?" for
/// a `PatchOperationConditional`/`PatchOperationTest` whose xpath is a
/// bare existence test on *another* def — the one cross-def question a
/// replay can answer without that def's own document. `None` means
/// "unknown", which keeps such an op [`ReplayError::Unsupported`] rather
/// than guessing a branch.
pub type DefExists<'a> = &'a dyn Fn(&str, &str) -> Option<bool>;

/// A [`DefExists`] that never knows anything — every cross-def existence
/// test stays `Unsupported`. The right default for a caller with no def
/// index at hand.
static NEVER_KNOWN: fn(&str, &str) -> Option<bool> = |_, _| None;

/// See [`NEVER_KNOWN`].
pub fn def_existence_unknown() -> DefExists<'static> {
    &NEVER_KNOWN
}

/// What a replay needs to know beyond the operations themselves: the
/// active install (for gate evaluation), the identity of the def
/// actually being replayed (so an op nested inside a contribution but
/// aimed at some *other* def is recognized and skipped rather than
/// mis-applied — see this module's doc comment), and a way to answer a
/// bare existence test on another def.
#[derive(Clone, Copy)]
pub struct ReplayContext<'a> {
    /// Every currently active mod (base ids).
    pub active_mods: &'a BTreeSet<ModId>,
    /// `PatchOperationFindMod` display name -> the [`ModId`] it names.
    /// **Exact (case-sensitive), on purpose, ground-truthed against the
    /// decompiled `Verse.ModLister.HasActiveModWithName`**: the real
    /// engine's own `PatchOperationFindMod.ApplyWorker` calls
    /// `ModLister.HasActiveModWithName(mods[i])`, whose body is a plain
    /// `mod.Active && mod.Name == name` — ordinary C# `==` on `string`,
    /// case-sensitive. This map's `.get(name)` in `apply_find_mod`
    /// already matches that exactly (a `BTreeMap`, built by the caller
    /// from each mod's own raw, unmodified display name — never
    /// lowercased anywhere in this crate). **Never lowercase this map or
    /// its lookup to "align" it with
    /// `rim_analyzer::analysis::indices::gate_open`'s own lowercased
    /// resolution** — that gate is the one that diverges from the real
    /// engine, not this one; see `gate_open`'s own doc comment for the
    /// full writeup and the one real-install name this was checked
    /// against (`"a real content mod's display name"`).
    pub mod_names_by_display: &'a BTreeMap<String, ModId>,
    /// The def type being replayed (`tree.root.tag`, conventionally).
    pub def_type: &'a str,
    /// The def name being replayed.
    pub def_name: &'a str,
    /// Which attribute identifies `def_name` — `DefName` for an ordinary
    /// def, `NameAttr` when replaying patches against a template.
    pub selector: Selector,
    /// See [`DefExists`].
    pub def_exists: DefExists<'a>,
    /// Whether the def actually being replayed (`def_type`/`def_name`)
    /// really exists anywhere — `true` for every ordinary replay (an
    /// owned def, or a template's own chain). `false` only for
    /// `rim_session::use_cases::verify_order`'s own zero-owner fast path,
    /// which replays against a synthetic, empty `<{def_type}></{def_type}>`
    /// placeholder built purely so a leaf mutation's own success-
    /// suppression logic still applies — that placeholder's tree has a
    /// real root node like any other, so a *bare def-head* xpath (no
    /// sub-path steps: `Defs/ThingDef[defName="X"]`, exactly a
    /// `PatchOperationConditional`/`Test`'s own existence check) would
    /// otherwise match it and answer "this def exists", inverting the
    /// whole point of the placeholder (see `matched_paths`'s own doc
    /// comment). Every other call site sets this `true`.
    pub this_def_present: bool,
    /// Which third-party operation classes this replay knows by name, and
    /// what each one does — **data, not a table in this module**. See
    /// [`PatchOperationBehaviours`]'s own doc comment for why the class
    /// *names* are data while the behaviours and every structural
    /// detection stay in code. [`PatchOperationBehaviours::none`] is the
    /// right value for a caller with no loaded data: every custom class
    /// then falls back to the structural detections in
    /// `apply_operation_body`, exactly as an unrecognized one always
    /// has.
    pub behaviours: &'a PatchOperationBehaviours,
}

impl fmt::Debug for ReplayContext<'_> {
    /// Hand-written because [`DefExists`] is a `&dyn Fn`, which has no
    /// `Debug` — every other field prints as it would from a derive.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReplayContext")
            .field("active_mods", &self.active_mods)
            .field("mod_names_by_display", &self.mod_names_by_display)
            .field("def_type", &self.def_type)
            .field("def_name", &self.def_name)
            .field("selector", &self.selector)
            .field("def_exists", &"<fn>")
            .field("this_def_present", &self.this_def_present)
            .field("behaviours", &self.behaviours)
            .finish()
    }
}

/// Why a replay stopped before finishing every contribution.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReplayError {
    /// An operation's xpath (or a custom class with no recognizable
    /// structure) fell outside the replay's xpath grammar.
    #[error("unsupported xpath on {xpath:?}: {reason}")]
    Unsupported {
        /// The xpath text that couldn't be replayed.
        xpath: String,
        /// Why, from [`rim_analyzer::extract::xpath_expr::XPathExpr::Unsupported`]
        /// or this module's own def-identity check.
        reason: String,
    },
    /// A contribution's own XML text failed to parse.
    #[error("{mod_id} shipped a patch operation that isn't well-formed XML")]
    MalformedOperation {
        /// The mod that shipped it.
        mod_id: ModId,
    },
}

/// What replaying every contribution produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayOutcome {
    /// The tree after every contribution has been applied (or attempted).
    pub tree: FieldTree,
    /// Non-blocking issues collected along the way.
    pub caveats: Vec<Caveat>,
    /// `Some(error)` when replay had to stop early — the whole replay
    /// stops there (no partial result is ever presented as final); the
    /// caller (`plan::plan_patch_collision`) turns this into
    /// `MergeState::CannotMerge`.
    pub error: Option<ReplayError>,
    /// One entry per top-level contribution this call actually reached
    /// (fewer than `contributions.len()` when [`Self::error`] stopped the
    /// loop early — the same truncation [`Self::caveats`] already has).
    /// Read by `VerifyOrder` for success suppression at top-level
    /// granularity — **alongside [`Self::caveats`], never a replacement
    /// for it**: [`Caveat::FailedOp`]'s own "a leaf matching nothing is
    /// always worth surfacing, regardless of `<success>`" semantics are
    /// correct for a merge preview and every other caller, and are
    /// unaffected by this field. See
    /// [`TopLevelOutcome`]'s own doc comment.
    pub top_level_outcomes: Vec<TopLevelOutcome>,
    /// How many operations this replay refused to turn into a *prediction*
    /// purely because their xpath used a **filter head**: a head that is a
    /// global query by construction (`[@ParentName="X"]`, a bare-type
    /// `Defs/ThingDef/…`, an `and`-composed content filter) selected
    /// nothing *on this def*, which says nothing at all about whether it
    /// selected something on some other def. Such an operation is reported
    /// as "succeeded elsewhere" with no [`Caveat::FailedOp`]; this counter
    /// is the honest record of how many would-be predictions that
    /// conservatism suppressed, and the number that decides whether to
    /// lift it. Group B's own child-value head (`resolve_head_content_predicate`)
    /// **is** counted here too — RimWorld evaluates it as a whole-document
    /// query exactly like the other filter heads (the real game evaluates
    /// a `PatchOperationConditional`'s `<xpath>` over the whole combined
    /// document), so an empty selection on this one def is suppressed the
    /// same way.
    pub suppressed_filter_head_ops: usize,
    /// The mod whose whole-def `Remove` emptied the def being replayed,
    /// if any — `Some` from
    /// the first contribution (in order) whose own replay pushed
    /// [`Caveat::DefRemoved`], `None` otherwise. A caller that replays
    /// contribution-by-contribution (`effective::compute`) reads this to
    /// rebuild its own [`ReplayContext`] with `this_def_present: false`
    /// for the next contribution, rather than re-deriving the fact from
    /// [`Self::caveats`] by hand, so the removal is an explicit, named
    /// carry.
    pub def_removed_by: Option<ModId>,
}

/// One top-level `<Operation>` `replay` actually processed, and what
/// RimWorld itself would report about it at its own top-level
/// `PatchOperation.Apply()` boundary — the only thing RimWorld ever logs
/// a "Patch operation ... failed" line for (never a nested leaf on its
/// own, if an enclosing `<success>` swallowed it). Verify's precision
/// depends on this boundary: `<success>Always</success>` is a common
/// compat-patch idiom on real installs, telling the engine a miss is
/// *expected* and must never be logged, which an "every
/// `Caveat::FailedOp` is a finding" reading has no way to honour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopLevelOutcome {
    /// The mod that shipped this top-level operation.
    pub mod_id: ModId,
    /// Whether this operation's own final, top-level outcome — after
    /// every enclosing `<success>Always`/`Invert`/`Never` along the way —
    /// is a success. This is the *same* boolean [`apply_operation`]'s own
    /// top-level call already computes (through its full recursive
    /// `<success>` propagation, unchanged by this field's own addition);
    /// it was simply discarded by every caller until now. `false` means
    /// RimWorld logs a failure naming [`Self::identity`]; `true` means
    /// nothing is logged, however many [`Caveat::FailedOp`]s
    /// [`Self::caveats`] still carries.
    pub succeeded: bool,
    /// RimWorld's own log identity for this operation — see
    /// [`operation_identity`]'s own doc comment for exactly which shapes
    /// this reproduces from real log evidence and which are inferred,
    /// not confirmed against one.
    pub identity: String,
    /// Every caveat *this* contribution's own replay raised — a private
    /// copy of exactly the slice `replay`'s own flat
    /// [`ReplayOutcome::caveats`] gained from processing this one
    /// contribution alone, never any other's. Lets a caller recover
    /// which specific leaf a `false` outcome traces to, for diagnosis,
    /// without needing to correlate against the flat list by hand.
    pub caveats: Vec<Caveat>,
    /// The `<xpath>` of the actual leaf operation that made this
    /// top-level operation fail — `None` when it succeeded, or when the
    /// failing node genuinely carries no `<xpath>` of its own (a bare
    /// `PatchOperationFindMod`, rare). **Tracked structurally through the
    /// real control flow as replay runs** — never derived after the fact
    /// by scanning [`Self::caveats`], which goes wrong in two real, probed
    /// ways: a `PatchOperationTest` that fails pushes no
    /// caveat at all (so a caveat-scan either finds nothing or, worse,
    /// finds an unrelated one), and an earlier `<success>Always>` sibling
    /// that itself matched nothing still leaves its own `Caveat::FailedOp`
    /// behind (by design — see `apply_success_mode`'s own doc comment)
    /// even though it never stopped anything and is not why this
    /// operation ultimately failed. [`Self::identity`]'s own
    /// `lastFailedOperation=` (for a sequence-shaped operation) is built
    /// from this exact same tracked node, so the two can never disagree
    /// about which leaf is to blame.
    pub failed_leaf_xpath: Option<String>,
}

/// The one mutable side-channel every `apply_*` helper writes to while a
/// replay runs: the flat caveat list, plus the filter-head
/// suppressed-prediction counter. One struct rather than two
/// parallel `&mut` parameters through ~20 private helpers — and the
/// counter has to travel exactly as far as the caveats do, since both
/// are raised at the same place, [`resolve_selection`].
#[derive(Debug, Default)]
struct ReplayLog {
    caveats: Vec<Caveat>,
    suppressed_filter_head_ops: usize,
}

/// Replays every contribution against `tree`, in the order given (the
/// caller guarantees load-order-then-file-order-then-document-order —
/// exactly what `SourceIndex::patch_ops_by_def` already provides).
#[must_use]
pub fn replay(
    tree: FieldTree,
    contributions: &[PatchContribution<'_>],
    context: &ReplayContext<'_>,
) -> ReplayOutcome {
    let mut tree = tree;
    let mut log = ReplayLog::default();
    let mut top_level_outcomes = Vec::new();
    // An owned, rebuildable copy of `context` (it's `Copy`) rather than
    // the borrowed parameter — once a contribution empties the def
    // (below), every *later* contribution in this same call must see
    // `this_def_present: false` too, and this is a rebuilt value passed
    // in fresh each iteration, never a mutation through a shared
    // reference.
    let mut current_context = *context;
    let mut def_removed_by: Option<ModId> = None;
    for contribution in contributions {
        // `roxmltree::Document::parse` below is a recursive-descent parser
        // over raw element nesting and can overflow the stack on
        // pathologically deep mod-provided XML **before** either its own
        // error return or `crate::xml::MAX_DEPTH`'s later, post-parse
        // check ever run (see `crate::xml::MAX_RAW_ELEMENT_DEPTH`'s own
        // doc comment) — reject it the same way a malformed operation
        // already is, rather than ever calling `parse` on it at all.
        if crate::xml::raw_element_nesting_exceeds(
            contribution.operation_xml,
            crate::xml::MAX_RAW_ELEMENT_DEPTH,
        ) {
            log.caveats.push(Caveat::MalformedOperation {
                mod_id: contribution.mod_id.clone(),
            });
            return ReplayOutcome {
                tree,
                caveats: log.caveats,
                suppressed_filter_head_ops: log.suppressed_filter_head_ops,
                error: Some(ReplayError::MalformedOperation {
                    mod_id: contribution.mod_id.clone(),
                }),
                top_level_outcomes,
                def_removed_by,
            };
        }
        let Ok(doc) = roxmltree::Document::parse(contribution.operation_xml) else {
            log.caveats.push(Caveat::MalformedOperation {
                mod_id: contribution.mod_id.clone(),
            });
            return ReplayOutcome {
                tree,
                caveats: log.caveats,
                suppressed_filter_head_ops: log.suppressed_filter_head_ops,
                error: Some(ReplayError::MalformedOperation {
                    mod_id: contribution.mod_id.clone(),
                }),
                top_level_outcomes,
                def_removed_by,
            };
        };
        let node = doc.root_element();
        if !node.is_element() {
            continue;
        }
        // Snapshot before/after this one contribution so `TopLevelOutcome::caveats`
        // holds exactly its own leaves' caveats, never a mix with an
        // earlier contribution's — `caveats` stays the one flat vec
        // every existing caller already reads, unchanged.
        let caveats_before = log.caveats.len();
        let mut failed_leaf: Option<Node<'_, '_>> = None;
        let outcome = apply_operation(
            node,
            &mut tree,
            &current_context,
            contribution.mod_id,
            &mut log,
            &mut failed_leaf,
        );
        let own_caveats = log.caveats[caveats_before..].to_vec();
        // A whole-def `Remove` pushes `Caveat::DefRemoved`
        // unconditionally (the underlying XML mutation happens whatever
        // `<success>` later reports, exactly like `Caveat::FailedOp`) —
        // read back here, once, so every contribution *after* this one
        // (in this call, and — via `ReplayOutcome::def_removed_by` — in
        // `effective::compute`'s own outer fold) replays against an
        // absent def. Checked before the `Ok`/`Err` match below so a
        // removal that happened earlier in this same top-level operation
        // is still recorded even if a later, unrelated leaf then errors.
        if def_removed_by.is_none() {
            def_removed_by = own_caveats.iter().find_map(|caveat| match caveat {
                Caveat::DefRemoved { mod_id } => Some(mod_id.clone()),
                _ => None,
            });
            if def_removed_by.is_some() {
                current_context.this_def_present = false;
            }
        }
        match outcome {
            Ok(succeeded) => {
                top_level_outcomes.push(TopLevelOutcome {
                    mod_id: contribution.mod_id.clone(),
                    succeeded,
                    identity: operation_identity(node, failed_leaf),
                    caveats: own_caveats,
                    failed_leaf_xpath: failed_leaf.and_then(|leaf| child_text(leaf, "xpath")),
                });
            }
            Err(error) => {
                return ReplayOutcome {
                    tree,
                    caveats: log.caveats,
                    suppressed_filter_head_ops: log.suppressed_filter_head_ops,
                    error: Some(error),
                    top_level_outcomes,
                    def_removed_by,
                };
            }
        }
    }
    ReplayOutcome {
        tree,
        caveats: log.caveats,
        suppressed_filter_head_ops: log.suppressed_filter_head_ops,
        error: None,
        top_level_outcomes,
        def_removed_by,
    }
}
