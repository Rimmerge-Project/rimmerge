//! [`FindingKey`]: the stable, rerere-style identity of one thing the
//! ledger asks about, and [`Finding`]: the evidence behind it.
//!
//! A decision applies iff its key matches exactly. Owner-set keys change
//! when a new owner joins an existing conflict (the conflict is
//! materially different, so an old decision must not silently apply);
//! pair-keyed findings survive list churn since the pair itself is the
//! identity.
//!
//! [`FindingKey`] renders to and parses from a canonical text form used
//! verbatim as the key in the decisions file — see [`FindingKey`]'s
//! `Display`/`FromStr` impls for the exact grammar. That grammar uses
//! `:`, `[`, `]`, and `,` as structural delimiters: field values (mod
//! ids, def names, texture paths, ...) must not contain them, which
//! holds for every value RimWorld itself can produce.

use rim_analyzer::domain::{
    AffectedDef, DanglingCause, DefRefParts, EdgeKind, EdgeStrength, InheritanceProblem, ModId,
    ModReferenceKind, NearMissRule, RefSiteSummary, Selector, XmlLocator,
};

use crate::domain::rule::{Placement, RuleOrigin};
use crate::domain::tag::{Tag, TagSignal};
use crate::sort::Layer;

mod key;
mod key_text;

pub use key::{DefKey, FindingKey};
pub use key_text::FindingKeyParseError;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "finding/finding_tests.rs"]
mod tests;

/// The edge that overruled a [`FindingKey::EdgeDropped`]'s edge in a
/// direct two-mod contradiction — mirrors [`crate::sort::OrderingEdge`]'s
/// own shape plus its layer,
/// rather than embedding that type directly: [`Finding`] carries plain
/// evidence [`crate::ledger::extract_findings`] derives from
/// [`crate::sort::SortOutcome`], not sort-module types verbatim, so a
/// future change to `OrderingEdge`'s own shape doesn't ripple into every
/// already-serialized/displayed `Finding`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeWinner {
    /// The winning edge's dependent side.
    pub after: ModId,
    /// The winning edge's dependency side.
    pub before: ModId,
    /// Which layer the winning edge belongs to.
    pub layer: Layer,
    /// A human-readable description of the winning edge (an engine
    /// edge's own `detail`, or a description of the rule/any-of choice).
    pub detail: String,
}

/// Why [`Finding::PatchWillFail`]'s operation is predicted to fail. The
/// first two are order-fixable (a `Reorder` against
/// the named mod is a real alternative); the last two are not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchFailureCause {
    /// An earlier-loading mod's own operation removes the node this
    /// operation targets (a `PatchRemovedNode` edge, or a same-def
    /// attribute-predicate remover that edge rule can't structurally see) — loading
    /// after that mod instead would let this operation run first.
    ///
    /// **This variant does not mean "a `PatchRemovedNode` edge exists"; it
    /// means "an earlier mod's operation destroys the node".** There is a
    /// second evidence route to exactly that fact:
    /// `rim_merge::effective::counterfactual` *replays* the def with the
    /// failing mod's whole contribution block moved, and names the mod
    /// whose block the subject had to cross to succeed. That is a
    /// demonstration rather than an inference from a pair-deduped static
    /// edge, and a consumer must not read this variant as implying an
    /// edge is present in the report.
    RemovedBy(ModId),
    /// This operation's own target is a node a *later*-loading mod's own
    /// `PatchOperationAdd` injects (a `PatchInjectedNode` edge) —
    /// loading after that mod instead would let this operation find it.
    ///
    /// Same second evidence route as [`Self::RemovedBy`]: the fact this
    /// variant asserts is "a later mod injects the target", and
    /// `rim_merge::effective::counterfactual` can demonstrate it by replay
    /// where no `PatchInjectedNode` edge was ever produced. Not an
    /// assertion that such an edge exists.
    NotYetInjected(ModId),
    /// The winning owner's own resolved structure never has this node at
    /// all, or removes it unconditionally regardless of load order — no
    /// reorder fixes it. Also the classification for a target def/template
    /// with no active owner anywhere (checked before any replay even
    /// runs — cheaper than watching the operation fail) and for an
    /// operation whose target only ever existed on a losing owner's own
    /// copy, never the winner's.
    DeadTarget,
    /// Neither of the above explains the failure. A high count here is a
    /// signal the classifier itself is missing a case, not an acceptable
    /// steady state.
    Unknown,
}

/// Whether [`Finding::PatchWillFail`]'s own `Reorder`-bearing cause
/// (`RemovedBy`/`NotYetInjected`) would change the def RimWorld actually
/// ends up with, or only which mod's own operation logs as failed.
/// [`Finding::PatchWillFail::reorder_kind`] is `None` for the two causes
/// that offer no reorder at all (`DeadTarget`/`Unknown`) — an illegal
/// `Cosmetic`-with-no-reorder or `Content`-with-no-reorder state is
/// unrepresentable by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReorderKind {
    /// Enforcing the reorder keeps content a different mod would
    /// otherwise lose — the ordinary case, and the only one that existed
    /// before `rim_merge::effective::counterfactual` could tell the two
    /// apart. Also the classification for the edge-evidence route
    /// (`RemovedBy`/`NotYetInjected` read straight off a
    /// `PatchRemovedNode`/`PatchInjectedNode` edge, never the *cosmetic*
    /// `PatchRemovedNodeCosmetic` kind, which never feeds that route at
    /// all).
    Content,
    /// The final resolved def is identical whichever mod loads first —
    /// proven by `rim_merge::effective::counterfactual`'s own
    /// `CounterfactualFix::final_def_unchanged` (the same "only the op
    /// that logs `failed` differs" shape a static cosmetic edge names,
    /// here demonstrated by replay rather than inferred from a static
    /// edge). A `set-pair` rule is not worth offering for a row
    /// classified this way.
    Cosmetic {
        /// An existing `PatchCollision` finding at the same def and
        /// sub_path, if the ledger has one — the way to keep the losing
        /// mod's intent instead of a reorder that would change nothing.
        /// `None` when no such finding exists.
        existing_merge: Option<FindingKey>,
    },
}

/// The full evidence behind a [`FindingKey`], for display.
///
/// Mirrors [`FindingKey`] one variant at a time, carrying the identity
/// fields plus whatever's already available from [`rim_analyzer`]'s
/// domain types, the sorter's witness cycles and placement explanations,
/// and the evaluator's order-aware evidence.
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// See [`FindingKey::EdgeDropped`].
    EdgeDropped {
        /// The edge's dependent side.
        after: ModId,
        /// The edge's dependency side.
        before: ModId,
        /// The kind of edge dropped.
        kind: EdgeKind,
        /// The engine's own description of the edge.
        detail: String,
        /// The dropped edge's actual strength — read off the layer it was
        /// dropped from (`sort::Layer::{Hard,Declared,Soft,Awareness}`),
        /// never re-derived from `kind` alone: `EdgeKind::strength()` can't
        /// tell a lazily-resolved `AssemblyRef` (`Soft`) from a load-time
        /// one (`Hard`) by itself (that distinction is the specific
        /// `Edge`'s own `load_time` flag, which no longer travels with the
        /// dropped edge by the time a `Finding` is built).
        strength: EdgeStrength,
        /// The edge that overruled this one, when the witness cycle was a
        /// direct two-mod contradiction. `None` for a longer cycle.
        winner: Option<EdgeWinner>,
    },
    /// See [`FindingKey::DeclarationQuestioned`].
    DeclarationQuestioned {
        /// The enforced edge's dependent side.
        declared_after: ModId,
        /// The enforced edge's dependency side.
        declared_before: ModId,
        /// Which layer the declared edge belongs to.
        declared_layer: Layer,
        /// A human-readable description of the declared edge.
        declared_detail: String,
        /// The advisory relation's kind.
        relation_kind: EdgeKind,
        /// A human-readable description of the advisory relation.
        relation_detail: String,
    },
    /// See [`FindingKey::DeclarationOverridden`].
    DeclarationOverridden {
        /// The declared edge's dependent side.
        declared_after: ModId,
        /// The declared edge's dependency side.
        declared_before: ModId,
        /// The kind of declared edge overridden.
        kind: EdgeKind,
        /// A human-readable description of the declared edge.
        detail: String,
        /// The declared-edge-override rule's edge that won.
        by: EdgeWinner,
    },
    /// See [`FindingKey::AnyOfChoice`].
    AnyOfChoice {
        /// The mod requiring one of the candidates.
        after: ModId,
        /// The shared assembly name.
        assembly: String,
        /// Every mod that could satisfy the constraint.
        candidates: Vec<ModId>,
    },
    /// See [`FindingKey::DefOverride`].
    DefOverride {
        /// The contested def.
        key: DefKey,
        /// Every owner, in the order the scan saw them (not the order the
        /// ledger was built for; read [`Self::DefOverride::winner`] for
        /// that).
        owners: Vec<ModId>,
        /// The owner loaded last under the order this ledger was built
        /// for (`Current` or `Suggested`), whose def actually applies.
        winner: ModId,
    },
    /// See [`FindingKey::PatchCollision`].
    PatchCollision {
        /// The patched def.
        key: DefKey,
        /// Which attribute the patch's predicate matched on.
        selector: Selector,
        /// The path under the def the patches target, if any.
        sub_path: Option<String>,
        /// Every contributing mod, in the order the scan saw them.
        mods: Vec<ModId>,
        /// The contributor whose operation runs last under the order this
        /// ledger was built for (`Current` or `Suggested`).
        winner: ModId,
    },
    /// See [`FindingKey::TextureOverride`].
    TextureOverride {
        /// The shared, normalized texture path.
        texture_path: String,
        /// Every owner, in the order the scan saw them (not the order the
        /// ledger was built for; read [`Self::TextureOverride::winner`]
        /// for that).
        owners: Vec<ModId>,
        /// The owner loaded last under the order this ledger was built
        /// for (`Current` or `Suggested`), whose file the game uses.
        winner: ModId,
    },
    /// See [`FindingKey::DuplicateAssembly`].
    DuplicateAssembly {
        /// The shared assembly name.
        assembly_name: String,
        /// Every shipping mod, in load order.
        owners: Vec<ModId>,
    },
    /// See [`FindingKey::DuplicateTemplateName`].
    DuplicateTemplateName {
        /// The shared template name.
        name: String,
        /// Every registering mod, in load order.
        owners: Vec<ModId>,
    },
    /// See [`FindingKey::KeyedTranslationCollision`].
    KeyedTranslationCollision {
        /// One of the two mods.
        a: ModId,
        /// The other.
        b: ModId,
        /// Every key this pair collides over.
        keys: Vec<String>,
    },
    /// See [`FindingKey::SoundOverride`].
    SoundOverride {
        /// The shared, normalized sound path.
        path: String,
        /// Every owner, in load order.
        owners: Vec<ModId>,
    },
    /// See [`FindingKey::UndeclaredTypeDependency`].
    UndeclaredTypeDependency {
        /// The mod using the type.
        user: ModId,
        /// The mod whose DLL defines the type.
        provider: ModId,
        /// The type name.
        type_name: String,
    },
    /// See [`FindingKey::RuntimePatchCollision`].
    RuntimePatchCollision {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
        /// Every patching mod, in load order.
        owners: Vec<ModId>,
    },
    /// See [`FindingKey::TranspilerCollision`].
    TranspilerCollision {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
        /// Every mod declaring a `Transpiler` role on it, in load order.
        owners: Vec<ModId>,
    },
    /// See [`FindingKey::RuleOverruled`].
    RuleOverruled {
        /// The losing rule's dependent side.
        after: ModId,
        /// The losing rule's dependency side.
        before: ModId,
        /// The losing rule's origin.
        origin: RuleOrigin,
        /// The rule's own free-text note, if any.
        comment: Option<String>,
        /// The edge that overruled it, when the witness cycle was a
        /// direct two-mod contradiction. `None` for a longer cycle — see
        /// `witness_cycle` for that case instead.
        winner: Option<EdgeWinner>,
        /// A path from `after` back around to `before`, entirely through
        /// edges still accepted at the time of the drop — populated only
        /// when `winner` is `None` (a cycle of three or more mods names the
        /// witness cycle instead).
        witness_cycle: Vec<ModId>,
        /// Whether the *losing* rule is itself a declared-edge override
        /// (`origin == UserDecision && overrides_declared`) —
        /// `ledger::suggest::rule_overruled_confidence`'s own tie-break
        /// signal: a `Layer::DeclaredOverride` winner beating a *plain*
        /// `UserDecision`/imported rule is a genuine, asymmetric
        /// cross-layer win (that layer sits ahead of every other
        /// rule-origin layer by design), but two declared-edge-override
        /// rules directly contradicting each other is the same
        /// same-layer-tie shape as a `UserDecision`-vs-`UserDecision`
        /// tie — `RuleOrigin::UserDecision` alone cannot tell the two
        /// apart, since it maps to two different layers
        /// (`Layer::UserDecision` and `Layer::DeclaredOverride`,
        /// depending on this very flag), so this field carries that bit.
        overrides_declared: bool,
    },
    /// See [`FindingKey::PlacementOverruled`].
    PlacementOverruled {
        /// The pinned mod.
        mod_id: ModId,
        /// Which tier it was pinned to.
        placement: Placement,
        /// The placement rule's origin.
        origin: RuleOrigin,
        /// The edge that overruled the placement.
        by: EdgeWinner,
        /// Where the mod actually landed in the emitted order.
        landed_at: usize,
    },
    /// See [`FindingKey::PlacementQuestioned`].
    PlacementQuestioned {
        /// The pinned mod.
        mod_id: ModId,
        /// Which tier it's pinned to.
        placement: Placement,
        /// The other mod named by the advisory relation — not part of the
        /// key (see [`FindingKey::PlacementQuestioned`]'s own doc comment
        /// on why), but needed here so the `Reorder` alternative can name
        /// the relation's own direction.
        other: ModId,
        /// The advisory relation's kind.
        relation_kind: EdgeKind,
        /// A human-readable description of the advisory relation.
        relation_detail: String,
    },
    /// See [`FindingKey::PlacementOrderingOverridden`].
    PlacementOrderingOverridden {
        /// The mod forced across the pin's own extreme edge.
        mod_id: ModId,
        /// The pin it could not be sorted around.
        pinned: ModId,
        /// Which tier the pin occupies.
        placement: Placement,
        /// The accepted edge that forced this ordering.
        by: EdgeWinner,
    },
    /// See [`FindingKey::PlacementPromotesDependents`].
    PlacementPromotesDependents {
        /// The pinned mod.
        mod_id: ModId,
        /// Which tier it's pinned to.
        placement: Placement,
        /// Every other mod attributed to this placement's own promotion,
        /// sorted — an attribution (via
        /// `sort::cycles::find_promotion_cause`'s nearest-placed-node BFS),
        /// not necessarily the placement's *entire* transitive reach: a mod
        /// promoted past more than one pin's boundary is attributed to
        /// whichever one the BFS reaches first, never counted here twice.
        promoted: Vec<ModId>,
    },
    /// See [`FindingKey::LikelyDuplicateMod`].
    LikelyDuplicateMod {
        /// One of the two mods.
        a: ModId,
        /// The other.
        b: ModId,
        /// Identical `(def_type, def_name)` keys both mods define.
        shared_defs: usize,
    },
    /// See [`FindingKey::MissingMod`].
    MissingMod {
        /// The missing mod.
        mod_id: ModId,
    },
    /// See [`FindingKey::MissingDependency`].
    MissingDependency {
        /// The mod with the unmet dependency.
        mod_id: ModId,
        /// The missing dependency.
        dependency: ModId,
        /// The display name the author gave the dependency.
        display_name: Option<String>,
    },
    /// See [`FindingKey::IncompatiblePair`].
    IncompatiblePair {
        /// One of the two mods.
        a: ModId,
        /// The other.
        b: ModId,
    },
    /// See [`FindingKey::UnsupportedVersion`].
    UnsupportedVersion {
        /// The mod.
        mod_id: ModId,
    },
    /// See [`FindingKey::UndeclaredHardDependency`].
    UndeclaredHardDependency {
        /// The edge's dependent side.
        after: ModId,
        /// The edge's dependency side.
        before: ModId,
        /// The engine's own description of the edge.
        detail: String,
    },
    /// See [`FindingKey::LazyReferenceViolated`].
    LazyReferenceViolated {
        /// The edge's dependent side, base id.
        after: ModId,
        /// The edge's dependency side, base id.
        before: ModId,
        /// The engine's own description of the edge.
        detail: String,
    },
    /// See [`FindingKey::TagInferred`].
    TagInferred {
        /// The tagged mod.
        mod_id: ModId,
        /// The inferred tag.
        tag: Tag,
        /// Every signal that matched.
        matched: Vec<TagSignal>,
    },
    /// See [`FindingKey::MissingTexturePath`].
    MissingTexturePath {
        /// The mod whose own copy of `def` carries this field.
        referrer: ModId,
        /// The def carrying the field.
        def: DefKey,
        /// The field's own tag name.
        field: String,
        /// The normalized path that resolved to no shipped file.
        path: String,
    },
    /// See [`FindingKey::PatchWillFail`].
    PatchWillFail {
        /// The mod whose operation is predicted to fail.
        mod_id: ModId,
        /// The def or template the operation targets.
        def_key: DefKey,
        /// Which attribute the operation's own target predicate matched
        /// on.
        selector: Selector,
        /// The **top-level** operation's own RimWorld-log identity text —
        /// see [`FindingKey::PatchWillFail`]'s own doc comment. Matches
        /// what a user sees in their own log.
        operation: String,
        /// The specific nested leaf mutation's own xpath, when the
        /// top-level failure traces to one identifiable leaf
        /// (`rim_merge::patch_eval::TopLevelOutcome::caveats`'s own last
        /// `Caveat::FailedOp`) — `None` when the top-level operation
        /// failed for a reason that never produced a
        /// `Caveat::FailedOp` at all (e.g. a bare `PatchOperationTest`).
        /// Diagnostic only, deliberately excluded from
        /// [`FindingKey::PatchWillFail`] (identity/evidence split, the
        /// same convention `EdgeDropped`/`RuleOverruled` already use for
        /// their own derived detail).
        leaf_xpath: Option<String>,
        /// Why, classified from the replay.
        cause: PatchFailureCause,
        /// Whether the `Reorder` this row's `cause` implies (if any)
        /// would change the final resolved def — `None` for
        /// `DeadTarget`/`Unknown`, which offer no reorder at all. See
        /// [`ReorderKind`].
        reorder_kind: Option<ReorderKind>,
    },
    /// See [`FindingKey::ContributesNothing`].
    ContributesNothing {
        /// The mod whose every contribution is inert.
        mod_id: ModId,
    },
    /// See [`FindingKey::UndecodableTexture`].
    UndecodableTexture {
        /// The mod shipping the file.
        mod_id: ModId,
        /// The normalized key.
        path: String,
        /// The DDS header's own width, in pixels.
        width: u32,
        /// The DDS header's own height, in pixels.
        height: u32,
        /// The pixel format's four-character-code tag; empty when the
        /// header was too malformed to read one.
        fourcc: String,
        /// Whether this mod also ships a non-`.dds` file at the same key
        /// (never actually loaded instead — evidence only).
        has_png_sibling: bool,
    },
    /// See [`FindingKey::BrokenInheritance`].
    BrokenInheritance {
        /// The mod whose def/template references `parent_name`.
        mod_id: ModId,
        /// The unresolved or wrong-typed `ParentName` value.
        parent_name: String,
        /// The representative referencing def/template — see
        /// [`rim_analyzer::domain::BrokenInheritance::child`].
        child: DefRefParts,
        /// Which problem, and (for a type mismatch) what it resolved to
        /// instead.
        problem: InheritanceProblem,
        /// Every active, gate-open concrete def that loses inherited
        /// content, bounded — see
        /// [`rim_analyzer::domain::BrokenInheritance::affected`].
        affected: Vec<AffectedDef>,
        /// How many more affected defs existed beyond `affected`'s own
        /// cap. `0` when nothing was truncated.
        truncated: usize,
    },
    /// See [`FindingKey::NearMissModReference`].
    NearMissModReference {
        /// The mod whose own file carries the written reference.
        referrer: ModId,
        /// Whether `written` is a `FindMod` name or a `MayRequire` id.
        kind: ModReferenceKind,
        /// The value as written.
        written: String,
        /// The closest active mod.
        candidate: ModId,
        /// `candidate`'s own display name, for evidence.
        candidate_name: String,
        /// Which similarity rule flagged this pair.
        rule: NearMissRule,
        /// Where the written value came from, when the extracted site
        /// carried one — see [`rim_analyzer::domain::XmlLocator`].
        locator: Option<XmlLocator>,
    },
    /// See [`FindingKey::DiscardedAddition`].
    DiscardedAddition {
        /// The mod whose `PatchOperationReplace` discards `adder`'s own
        /// content.
        replacer: ModId,
        /// The mod whose earlier addition is discarded.
        adder: ModId,
        /// The replaced def.
        def: DefKey,
        /// The replaced node's own display path.
        path: String,
        /// `adder`'s own op's target display path — evidence of exactly
        /// what was discarded.
        adder_path: String,
    },
    /// See [`FindingKey::DanglingDefReference`].
    DanglingDefReference {
        /// The dangling `defName`.
        name: String,
        /// Every distinct referrer, bounded — see
        /// [`rim_analyzer::domain::DanglingDefReference::referrers`].
        referrers: Vec<RefSiteSummary>,
        /// How many more referrers existed beyond `referrers`'s own cap.
        truncated_referrers: usize,
        /// Why the name never resolves — see
        /// [`rim_analyzer::domain::DanglingCause`].
        cause: DanglingCause,
        /// Set when the voted field's own resolved values are mostly
        /// `SoundDef`s — see
        /// [`rim_analyzer::domain::DanglingDefReference::likely_sound`].
        likely_sound: bool,
    },
}

/// Collapses every run of whitespace (including embedded newlines and
/// tabs) to a single space and trims the ends — so
/// [`FindingKey::PatchWillFail::operation`]/[`Finding::PatchWillFail::leaf_xpath`],
/// which can embed real newlines and tabs from a multi-line xpath
/// (`verify --json`'s own captured text, e.g. a real-install case's own
/// four-way `@Name` disjunction), compare equal to how RimWorld's own
/// `Player.log` renders the identical text on one line.
///
/// Lives here (`rim-resolve`, the crate `FindingKey::PatchWillFail`
/// itself lives in) rather than in `rim-io`'s log parser: the
/// `Player.log` import (`rim-io`/`rim-session`) needs this to join a
/// parsed failure back to a predicted one, but so does whatever builds
/// that join (`rim-session` or a composition root) —
/// neither can depend on `rim-io` (root `CLAUDE.md`'s layering), while
/// every layer above `rim-resolve` can already reach this crate.
#[must_use]
pub fn normalize_log_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
