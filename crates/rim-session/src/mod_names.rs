//! [`mod_names`]: every installed mod's display name keyed by id, built by
//! [`crate::Session::mod_names`]. Pure over the report.

use std::collections::BTreeMap;

use rim_analyzer::domain::{ModId, Report};

/// Every active or inactive mod's id mapped to its display name.
///
/// An active id always keeps its own name. A `_steam`-suffixed active id
/// also gets its [`ModId::base`] entered as a second key, but never over an
/// id that is itself active; an inactive mod fills only ids no active entry
/// claimed. So a caller holding either spelling resolves a name, and an
/// active mod's name wins over an inactive one's.
pub(crate) fn mod_names(report: &Report) -> BTreeMap<ModId, String> {
    let mut names: BTreeMap<ModId, String> = report
        .mods
        .iter()
        .map(|entry| (entry.id.clone(), entry.name.clone()))
        .collect();
    for entry in &report.mods {
        names
            .entry(entry.id.base())
            .or_insert_with(|| entry.name.clone());
    }
    for entry in &report.inactive_mods {
        names
            .entry(entry.id.clone())
            .or_insert_with(|| entry.name.clone());
    }
    names
}

#[cfg(test)]
mod tests {
    use rim_resolve::test_support::ReportBuilder;

    use super::*;

    fn name_of(names: &BTreeMap<ModId, String>, id: &str) -> Option<String> {
        names.get(&ModId::new(id)).cloned()
    }

    #[test]
    fn an_inactive_mod_is_named_too() {
        let report = ReportBuilder::new().mod_("a").inactive("b").build();

        let names = mod_names(&report);

        assert!(name_of(&names, "a").is_some());
        assert!(name_of(&names, "b").is_some());
    }

    #[test]
    fn a_steam_suffixed_active_mod_is_also_named_by_its_base_id() {
        let report = ReportBuilder::new().mod_("x_steam").build();

        let names = mod_names(&report);

        assert_eq!(name_of(&names, "x"), name_of(&names, "x_steam"));
        assert!(name_of(&names, "x").is_some());
    }

    #[test]
    fn an_active_base_id_keeps_its_own_name_over_a_steam_copy() {
        let report = ReportBuilder::new()
            .mod_with("x_steam", |entry| entry.name = "Steam copy".to_string())
            .mod_with("x", |entry| entry.name = "Plain copy".to_string())
            .build();

        let names = mod_names(&report);

        assert_eq!(name_of(&names, "x").as_deref(), Some("Plain copy"));
        assert_eq!(name_of(&names, "x_steam").as_deref(), Some("Steam copy"));
    }

    #[test]
    fn an_inactive_mod_never_overrides_an_active_base_entry() {
        let mut report = ReportBuilder::new()
            .mod_with("x_steam", |entry| entry.name = "Active".to_string())
            .inactive("x")
            .build();
        report.inactive_mods[0].name = "Inactive".to_string();

        let names = mod_names(&report);

        assert_eq!(name_of(&names, "x").as_deref(), Some("Active"));
    }
}
