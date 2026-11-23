//! [`Session::explain_placement`]: one mod's placement explanation in the
//! suggested order, looked up by the pure [`crate::placement`] module.

use rim_analyzer::domain::ModId;
use rim_resolve::sort::PlacementExplanation;

use super::Session;
use crate::mod_detail::ModNotActive;
use crate::placement::find_placement;

impl Session {
    /// The sorter's placement explanation for `id` in the suggested order
    /// (the only order the sorter explains): the placement for exactly that
    /// id first, else the first sharing its [`ModId::base`], so an active
    /// `_steam` copy is found by its base id and a plain and a `_steam` copy
    /// of one mod each answer for themselves. Every active mod has a
    /// placement, so a miss means `id` is not an active mod.
    ///
    /// # Errors
    ///
    /// Returns [`ModNotActive`] when no placement matches `id`.
    pub fn explain_placement(&self, id: &ModId) -> Result<&PlacementExplanation, ModNotActive> {
        find_placement(&self.sort.placements, id).ok_or_else(|| ModNotActive(id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;

    use crate::ModNotActive;
    use crate::test_support::session_fixture;

    #[test]
    fn explain_placement_finds_a_steam_copy_by_its_base_id() {
        let session = session_fixture(&["x_steam", "y"]);

        let explanation = session
            .explain_placement(&ModId::new("x"))
            .expect("x_steam must be found by its base id");

        assert_eq!(explanation.mod_id, ModId::new("x_steam"));
    }

    #[test]
    fn explain_placement_prefers_the_exact_copy_when_both_are_active() {
        let session = session_fixture(&["x", "x_steam"]);

        let plain = session.explain_placement(&ModId::new("x")).expect("x");
        let steam = session
            .explain_placement(&ModId::new("x_steam"))
            .expect("x_steam");

        assert_eq!(plain.mod_id, ModId::new("x"));
        assert_eq!(steam.mod_id, ModId::new("x_steam"));
    }

    #[test]
    fn explain_placement_refuses_a_mod_with_no_placement() {
        let session = session_fixture(&["a"]);

        let result = session.explain_placement(&ModId::new("nobody"));

        assert_eq!(result.unwrap_err(), ModNotActive(ModId::new("nobody")));
    }
}
