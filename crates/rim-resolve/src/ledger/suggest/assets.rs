//! Suggestions for asset findings: textures, sounds, templates, and keyed translations.

use super::confidence;
use crate::domain::{Action, Alternative, DefKey, Rationale};
use rim_analyzer::domain::ModId;

pub(super) fn texture_override(texture_path: &str, owners: &[ModId]) -> crate::domain::Suggestion {
    use crate::domain::Suggestion;

    // `Action::PreferWinner` is typed for a `DefKey`; a texture path isn't
    // one, so this synthesizes a display-only key via
    // `DefKey::synthesize_for_texture`. `DecisionSet::sorter_overrides`'s
    // `owners_of` recognizes that same synthesized key back against this
    // finding's own `texture_path` (see its doc comment), so choosing this
    // alternative expands to a real `Reorder` exactly like it would for a
    // `DefOverride`/`PatchCollision` finding.
    let mut alternatives: Vec<Alternative> = owners
        .iter()
        .map(|owner| Alternative {
            action: Action::PreferWinner {
                key: DefKey::synthesize_for_texture(texture_path),
                winner: owner.clone(),
            },
            rationale: Rationale::ForceTextureWinner,
        })
        .collect();
    // `ShipAsset` copies one owner's file into the generated merge mod
    // outright, so the choice is order-independent — unlike
    // `PreferWinner`, which just reorders who wins today.
    alternatives.extend(owners.iter().map(|owner| Alternative {
        action: Action::ShipAsset {
            texture_path: texture_path.to_string(),
            from: owner.clone(),
        },
        rationale: Rationale::CopyTextureIntoMergeMod,
    }));

    // A texture override is cosmetic and browsable in-game regardless of
    // who wrote it or which mod wins — never worth blocking the inbox on,
    // author match or not.
    Suggestion {
        action: Action::Accept,
        confidence: confidence(90),
        rationale: Rationale::TextureOverrideCosmetic,
        alternatives,
    }
}

/// A template `Name` registered by more than one active mod.
///
/// Ground-truthed against the decompiled `Verse.XmlInheritance`
/// (`TryRegister`/`GetBestParentFor` in `Assembly-CSharp.dll`): registering
/// the same `Name` twice only ever errors *within one mod*
/// (`TryRegister`'s own `value[i].mod == mod` check) — across different
/// mods, every registration coexists in RimWorld's own dictionary, and
/// there is no single "last one wins" resolution at registration time at
/// all. Instead, `GetBestParentFor` resolves **each child independently**
/// to whichever registration belongs to the nearest mod loading at or
/// before it — falling back to a vanilla registration (or, with none,
/// the lowest-loaded mod's) only when nothing qualifies. So two different
/// children of the *same* duplicated name can legitimately end up
/// inheriting from two different mods, depending on where each child's
/// own mod happens to sit — there is no single global winner this
/// finding could ever correctly name, and a duplicate name is only
/// actually harmful for a child that resolves to a registration the user
/// didn't intend, never merely because two mods share the name.
///
/// `rim-resolve` has no XML of its own, so it can't say *which* children
/// are affected or whether any of them currently resolve differently
/// than the suggested order would produce — only `rim-session`'s own
/// pass (`Session::duplicate_template_children`, sourced from
/// `SourceIndex::children_by_template`) has that data, and it's also what
/// makes the `PreferWinner` alternatives below take effect:
/// choosing one orders that mod's registration immediately before every
/// one of the template's own (non-vanilla, non-registrant) children.
pub(super) fn duplicate_template_name(name: &str, owners: &[ModId]) -> crate::domain::Suggestion {
    use crate::domain::Suggestion;

    let alternatives: Vec<Alternative> = owners
        .iter()
        .map(|owner| Alternative {
            action: Action::PreferWinner {
                key: DefKey::synthesize_for_template_name(name),
                winner: owner.clone(),
            },
            rationale: Rationale::ForceTemplateRegistrationWinner,
        })
        .collect();

    Suggestion {
        action: Action::Accept,
        confidence: confidence(60),
        rationale: Rationale::DuplicateTemplateNameExplanation {
            owner_count: owners.len(),
        },
        alternatives,
    }
}

/// The same `Languages/*/Keyed` key defined by more than one active
/// mod — the last-loaded definition wins, and the ledger groups every key
/// two mods collide over into one finding per pair (see
/// `ledger::findings::extract`), so `keys.len()` is `Finding`'s own
/// aggregated count, not a single key's identity.
pub(super) fn keyed_translation_collision(key_count: usize) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(80),
        rationale: Rationale::KeyedTranslationCollision { key_count },
        alternatives: Vec::new(),
    }
}

/// The same normalized sound path shipped by more than one active
/// mod — mirrors [`texture_override`], minus `ShipAsset` (that action's
/// own `texture_path` field is texture-specific; sounds get `PreferWinner`
/// only).
pub(super) fn sound_override(path: &str, owners: &[ModId]) -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(90),
        rationale: Rationale::SoundOverrideCosmetic,
        alternatives: owners
            .iter()
            .map(|owner| Alternative {
                action: Action::PreferWinner {
                    key: DefKey::synthesize_for_sound(path),
                    winner: owner.clone(),
                },
                rationale: Rationale::ForceSoundWinner,
            })
            .collect(),
    }
}

/// A `texPath`/`texPathFemale`/`iconPath`/`uiIconPath`
/// value naming a texture no active mod ships — diagnostic only, `Accept`
/// 90, never an ordering action (no load-order fix could make a missing
/// texture appear).
pub(super) fn missing_texture_path() -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(90),
        rationale: Rationale::MissingTexturePath,
        alternatives: Vec::new(),
    }
}

/// See [`crate::domain::Finding::UndecodableTexture`]. Informational, like
/// [`missing_texture_path`] above: there is no alternative to offer — the
/// fix is re-encoding the file, something outside this workspace.
pub(super) fn undecodable_texture() -> crate::domain::Suggestion {
    crate::domain::Suggestion {
        action: Action::Accept,
        confidence: confidence(90),
        rationale: Rationale::UndecodableTexture,
        alternatives: Vec::new(),
    }
}
