//! Hiding findings a generated merge or compatibility mod already resolves.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;

use crate::domain::{FindingKey, GeneratedMods};

/// Whether every other mod in `owners` besides some Rimmerge-generated
/// member is inside that member's declared scope — the owner-set-shaped
/// half of [`hidden_by_generated`]'s rule (`PatchCollision`/
/// `TextureOverride`): a finding naming several owners is hidden the
/// moment *any one* of them is a generated mod whose scope fully covers
/// the rest, since that finding is then entirely that mod's own doing
/// (see [`GeneratedMods::hides`]).
fn owner_set_hidden(generated: &GeneratedMods, owners: &BTreeSet<ModId>) -> bool {
    owners.iter().any(|owner| {
        generated.contains(owner)
            && generated.hides(owner, owners.iter().filter(|other| *other != owner))
    })
}

/// The pair-shaped half of [`hidden_by_generated`]'s rule
/// (`UndeclaredHardDependency`/`LazyReferenceViolated`/`MissingDependency`):
/// hidden when either side is a Rimmerge-generated mod whose declared scope
/// covers the other side.
fn pair_hidden(generated: &GeneratedMods, a: &ModId, b: &ModId) -> bool {
    (generated.contains(a) && generated.hides(a, std::iter::once(b)))
        || (generated.contains(b) && generated.hides(b, std::iter::once(a)))
}

/// Whether `key` involves a Rimmerge-generated mod (the profile merge mod,
/// or an exported compat patch — see [`GeneratedMods`]) in a way that
/// mod's own declared scope covers, and should therefore be hidden rather
/// than surfaced: the generated mod is regenerated from its own decisions,
/// so a finding entirely about its own doing would just be noise reporting
/// on Rimmerge's own output. A finding a generated mod is party to but its
/// scope does *not* cover — including a collision between two Rimmerge
/// patches with overlapping scopes — stays visible (the "two active
/// Rimmerge patches" risk, deliberately not solved here).
///
/// Matched exhaustively, with no `_` arm, so a new [`FindingKey`] variant
/// forces this function to say whether it can ever name a mod a generated
/// mod could be (a generated mod shows up in the next scan like any other
/// mod). A generated mod (profile merge mod or compat patch alike) ships no
/// `Defs/` and no assemblies of its own, so it can never be a
/// `DefOverride`/`DuplicateAssembly`/`LikelyDuplicateMod`/`MissingMod`/
/// `IncompatiblePair`/`EdgeDropped`/`AnyOfChoice` owner — those kinds
/// always return `false`.
///
/// `TagInferred` *is* filtered, unlike the kinds above: a generated mod
/// lives under `Mods/` like any other, so a broadly-matching tag rule
/// could in principle still tag it.
/// [`crate::tags::infer_tags`]/[`crate::tags::collect_evidence`] already
/// exclude every generated mod so the tag is never actually inferred for
/// one in the first place (see those modules' own doc comments) — this arm
/// is the second line of defense, kept because a hand-authored tag rule
/// could still name a generated mod directly, bypassing inference
/// entirely.
pub(super) fn hidden_by_generated(key: &FindingKey, generated: &GeneratedMods) -> bool {
    match key {
        FindingKey::PatchCollision { mods, .. } => owner_set_hidden(generated, mods),
        FindingKey::TextureOverride { owners, .. } => owner_set_hidden(generated, owners),
        FindingKey::UndeclaredHardDependency { after, before }
        | FindingKey::LazyReferenceViolated { after, before } => {
            pair_hidden(generated, after, before)
        }
        FindingKey::DeclarationQuestioned {
            declared_after,
            declared_before,
            ..
        }
        | FindingKey::DeclarationOverridden {
            declared_after,
            declared_before,
            ..
        } => pair_hidden(generated, declared_after, declared_before),
        FindingKey::UnsupportedVersion { mod_id } | FindingKey::TagInferred { mod_id, .. } => {
            generated.contains(mod_id)
        }
        FindingKey::MissingDependency { mod_id, dependency } => {
            pair_hidden(generated, mod_id, dependency)
        }
        FindingKey::KeyedTranslationCollision { pair } => pair_hidden(generated, &pair.0, &pair.1),
        FindingKey::RuntimePatchCollision { owners, .. } => owner_set_hidden(generated, owners),
        FindingKey::TranspilerCollision { owners, .. } => owner_set_hidden(generated, owners),
        FindingKey::UndeclaredTypeDependency { user, provider, .. } => {
            pair_hidden(generated, user, provider)
        }
        FindingKey::RuleOverruled { after, before, .. } => pair_hidden(generated, after, before),
        // A `PlacementRule` pins a single mod — same reasoning as
        // `UnsupportedVersion`/`TagInferred` above.
        FindingKey::PlacementOverruled { mod_id, .. }
        | FindingKey::PlacementQuestioned { mod_id, .. } => generated.contains(mod_id),
        FindingKey::PlacementOrderingOverridden { mod_id, pinned, .. } => {
            pair_hidden(generated, mod_id, pinned)
        }
        FindingKey::PlacementPromotesDependents { mod_id, .. } => generated.contains(mod_id),
        // A generated mod (profile merge mod or compat patch alike) ships
        // no `Sounds/`, `Languages/*/Keyed/` overrides, or `Name`-attribute
        // templates of its own — see this function's own doc comment for
        // why the same is already true of `DefOverride`/`DuplicateAssembly`.
        // `referrer` is the only mod this finding is
        // about — a generated mod (profile merge mod or compat patch
        // alike) never carries a `texPath`/`iconPath` of its own (same
        // reasoning as `DefOverride`/`DuplicateAssembly` above, restated
        // in this function's own doc comment).
        FindingKey::MissingTexturePath { referrer, .. } => generated.contains(referrer),
        // `mod_id` is the only mod the key itself names
        // (see `PatchScope::membership`'s own doc comment on this same
        // variant for why `cause`'s mod isn't available here either) — if
        // the generated mod hides the failing patcher's own contribution,
        // the predicted operation no longer even runs, so the finding is
        // stale the same way any other single-mod fact's is.
        FindingKey::PatchWillFail { mod_id, .. } => generated.contains(mod_id),
        // `mod_id` is the only mod this key names — same
        // reasoning as `UnsupportedVersion`/`TagInferred` above. A
        // generated mod (profile merge mod or compat patch) ships no
        // patches/defs/textures/translations of its own that this
        // finding's own replay would ever credit it with anyway, but
        // hiding it here is still correct and consistent should one ever
        // somehow qualify.
        FindingKey::ContributesNothing { mod_id } => generated.contains(mod_id),
        // `mod_id` is the only mod this key names — same reasoning as
        // `MissingTexturePath` above: a generated mod ships no `.dds`
        // files of its own, but hiding it here is still correct and
        // consistent should one ever somehow qualify.
        FindingKey::UndecodableTexture { mod_id, .. } => generated.contains(mod_id),
        // `mod_id` is the only mod this key names — a generated mod ships
        // no `ParentName`-inheriting `Defs/` of its own, but hiding it
        // here is still correct and consistent should one ever somehow
        // qualify, same reasoning as `UndecodableTexture` above.
        FindingKey::BrokenInheritance { mod_id, .. } => generated.contains(mod_id),
        // `referrer` is the only mod this key names — a generated mod
        // ships no `FindMod`/`MayRequire` reference of its own, same
        // reasoning as `MissingTexturePath` above.
        FindingKey::NearMissModReference { referrer, .. } => generated.contains(referrer),
        // A pair fact between two real mods' own patch ops — same shape as
        // `UndeclaredHardDependency`/`MissingDependency` above: hidden when
        // either side is a generated mod whose own declared scope covers
        // the other. A generated compat patch's own exported
        // `PatchOperationReplace`/`Add` genuinely can be either side of
        // this relationship (unlike the single-mod facts above), so this
        // is the pair-shaped rule, not `generated.contains` alone.
        FindingKey::DiscardedAddition {
            replacer, adder, ..
        } => pair_hidden(generated, replacer, adder),
        FindingKey::EdgeDropped { .. }
        | FindingKey::AnyOfChoice { .. }
        | FindingKey::DefOverride { .. }
        | FindingKey::DuplicateAssembly { .. }
        | FindingKey::DuplicateTemplateName { .. }
        | FindingKey::SoundOverride { .. }
        | FindingKey::LikelyDuplicateMod { .. }
        | FindingKey::MissingMod { .. }
        | FindingKey::IncompatiblePair { .. }
        // The key names no mod at all (`referrers`/`cause` are `Finding`
        // evidence, never part of the identity — see
        // `FindingKey::DanglingDefReference`'s own doc comment), so there
        // is nothing here to check against `generated`.
        | FindingKey::DanglingDefReference { .. } => false,
    }
}
