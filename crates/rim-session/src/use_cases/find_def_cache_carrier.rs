//! [`FindDefCacheCarrier`]: finds def-cache plugins. A
//! packageId in the analyzer's mod list cannot identify one
//! — a def-cache plugin
//! ships inside a performance mod's own `Plugins/` folder, a path the
//! scan never walks — so this reads the fact directly off each active
//! mod's own [`rim_analyzer::domain::Mod::loaded_folders`] through a
//! [`DefCacheCarrierProbe`] port, never a hardcoded packageId.
//!
//! *Which* plugin file to look for is likewise not a fact about RimWorld:
//! the [`DefCacheCarrier`] list is data, loaded with the rest of
//! [`crate::ModKnowledge`] and
//! handed to this use case at construction.

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::ports::{DefCacheCarrier, DefCacheCarrierProbe};

/// Finds the active mod, if any, carrying a known def-cache plugin.
pub struct FindDefCacheCarrier<Probe> {
    probe: Probe,
    carriers: Vec<DefCacheCarrier>,
}

impl<Probe: DefCacheCarrierProbe> FindDefCacheCarrier<Probe> {
    /// Builds the use case from its probe port and the carrier list to
    /// look for (`session.mod_knowledge().def_cache_carriers()`). An
    /// empty list can never find anything — the honest answer for a
    /// session whose knowledge was never loaded.
    #[must_use]
    pub fn new(probe: Probe, carriers: Vec<DefCacheCarrier>) -> Self {
        Self { probe, carriers }
    }

    /// Returns the first active mod (scan order — deterministic) whose
    /// own `loaded_folders` carry one of this use case's carriers.
    /// Returns the first rather than asserting there is exactly one: two
    /// performance-mod forks could in principle both be active at once,
    /// even though that's not a supported configuration.
    #[must_use]
    pub fn execute(&self, session: &Session) -> Option<ModId> {
        if self.carriers.is_empty() {
            return None;
        }
        session
            .report()
            .mods
            .iter()
            .find(|m| {
                self.probe
                    .has_def_cache_plugin(&m.loaded_folders, &self.carriers)
            })
            .map(|m| m.id.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use rim_analyzer::analysis::SourceIndex;
    use rim_resolve::domain::DecisionSet;
    use rim_resolve::test_support::ReportBuilder;

    use super::*;
    use crate::ports::{ModsConfigFile, StoredRules};
    use crate::test_support::{FakeDefCacheCarrierProbe, example_carriers};
    use crate::{ProjectPaths, Session};

    /// A session whose mods' `loaded_folders` are set explicitly —
    /// `crate::test_support::session_fixture`'s plain mods have an empty
    /// `loaded_folders`, which can't exercise a probe over it at all.
    fn session_with_loaded_folders(specs: &[(&str, &[&str])]) -> Session {
        let mut builder = ReportBuilder::new();
        for &(id, loaded_folders) in specs {
            builder = builder.mod_with(id, |m| {
                m.loaded_folders = loaded_folders.iter().map(Into::into).collect();
            });
        }
        let report = builder.build();
        Session::new(
            ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            SourceIndex::default(),
            StoredRules::default(),
            DecisionSet::new(),
            ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: specs.iter().map(|(id, ..)| ModId::new(*id)).collect(),
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn finds_the_active_mod_whose_loaded_folder_the_probe_flags() {
        let session =
            session_with_loaded_folders(&[("a.mod", &["a"]), ("b.mod", &["b", "b-common"])]);
        let use_case = FindDefCacheCarrier::new(
            FakeDefCacheCarrierProbe::with_carrier_folders([PathBuf::from("b-common")]),
            example_carriers(),
        );

        assert_eq!(use_case.execute(&session), Some(ModId::new("b.mod")));
    }

    #[test]
    fn no_active_mod_is_a_carrier_when_the_probe_flags_none_of_them() {
        let session = session_with_loaded_folders(&[("a.mod", &["a"])]);
        let use_case =
            FindDefCacheCarrier::new(FakeDefCacheCarrierProbe::none(), example_carriers());

        assert_eq!(use_case.execute(&session), None);
    }

    /// With no carriers loaded there is nothing to look for, so the probe
    /// is never even consulted — the honest answer, not a guess.
    #[test]
    fn an_empty_carrier_list_never_finds_a_carrier_and_never_probes() {
        let session = session_with_loaded_folders(&[("a.mod", &["a"])]);
        let probe = FakeDefCacheCarrierProbe::with_carrier_folders([PathBuf::from("a")]);
        let use_case = FindDefCacheCarrier::new(probe, Vec::new());

        assert_eq!(use_case.execute(&session), None);
    }
}
