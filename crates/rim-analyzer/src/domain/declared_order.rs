//! [`DeclaredOrder`]: the load-order hints a mod declares about itself in
//! `About.xml`, already merged with the `ByVersion` variants for the
//! current game version.

use serde::{Deserialize, Serialize};

use super::mod_id::ModId;

/// A `modDependencies` entry: a required mod plus the human-readable name
/// the author gave it (for reporting when the dependency is missing).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModDependency {
    pub id: ModId,
    pub display_name: Option<String>,
}

/// Load-order and compatibility hints declared by a mod's `About.xml`,
/// with the base list and its `ByVersion` counterpart for the active game
/// version already merged (base entries first, then version-specific
/// additions, duplicates by [`ModId`] removed).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeclaredOrder {
    pub load_after: Vec<ModId>,
    pub load_before: Vec<ModId>,
    pub force_load_after: Vec<ModId>,
    pub force_load_before: Vec<ModId>,
    pub dependencies: Vec<ModDependency>,
    pub incompatible_with: Vec<ModId>,
}

impl DeclaredOrder {
    /// Whether this declares a `loadAfter`, `forceLoadAfter`, or
    /// `modDependencies` relation naming `target`, comparing base ids
    /// (see [`ModId::base`]) since `About.xml` never carries the
    /// `_steam` suffix an active id can.
    #[must_use]
    pub fn declares_after_or_dependency(&self, target: &ModId) -> bool {
        let base = target.base();
        self.load_after.iter().any(|id| id.base() == base)
            || self.force_load_after.iter().any(|id| id.base() == base)
            || self.dependencies.iter().any(|d| d.id.base() == base)
    }

    /// Whether this declares any load-order relation at all
    /// (`loadAfter`/`loadBefore`/`forceLoadAfter`/`forceLoadBefore`/
    /// `modDependencies`) naming `target`, base-id aware.
    #[must_use]
    pub fn declares_any_relation(&self, target: &ModId) -> bool {
        let base = target.base();
        self.load_after.iter().any(|id| id.base() == base)
            || self.load_before.iter().any(|id| id.base() == base)
            || self.force_load_after.iter().any(|id| id.base() == base)
            || self.force_load_before.iter().any(|id| id.base() == base)
            || self.dependencies.iter().any(|d| d.id.base() == base)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn declared_with_load_after(id: &str) -> DeclaredOrder {
        DeclaredOrder {
            load_after: vec![ModId::new(id)],
            ..DeclaredOrder::default()
        }
    }

    #[test]
    fn declares_after_or_dependency_matches_load_after_base_id() {
        let declared = declared_with_load_after("foo.bar");
        assert!(declared.declares_after_or_dependency(&ModId::new("foo.bar_steam")));
    }

    #[test]
    fn declares_after_or_dependency_false_when_only_load_before_is_declared() {
        let declared = DeclaredOrder {
            load_before: vec![ModId::new("foo.bar")],
            ..DeclaredOrder::default()
        };
        assert!(!declared.declares_after_or_dependency(&ModId::new("foo.bar")));
    }

    #[test]
    fn declares_any_relation_matches_load_before() {
        let declared = DeclaredOrder {
            load_before: vec![ModId::new("foo.bar")],
            ..DeclaredOrder::default()
        };
        assert!(declared.declares_any_relation(&ModId::new("foo.bar")));
    }

    #[test]
    fn declares_any_relation_false_when_nothing_matches() {
        let declared = declared_with_load_after("other.mod");
        assert!(!declared.declares_any_relation(&ModId::new("foo.bar")));
    }
}
