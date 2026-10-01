//! A patch's scope: the mods it covers, and which findings belong to it.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;

use super::identity::CORE_MOD_ID;
use crate::domain::finding::FindingKey;

/// [`PatchScope::new`] rejects a scope with fewer than two members.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a patch scope needs at least two distinct mods, found {0}")]
pub struct PatchScopeError(pub(super) usize);

/// Two or more distinct active mods (base ids) a compat patch is *about*.
/// Membership tests compare [`ModId::base`], so a `_steam` copy of a scope
/// member counts as that member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchScope(BTreeSet<ModId>);

/// Whether a [`FindingKey`] is one a [`PatchScope`] can offer a decision on
/// — see [`PatchScope::membership`] for the per-variant rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScopeMembership {
    /// Every mod the key names is a scope member (Core excepted — see
    /// [`PatchScope::membership`]'s doc comment).
    Full,
    /// At least two scope members are involved, but `outside` also are.
    Partial {
        /// The owners this key names that are neither a scope member nor
        /// Core.
        outside: BTreeSet<ModId>,
    },
    /// Not a conflict between (enough) scope members for this patch to
    /// address.
    Outside,
}

impl PatchScope {
    /// Builds a scope from at least two distinct mods, normalizing every id
    /// to [`ModId::base`] first (so a `_steam` copy and its local
    /// counterpart never count as two separate members).
    ///
    /// # Errors
    ///
    /// Returns [`PatchScopeError`] when fewer than two distinct base ids
    /// remain after normalizing.
    pub fn new(members: impl IntoIterator<Item = ModId>) -> Result<Self, PatchScopeError> {
        let bases: BTreeSet<ModId> = members.into_iter().map(|id| id.base()).collect();
        if bases.len() < 2 {
            return Err(PatchScopeError(bases.len()));
        }
        Ok(Self(bases))
    }

    /// Every member, by base id.
    #[must_use]
    pub fn members(&self) -> &BTreeSet<ModId> {
        &self.0
    }

    /// Whether `id` (any `_steam`-suffixed variant included) is a scope
    /// member.
    #[must_use]
    pub fn contains(&self, id: &ModId) -> bool {
        self.0.contains(&id.base())
    }

    /// Whether `id` is an admissible source for a `Merge` field choice
    /// (`MergeChoice::From`): a declared scope member, or Core. Core is
    /// an implicit diff participant for any `DefOverride`/
    /// `PatchCollision` a scope admits (see [`Self::membership`]'s own doc
    /// comment) — the emitter already drops Core from `depends_on`, so a
    /// choice naming it costs the patch nothing to declare and must not be
    /// rejected as "outside scope" the way a real third-party mod would be.
    /// Unlike [`Self::contains`], this is **not** used for a `ShipAsset`
    /// source: Core ships no loose texture a patch could copy, so
    /// `PatchProject::decide`/`set_scope` keep using `contains` there.
    pub(super) fn admits_owner(&self, id: &ModId) -> bool {
        self.contains(id) || id.base().as_str() == CORE_MOD_ID
    }

    /// Whether `owners ∩ scope` is at least two, and if so, whether every
    /// other owner (`owners − scope − {Core}`) is also in scope
    /// ([`ScopeMembership::Full`]) or not
    /// ([`ScopeMembership::Partial`]).
    fn owner_set_membership(&self, owners: &BTreeSet<ModId>) -> ScopeMembership {
        let bases: BTreeSet<ModId> = owners.iter().map(ModId::base).collect();
        let inside_count = bases.iter().filter(|id| self.0.contains(*id)).count();
        if inside_count < 2 {
            return ScopeMembership::Outside;
        }
        let outside: BTreeSet<ModId> = bases
            .into_iter()
            .filter(|id| !self.0.contains(id) && id.as_str() != CORE_MOD_ID)
            .collect();
        if outside.is_empty() {
            ScopeMembership::Full
        } else {
            ScopeMembership::Partial { outside }
        }
    }

    /// Whether both `a` and `b` are scope members — the rule shared by
    /// every pair-shaped [`FindingKey`] variant, none of which has a
    /// `Partial` reading (a pair finding is either wholly about the scope
    /// or it isn't).
    fn pair_membership(&self, a: &ModId, b: &ModId) -> ScopeMembership {
        if self.contains(a) && self.contains(b) {
            ScopeMembership::Full
        } else {
            ScopeMembership::Outside
        }
    }

    /// The per-`FindingKey`-variant membership rule. Exhaustive, no `_`
    /// arm, so a future
    /// `FindingKey` variant is a compile error here rather than silently
    /// falling through to `Outside`.
    #[must_use]
    pub fn membership(&self, key: &FindingKey) -> ScopeMembership {
        match key {
            FindingKey::EdgeDropped { after, before, .. } => self.pair_membership(after, before),
            FindingKey::DeclarationQuestioned {
                declared_after,
                declared_before,
                ..
            }
            | FindingKey::DeclarationOverridden {
                declared_after,
                declared_before,
                ..
            } => self.pair_membership(declared_after, declared_before),
            FindingKey::AnyOfChoice { .. } => ScopeMembership::Outside,
            FindingKey::DefOverride { owners, .. } => self.owner_set_membership(owners),
            FindingKey::PatchCollision { mods, .. } => self.owner_set_membership(mods),
            FindingKey::TextureOverride { owners, .. } => self.owner_set_membership(owners),
            FindingKey::DuplicateAssembly { owners, .. } => self.owner_set_membership(owners),
            FindingKey::DuplicateTemplateName { owners, .. } => self.owner_set_membership(owners),
            FindingKey::KeyedTranslationCollision { pair } => {
                self.pair_membership(&pair.0, &pair.1)
            }
            FindingKey::RuntimePatchCollision { owners, .. } => self.owner_set_membership(owners),
            FindingKey::TranspilerCollision { owners, .. } => self.owner_set_membership(owners),
            FindingKey::SoundOverride { owners, .. } => self.owner_set_membership(owners),
            FindingKey::UndeclaredTypeDependency { user, provider, .. } => {
                self.pair_membership(user, provider)
            }
            FindingKey::LikelyDuplicateMod { pair } => self.pair_membership(&pair.0, &pair.1),
            FindingKey::MissingMod { .. } => ScopeMembership::Outside,
            FindingKey::MissingDependency { mod_id, dependency } => {
                self.pair_membership(mod_id, dependency)
            }
            FindingKey::IncompatiblePair { pair } => self.pair_membership(&pair.0, &pair.1),
            FindingKey::UnsupportedVersion { .. } => ScopeMembership::Outside,
            FindingKey::UndeclaredHardDependency { after, before } => {
                self.pair_membership(after, before)
            }
            FindingKey::LazyReferenceViolated { after, before } => {
                self.pair_membership(after, before)
            }
            FindingKey::TagInferred { .. } => ScopeMembership::Outside,
            FindingKey::RuleOverruled { after, before, .. } => self.pair_membership(after, before),
            // A `PlacementRule` pins a single mod, not a relation between
            // two — same reasoning as `MissingMod`/`UnsupportedVersion`/
            // `TagInferred` above: a single-mod fact isn't addressable by
            // a compat patch, which edits XML for a conflict *between*
            // scope members.
            FindingKey::PlacementOverruled { .. } | FindingKey::PlacementQuestioned { .. } => {
                ScopeMembership::Outside
            }
            // A relation between two mods (the one forced across the
            // pin's own extreme edge, and the pin itself) — same
            // reasoning as `RuleOverruled` above.
            FindingKey::PlacementOrderingOverridden { mod_id, pinned, .. } => {
                self.pair_membership(mod_id, pinned)
            }
            // A single-mod fact about the placement rule itself — same
            // reasoning as `PlacementOverruled`/`PlacementQuestioned`
            // above, not addressable by a compat patch.
            FindingKey::PlacementPromotesDependents { .. } => ScopeMembership::Outside,
            // A single-mod diagnostic fact about one
            // def's own field value — not a conflict *between* scope
            // members, so not addressable by a compat patch the same way
            // `MissingMod`/`UnsupportedVersion`/`TagInferred` above
            // aren't.
            FindingKey::MissingTexturePath { .. } => ScopeMembership::Outside,
            // Considered, not a default. The key names
            // only the failing patcher (`mod_id`) — the mod that could
            // fix it via reorder lives on `Finding::PatchWillFail.cause`,
            // not on the key, so `membership` (which never sees a
            // `Finding`) has no second party to pair it against the way
            // `RuleOverruled`/`PlacementOrderingOverridden` above do. Nor
            // is that the real reason to keep it `Outside`, though: a
            // predicted patch failure isn't a conflict a compat patch
            // *edits XML to resolve* the way `DefOverride`/`PatchCollision`
            // are — its own fix is a `Reorder` decision (or leaving the
            // operation failing), neither of which touches a patch
            // project's own scope at all. Single-mod diagnostic fact,
            // same category as `MissingTexturePath` above.
            FindingKey::PatchWillFail { .. } => ScopeMembership::Outside,
            // A single-mod fact about `mod_id`'s own
            // aggregate inertness, same category as `MissingMod`/
            // `UnsupportedVersion`/`TagInferred` above — not a conflict
            // between scope members a compat patch edits XML to resolve.
            FindingKey::ContributesNothing { .. } => ScopeMembership::Outside,
            // A single-mod diagnostic fact about one shipped file — not a
            // conflict *between* scope members, same reasoning as
            // `MissingTexturePath` above.
            FindingKey::UndecodableTexture { .. } => ScopeMembership::Outside,
            // A single-mod fact about one def/template's own broken
            // `ParentName` — not a conflict *between* scope members, same
            // reasoning as `MissingTexturePath`/`UndecodableTexture`
            // above.
            FindingKey::BrokenInheritance { .. } => ScopeMembership::Outside,
            // A single-mod fact about one written reference — same
            // reasoning as `BrokenInheritance` above.
            FindingKey::NearMissModReference { .. } => ScopeMembership::Outside,
            // A load-order fact (an edge, when not deliberately
            // overridden) or, when it is, a disclosure of the replacer's
            // own choice — not a conflict a compat patch edits XML to
            // resolve, the same reasoning as `EdgeDropped`/`PatchWillFail`
            // above.
            FindingKey::DiscardedAddition { .. } => ScopeMembership::Outside,
            // A name-shaped fact with no def to merge subject at all —
            // resolution doesn't depend on load order (cross-references
            // resolve once, after every def and patch has loaded), and
            // there's nothing here a compat patch's own XML could fix.
            FindingKey::DanglingDefReference { .. } => ScopeMembership::Outside,
        }
    }

    /// Whether this scope admits `key` at all — [`ScopeMembership::Full`]
    /// or [`ScopeMembership::Partial`], never [`ScopeMembership::Outside`].
    #[must_use]
    pub fn admits(&self, key: &FindingKey) -> bool {
        matches!(
            self.membership(key),
            ScopeMembership::Full | ScopeMembership::Partial { .. }
        )
    }
}
