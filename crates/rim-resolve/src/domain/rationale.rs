//! [`Rationale`]: the ledger's own structured "why" for a [`super::Suggestion`]/
//! [`super::Alternative`], replacing what used to be a hand-built
//! `String` one template at a time: the frontend renders a localized
//! sentence from a variant's own typed fields; `Display` reproduces the
//! original English text, unchanged, for the CLI, `rimmerge verify`'s
//! generated `--comment`, and every existing rationale-text assertion
//! across this workspace — that byte-for-byte match was this migration's
//! own oracle. Every `ledger::suggest` module
//! (`edges`/`rules`/`defs`/`assets`/`mods`) and `domain::resolution`'s
//! own redecide functions construct a real, typed variant below; there is
//! no untyped fallback — a caller with genuinely one-off text has no
//! variant to reach for, which is deliberate.

use rim_analyzer::domain::{DanglingCause, EdgeKind, ModId, ModReferenceKind};

use super::finding::EdgeWinner;
use super::rule::{Placement, RuleOrigin};
use crate::sort::Layer;

/// A short, human phrase for one [`Layer`] — shared by every `Rationale`
/// variant that names which kind of evidence a winning edge or rule was.
fn layer_phrase(layer: Layer) -> &'static str {
    match layer {
        Layer::Hard => "a hard (load-time) requirement",
        Layer::AnyOf => "a resolved any-of requirement",
        Layer::DeclaredOverride => "your own explicit override of the author's declaration",
        Layer::Declared => "an author declaration",
        Layer::UserDecision => "your own decision",
        Layer::RimSortUser => "a RimSort user rule",
        Layer::RimSortCommunity => "a RimSort community rule",
        Layer::SteamDb => "a Steam Workshop dependency",
        Layer::Soft | Layer::Awareness | Layer::Inferred => "another edge",
    }
}

/// A short, human phrase for one [`Placement`].
fn placement_phrase(placement: Placement) -> &'static str {
    match placement {
        Placement::Top => "at the top",
        Placement::Bottom => "at the bottom",
    }
}

/// The fixed, per-[`EdgeKind`] explanation for why an advisory relation
/// needs no order — shared by both [`Rationale::DeclarationQuestioned`]
/// and [`Rationale::PlacementQuestioned`]'s `Display` arms.
fn relation_explanation(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::FindMod | EdgeKind::MayRequire | EdgeKind::IfModActive => {
            "this only tests whether a mod is present, not its load position"
        }
        EdgeKind::PatchTargetsDef => {
            "patches apply to the merged def document after every mod's defs are loaded, regardless of load order"
        }
        EdgeKind::ParentTemplate => {
            "unreachable: a ParentName edge is a load-time ordering fact (Hard), never advisory"
        }
        EdgeKind::UsesType => {
            "the type is resolved across every loaded assembly regardless of load order"
        }
        EdgeKind::PatchSelectsInjectedNode => {
            "the selected node may already exist another way (written inline, injected by more than one active mod, or injected by the selecting mod's own patch), or lies deeper than the analyzer indexes to check — so this isn't a proven load-order requirement"
        }
        EdgeKind::PatchRemovedNode => {
            "the analyzer infers this from patch content: one mod removes a node another mod also patches, and the remover must load last or the other patch finds nothing left to touch — not an author's own declared load order, so it yields when a declaration or placement disagrees"
        }
        EdgeKind::RetextureAfterOwner => {
            "the analyzer infers this from a shared texture path: a texture-only retexture pack must load after the content mod it reskins, or its replacement is overwritten by the content it's meant to retexture — not an author's own declared load order, so it yields when a declaration or placement disagrees"
        }
        EdgeKind::DefOverrideAfterOrigin => {
            "the analyzer infers this from template/def-count evidence: a mod that also ships a def belonging to another mod's own namespace must load after that namespace's owner, or its override lands under the origin's own definition instead of on top of it — not an author's own declared load order, so it yields when a declaration or placement disagrees"
        }
        EdgeKind::PatchInvalidatesPredicate => {
            "the analyzer infers this from patch content: a Replace/Remove rewrites or deletes a node another mod's own xpath predicate reads, and the predicate-reading op must load first or its own predicate no longer matches — not an author's own declared load order, so it yields when a declaration or placement disagrees"
        }
        EdgeKind::PatchRemovedNodeCosmetic => {
            "the analyzer infers this from patch content, but the final defs are identical either order here — only which mod's own op logs the failure changes — so it's weaker evidence than an author's own declared load order and yields when a declaration or placement disagrees"
        }
        EdgeKind::ReplaceDiscardsAddition => {
            "the analyzer infers this from patch content: a Replace swaps in a whole new node, silently discarding whatever an earlier addition wrote into it, so the addition must load after the replace or its own content is lost with no log line at all — not an author's own declared load order, so it yields when a declaration or placement disagrees"
        }
        EdgeKind::AssemblyRef
        | EdgeKind::ForceLoadAfter
        | EdgeKind::ForceLoadBefore
        | EdgeKind::LoadAfter
        | EdgeKind::LoadBefore
        | EdgeKind::ModDependency
        | EdgeKind::PatchInjectedNode
        | EdgeKind::AssemblyVersionPrecedence => {
            "this is evidence the mods interact, not an ordering fact the loader itself needs"
        }
    }
}

/// The phrase `ledger::suggest::rules::rule_overruled` builds for the
/// overruled rule itself — shared by [`Rationale::RuleOverruledLongerCycle`]
/// and [`Rationale::RuleOverruledByWinner`]'s `Display` arms.
fn rule_phrase(
    after: &ModId,
    before: &ModId,
    origin: RuleOrigin,
    overrides_declared: bool,
) -> String {
    if origin == RuleOrigin::UserDecision {
        if overrides_declared {
            "your own declared-edge override".to_string()
        } else {
            "your own decision".to_string()
        }
    } else {
        let layer = match origin {
            RuleOrigin::UserDecision => Layer::UserDecision,
            RuleOrigin::RimSortUser => Layer::RimSortUser,
            RuleOrigin::RimSortCommunity => Layer::RimSortCommunity,
            RuleOrigin::SteamDb => Layer::SteamDb,
        };
        format!("{after} loading after {before} per {}", layer_phrase(layer))
    }
}

/// Why the ledger recommended a [`super::Suggestion`]'s action, or offered
/// one of its [`super::Alternative`]s — see this module's own doc comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rationale {
    // -- edges.rs --
    /// An `Inferred`-strength [`super::FindingKey::EdgeDropped`], worded
    /// per the analyzer rule that produced `kind`.
    EdgeDroppedInferred {
        /// Which analyzer rule inferred the edge.
        kind: EdgeKind,
        /// The edge's dependent side.
        after: ModId,
        /// The edge's dependency side.
        before: ModId,
    },
    /// A `PatchRemovedNodeCosmetic` edge dropped to break a cycle: the
    /// final defs are identical either way.
    EdgeDroppedCosmetic,
    /// Keep a dropped edge instead; the sorter drops the next-weakest one
    /// in the cycle.
    KeepEdgeInCycle,
    /// Force this mod's own order instead of the edge that won.
    ReorderOverridingEdgeWinner,
    /// An `EdgeDropped` finding whose witness cycle was a direct two-mod
    /// contradiction — names the edge that won instead of only describing
    /// the one that lost.
    EdgeDroppedWithWinner {
        /// The dropped edge's dependent side.
        after: ModId,
        /// The dropped edge's dependency side.
        before: ModId,
        /// The edge that overruled it.
        winner: EdgeWinner,
    },
    /// A `Soft`/`Awareness`-strength edge dropped to break a cycle.
    EdgeDroppedSoftOrAwareness,
    /// A `Declared`-strength (author) edge dropped to break a cycle.
    EdgeDroppedDeclared,
    /// A `Hard`-strength edge dropped to break a cycle: the mods are
    /// genuinely in conflict.
    EdgeDroppedHard,
    /// Remove the dependent mod instead of accepting a broken load order.
    RemoveDependentModBrokenOrder,
    /// An advisory relation contradicting a declared edge — the
    /// declaration wins.
    DeclarationQuestioned {
        /// The declared edge's dependent side.
        declared_after: ModId,
        /// The declared edge's dependency side.
        declared_before: ModId,
        /// The declared edge's own layer.
        declared_layer: Layer,
        /// The declared edge's own detail text.
        declared_detail: String,
        /// The advisory relation's own kind.
        relation_kind: EdgeKind,
        /// The advisory relation's own detail text.
        relation_detail: String,
    },
    /// Force the opposite order the relation implies, as a user decision.
    ReorderOppositeOfRelation,
    /// A `Declared`-strength edge lost a cycle to a rule the user flagged
    /// to override it — the edge always wins; this is disclosure.
    DeclarationOverridden {
        /// The declared edge's dependent side.
        declared_after: ModId,
        /// The declared edge's dependency side.
        declared_before: ModId,
        /// The declared edge's own detail text.
        detail: String,
        /// The rule that overrode it.
        by: EdgeWinner,
    },
    /// No any-of candidate could satisfy the constraint without closing a
    /// cycle.
    AnyOfNoCandidate,
    /// Force this any-of candidate, accepting whatever cycle results
    /// elsewhere.
    ForceAnyOfCandidateAcceptingCycle,
    /// The chosen any-of candidate already loads before the dependent mod
    /// today.
    AnyOfChosenAlreadyBefore,
    /// An alternative any-of candidate could also satisfy the constraint.
    AnyOfAlternativeCandidate,
    /// The chosen any-of candidate is the uniquely most-depended-on mod
    /// among the candidates.
    AnyOfUniqueMaxDependents,
    /// The any-of candidate was chosen by the smallest id among
    /// otherwise-tied candidates.
    AnyOfSmallestId,

    // -- rules.rs --
    /// A pair rule lost a cycle with no single edge to blame — a 3+-mod
    /// cycle, named by the witness cycle instead.
    RuleOverruledLongerCycle {
        /// The rule's dependent side.
        after: ModId,
        /// The rule's dependency side.
        before: ModId,
        /// Where the rule came from.
        origin: RuleOrigin,
        /// Whether the rule itself overrides a declaration.
        overrides_declared: bool,
    },
    /// A pair rule lost a cycle to a specific winning edge.
    RuleOverruledByWinner {
        /// The rule's dependent side.
        after: ModId,
        /// The rule's dependency side.
        before: ModId,
        /// Where the rule came from.
        origin: RuleOrigin,
        /// Whether the rule itself overrides a declaration.
        overrides_declared: bool,
        /// The edge that won.
        winner: EdgeWinner,
    },
    /// Force this rule's own order instead of the edge that won.
    ReorderOverridingRuleWinner,
    /// Keep this rule as your own decision, so it survives even though it
    /// stays overruled.
    PromoteRuleDespiteOverruled,
    /// A placement pin was overruled by a stronger edge crossing the tier
    /// boundary.
    PlacementOverruled {
        /// The pinned mod.
        mod_id: ModId,
        /// Which tier it was pinned to.
        placement: Placement,
        /// The edge that overruled the pin.
        by: EdgeWinner,
    },
    /// Keep this placement as your own decision, so it survives even
    /// though it's overruled.
    PromotePlacementDespiteOverruled {
        /// Which tier the pin is on.
        placement: Placement,
    },
    /// Drop the rule that overruled this placement, keeping the pin.
    DropRuleOverrulingPlacement {
        /// The pinned mod.
        mod_id: ModId,
        /// Which tier it's pinned to.
        placement: Placement,
    },
    /// A placement pin holds, but an advisory relation can never be
    /// satisfied under it.
    PlacementQuestioned {
        /// The pinned mod.
        mod_id: ModId,
        /// Which tier it's pinned to.
        placement: Placement,
        /// The advisory relation's own kind.
        relation_kind: EdgeKind,
        /// The advisory relation's own detail text.
        relation_detail: String,
    },
    /// Enforce this relation explicitly as a user decision.
    EnforceRelationAsUserDecision,
    /// A holding placement pin's own extreme-edge ordering preference was
    /// defeated by another mod's accepted edge — the edge always wins;
    /// this is disclosure, not a decision point.
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
    /// A holding placement pin promotes one or more other mods past its
    /// own tier boundary — disclosure only.
    PlacementPromotesDependents {
        /// The pinned mod.
        mod_id: ModId,
        /// Which tier it's pinned to.
        placement: Placement,
        /// Every other mod attributed to this pin's own promotion, sorted.
        promoted: Vec<ModId>,
    },
    /// How many signals matched to infer this mod's own tag.
    TagInferredSignalCount {
        /// The matched-signal count.
        count: usize,
    },
    /// Reject the inferred tag.
    RejectInferredTag,

    // -- defs.rs --
    /// Force a specific def-override owner to win regardless of load
    /// order.
    ForceDefOverrideWinner,
    /// A def override's winning mod explicitly declares a load-order
    /// relation to every other owner.
    DefOverrideWinnerDeclaresRelation,
    /// Every owner of a def override shares an author.
    DefOverrideSameAuthor,
    /// Exactly one active, non-vanilla owner overrides a vanilla def.
    DefOverrideLoneNonVanillaOwner,
    /// A leaf mod is quietly shadowing a shared framework's def.
    DefOverrideShadowsFramework,
    /// Keep the current def-override winner instead of the shadowed
    /// framework's own def.
    KeepCurrentWinner,
    /// No signal explains a def override's winner.
    DefOverrideUnexplained,
    /// Merge field by field instead of accepting one owner's
    /// unexplained def override outright.
    MergeUnexplainedDefOverride,
    /// Merge field by field, keeping the shadowed framework's own
    /// values where the leaf mod didn't change them.
    MergeShadowsFrameworkFieldByField,
    /// Force one mod's patch to apply exclusively in a patch collision.
    ForcePatchCollisionWinner,
    /// Every contributing patch in a collision only adds to the
    /// target; order doesn't change the outcome.
    PatchCollisionAdditive,
    /// At least one contributing patch in a collision mutates the
    /// target; load order decides the outcome.
    PatchCollisionContested,
    /// The mod whose patch runs last in a collision declares, in its own
    /// `About.xml`, that it loads after (or depends on) every other mod
    /// patching the same field, and none of them removes it — its change
    /// is the author's deliberate choice.
    PatchCollisionWinnerDeclaresRelation {
        /// The mod whose operation runs last.
        winner: ModId,
        /// Every other mod patching the field, in load order.
        others: Vec<ModId>,
        /// The patched field's path under the def.
        field: String,
    },
    /// Merge field by field instead of letting load order decide a
    /// contested patch collision's whole def.
    MergeContestedPatchCollision,
    /// A real replay predicts this operation fails because an earlier
    /// remover's own operation removes the node it targets.
    PatchWillFailRemovedBy {
        /// The mod whose operation removed the targeted node.
        remover: ModId,
        /// The mod whose operation is predicted to fail.
        mod_id: ModId,
        /// The failing operation's own leaf xpath (or top-level
        /// identity, when no leaf is on hand).
        xpath: String,
    },
    /// Reorder the failing mod before the remover, so its operation
    /// runs while the node still exists.
    ReorderBeforeRemover {
        /// The mod whose operation is predicted to fail.
        mod_id: ModId,
        /// The mod whose operation removed the targeted node.
        remover: ModId,
    },
    /// A real replay predicts this operation fails because the node it
    /// targets is injected by a mod that hasn't loaded yet.
    PatchWillFailNotYetInjected {
        /// The mod whose operation is predicted to fail.
        mod_id: ModId,
        /// The mod whose own operation injects the targeted node.
        injector: ModId,
        /// The failing operation's own leaf xpath (or top-level
        /// identity, when no leaf is on hand).
        xpath: String,
    },
    /// Reorder the failing mod after the injector, so its operation
    /// finds the node once injected.
    ReorderAfterInjector {
        /// The mod whose operation is predicted to fail.
        mod_id: ModId,
        /// The mod whose own operation injects the targeted node.
        injector: ModId,
    },
    /// A real replay predicts this operation fails because the winning
    /// owner's own resolved structure never has the targeted node,
    /// regardless of load order.
    PatchWillFailDeadTarget {
        /// The mod whose operation is predicted to fail.
        mod_id: ModId,
        /// The failing operation's own leaf xpath (or top-level
        /// identity, when no leaf is on hand).
        xpath: String,
    },
    /// A real replay predicts this operation fails; the cause couldn't
    /// be classified.
    PatchWillFailUnknownCause {
        /// The mod whose operation is predicted to fail.
        mod_id: ModId,
        /// The failing operation's own leaf xpath (or top-level
        /// identity, when no leaf is on hand).
        xpath: String,
    },
    /// A `ParentName` reference doesn't resolve the way the child's own
    /// element type expects.
    BrokenInheritance,
    /// A cross-reference resolves to no active def of any type.
    DanglingDefReference {
        /// Why the referenced name never resolves.
        cause: DanglingCause,
        /// Whether the referencing field's own resolved values are
        /// mostly sounds, so a `SoundDef`-fallback caveat applies.
        likely_sound: bool,
    },
    /// A mod deliberately discards another mod's own addition via a
    /// declared-after `Replace`.
    DiscardedAdditionDeliberate {
        /// The mod whose `Replace` discards the addition.
        replacer: ModId,
        /// The mod whose addition is discarded.
        adder: ModId,
        /// The path `replacer`'s own `Replace` targets.
        path: String,
        /// The path `adder`'s own addition wrote into.
        adder_path: String,
    },

    // -- assets.rs --
    /// Force one mod's texture to win.
    ForceTextureWinner,
    /// Copy this mod's texture file into the merge mod, independent of
    /// load order.
    CopyTextureIntoMergeMod,
    /// A texture override is cosmetic and browsable in-game; it never
    /// blocks.
    TextureOverrideCosmetic,
    /// Force this mod's template registration to apply to every one of
    /// the template's own children, regardless of load order.
    ForceTemplateRegistrationWinner,
    /// More than one mod registers the same template `Name`; each
    /// child independently inherits from whichever registration loads
    /// nearest at or before it.
    DuplicateTemplateNameExplanation {
        /// How many mods register this template name.
        owner_count: usize,
    },
    /// The same `Languages/*/Keyed` translation key is defined by more
    /// than one active mod; the last-loaded one wins.
    KeyedTranslationCollision {
        /// How many keys the two mods collide over.
        key_count: usize,
    },
    /// A sound override is cosmetic and audible in-game; it never
    /// blocks.
    SoundOverrideCosmetic,
    /// Force one mod's sound to win.
    ForceSoundWinner,
    /// A texture path is shipped by no active mod.
    MissingTexturePath,
    /// A `.dds` file won't decode.
    UndecodableTexture,

    // -- mods.rs --
    /// A mod contributes nothing observable under this order.
    ContributesNothing {
        /// The mod that contributes nothing.
        mod_id: ModId,
    },
    /// A def in one mod names a type from another mod's DLL with no
    /// declared relation.
    UndeclaredTypeDependency {
        /// The mod using the type.
        user: ModId,
        /// The mod whose assembly declares the type.
        provider: ModId,
    },
    /// Pin a relation explicitly as a load-order rule.
    PinRelationExplicitly,
    /// Two or more mods patch the same target with no last patcher known
    /// (shouldn't happen for active mods, but stays total).
    RuntimePatchCollisionNoLastPatcher {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
    },
    /// Two or more mods patch the same target; names the current order's
    /// last patcher.
    RuntimePatchCollisionLastPatcher {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
        /// How many mods patch this target.
        owner_count: usize,
        /// The mod whose patch runs last today.
        last: ModId,
    },
    /// Force this mod's runtime patch to run last, regardless of load
    /// order.
    ForceRuntimePatchLastWinner,
    /// Two or more mods each rewrite the same method's IL via a
    /// runtime-patch transpiler.
    TranspilerCollision {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
        /// How many mods rewrite this method's IL.
        owner_count: usize,
    },
    /// More than one active mod ships an assembly of this name; the
    /// first-loaded copy wins at runtime.
    DuplicateAssemblyFirstLoadedWins,
    /// Remove one of the duplicate assembly copies.
    RemoveDuplicateAssemblyCopy,
    /// Two mods share a large number of identical defs with no declared
    /// relation between them.
    LikelyDuplicateMod,
    /// Remove one of the likely-duplicate mods.
    RemoveLikelyDuplicateModA,
    /// Remove the other likely-duplicate mod.
    RemoveLikelyDuplicateModB,
    /// This mod is active in `ModsConfig.xml` but isn't installed.
    MissingModNotInstalled,
    /// Keep the id in `ModsConfig.xml` in case the mod reappears.
    KeepMissingModId,
    /// A declared dependency isn't active; RimWorld only warns about
    /// this, it doesn't enforce it.
    MissingDependencyNotEnforced,
    /// Two mods declare each other incompatible.
    IncompatiblePair,
    /// Remove one of the incompatible mods.
    RemoveIncompatibleModA,
    /// Remove the other incompatible mod.
    RemoveIncompatibleModB,
    /// This mod doesn't list the active game version as supported; it
    /// often still works anyway.
    UnsupportedVersionOftenWorks,
    /// The sorter already honors a hard (load-time) edge the author
    /// never declared.
    UndeclaredHardDependencyHonored,
    /// Pin a relation explicitly as a load-order rule anyway (the lazy
    /// reference is already unaffected by load order).
    PinRelationExplicitlyAnyway,
    /// A lazily-resolved assembly reference isn't honored by the current
    /// order, but resolves at JIT time after every assembly is loaded.
    LazyReferenceViolatedJitResolved,
    /// A `FindMod` name or `MayRequire` id closely resembles an active
    /// mod's own identity — possibly a typo.
    NearMissModReference {
        /// Which kind of reference this is.
        kind: ModReferenceKind,
        /// The name or id as written.
        written: String,
        /// The active mod it closely resembles.
        candidate_name: String,
    },

    // -- domain::resolution.rs (redecide_for_clean_merge / redecide_for_identical_copies) --
    /// A clean patch collision has nothing to merge: the game applies
    /// every contribution in sequence to one value.
    MergeCompleteNothingToMerge,
    /// A clean def-override merge preview: the mods change different
    /// fields, so merging would combine both.
    MergeLeadDefOverrideCombine,
    /// A clean patch-collision merge preview promoted to the suggestion
    /// itself: the mods change different fields, so merging keeps both.
    MergePromotedPatchCollisionKeepsBoth,
    /// Keep the load-order winner instead of merging.
    KeepLoadOrderWinner,
    /// A structural guard field makes field-level merging unsafe; confirm
    /// the load-order winner instead.
    MergeStructuralGuard {
        /// The guard's own field name.
        field: String,
    },
    /// A merge preview still needs a per-field choice.
    MergeFieldsNeedChoice {
        /// How many fields still need a choice.
        unresolved: usize,
    },
    /// Two active owners' copies are byte-identical; order cannot change
    /// the outcome.
    IdenticalCopiesOrderIrrelevant,

    // -- ledger/scoped.rs --
    /// Not addressed by this patch; load order decides as today.
    NotAddressedByPatch,
}

impl std::fmt::Display for Rationale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Rationale::EdgeDroppedInferred {
                kind,
                after,
                before,
            } => match kind {
                EdgeKind::PatchRemovedNode => write!(
                    f,
                    "{after} removes a node that {before} also patches; {after} must load after {before} or {before}'s own patch would find nothing left to touch. The analyzer inferred this from patch content, not from either mod's own declared load order."
                ),
                EdgeKind::RetextureAfterOwner => write!(
                    f,
                    "{after} is a texture-only retexture pack sharing a texture path with {before}'s own content; {after} must load after {before} or its replacement texture is overwritten by the content it's meant to reskin. The analyzer inferred this from the shared texture path, not from either mod's own declared load order."
                ),
                EdgeKind::DefOverrideAfterOrigin => write!(
                    f,
                    "{before} owns the template or def family this override belongs to; {after} also ships a copy of it and must load after {before} so the override lands on top of the origin's own definition. The analyzer inferred this from template/def-count evidence, not from either mod's own declared load order."
                ),
                EdgeKind::PatchInvalidatesPredicate => write!(
                    f,
                    "{after} replaces or removes a node that {before}'s own patch selects through a predicate; {after} must load after {before} or {before}'s own predicate would no longer match. The analyzer inferred this from patch content, not from either mod's own declared load order."
                ),
                EdgeKind::ReplaceDiscardsAddition => write!(
                    f,
                    "{before} replaces a node that {after} also adds content into; {after} must load after {before} or its own addition is silently discarded when {before}'s Replace swaps in a whole new node, with no log line at all. The analyzer inferred this from patch content, not from either mod's own declared load order."
                ),
                _ => unreachable!(
                    "Rationale::EdgeDroppedInferred is only ever constructed from the EdgeStrength::Inferred arm, which only PatchRemovedNode/RetextureAfterOwner/DefOverrideAfterOrigin/PatchInvalidatesPredicate/ReplaceDiscardsAddition ever carry"
                ),
            },
            Rationale::EdgeDroppedCosmetic => write!(
                f,
                "cosmetic: the final defs are identical in either order; only which log line appears changes"
            ),
            Rationale::KeepEdgeInCycle => write!(
                f,
                "Keep this edge; the sorter will drop the next-weakest edge in the cycle instead."
            ),
            Rationale::ReorderOverridingEdgeWinner => {
                write!(
                    f,
                    "Force this mod's own order instead, overriding the winner."
                )
            }
            Rationale::EdgeDroppedWithWinner {
                after,
                before,
                winner,
            } => write!(
                f,
                "{after} must load before {before} instead: {} says so ({}).",
                layer_phrase(winner.layer),
                winner.detail
            ),
            Rationale::EdgeDroppedSoftOrAwareness => write!(
                f,
                "Dropping a soft or awareness-only edge to break a cycle rarely changes behavior."
            ),
            Rationale::EdgeDroppedDeclared => {
                write!(
                    f,
                    "An author-declared edge had to be dropped to break a cycle."
                )
            }
            Rationale::EdgeDroppedHard => write!(
                f,
                "A hard (load-time) edge had to be dropped to break a cycle: the mods are genuinely in conflict."
            ),
            Rationale::RemoveDependentModBrokenOrder => write!(
                f,
                "Remove the dependent mod instead of accepting a broken load order."
            ),
            Rationale::DeclarationQuestioned {
                declared_after,
                declared_before,
                declared_layer,
                declared_detail,
                relation_kind,
                relation_detail,
            } => write!(
                f,
                "{declared_after} loads after {declared_before} per {} ({declared_detail}); {} ({relation_detail}).",
                layer_phrase(*declared_layer),
                relation_explanation(*relation_kind)
            ),
            Rationale::ReorderOppositeOfRelation => {
                write!(
                    f,
                    "Force the opposite order the relation implies, as a user decision."
                )
            }
            Rationale::DeclarationOverridden {
                declared_after,
                declared_before,
                detail,
                by,
            } => write!(
                f,
                "{declared_after} is declared to load after {declared_before} ({detail}), but {} \
                 says otherwise: {} loads after {} instead ({}).",
                layer_phrase(by.layer),
                by.after,
                by.before,
                by.detail
            ),
            Rationale::AnyOfNoCandidate => {
                write!(
                    f,
                    "No candidate could satisfy this constraint without closing a cycle."
                )
            }
            Rationale::ForceAnyOfCandidateAcceptingCycle => {
                write!(
                    f,
                    "Force this candidate, accepting whatever cycle results elsewhere."
                )
            }
            Rationale::AnyOfChosenAlreadyBefore => {
                write!(
                    f,
                    "The chosen candidate already loads before the dependent mod today."
                )
            }
            Rationale::AnyOfAlternativeCandidate => {
                write!(
                    f,
                    "An alternative candidate could also satisfy this constraint."
                )
            }
            Rationale::AnyOfUniqueMaxDependents => write!(
                f,
                "The chosen candidate is the uniquely most-depended-on mod among the candidates."
            ),
            Rationale::AnyOfSmallestId => {
                write!(
                    f,
                    "Chosen by the smallest id among otherwise-tied candidates."
                )
            }
            Rationale::RuleOverruledLongerCycle {
                after,
                before,
                origin,
                overrides_declared,
            } => write!(
                f,
                "{} had to be dropped to break a longer cycle among several mods.",
                rule_phrase(after, before, *origin, *overrides_declared)
            ),
            Rationale::RuleOverruledByWinner {
                after,
                before,
                origin,
                overrides_declared,
                winner,
            } => write!(
                f,
                "{} was overruled: {} says {before} loads after {after} instead ({}).",
                rule_phrase(after, before, *origin, *overrides_declared),
                layer_phrase(winner.layer),
                winner.detail
            ),
            Rationale::ReorderOverridingRuleWinner => {
                write!(
                    f,
                    "Force this rule's own order instead, overriding the winner."
                )
            }
            Rationale::PromoteRuleDespiteOverruled => write!(
                f,
                "Keep this rule as your own decision, so it survives even though it stays overruled."
            ),
            Rationale::PlacementOverruled {
                mod_id,
                placement,
                by,
            } => write!(
                f,
                "{mod_id} is pinned {}, but {} overrules it: {}.",
                placement_phrase(*placement),
                layer_phrase(by.layer),
                by.detail
            ),
            Rationale::PromotePlacementDespiteOverruled { placement } => write!(
                f,
                "Keep {} placement as your own decision, so it survives even though it's overruled.",
                placement_phrase(*placement)
            ),
            Rationale::DropRuleOverrulingPlacement { mod_id, placement } => write!(
                f,
                "Drop the rule that overruled this placement, keeping {mod_id} {}.",
                placement_phrase(*placement)
            ),
            Rationale::PlacementQuestioned {
                mod_id,
                placement,
                relation_kind,
                relation_detail,
            } => write!(
                f,
                "{mod_id} is pinned {}; {} ({relation_detail}).",
                placement_phrase(*placement),
                relation_explanation(*relation_kind)
            ),
            Rationale::EnforceRelationAsUserDecision => {
                write!(f, "Enforce this relation explicitly as a user decision.")
            }
            Rationale::PlacementOrderingOverridden {
                mod_id,
                pinned,
                placement,
                by,
            } => {
                let relation_word = match placement {
                    Placement::Bottom => "after",
                    Placement::Top => "before",
                };
                write!(
                    f,
                    "{pinned} is pinned {}, but {mod_id} must load {relation_word} it \
                     anyway: {} ({}).",
                    placement_phrase(*placement),
                    layer_phrase(by.layer),
                    by.detail
                )
            }
            Rationale::PlacementPromotesDependents {
                mod_id,
                placement,
                promoted,
            } => {
                let count = promoted.len();
                let sample: Vec<String> =
                    promoted.iter().take(3).map(ToString::to_string).collect();
                let sample_suffix = if count > sample.len() {
                    format!(" (e.g. {}, ...)", sample.join(", "))
                } else {
                    format!(" ({})", sample.join(", "))
                };
                write!(
                    f,
                    "{mod_id} is pinned {}; {count} other mod{} {} attributed to it as \
                     promoted past its own tier boundary too{sample_suffix}.",
                    placement_phrase(*placement),
                    if count == 1 { "" } else { "s" },
                    if count == 1 { "is" } else { "are" }
                )
            }
            Rationale::TagInferredSignalCount { count } => write!(f, "{count} matching signal(s)."),
            Rationale::RejectInferredTag => write!(f, "Reject the inferred tag."),
            Rationale::ForceDefOverrideWinner => {
                write!(
                    f,
                    "Force a specific owner's def to win regardless of load order."
                )
            }
            Rationale::DefOverrideWinnerDeclaresRelation => write!(
                f,
                "The winning mod explicitly declares a load-order relation to every other owner."
            ),
            Rationale::DefOverrideSameAuthor => write!(f, "Every owner shares an author."),
            Rationale::DefOverrideLoneNonVanillaOwner => write!(
                f,
                "Exactly one active mod overrides this vanilla def; nothing else contests it."
            ),
            Rationale::DefOverrideShadowsFramework => {
                write!(
                    f,
                    "A leaf mod is quietly shadowing a shared framework's def."
                )
            }
            Rationale::KeepCurrentWinner => write!(f, "Keep the current winner instead."),
            Rationale::DefOverrideUnexplained => write!(
                f,
                "Multiple mods define this def with no strong signal explaining the override."
            ),
            Rationale::MergeUnexplainedDefOverride => write!(
                f,
                "Merge field by field instead of accepting one owner's def outright."
            ),
            Rationale::MergeShadowsFrameworkFieldByField => write!(
                f,
                "Merge field by field; keep the framework's values where the leaf mod didn't change them."
            ),
            Rationale::ForcePatchCollisionWinner => {
                write!(f, "Force one mod's patch to apply exclusively.")
            }
            Rationale::PatchCollisionAdditive => write!(
                f,
                "Every contributing patch only adds to the target; order doesn't change the outcome."
            ),
            Rationale::PatchCollisionContested => write!(
                f,
                "At least one contributing patch mutates the target; load order decides the outcome."
            ),
            Rationale::PatchCollisionWinnerDeclaresRelation {
                winner,
                others,
                field,
            } => {
                let others_text = others
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "{winner} declares it loads after {others_text}, so its change to '{field}' is kept as written."
                )
            }
            Rationale::MergeContestedPatchCollision => write!(
                f,
                "Merge field by field instead of letting load order decide the whole def."
            ),
            Rationale::PatchWillFailRemovedBy {
                remover,
                mod_id,
                xpath,
            } => write!(
                f,
                "{remover}'s own operation removes the node {mod_id}'s operation at '{xpath}' targets, loading before it in the order this was checked against."
            ),
            Rationale::ReorderBeforeRemover { mod_id, remover } => write!(
                f,
                "Load {mod_id} before {remover}, so its operation runs while the node still exists."
            ),
            Rationale::PatchWillFailNotYetInjected {
                mod_id,
                injector,
                xpath,
            } => write!(
                f,
                "{mod_id}'s operation at '{xpath}' targets a node {injector} injects, but {injector} loads after it in the order this was checked against."
            ),
            Rationale::ReorderAfterInjector { mod_id, injector } => write!(
                f,
                "Load {mod_id} after {injector}, so its operation finds the node once injected."
            ),
            Rationale::PatchWillFailDeadTarget { mod_id, xpath } => write!(
                f,
                "{mod_id}'s operation at '{xpath}' targets a node the winning owner's own resolved structure never has, regardless of load order — no reorder fixes this."
            ),
            Rationale::PatchWillFailUnknownCause { mod_id, xpath } => write!(
                f,
                "{mod_id}'s operation at '{xpath}' is predicted to fail; the cause couldn't be classified."
            ),
            Rationale::BrokenInheritance => write!(
                f,
                "This `ParentName` reference doesn't resolve the way the child's own \
                 element type expects — the def loads with content missing or wrong. \
                 Report this to the referencing mod's author."
            ),
            Rationale::DanglingDefReference {
                cause,
                likely_sound,
            } => {
                let cause_text = match cause {
                    DanglingCause::RemovedBy { mod_id, .. } => {
                        format!("it was removed by {mod_id}'s own patch")
                    }
                    DanglingCause::OnlyInUnloadedFolder { mod_id, folder } => format!(
                        "it's defined only in {mod_id}'s own '{folder}' folder, which this install doesn't load"
                    ),
                    DanglingCause::OnlyInInactiveMod { mod_id } => {
                        format!("it's defined only in {mod_id}, which isn't active")
                    }
                    DanglingCause::DefinedNowhere => {
                        "no installed mod, active or not, defines it".to_string()
                    }
                    DanglingCause::Unexplained => {
                        "why it's missing couldn't be determined".to_string()
                    }
                };
                let sound_caveat = if *likely_sound {
                    " This looks like a sound reference — a missing `SoundDef` logs a warning and silently \
                     falls back to an undefined sound rather than failing to load."
                } else {
                    ""
                };
                write!(
                    f,
                    "This name resolves to no active def of any type — {cause_text}.{sound_caveat} \
                     Cross-references resolve after every mod's defs and patches have loaded, so this \
                     never depends on load order."
                )
            }
            Rationale::DiscardedAdditionDeliberate {
                replacer,
                adder,
                path,
                adder_path,
            } => write!(
                f,
                "{replacer} declares it loads after {adder} and replaces '{path}': {adder}'s own addition at '{adder_path}' is discarded on purpose."
            ),
            Rationale::ForceTextureWinner => write!(f, "Force one mod's texture to win."),
            Rationale::CopyTextureIntoMergeMod => write!(
                f,
                "Copy this mod's texture file into the merge mod, independent of load order."
            ),
            Rationale::TextureOverrideCosmetic => write!(
                f,
                "A texture override is cosmetic and browsable in-game; it never blocks."
            ),
            Rationale::ForceTemplateRegistrationWinner => write!(
                f,
                "Force this mod's registration to apply to every one of the template's own children, regardless of load order."
            ),
            Rationale::DuplicateTemplateNameExplanation { owner_count } => write!(
                f,
                "{owner_count} mods register this template name; each child independently inherits from whichever registration loads nearest at or before it (falling back to a vanilla registration, or the lowest-loaded mod's, if none does), so different children can resolve to different mods."
            ),
            Rationale::KeyedTranslationCollision { key_count } => {
                if *key_count == 1 {
                    write!(
                        f,
                        "These two mods define the same translation key; the last-loaded one wins."
                    )
                } else {
                    write!(
                        f,
                        "These two mods define {key_count} of the same translation keys; the last-loaded one wins each time."
                    )
                }
            }
            Rationale::SoundOverrideCosmetic => write!(
                f,
                "A sound override is cosmetic and audible in-game; it never blocks."
            ),
            Rationale::ForceSoundWinner => write!(f, "Force one mod's sound to win."),
            Rationale::MissingTexturePath => write!(
                f,
                "This texture path is shipped by no active mod — the game will show a magenta placeholder in its place."
            ),
            Rationale::UndecodableTexture => write!(
                f,
                "This .dds file won't decode — the base game shows the bad-texture placeholder in its place."
            ),
            Rationale::ContributesNothing { mod_id } => write!(
                f,
                "{mod_id} contributes nothing observable under this order: it ships no assembly, every def it owns is overridden by a later copy, every texture and translation it ships is overridden, and every patch operation it has either fails to apply or never runs — disabling it changes nothing in-game and saves its load cost."
            ),
            Rationale::UndeclaredTypeDependency { user, provider } => write!(
                f,
                "{user} uses a type from {provider}'s assembly but declares no dependency on it."
            ),
            Rationale::PinRelationExplicitly => {
                write!(f, "Pin the relation explicitly as a load-order rule.")
            }
            Rationale::RuntimePatchCollisionNoLastPatcher {
                target_type,
                target_method,
            } => {
                write!(f, "Multiple mods patch {target_type}.{target_method}.")
            }
            Rationale::RuntimePatchCollisionLastPatcher {
                target_type,
                target_method,
                owner_count,
                last,
            } => write!(
                f,
                "{owner_count} mods patch {target_type}.{target_method}; {last} loads last today, so its patch runs last."
            ),
            Rationale::ForceRuntimePatchLastWinner => write!(
                f,
                "Force this mod's patch to run last, regardless of load order."
            ),
            Rationale::TranspilerCollision {
                target_type,
                target_method,
                owner_count,
            } => write!(
                f,
                "{owner_count} mods each rewrite {target_type}.{target_method}'s IL with a runtime-patch \
                 transpiler; the order they run in can decide whether a later one's own pattern still matches. \
                 Most such collisions are harmless — if one of these mods logs a patching error, \
                 state the working order with `rimmerge rule set-pair`."
            ),
            Rationale::DuplicateAssemblyFirstLoadedWins => write!(
                f,
                "More than one active mod ships an assembly of this name; the first-loaded copy wins at runtime."
            ),
            Rationale::RemoveDuplicateAssemblyCopy => {
                write!(f, "Remove one of the duplicate copies.")
            }
            Rationale::LikelyDuplicateMod => write!(
                f,
                "These two mods share a large number of identical defs with no declared relation between them."
            ),
            Rationale::RemoveLikelyDuplicateModA => {
                write!(f, "Remove one of the likely-duplicate mods.")
            }
            Rationale::RemoveLikelyDuplicateModB => {
                write!(f, "Remove the other likely-duplicate mod.")
            }
            Rationale::MissingModNotInstalled => write!(
                f,
                "This mod is active in ModsConfig.xml but isn't installed; RimWorld drops unknown ids anyway."
            ),
            Rationale::KeepMissingModId => write!(
                f,
                "Keep the id in ModsConfig.xml in case the mod reappears."
            ),
            Rationale::MissingDependencyNotEnforced => write!(
                f,
                "A declared dependency isn't active; RimWorld only warns about this, it doesn't enforce it."
            ),
            Rationale::IncompatiblePair => write!(f, "These mods declare each other incompatible."),
            Rationale::RemoveIncompatibleModA => write!(f, "Remove one of the incompatible mods."),
            Rationale::RemoveIncompatibleModB => write!(f, "Remove the other incompatible mod."),
            Rationale::UnsupportedVersionOftenWorks => write!(
                f,
                "This mod doesn't list the active game version as supported; it often still works anyway."
            ),
            Rationale::UndeclaredHardDependencyHonored => write!(
                f,
                "The sorter already honors this hard (load-time) edge even though the author never declared it."
            ),
            Rationale::PinRelationExplicitlyAnyway => {
                write!(
                    f,
                    "Pin the relation explicitly as a load-order rule anyway."
                )
            }
            Rationale::LazyReferenceViolatedJitResolved => write!(
                f,
                "This lazily-resolved assembly reference isn't honored by the current order, but it resolves at JIT time after every assembly is loaded, so load order doesn't actually affect it."
            ),
            Rationale::NearMissModReference {
                kind,
                written,
                candidate_name,
            } => {
                let field = match kind {
                    ModReferenceKind::FindModName => "FindMod name",
                    ModReferenceKind::MayRequireId => "MayRequire id",
                };
                write!(
                    f,
                    "This {field} '{written}' resolves to no active or installed mod, but closely resembles the active mod '{candidate_name}' — possibly a typo. Report this to the referencing mod's author."
                )
            }
            Rationale::MergeCompleteNothingToMerge => write!(
                f,
                "the game applies these contributions in sequence to a single value; nothing to merge"
            ),
            Rationale::MergeLeadDefOverrideCombine => write!(
                f,
                "The mods change different fields; merging would combine both."
            ),
            Rationale::MergePromotedPatchCollisionKeepsBoth => {
                write!(f, "The mods change different fields; merging keeps both.")
            }
            Rationale::KeepLoadOrderWinner => write!(f, "Keep the load-order winner."),
            Rationale::MergeStructuralGuard { field } => write!(
                f,
                "{field} differs between owners — field-level merging is unsafe; confirm the load-order winner"
            ),
            Rationale::MergeFieldsNeedChoice { unresolved } => {
                if *unresolved == 1 {
                    write!(f, "Merge, 1 field needs a choice.")
                } else {
                    write!(f, "Merge, {unresolved} fields need a choice.")
                }
            }
            Rationale::IdenticalCopiesOrderIrrelevant => {
                write!(f, "identical copies; order cannot change the outcome")
            }
            Rationale::NotAddressedByPatch => {
                write!(
                    f,
                    "Not addressed by this patch; load order decides as today."
                )
            }
        }
    }
}
