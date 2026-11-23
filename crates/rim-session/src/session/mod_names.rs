//! [`Session::mod_names`]: id-to-name lookup over every installed mod, built
//! by the pure [`crate::mod_names`] module.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;

use super::Session;
use crate::mod_names::mod_names;

impl Session {
    /// Every active or inactive mod's display name by id, so a render site
    /// shows names without a per-render lookup. A `_steam`-suffixed active
    /// id also answers to its base id (the rules are in `crate::mod_names`).
    #[must_use]
    pub fn mod_names(&self) -> BTreeMap<ModId, String> {
        mod_names(&self.report)
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;

    use crate::test_support::session_fixture_with_inactive;

    #[test]
    fn names_cover_active_and_inactive_mods() {
        let session = session_fixture_with_inactive(&["a_steam"], &["b"]);

        let names = session.mod_names();

        let keys: Vec<&str> = names.keys().map(ModId::as_str).collect();
        assert_eq!(keys, ["a", "a_steam", "b"]);
    }
}
