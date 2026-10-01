//! [`Edge`]: one load-order-relevant relationship between two mods.

use serde::{Deserialize, Serialize};

use super::mod_id::ModId;

/// How an edge was derived. `PartialOrd`/`Ord`/`Hash` let
/// `rim-resolve`'s `FindingKey` (which embeds an `EdgeKind`) derive them
/// in turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// A shipped DLL references an assembly owned by another mod.
    AssemblyRef,
    ForceLoadAfter,
    ForceLoadBefore,
    LoadAfter,
    LoadBefore,
    /// A `modDependencies` entry. RimWorld itself only warns when the
    /// target is missing (see [`crate::domain::MissingDependency`]) and
    /// never reorders by it — but it is the author's own statement of a
    /// load-order requirement (RimSort treats it as `loadAfter`), so the
    /// engine enforces it as `Declared` too; see [`EdgeKind::strength`].
    ModDependency,
    /// A `PatchOperationFindMod` names another mod by display name.
    FindMod,
    /// A `LoadFolders.xml` folder is gated on another mod via `IfModActive`.
    IfModActive,
    /// A mutating patch operation targets a def owned by exactly one other mod.
    PatchTargetsDef,
    /// A def or patch operation carries `MayRequire`/`MayRequireAnyOf`
    /// naming another active mod.
    MayRequire,
    /// A mutating patch operation's xpath selects a type string another
    /// active mod's own patch injects, and no active mod writes that type
    /// inline anywhere in its `Defs/`. An XML mechanism that genuinely
    /// needs load order: RimWorld applies every mod's patches
    /// after every mod's defs load, so the injected node must exist by
    /// the time this op's xpath runs.
    PatchInjectedNode,
    /// Two active mods ship the same-named assembly at different
    /// versions: the higher-version shipper must load before every lower
    /// one — the first-loaded copy wins at runtime, and an older copy
    /// loading first is the more likely API break.
    AssemblyVersionPrecedence,
    /// A def in this mod names a type owned by another active mod's
    /// shipped DLL — ownership by the DLL's own assembly name, never by
    /// which mods merely reference the same namespace. Also carries
    /// `PatchInjectedNode`'s demoted class-string cases: a patch-injected
    /// type this xpath selects that
    /// already exists inline somewhere, so the order isn't provably
    /// required.
    UsesType,
    /// A def's (or template's) `ParentName` names a `Name`-attributed
    /// template registered by exactly one other active mod, with no
    /// self-, vanilla- or patch-registered copy to fall back on.
    ///
    /// **A load-time ordering fact, not awareness**, per the decompiled
    /// `Verse.XmlInheritance.GetBestParentFor`. The rule is: a child
    /// resolves its `ParentName` to the registration with the
    /// greatest `mod.loadOrder` among those `<= ` its own, else to a
    /// `mod == null` registration (vanilla content and anything a patch
    /// added), else *none* — and none means
    /// `Log.Error("XML error: Could not find parent node named ...")` and
    /// the def, plus everything inheriting from it, loads without any of
    /// the parent's own inherited fields. So when the only registration
    /// belongs to one other mod, that mod must load at or before this one
    /// or content silently goes missing at load time: the same class of
    /// failure a load-time `AssemblyRef` is `Hard` for, which is why
    /// [`EdgeKind::strength`] returns `Hard` here.
    ///
    /// Every case where the lookup *cannot* fail produces no edge at all
    /// — the child mod's own copy, a Core/DLC copy (vanilla loads first
    /// by tier, so its `loadOrder` is `<=` everyone's), and a copy any
    /// active mod's patch registers. A name with two or more foreign
    /// registrations is an any-of requirement rather than a single edge
    /// and is skipped; see [`crate::analysis::edges::parent_template_edges`].
    ParentTemplate,
    // The variants below are appended at the end of the enum on purpose,
    // never interleaved among the ones above — `EdgeKind` derives
    // `Ord`/`Hash` and `rim-resolve`'s `FindingKey` embeds it, so appending
    // is what keeps every existing `FindingKey` ordering (and any
    // already-persisted `decisions.json` sort-derived comparison) stable
    // when a variant is added.
    /// "Remover loads last": an active `PatchOperationRemove`
    /// (or a `PatchOperationConditional` whose own xpath equals the
    /// Remove's, i.e. an "if exists, then remove" that runs unconditionally)
    /// in `after` targets the same def path (or an ancestor path of it) a
    /// mutating op in `before` also targets — a genuinely conditional
    /// remove (nested under a Conditional with a *different* xpath) is
    /// excluded here and stays advisory instead. `Edge.subject` carries
    /// the removed node's own path, `"{def_type}/{def_name}/{sub_path}"`
    /// (no `sub_path` segment for a whole-def remove). **Known gap**: an
    /// attribute-predicate remover (e.g.
    /// `ThingDef[@EX_Legacy_Hook="RemoveMe"]`) has no [`crate::domain::DefTarget`]
    /// — [`crate::extract::xpath_target`] can't resolve it — so it
    /// produces no edge of this kind at all; the patch replay is what
    /// catches that case, not this one. **A node-keeping `PatchOperationReplace`
    /// is a remover too**, of everything strictly below the node it
    /// keeps — see [`crate::analysis::edges::patch_removed_node_edges`]'s
    /// own doc comment for the full rule (the restricted, `Remove`/
    /// `Replace`/`Insert`-only toucher set that pass alone applies, and
    /// why it never overlaps [`Self::ReplaceDiscardsAddition`]).
    PatchRemovedNode,
    /// A texture-only active mod (no defs, templates,
    /// mutating patch ops, or assemblies of its own) ships the same
    /// texture path as a content-shipping active mod — the texture-only
    /// mod must load after every content owner it overrides, so its
    /// replacement texture is the one that actually wins. `Edge.subject`
    /// carries the normalized texture path.
    RetextureAfterOwner,
    /// A def with two or more active owners and no
    /// `Declared`/`Hard` edge between them, where exactly one owner can be
    /// inferred as the def's *origin* (it owns the `Name`-attributed
    /// template the def's `ParentName` names, or it owns strictly more
    /// `Defs/`-inline defs sharing the def's own name prefix than every
    /// other owner) — every other owner is ordered after the origin.
    /// `Edge.subject` carries the def's own key,
    /// `"{def_type}/{def_name}"`.
    DefOverrideAfterOrigin,
    /// A mutating patch operation's xpath selects a *node path* another active
    /// mod's own patch injects, but that path already ships inline (or lies
    /// deeper than the inline-node index reaches), so the order isn't provably
    /// required (the path-shape rule, `emit_path_injected_node_edge`) — the
    /// demoted sibling of `PatchInjectedNode`'s path shape, the way `UsesType`
    /// is the demoted sibling of its *class*-string shape. Split out from
    /// `UsesType` because `Edge.subject` here is an XML path
    /// (`"{def_type}/{name_segment}[/{sub_path}]"`), never a type name —
    /// `UsesType`'s own original meaning ("this mod's assembly names a type
    /// from that mod's assembly") and its class-shape demotion (`patch selects
    /// '<Type.Name>'`) both genuinely name a type. Appended here, not
    /// interleaved with `UsesType` above, for the `Ord`/`Hash` stability reason
    /// in the comment above `PatchRemovedNode`.
    PatchSelectsInjectedNode,
    /// An active `PatchOperationReplace`/`PatchOperationRemove` in
    /// `after` replaces or removes a node another active mutating op in
    /// `before` reads through a predicate — `.../li[label="X"]/label`
    /// (the predicate's own key child) or `.../li[label="X"]` itself
    /// (a `Replace` of the whole predicated node; a same-shape `Remove`
    /// is already [`EdgeKind::PatchRemovedNode`]'s job) — where `before`'s
    /// own xpath depends on that predicate still resolving. Engine fact:
    /// RimWorld combines every active mod's Defs into one document and
    /// runs every mod's patch operations over it in load order, each
    /// xpath evaluated against the document's *current* state, so once
    /// `after` rewrites or deletes the node the predicate reads, any op
    /// still relying on it must already have run. `Edge.subject` carries
    /// the predicate key child's own display path,
    /// `"{def_type}/{def_name}/{sub_path}"`. Appended here, at the end,
    /// for the same `Ord`/`Hash` stability reason.
    PatchInvalidatesPredicate,
    /// The cosmetic sibling of [`Self::PatchRemovedNode`]: emitted instead
    /// of it for a remover/toucher pair whose final document is
    /// provably identical either order (a `Replace` swaps in
    /// its own value regardless of order, a `Remove` either deletes
    /// first or deletes the toucher's own just-written content
    /// afterward, so nothing of the toucher survives in either
    /// ordering), so only which mod's own op logs `failed` changes, not
    /// the game's final defs. See
    /// [`crate::analysis::edges::patch_removed_node_edges`]'s own doc
    /// comment for the four conditions that make a candidate pair
    /// cosmetic rather than content. `Edge.subject` carries the same
    /// removed-node path text `PatchRemovedNode` does. Appended here, at
    /// the end, for the same `Ord`/`Hash` stability reason as every
    /// other variant above.
    PatchRemovedNodeCosmetic,
    /// An active `PatchOperationReplace` (`before`) discards `after`'s
    /// own earlier addition inside the replaced node — RimWorld applies
    /// patches in one flat load-order sequence over one combined
    /// document, so a `Replace` that runs after an `Add`/
    /// `AddModExtension`/`AttributeAdd`/`AttributeSet` swaps in a whole
    /// new node and silently discards whatever the earlier op wrote into
    /// it, with **no log line at all** — both operations report success.
    /// `after` must load first so its content lands *inside* the
    /// replacement instead. `PatchOperationInsert` is deliberately not
    /// one of the additive classes this covers — its own required anchor
    /// makes it read/modify content instead, [`Self::PatchRemovedNode`]'s
    /// own domain (see `analysis::edges::patches::READING_CLASS_SUFFIXES`'s
    /// doc comment for the full reasoning). See
    /// [`crate::analysis::edges::replace_discards_addition`]'s
    /// own doc comment for the full rule (same-node and ancestor cases,
    /// the "keeps the node" exclusion, the duplicate-content exclusion,
    /// and the declared-override exclusion that routes to
    /// [`crate::domain::DiscardedAddition`] instead of this edge).
    /// `Edge.subject` carries the replaced node's own display path, the
    /// same convention [`Self::PatchRemovedNode`] uses. Appended here, at
    /// the end, for the same `Ord`/`Hash` stability reason as every
    /// other variant above.
    ReplaceDiscardsAddition,
}

/// How strongly an [`EdgeKind`] constrains load order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeStrength {
    /// Breaking this breaks the game: a missing type/method at runtime.
    Hard,
    /// The author declared this ordering explicitly.
    Declared,
    /// A DLL-load-time relationship RimWorld doesn't actually enforce
    /// (e.g. a lazily-resolved `AssemblyRef`): weaker than `Declared`
    /// because no author wrote it down, but more specific than a bare
    /// awareness signal.
    Soft,
    /// A heuristic the analyzer infers rather than reads off an author's own
    /// declaration — `PatchRemovedNode`, `RetextureAfterOwner`,
    /// `DefOverrideAfterOrigin`. `EdgeStrength` itself derives no `Ord` (this
    /// variant's position among its siblings above carries no ordering
    /// meaning); the real placement is `rim-resolve`'s own `Layer::Inferred`,
    /// documented there to sit after `SteamDb` and before `Soft` in
    /// `LAYER_ORDER` — strong enough to beat the bare name tie-break (otherwise
    /// the heuristic changes nothing) but weaker than every author declaration,
    /// the user's own decisions, and any imported RimSort/SteamDB rule, since
    /// none of those are proven the way a `Hard`/`Declared` edge is.
    /// `rim-resolve`'s own `EnforcedLayers::inferred` defaults `true` (unlike
    /// `soft`/`awareness`, which default `false`): see that field's own doc
    /// comment for why a heuristic edge is enforced by default while a
    /// merely-advisory one isn't.
    Inferred,
    /// Evidence the mods interact, without an explicit ordering promise.
    Awareness,
}

impl EdgeKind {
    /// This kind's strength on its own, ignoring any edge-specific
    /// evidence (see [`Edge::strength`] for the cases — `AssemblyRef` —
    /// where the edge itself refines this).
    #[must_use]
    pub fn strength(self) -> EdgeStrength {
        match self {
            // `PatchInjectedNode` and `ParentTemplate` are the XML
            // mechanisms that genuinely need load order: a violated
            // `ParentTemplate` edge loads its def without any of the
            // parent's inherited fields, and logs an `XML error` (see that
            // variant's own doc comment for the decompiled `XmlInheritance`
            // rule). `Hard` rather than
            // `Declared`: nobody *declared* this ordering, so `Declared`'s
            // own "the author declared this explicitly" does not describe
            // it, while `Hard`'s "breaking this breaks the game" does.
            Self::ForceLoadAfter
            | Self::ForceLoadBefore
            | Self::PatchInjectedNode
            | Self::ParentTemplate => EdgeStrength::Hard,
            // Baseline for an `AssemblyRef` edge; `Edge::strength` always
            // overrides this using the edge's own `load_time` flag.
            Self::AssemblyRef => EdgeStrength::Hard,
            // `ModDependency` is `Declared`, not just `LoadAfter`/`LoadBefore`
            // themselves: see the variant's own doc comment.
            // `AssemblyVersionPrecedence` is `Declared`, not `Hard`:
            // the analyzer cannot prove the older copy is API-incompatible,
            // so a user decision must be able to override it.
            Self::LoadAfter
            | Self::LoadBefore
            | Self::ModDependency
            | Self::AssemblyVersionPrecedence => EdgeStrength::Declared,
            Self::FindMod
            | Self::IfModActive
            | Self::PatchTargetsDef
            | Self::MayRequire
            | Self::UsesType
            | Self::PatchSelectsInjectedNode => EdgeStrength::Awareness,
            // Every heuristic edge kind is `Inferred` —
            // `PatchInvalidatesPredicate` included: droppable like
            // `PatchRemovedNode`, not `Hard` like
            // `PatchInjectedNode`/`ParentTemplate`, since the analyzer
            // infers it from patch content rather than reading it off a
            // proven-always-fails engine mechanism the way those two are.
            // `PatchRemovedNodeCosmetic` is `Inferred` too — the owner's
            // own decision is "enforced when free, but dropped
            // first among Inferred siblings in a cycle" (`Soft` would be
            // advisory-only and never move a mod, which the wording
            // reads as the wrong reading) — see `sort::cycles::kind_rank`'s
            // own doc comment for where "dropped first" actually lives.
            // `ReplaceDiscardsAddition` is `Inferred` for the identical
            // reason: a heuristic conclusion the analyzer draws from
            // patch content, droppable in a cycle, never `Hard`.
            Self::PatchRemovedNode
            | Self::RetextureAfterOwner
            | Self::DefOverrideAfterOrigin
            | Self::PatchInvalidatesPredicate
            | Self::PatchRemovedNodeCosmetic
            | Self::ReplaceDiscardsAddition => EdgeStrength::Inferred,
        }
    }
}

/// A directed load-order constraint: `after` must load after `before`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub after: ModId,
    pub before: ModId,
    pub kind: EdgeKind,
    pub detail: String,
    /// For an `AssemblyRef` edge: whether the reference is one RimWorld
    /// resolves at DLL-load time (a base type or implemented interface),
    /// as opposed to a lazily-resolved method/field/attribute reference.
    /// Meaningless for every other `EdgeKind` (always `true` there, so
    /// [`EdgeKind::strength`] is left untouched).
    pub load_time: bool,
    /// The specific name this edge is evidence about, when one exists
    /// structurally rather than only inside `detail`'s free text: the
    /// type string for
    /// [`EdgeKind::UsesType`], the template name for
    /// [`EdgeKind::ParentTemplate`], the injected class or node path for
    /// [`EdgeKind::PatchInjectedNode`], the removed node's own path for
    /// [`EdgeKind::PatchRemovedNode`], the texture path for
    /// [`EdgeKind::RetextureAfterOwner`], the def key for
    /// [`EdgeKind::DefOverrideAfterOrigin`], the selected node path
    /// for [`EdgeKind::PatchSelectsInjectedNode`], the predicate key
    /// child's own display path for
    /// [`EdgeKind::PatchInvalidatesPredicate`], and the same removed-node
    /// path text `PatchRemovedNode` uses for
    /// [`EdgeKind::PatchRemovedNodeCosmetic`] and
    /// [`EdgeKind::ReplaceDiscardsAddition`] (the replaced node's own
    /// display path). `None` for every other kind.
    /// `#[serde(default)]` so a report cached before this field existed
    /// still deserializes (as `None`, the right value for an edge that
    /// predates the field).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
}

impl Edge {
    /// This edge's effective strength: for `AssemblyRef`, `load_time`
    /// decides between `Hard` (resolved at DLL-load time — breaking load
    /// order breaks the game) and `Soft` (resolved lazily — works in any
    /// order); every other kind uses [`EdgeKind::strength`] unchanged.
    #[must_use]
    pub fn strength(&self) -> EdgeStrength {
        match self.kind {
            EdgeKind::AssemblyRef => {
                if self.load_time {
                    EdgeStrength::Hard
                } else {
                    EdgeStrength::Soft
                }
            }
            other => other.strength(),
        }
    }
}

/// Whether the current load order satisfies an [`Edge`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeStatus {
    Satisfied,
    Violated,
    /// One or both endpoints aren't in the current load order, so the
    /// edge can't be evaluated against it.
    Unevaluated,
}

/// An [`Edge`] paired with its evaluation against the current load order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeReport {
    #[serde(flatten)]
    pub edge: Edge,
    pub status: EdgeStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edge(kind: EdgeKind, load_time: bool) -> Edge {
        Edge {
            after: ModId::new("a"),
            before: ModId::new("b"),
            kind,
            detail: String::new(),
            load_time,
            subject: None,
        }
    }

    #[test]
    fn force_load_edges_are_always_hard() {
        assert_eq!(EdgeKind::ForceLoadAfter.strength(), EdgeStrength::Hard);
        assert_eq!(EdgeKind::ForceLoadBefore.strength(), EdgeStrength::Hard);
    }

    #[test]
    fn declared_edges_are_author_stated_ordering() {
        assert_eq!(EdgeKind::LoadAfter.strength(), EdgeStrength::Declared);
        assert_eq!(EdgeKind::LoadBefore.strength(), EdgeStrength::Declared);
    }

    /// `modDependencies` is the author's own declared requirement —
    /// RimSort treats it as `loadAfter`, and so does the engine now.
    #[test]
    fn mod_dependency_is_declared_ordering() {
        assert_eq!(EdgeKind::ModDependency.strength(), EdgeStrength::Declared);
    }

    #[test]
    fn awareness_edges_are_soft_evidence() {
        assert_eq!(EdgeKind::FindMod.strength(), EdgeStrength::Awareness);
        assert_eq!(EdgeKind::IfModActive.strength(), EdgeStrength::Awareness);
        assert_eq!(
            EdgeKind::PatchTargetsDef.strength(),
            EdgeStrength::Awareness
        );
        assert_eq!(EdgeKind::MayRequire.strength(), EdgeStrength::Awareness);
        assert_eq!(EdgeKind::UsesType.strength(), EdgeStrength::Awareness);
    }

    /// The decompiled `XmlInheritance` rule makes an unresolvable `ParentName`
    /// load its def without the parent's inherited fields, and an `XML error`
    /// at load time, not a mere awareness signal — the producer emits an
    /// edge only for the cases where the lookup genuinely can fail, so
    /// every surviving edge is `Hard`.
    #[test]
    fn parent_template_is_a_load_time_hard_fact() {
        assert_eq!(EdgeKind::ParentTemplate.strength(), EdgeStrength::Hard);
    }

    /// The path-shape sibling of `UsesType` carries the same `Awareness`
    /// strength as `UsesType` — the label differs, never how strongly the
    /// sorter treats it.
    #[test]
    fn patch_selects_injected_node_is_awareness_strength() {
        assert_eq!(
            EdgeKind::PatchSelectsInjectedNode.strength(),
            EdgeStrength::Awareness
        );
    }

    /// A patch-injected node that no active mod writes inline is an XML
    /// mechanism the loader genuinely needs ordered — `Hard`.
    #[test]
    fn patch_injected_node_is_hard() {
        assert_eq!(EdgeKind::PatchInjectedNode.strength(), EdgeStrength::Hard);
    }

    /// Newest-version-first is the author's own implied statement
    /// (RimWorld: first loaded wins), but not provably safe — `Declared`,
    /// overridable by a user decision.
    #[test]
    fn assembly_version_precedence_is_declared() {
        assert_eq!(
            EdgeKind::AssemblyVersionPrecedence.strength(),
            EdgeStrength::Declared
        );
    }

    #[test]
    fn load_time_assembly_ref_is_hard() {
        let e = edge(EdgeKind::AssemblyRef, true);
        assert_eq!(e.strength(), EdgeStrength::Hard);
    }

    #[test]
    fn lazy_assembly_ref_is_soft() {
        let e = edge(EdgeKind::AssemblyRef, false);
        assert_eq!(e.strength(), EdgeStrength::Soft);
    }

    /// Every heuristic edge kind is `Inferred`.
    #[test]
    fn inferred_edge_kinds_are_inferred_strength() {
        assert_eq!(
            EdgeKind::PatchRemovedNode.strength(),
            EdgeStrength::Inferred
        );
        assert_eq!(
            EdgeKind::RetextureAfterOwner.strength(),
            EdgeStrength::Inferred
        );
        assert_eq!(
            EdgeKind::DefOverrideAfterOrigin.strength(),
            EdgeStrength::Inferred
        );
        assert_eq!(
            EdgeKind::PatchInvalidatesPredicate.strength(),
            EdgeStrength::Inferred
        );
        assert_eq!(
            EdgeKind::ReplaceDiscardsAddition.strength(),
            EdgeStrength::Inferred
        );
    }
}
