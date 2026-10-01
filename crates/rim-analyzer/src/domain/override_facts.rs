//! Winner-relative facts about def overrides and patch collisions.
//!
//! The report records only what is true in every load order (the owners,
//! each mod's declared relations, which mods remove a patched node). Which
//! owner wins, and whether that winner ordered itself on purpose, depend on
//! the order being judged, so each consumer asks here with its own order:
//! the ledger with the selected order, the analyzer's text summary with the
//! order the scan ran in.

use std::collections::BTreeMap;

use super::conflict::{DefOverride, PatchCollision};
use super::load_order::LoadOrder;
use super::mod_entry::Mod;
use super::mod_id::ModId;

/// The active mods by id, as the winner-relative facts below read them.
pub type ModsById<'a> = BTreeMap<ModId, &'a Mod>;

/// The owner that loads last in `order`: the one whose file the game
/// uses. An owner `order` does not list never wins over one it does; if
/// none is listed, the last owner as given stands in. `None` only for an
/// empty owner list.
#[must_use]
pub fn last_loaded(owners: &[ModId], order: &LoadOrder) -> Option<ModId> {
    owners
        .iter()
        .filter_map(|owner| order.position(owner).map(|position| (position, owner)))
        .max_by_key(|(position, _)| *position)
        .map(|(_, owner)| owner)
        .or_else(|| owners.last())
        .cloned()
}

fn declares_after_or_dependency(mods: &ModsById<'_>, declarer: &ModId, target: &ModId) -> bool {
    mods.get(declarer)
        .is_some_and(|declaring| declaring.declared.declares_after_or_dependency(target))
}

impl DefOverride {
    /// The winner declares a `loadAfter`, `forceLoadAfter`, or
    /// `modDependencies` relation naming every other owner (base-id aware)
    /// — this override was very likely intentional.
    #[must_use]
    pub fn winner_declares_relation(&self, winner: &ModId, mods: &ModsById<'_>) -> bool {
        mods.contains_key(winner)
            && self
                .owners
                .iter()
                .filter(|owner| *owner != winner)
                .all(|other| declares_after_or_dependency(mods, winner, other))
    }

    /// An earlier (non-winning) owner is a framework candidate (see
    /// [`Mod::is_framework_candidate`]) and the winner is not — a leaf mod
    /// is quietly shadowing a shared library's def.
    #[must_use]
    pub fn shadows_framework(&self, winner: &ModId, mods: &ModsById<'_>) -> bool {
        let is_framework = |id: &ModId| mods.get(id).is_some_and(|m| m.is_framework_candidate);
        !is_framework(winner)
            && self
                .owners
                .iter()
                .any(|owner| owner != winner && is_framework(owner))
    }
}

impl PatchCollision {
    /// `winner` (the contributor whose operation runs last in the order
    /// being judged) declares a `loadAfter`, `forceLoadAfter`, or
    /// `modDependencies` relation naming every other contributing mod
    /// (base-id aware), none of them declares one back toward it,
    /// `sub_path` names a field, and no mod other than the winner removes
    /// that field or an ancestor of it ([`Self::removed_by`]) — the author
    /// ordered this change after the others on purpose, and the winner's
    /// own operation really applies.
    #[must_use]
    pub fn winner_declares_relation(&self, winner: &ModId, mods: &ModsById<'_>) -> bool {
        if self.sub_path.is_none() {
            return false;
        }
        let mut others = self
            .mods
            .iter()
            .map(|entry| &entry.mod_id)
            .filter(|id| *id != winner)
            .peekable();
        if others.peek().is_none() {
            return false;
        }
        let ordered_after_all = others.all(|other| {
            declares_after_or_dependency(mods, winner, other)
                && !declares_after_or_dependency(mods, other, winner)
        });
        ordered_after_all && self.removed_by.iter().all(|remover| remover == winner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{PatchCollisionEntry, PatchCollisionSeverity, Selector};

    fn mod_json(id: &str, load_after: &[&str], is_framework: bool) -> Mod {
        let load_after = load_after
            .iter()
            .map(|other| format!("\"{other}\""))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            r#"{{
                "id": "{id}", "name": "{id}", "authors": [], "url": null,
                "path": "{id}", "source": "local", "supported_versions": [],
                "declared": {{
                    "load_after": [{load_after}], "load_before": [],
                    "force_load_after": [], "force_load_before": [],
                    "dependencies": [], "incompatible_with": []
                }},
                "loaded_folders": [], "hard_dependents": 0, "soft_dependents": 0,
                "awareness_dependents": 0, "is_framework_candidate": {is_framework}
            }}"#
        );
        serde_json::from_str(&json).expect("a valid mod literal")
    }

    fn by_id(mods: &[Mod]) -> ModsById<'_> {
        mods.iter().map(|m| (m.id.clone(), m)).collect()
    }

    fn id(name: &str) -> ModId {
        ModId::new(name)
    }

    fn def_override(owners: &[&str]) -> DefOverride {
        DefOverride {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            owners: owners.iter().map(|owner| id(owner)).collect(),
            overrides_vanilla: false,
            same_author: false,
        }
    }

    fn collision(mods: &[&str], sub_path: Option<&str>, removed_by: &[&str]) -> PatchCollision {
        PatchCollision {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            selector: Selector::DefName,
            sub_path: sub_path.map(str::to_string),
            mods: mods
                .iter()
                .map(|mod_id| PatchCollisionEntry {
                    mod_id: id(mod_id),
                    op_class: "PatchOperationReplace".to_string(),
                })
                .collect(),
            severity: PatchCollisionSeverity::Contested,
            removed_by: removed_by.iter().map(|remover| id(remover)).collect(),
        }
    }

    #[test]
    fn last_loaded_follows_the_given_order_not_the_owner_list() {
        let owners = [id("w"), id("a")];
        let current = LoadOrder::new(vec![id("w"), id("a")]);
        let suggested = LoadOrder::new(vec![id("a"), id("w")]);

        assert_eq!(last_loaded(&owners, &current), Some(id("a")));
        assert_eq!(last_loaded(&owners, &suggested), Some(id("w")));
    }

    #[test]
    fn last_loaded_prefers_an_owner_the_order_lists_and_is_none_for_no_owners() {
        let order = LoadOrder::new(vec![id("a")]);

        assert_eq!(last_loaded(&[id("a"), id("z")], &order), Some(id("a")));
        assert_eq!(last_loaded(&[], &order), None);
    }

    #[test]
    fn a_def_override_winner_declares_its_relation_only_when_it_is_the_declarer() {
        let mods = [mod_json("w", &["a"], false), mod_json("a", &[], false)];
        let conflict = def_override(&["w", "a"]);

        assert!(conflict.winner_declares_relation(&id("w"), &by_id(&mods)));
        assert!(!conflict.winner_declares_relation(&id("a"), &by_id(&mods)));
    }

    #[test]
    fn shadows_framework_needs_a_framework_owner_that_is_not_the_winner() {
        let mods = [mod_json("leaf", &[], false), mod_json("lib", &[], true)];
        let conflict = def_override(&["lib", "leaf"]);

        assert!(conflict.shadows_framework(&id("leaf"), &by_id(&mods)));
        assert!(!conflict.shadows_framework(&id("lib"), &by_id(&mods)));
    }

    #[test]
    fn a_patch_collision_winner_must_declare_every_other_contributor_and_no_one_back() {
        let mods = [mod_json("w", &["a"], false), mod_json("a", &[], false)];
        let mods = by_id(&mods);
        let field = collision(&["a", "w"], Some("comps"), &[]);

        assert!(field.winner_declares_relation(&id("w"), &mods));
        assert!(!field.winner_declares_relation(&id("a"), &mods));
    }

    #[test]
    fn a_patch_collision_on_the_def_root_has_no_field_to_order() {
        let mods = [mod_json("w", &["a"], false), mod_json("a", &[], false)];

        let root = collision(&["a", "w"], None, &[]);

        assert!(!root.winner_declares_relation(&id("w"), &by_id(&mods)));
    }

    #[test]
    fn a_remover_other_than_the_winner_blocks_the_declared_relation() {
        let mods = [
            mod_json("w", &["a", "r"], false),
            mod_json("a", &[], false),
            mod_json("r", &[], false),
        ];
        let mods = by_id(&mods);

        let by_other = collision(&["a", "w"], Some("comps"), &["r"]);
        let by_winner = collision(&["a", "w"], Some("comps"), &["w"]);

        assert!(!by_other.winner_declares_relation(&id("w"), &mods));
        assert!(by_winner.winner_declares_relation(&id("w"), &mods));
    }
}
