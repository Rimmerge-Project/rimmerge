//! `DefKey` and `FindingKey`: the identity of a def and of a finding, plus the synthetic def types the ledger keys on.

use std::collections::BTreeSet;
use std::fmt;

use rim_analyzer::domain::{EdgeKind, InheritanceProblemKind, ModId, ModReferenceKind, Selector};
use serde::{Deserialize, Serialize};

use crate::domain::def_ref::DefRef;
use crate::domain::rule::{Placement, RuleOrigin};
use crate::domain::tag::Tag;

/// The `def_type` [`DefKey::synthesize_for_texture`] uses — never a real
/// def type an author could ship (RimWorld def types are XML tag names,
/// which this crate's own `EdgeKind`/`Selector` parsing already assumes
/// are never `"texture"` verbatim), so a synthesized texture key can never
/// collide with a genuine def one.
const TEXTURE_DEF_TYPE: &str = "texture";

/// The `def_type` [`DefKey::synthesize_for_template_name`] uses — same
/// reasoning as [`TEXTURE_DEF_TYPE`], for `DuplicateTemplateName`.
const TEMPLATE_DEF_TYPE: &str = "template_name";

/// The `def_type` [`DefKey::synthesize_for_sound`] uses — same reasoning
/// as [`TEXTURE_DEF_TYPE`], for `SoundOverride`.
const SOUND_DEF_TYPE: &str = "sound";

/// The `def_type` [`DefKey::synthesize_for_runtime_target`] uses — same
/// reasoning as [`TEXTURE_DEF_TYPE`], for the per-target
/// `RuntimePatchCollision` grouping.
const RUNTIME_TARGET_DEF_TYPE: &str = "runtime_target";

/// The separator [`DefKey::synthesize_for_runtime_target`] joins
/// `target_type`/`target_method` with inside a synthesized `def_name` — a
/// literal `.` a real runtime-patch target's own type name could also contain
/// (`Verse.Pawn`), so this uses `::` (never valid inside a C# identifier)
/// instead, matching how a runtime patch's own fully-qualified name is
/// conventionally written (`Verse.Pawn::Kill`).
const RUNTIME_TARGET_SEPARATOR: &str = "::";

/// The identity of a def: its type tag and its `defName`. Shared with the
/// merge generator, which needs exactly this identity to locate the
/// owning XML elements.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DefKey {
    /// The def's type tag, e.g. `ThingDef` or `example.PartDef`.
    pub def_type: String,
    /// The def's `defName`.
    pub def_name: String,
}

impl fmt::Display for DefKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.def_type, self.def_name)
    }
}

impl DefKey {
    /// A display-only key standing in for [`FindingKey::TextureOverride`]'s
    /// `texture_path`, so [`crate::domain::Action::PreferWinner`] — typed
    /// for a [`DefKey`], the only contested-ownership shape it has — can
    /// also target a texture override. Round-trips through
    /// [`DefKey::texture_path`].
    #[must_use]
    pub fn synthesize_for_texture(texture_path: impl Into<String>) -> Self {
        Self {
            def_type: TEXTURE_DEF_TYPE.to_string(),
            def_name: texture_path.into(),
        }
    }

    /// The texture path this key stands in for, if [`Self::synthesize_for_texture`]
    /// built it.
    #[must_use]
    pub fn texture_path(&self) -> Option<&str> {
        (self.def_type == TEXTURE_DEF_TYPE).then_some(self.def_name.as_str())
    }

    /// A display-only key standing in for
    /// [`FindingKey::DuplicateTemplateName`]'s `name`, the same reason and
    /// mechanism as [`Self::synthesize_for_texture`].
    #[must_use]
    pub fn synthesize_for_template_name(name: impl Into<String>) -> Self {
        Self {
            def_type: TEMPLATE_DEF_TYPE.to_string(),
            def_name: name.into(),
        }
    }

    /// The template name this key stands in for, if
    /// [`Self::synthesize_for_template_name`] built it.
    #[must_use]
    pub fn template_name(&self) -> Option<&str> {
        (self.def_type == TEMPLATE_DEF_TYPE).then_some(self.def_name.as_str())
    }

    /// A display-only key standing in for [`FindingKey::SoundOverride`]'s
    /// `path`, the same reason and mechanism as
    /// [`Self::synthesize_for_texture`].
    #[must_use]
    pub fn synthesize_for_sound(path: impl Into<String>) -> Self {
        Self {
            def_type: SOUND_DEF_TYPE.to_string(),
            def_name: path.into(),
        }
    }

    /// The sound path this key stands in for, if [`Self::synthesize_for_sound`]
    /// built it.
    #[must_use]
    pub fn sound_path(&self) -> Option<&str> {
        (self.def_type == SOUND_DEF_TYPE).then_some(self.def_name.as_str())
    }

    /// A display-only key standing in for a
    /// [`FindingKey::RuntimePatchCollision`] target, the same reason as
    /// [`Self::synthesize_for_texture`]: `Action::PreferWinner` is typed
    /// for a [`DefKey`], so a
    /// per-owner "force this mod's patch to run last" alternative needs
    /// one even though a runtime-patch target is a `(type, method)` pair, not a
    /// def.
    #[must_use]
    pub fn synthesize_for_runtime_target(target_type: &str, target_method: &str) -> Self {
        Self {
            def_type: RUNTIME_TARGET_DEF_TYPE.to_string(),
            def_name: format!("{target_type}{RUNTIME_TARGET_SEPARATOR}{target_method}"),
        }
    }

    /// The `(target_type, target_method)` pair this key stands in for, if
    /// [`Self::synthesize_for_runtime_target`] built it.
    #[must_use]
    pub fn runtime_target(&self) -> Option<(&str, &str)> {
        if self.def_type != RUNTIME_TARGET_DEF_TYPE {
            return None;
        }
        self.def_name.split_once(RUNTIME_TARGET_SEPARATOR)
    }
}

/// The stable identity of one finding the ledger tracks a decision for.
///
/// Ids are stored exactly as given by whatever constructed the key
/// (typically [`crate::ledger::extract_findings`] — the one deliberate
/// exception is [`FindingKey::LazyReferenceViolated`], keyed on
/// [`ModId::base`] ids); every owner/mod *set* is a [`BTreeSet`] so equal
/// findings always compare equal regardless of discovery order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FindingKey {
    /// The sorter dropped this edge to break a cycle.
    EdgeDropped {
        /// The edge's dependent side.
        after: ModId,
        /// The edge's dependency side.
        before: ModId,
        /// The kind of edge dropped.
        kind: EdgeKind,
    },
    /// An advisory (`Awareness`-strength) edge points the opposite way
    /// from an enforced `Declared`/db edge between the same two mods. The
    /// declaration always
    /// wins (it was never dropped — the advisory edge was never added to
    /// the graph in the first place); this just surfaces the
    /// contradiction. Keyed by the declared edge's own pair, plus the
    /// opposing relation's kind (so two different advisory relations
    /// contradicting the same declared edge are tracked separately).
    DeclarationQuestioned {
        /// The enforced edge's dependent side.
        declared_after: ModId,
        /// The enforced edge's dependency side.
        declared_before: ModId,
        /// The advisory relation's kind (the opposing direction is
        /// implied: `declared_before` after `declared_after`).
        relation_kind: EdgeKind,
    },
    /// The declared-edge override (deferred
    /// half of the pair-rule feature): a `Declared`-strength engine edge
    /// (`loadAfter`/`loadBefore`/`modDependencies`) lost a cycle to a
    /// [`super::PairRule`](crate::domain::PairRule) the user explicitly flagged
    /// `overrides_declared` — a sibling of [`Self::RuleOverruled`]/
    /// [`Self::PlacementOrderingOverridden`], not a repurposing of
    /// either: `RuleOverruled` fires when a *rule* loses (this is the
    /// mirror image, a *declaration* losing to a rule) and
    /// `DeclarationQuestioned` above fires only for an `Awareness`-
    /// strength edge merely opposing an enforced one, never on a
    /// `Declared` edge actually being dropped. Fired **alongside** the
    /// ordinary [`Self::EdgeDropped`] a dropped `Declared` edge already
    /// produces (never instead of it — see
    /// `ledger::findings::extract`), gated on the winner specifically
    /// being a `Layer::DeclaredOverride` edge. `Action::Accept`, no
    /// alternative — disclosure only, mirroring
    /// [`Self::PlacementOrderingOverridden`]'s own shape ("the edge
    /// always wins; this is disclosure of a correct, if easy-to-miss,
    /// fact, not a decision point"). Keyed by the declared edge's own
    /// pair and kind, the same fields [`Self::EdgeDropped`] uses — two
    /// different declared-edge kinds between the same pair (e.g. a
    /// `loadAfter` and a `modDependencies` both losing) are tracked
    /// separately.
    DeclarationOverridden {
        /// The declared edge's dependent side.
        declared_after: ModId,
        /// The declared edge's dependency side.
        declared_before: ModId,
        /// The kind of declared edge overridden.
        kind: EdgeKind,
    },
    /// An any-of constraint needed a candidate chosen.
    AnyOfChoice {
        /// The mod requiring one of the candidates.
        after: ModId,
        /// The shared assembly name the candidates all ship.
        assembly: String,
    },
    /// More than one active mod defines the same def.
    DefOverride {
        /// The contested def.
        key: DefKey,
        /// Every mod that defines it.
        owners: BTreeSet<ModId>,
    },
    /// More than one active mod patches the same target.
    PatchCollision {
        /// The patched def.
        key: DefKey,
        /// Which attribute the patch's predicate matched on.
        selector: Selector,
        /// The path under the def the patches target, if any.
        sub_path: Option<String>,
        /// Every mod that patches it.
        mods: BTreeSet<ModId>,
    },
    /// More than one active mod ships the same normalized texture path.
    TextureOverride {
        /// The shared, normalized texture path.
        texture_path: String,
        /// Every mod that ships it.
        owners: BTreeSet<ModId>,
    },
    /// More than one active mod ships an assembly of the same name.
    DuplicateAssembly {
        /// The shared assembly name.
        assembly_name: String,
        /// Every mod that ships it.
        owners: BTreeSet<ModId>,
    },
    /// More than one active mod registers the same template `Name`.
    DuplicateTemplateName {
        /// The shared template name.
        name: String,
        /// Every mod that registers it.
        owners: BTreeSet<ModId>,
    },
    /// Two active mods define the same `Languages/*/Keyed` translation key,
    /// grouped by mod pair (a mod pair can share many keys, and
    /// each one would otherwise be its own row).
    KeyedTranslationCollision {
        /// The two mods, in a fixed order.
        pair: (ModId, ModId),
    },
    /// More than one active mod ships the same normalized sound path —
    /// mirrors [`FindingKey::TextureOverride`] for `Sounds/`.
    SoundOverride {
        /// The shared, normalized sound path.
        path: String,
        /// Every mod that ships it.
        owners: BTreeSet<ModId>,
    },
    /// A def in `user` names a type from `provider`'s DLL with no
    /// declared relation onto `provider`.
    UndeclaredTypeDependency {
        /// The mod using the type.
        user: ModId,
        /// The mod whose DLL defines the type.
        provider: ModId,
        /// The type name.
        type_name: String,
    },
    /// Two or more active mods declare a runtime patch on the same
    /// `(type, method)`, one finding per target. Grouping by mod pair
    /// instead fans a single popular target's owners out into every
    /// pairwise combination (a target with 48 owners becomes 1,128 pairs);
    /// one finding per target — matching
    /// `rim_analyzer::domain::Conflict::RuntimePatchCollision` one to one,
    /// the same shape `DuplicateTemplateName`/`SoundOverride` already use —
    /// is both the smaller number and the more honest one, since "who's
    /// colliding" is a per-target fact, not a per-pair one.
    RuntimePatchCollision {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
        /// Every mod that patches it.
        owners: BTreeSet<ModId>,
    },
    /// Two or more
    /// active mods each declare a `Transpiler` role on the same `(type,
    /// method)` — the narrow, order-sensitive subset of
    /// [`Self::RuntimePatchCollision`]. Disclosure
    /// only, same as that finding: no ordering edge backs this (the right
    /// transpiler order depends on whose IL
    /// pattern survives whose rewrite, which isn't recoverable from
    /// metadata). `owners` is scoped to the mods declaring the
    /// `Transpiler` role specifically, a possibly-smaller set than the
    /// sibling `RuntimePatchCollision` on the same target (which may also
    /// include `Prefix`/`Postfix`-only patchers uninvolved in this risk).
    TranspilerCollision {
        /// The patched type.
        target_type: String,
        /// The patched method.
        target_method: String,
        /// Every mod declaring a `Transpiler` role on it.
        owners: BTreeSet<ModId>,
    },
    /// A pair rule (RimSort import or the user's own prior decision) lost
    /// a cycle to a stronger edge. Every origin gets a finding, not only
    /// the `UserDecision` one `SortWarning::UserDecisionOverruled` covers.
    /// Keyed by the losing rule's own endpoints plus its origin, since two
    /// different-origin pair rules can name the same two mods at once
    /// (e.g. an imported rule and its own promoted `UserDecision` copy)
    /// and losing one doesn't mean the other did.
    RuleOverruled {
        /// The losing rule's dependent side.
        after: ModId,
        /// The losing rule's dependency side.
        before: ModId,
        /// The losing rule's origin.
        origin: RuleOrigin,
    },
    /// A `PlacementRule` (Top/Bottom) was overruled: the pinned mod's tier
    /// membership edge was dropped because a stronger `Real` edge crossed
    /// the tier boundary, the case `sort::TierReason::PromotedBy` already
    /// records.
    /// Keyed by the pinned mod, its intended placement, and the rule's
    /// origin (mirroring `RuleOverruled`'s own reasoning: a promoted copy
    /// and its imported original can coexist).
    PlacementOverruled {
        /// The pinned mod.
        mod_id: ModId,
        /// Which tier it was pinned to.
        placement: Placement,
        /// The placement rule's origin.
        origin: RuleOrigin,
    },
    /// A `PlacementRule` (Top/Bottom) holds, but an advisory `Soft`/
    /// `Awareness` edge can never be satisfied under it. Keyed by
    /// the pinned mod, its placement, and the opposing relation's kind
    /// (mirroring `DeclarationQuestioned`'s own reasoning: two different
    /// advisory relations contradicting the same placement are tracked
    /// separately). The *other* mod the relation names is deliberately not
    /// part of the key — a placement pin has no natural second endpoint
    /// the way a declared edge does, and two different mods opposing the
    /// same placement through the same `EdgeKind` are rare enough that
    /// collapsing them (the later one found wins) was preferred over a
    /// third key field that would make an otherwise-identical relation
    /// against a different partner churn a stored decision — see
    /// `Finding::PlacementQuestioned`'s own `other` field, which keeps that
    /// mod for display/`Reorder` without making it part of the key.
    PlacementQuestioned {
        /// The pinned mod.
        mod_id: ModId,
        /// Which tier it's pinned to.
        placement: Placement,
        /// The advisory relation's kind.
        relation: EdgeKind,
    },
    /// A `PlacementRule`
    /// (`Top`/`Bottom`) *holds* — unlike `PlacementOverruled`, the pin's
    /// own tier membership is intact — but another mod's accepted `Real`
    /// edge crosses the pin's own extreme-edge region-ordering preference
    /// the wrong way: a mod required *after* a holding `Bottom` pin, or
    /// *before* a holding `Top` pin (`sort::emit::placement_bias`). Not
    /// `PlacementOverruled` (that fires on the *pinned* mod's own
    /// `TierReason::PromotedBy`, never on a *different* mod) and not
    /// `PlacementQuestioned` (that scans only excluded `Soft`/`Awareness`
    /// advisory edges; a `Real` edge is never excluded, so it never
    /// reaches that loop at all). The edge always wins — this is
    /// disclosure of a correct, if easy-to-miss, fact, not a decision
    /// point (`ledger::suggest::placement_ordering_overridden` offers no
    /// alternative). Keyed by the mod whose ordering preference lost, the
    /// pin it lost to, and which tier — two different pins overriding the
    /// same mod are tracked separately, mirroring `RuleOverruled`'s own
    /// per-origin keying reasoning.
    PlacementOrderingOverridden {
        /// The mod forced across the pin's own extreme edge.
        mod_id: ModId,
        /// The pin it could not be sorted around.
        pinned: ModId,
        /// Which tier the pin occupies.
        placement: Placement,
    },
    /// The rule-level companion of
    /// [`Self::PlacementOrderingOverridden`]: a *holding* placement
    /// promotes one or more other mods past its own tier boundary. `promoted`
    /// (on the [`Finding`](super::Finding) this key backs) is an **attribution, not an
    /// exhaustive reach count**: it names every mod `sort::cycles::
    /// find_promotion_cause`'s nearest-placed-node BFS (reused from
    /// `TierReason::PromotedBy`) attributes to *this*
    /// pin specifically, and that walk stops at the first placed node it
    /// reaches — so a mod promoted past *two* pins' own boundaries (e.g. it
    /// declares `loadAfter` both) is attributed to whichever one the BFS
    /// finds first, never both. A pin whose walk attributes zero
    /// promotions to it (the common case — most pins are leaf mods with no
    /// dependents of their own) never produces this finding at all. A
    /// pin's attributed count can therefore be smaller than its full
    /// transitive closure, when some of those mods are attributed to a
    /// *different* pin instead. Distinct from
    /// [`Self::PlacementOrderingOverridden`] above, which is per
    /// *colliding* mod and only fires when a crossing edge defeats the
    /// pin's own ordering preference relative to *another* pin — this one
    /// fires on the placement itself, regardless of whether any of its
    /// dependents also happen to collide with a different pin, since a
    /// framework placement dissolving its own tier for everyone else is
    /// worth flagging even when nothing else is pinned to collide with.
    /// Keyed by the pinned mod and its placement — a mod's *active*
    /// placement is always exactly one rule regardless of how many
    /// origins name it (`sort::tiers::assign`'s "first match wins"), so
    /// unlike `RuleOverruled`/`PlacementOverruled` no origin is needed in
    /// the key.
    PlacementPromotesDependents {
        /// The pinned mod.
        mod_id: ModId,
        /// Which tier it's pinned to.
        placement: Placement,
    },
    /// Two active mods look like duplicates or forks of the same content.
    LikelyDuplicateMod {
        /// The two mods, in a fixed order (see this variant's
        /// constructor sites — the ordering is a caller convention, not
        /// enforced here).
        pair: (ModId, ModId),
    },
    /// An active mod (per `ModsConfig.xml`) has no directory on disk.
    MissingMod {
        /// The missing mod.
        mod_id: ModId,
    },
    /// An active mod's declared dependency is not itself active.
    MissingDependency {
        /// The mod with the unmet dependency.
        mod_id: ModId,
        /// The missing dependency.
        dependency: ModId,
    },
    /// Two active mods declare each other incompatible.
    IncompatiblePair {
        /// The two mods, in a fixed order.
        pair: (ModId, ModId),
    },
    /// An active mod's `supportedVersions` doesn't list the game version.
    UnsupportedVersion {
        /// The mod.
        mod_id: ModId,
    },
    /// An `AssemblyRef` edge exists with no matching declared edge.
    UndeclaredHardDependency {
        /// The edge's dependent side.
        after: ModId,
        /// The edge's dependency side.
        before: ModId,
    },
    /// A lazily-resolved `AssemblyRef` (`EdgeStrength::Soft`) is violated
    /// by the order in effect. Keyed on base ids (kind-level: this is
    /// about the relation between the two mods, not a specific `_steam`
    /// variant) so it doesn't churn if a workshop/local copy swap changes
    /// which exact id the engine reports.
    LazyReferenceViolated {
        /// The edge's dependent side, base id.
        after: ModId,
        /// The edge's dependency side, base id.
        before: ModId,
    },
    /// A tag was inferred for a mod.
    TagInferred {
        /// The tagged mod.
        mod_id: ModId,
        /// The inferred tag.
        tag: Tag,
    },
    /// See [`rim_analyzer::domain::MissingTexturePath`].
    MissingTexturePath {
        /// The mod whose own copy of `def` carries this field.
        referrer: ModId,
        /// The def carrying the field.
        def: DefKey,
        /// The field's own tag name.
        field: String,
        /// The normalized path (same convention as
        /// `rim_analyzer::domain::Indices::texture_owners`' own keys)
        /// that resolved to no shipped file.
        path: String,
    },
    /// A real, order-aware replay of every active patcher against
    /// `def_key`'s winner predicts that `mod_id`'s own **top-level**
    /// `<Operation>` fails to apply under the order this pass was run
    /// for — `rim_merge::patch_eval::TopLevelOutcome::succeeded ==
    /// false`, surfaced by `rim_merge::effective::compute`, joined back
    /// to the def it belongs to.
    ///
    /// **`operation` names the top-level operation, not a nested leaf**:
    /// RimWorld's own `Player.log` names the top-level `<Operation>` that
    /// failed — its class and identifying text
    /// (`Verse.PatchOperationFindMod(Example Climate Pack)`,
    /// `Verse.PatchOperationSequence(count=7, lastFailedOperation=...)`), never the specific leaf mutation
    /// inside it that actually matched nothing, and never logs anything
    /// at all for a leaf whose own failure an enclosing
    /// `<success>Always</success>` swallowed (a common compat-patch
    /// idiom, with thousands of occurrences on a large install). This key
    /// is built to match exactly what a user sees in their own log, so they
    /// can find one from the other; see
    /// [`Finding::PatchWillFail::leaf_xpath`](super::Finding::PatchWillFail::leaf_xpath)
    /// for the nested diagnostic detail this key deliberately excludes.
    ///
    /// `cause` (order-fixable or not) is deliberately **not** part of
    /// this key, only of [`Finding::PatchWillFail`](super::Finding::PatchWillFail): like
    /// [`Self::RuleOverruled`]'s winner or [`Self::EdgeDropped`]'s
    /// `detail`, it's a derived classification of the same underlying
    /// fact, recomputed identically from `(mod_id, operation, def_key,
    /// selector)` plus whichever order the pass ran against — not a
    /// second, independent fact this key needs to distinguish two
    /// otherwise-identical findings by.
    ///
    /// **Text-form fragility**: `operation`'s own text embeds a
    /// `PatchOperationFindMod`'s own mod display names verbatim, and **real
    /// mods commonly have a literal `:` in their own display name**
    /// (`Example Splice: Core`, `Example Flora: Overgrowth`, ...) — not a
    /// hypothetical risk. Because `operation` is always the *last* of this
    /// key's four fields, `FromStr` parses it
    /// by re-splitting the original text with a bounded count
    /// (`splitn(5, ':')` — the kind tag plus exactly four fields, the
    /// last one capturing everything remaining, embedded colons
    /// included) rather than the shared, unconditionally-over-split
    /// field list every other `FindingKey` kind uses; `mod_id`/`def_key`/
    /// `selector` still can't safely contain a `:` of their own (the
    /// same, much lower-risk exposure every other key's fields share — a
    /// package id or an XML tag/attribute name would have to contain one).
    PatchWillFail {
        /// The mod whose operation is predicted to fail.
        mod_id: ModId,
        /// The def or template the operation targets.
        def_key: DefKey,
        /// Which attribute the operation's own target predicate matched
        /// on — needed alongside `def_key` the same reason
        /// [`Self::PatchCollision`] carries it separately: a `[@Name="X"]`
        /// template and a `[defName="X"]` def can share one name under
        /// one `def_type`.
        selector: Selector,
        /// The **top-level** operation's own RimWorld-log identity text
        /// (`rim_merge::patch_eval::TopLevelOutcome::identity`) —
        /// verbatim, not a nested leaf's own xpath. See this variant's
        /// own doc comment for why.
        operation: String,
    },
    /// Every observable
    /// contribution `mod_id` makes under the order this was checked
    /// against is inert — it ships no assembly, every def it owns is
    /// overridden by a later-loading copy, every texture and keyed
    /// translation string it ships is overridden, and every mutating
    /// patch op it has either replays to a failure or never runs at all
    /// (gated off). The producer (`rim_session::use_cases::ContributesNothing`)
    /// lives in `rim-session`, not here — this crate has no replay of its
    /// own, exactly the identity-in-`rim-resolve`/producer-in-`rim-session`
    /// split [`Self::PatchWillFail`] also uses.
    ///
    /// Keyed on `mod_id` alone: unlike an owner-*set* key
    /// (`DefOverride`/`TextureOverride`/...), this finding is about one
    /// mod's own aggregate inertness, not a specific contested resource —
    /// there is no natural second field to widen the key with the way an
    /// owner set would need to when a new owner joins a conflict.
    ContributesNothing {
        /// The mod whose every contribution is inert.
        mod_id: ModId,
    },
    /// See [`rim_analyzer::domain::UndecodableTexture`]. Keyed on
    /// `(mod_id, path)`, not an owner set — this is a single-mod fact about
    /// one shipped file, the same shape [`Self::MissingTexturePath`] is
    /// keyed on, not a contested-resource shape that grows an owner list.
    UndecodableTexture {
        /// The mod shipping the undecodable file.
        mod_id: ModId,
        /// The normalized key (see [`rim_analyzer::domain::ScannedMod::undecodable_textures`]).
        path: String,
    },
    /// See [`rim_analyzer::domain::BrokenInheritance`]. Keyed on `(mod,
    /// parent_name, problem_kind)`, not the representative `child` or the
    /// `affected` list — both are evidence, derived fresh from data on
    /// every scan, never part of this finding's own identity. The two
    /// problem kinds never both occur for the same `(mod, parent_name)`
    /// pair (see that struct's own producer), but the key still carries
    /// `problem_kind` so a install that somehow flips from one to the
    /// other between scans reopens as a fresh finding rather than
    /// silently keeping a stale decision.
    BrokenInheritance {
        /// The mod whose def/template references `parent_name`.
        mod_id: ModId,
        /// The unresolved or wrong-typed `ParentName` value.
        parent_name: String,
        /// Which of the two problems this is.
        problem_kind: InheritanceProblemKind,
    },
    /// See [`rim_analyzer::domain::NearMissModReference`]. Keyed on
    /// `(referrer, kind, written)` — not `candidate`, so the finding
    /// survives a later scan resolving the typo to a *different*
    /// best-matching active mod (an installed-mod-list change), rather
    /// than silently orphaning the user's prior dismissal of it.
    NearMissModReference {
        /// The mod whose own file carries the written reference.
        referrer: ModId,
        /// Whether `written` is a `FindMod` name or a `MayRequire` id.
        kind: ModReferenceKind,
        /// The value as written.
        written: String,
    },
    /// See [`rim_analyzer::domain::DiscardedAddition`]. Keyed on
    /// `(replacer, adder, def, path)`, not `adder_path` — that field is
    /// evidence (which specific op of `adder`'s content was discarded),
    /// never part of the identity, the same "evidence, not identity" split
    /// [`Self::BrokenInheritance`] applies to its own `child`/`affected`.
    /// `def` is boxed: two `ModId`s plus an unboxed `DefKey` plus `path`
    /// pushed this variant, and every `Result` carrying a bare
    /// `FindingKey` in its `Err` (`PatchDecisionError::OutOfScope` and
    /// several `rim-session` error enums), past clippy's
    /// `result_large_err` threshold.
    DiscardedAddition {
        /// The mod whose `PatchOperationReplace` discards `adder`'s own
        /// content.
        replacer: ModId,
        /// The mod whose earlier addition is discarded.
        adder: ModId,
        /// The replaced def.
        def: Box<DefKey>,
        /// The replaced node's own display path.
        path: String,
    },
    /// See [`rim_analyzer::domain::DanglingDefReference`]. Keyed on
    /// `name` alone — the owner mods (`referrers`, `cause`) are evidence,
    /// derived fresh from data on every scan, never part of the identity,
    /// so this key survives a referrer being added, removed, or
    /// re-explained differently between scans, and a user's decision on
    /// one dangling name stays attached to it regardless of who else
    /// starts (or stops) referencing it.
    DanglingDefReference {
        /// The dangling `defName`.
        name: String,
    },
}

impl FindingKey {
    /// The def or `Name`-attributed template this finding is about, if
    /// any — the one link from the ledger to the def inspector, used to
    /// decide
    /// which finding cards get an "inspect" link. Exhaustive, no wildcard
    /// arm, the same discipline as `PatchScope::membership` and
    /// `ledger::findings::hidden_by_generated`: a new variant must decide
    /// explicitly whether it names a def, not fall through to `None` by
    /// omission.
    ///
    /// `DuplicateTemplateName` returns [`DefRef::name_only`] — a `@<name>`
    /// ref with no `def_type` at all, not
    /// [`DefKey::synthesize_for_template_name`]'s display-only sentinel
    /// type (still used by `Action::PreferWinner`, which is typed for a
    /// [`DefKey`] and has no ref grammar to round-trip through) — because
    /// the finding key itself never recorded the template's genuine
    /// `def_type`: `rim_analyzer`'s own `Indices::template_owners` is
    /// keyed by `Name` alone, collapsing two different-`def_type`
    /// templates that happen to share a `Name` into one entry (a quirk
    /// this method inherits rather than fixes). `InspectDef` must resolve a
    /// name-only `DefRef`
    /// by a Name-only scan across every def type
    /// (`SourceIndex::children_by_template`'s and `SourceIndex::templates`'s
    /// keys), never a `(def_type, Name)` lookup — a sentinel `def_type`
    /// would be a dead address there, since no real template is ever keyed
    /// under it.
    ///
    /// `TextureOverride`/`DuplicateAssembly`/`SoundOverride`/
    /// `RuntimePatchCollision` also carry a [`DefKey`] (synthesized the
    /// same way, for [`crate::domain::Action::PreferWinner`]'s sake), but
    /// a texture path, an assembly name, and a runtime-patch target are not
    /// defs or templates the inspector addresses (an overridden asset is a
    /// separate, non-`DefRef` category), so they return `None` here.
    /// `TranspilerCollision` returns `None` for the same "not a def"
    /// reason, but has no synthesized [`DefKey`] at all (no `PreferWinner`
    /// alternative is offered for it — see its own doc comment).
    #[must_use]
    pub fn def_ref(&self) -> Option<DefRef> {
        match self {
            Self::DefOverride { key, .. } => Some(DefRef::new(key.clone(), Selector::DefName)),
            Self::PatchCollision { key, selector, .. } => Some(DefRef::new(key.clone(), *selector)),
            Self::DuplicateTemplateName { name, .. } => Some(DefRef::name_only(name.clone())),
            // Names a specific referrer def directly
            // (`def`), unlike `TextureOverride`/`DuplicateAssembly`/
            // `SoundOverride`/`RuntimePatchCollision` below, which are
            // about an asset or a DLL target, never a def instance — so
            // this one *does* link to the def inspector.
            Self::MissingTexturePath { def, .. } => {
                Some(DefRef::new(def.clone(), Selector::DefName))
            }
            // Names a specific def/template instance directly, the same
            // reasoning as `MissingTexturePath` above — a def inspector link
            // can point straight at it.
            Self::PatchWillFail {
                def_key, selector, ..
            } => Some(DefRef::new(def_key.clone(), *selector)),
            Self::EdgeDropped { .. }
            | Self::DeclarationQuestioned { .. }
            | Self::DeclarationOverridden { .. }
            | Self::AnyOfChoice { .. }
            | Self::TextureOverride { .. }
            | Self::DuplicateAssembly { .. }
            | Self::KeyedTranslationCollision { .. }
            | Self::SoundOverride { .. }
            | Self::UndeclaredTypeDependency { .. }
            | Self::RuntimePatchCollision { .. }
            | Self::TranspilerCollision { .. }
            | Self::RuleOverruled { .. }
            | Self::PlacementOverruled { .. }
            | Self::PlacementQuestioned { .. }
            | Self::PlacementOrderingOverridden { .. }
            | Self::PlacementPromotesDependents { .. }
            | Self::LikelyDuplicateMod { .. }
            | Self::MissingMod { .. }
            | Self::MissingDependency { .. }
            | Self::IncompatiblePair { .. }
            | Self::UnsupportedVersion { .. }
            | Self::UndeclaredHardDependency { .. }
            | Self::LazyReferenceViolated { .. }
            | Self::TagInferred { .. }
            | Self::ContributesNothing { .. }
            // A texture path, not a def or template — same reasoning as
            // `TextureOverride`/`DuplicateAssembly`/`SoundOverride` above.
            | Self::UndecodableTexture { .. }
            // The *key* names a mod and a parent-name string, not one
            // specific def/template — `child`/`affected` are `Finding`
            // evidence the desktop links individually, not a single
            // address this key itself can hand back.
            | Self::BrokenInheritance { .. }
            // A written string reference, not a def or template at all.
            | Self::NearMissModReference { .. }
            // A dangling name resolves to no def anywhere — that's the
            // whole point of the finding, so there is deliberately no def
            // to hand back here (unlike `DuplicateTemplateName`'s own
            // `DefRef::name_only`, which addresses a name that *does*
            // resolve, just ambiguously).
            | Self::DanglingDefReference { .. } => None,
            // Names the replaced def directly, the same reasoning as
            // `MissingTexturePath`/`PatchWillFail` above — always
            // `DefName`: this finding's own producer
            // (`analysis::edges::patches::replace_discards_addition`) only
            // ever targets a `PatchOperationReplace`'s own resolved
            // `DefTarget`, which this key's own `def` is built from, and a
            // `[@Name="X"]` template replace is rare enough there's no
            // separate selector field to round-trip here.
            Self::DiscardedAddition { def, .. } => {
                Some(DefRef::new(def.as_ref().clone(), Selector::DefName))
            }
        }
    }
}
