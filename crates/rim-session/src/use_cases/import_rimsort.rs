//! [`ImportRimSort`]: imports RimSort's three database files, applies the
//! result to the session (replacing any earlier import), persists the
//! merged rules, and — only once that persistence has actually
//! succeeded — records each imported source's provenance in
//! `<profile>/imports/manifest.json` through [`ImportManifestStore`].

use crate::Session;
use crate::ports::{
    ImportError, ImportManifestStore, ImportedRules, RimSortImporter, RimSortPaths, RuleStore,
    StoreError,
};

/// Everything that can go wrong importing from RimSort.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportRimSortError {
    /// A database file couldn't be read or parsed.
    #[error(transparent)]
    Import(#[from] ImportError),
    /// Saving the merged rules failed — the import is rolled back, so
    /// the session and `rules.json` are exactly as they were before this
    /// call.
    #[error("saving imported rules: {0}")]
    Store(#[from] StoreError),
    /// The rules themselves were imported and saved successfully — the
    /// session already reflects them, unrolled-back — but recording
    /// their provenance in `<profile>/imports/manifest.json` failed.
    /// **Deliberately its own variant, not folded into [`Self::Store`]**:
    /// through `apps/desktop`'s own `CommandError`, a shared variant would
    /// surface as a bare "saving imported rules: {0}" — which is simply
    /// false on this path. A user seeing that message at the one moment
    /// they need to know whether to re-import would be told the opposite
    /// of what happened. `#[from]` can only appear once per source type in
    /// a `thiserror` enum, so this variant is always constructed explicitly
    /// (`ImportRimSortError::Manifest`), never via `?` — [`Self::Store`]'s
    /// own `#[from] StoreError` is used as usual at its one call site.
    #[error(
        "the rules were imported and saved successfully; recording the import manifest failed: {0}"
    )]
    Manifest(StoreError),
}

/// Imports RimSort's `userRules.json`/`communityRules.json`/`steamDB.json`,
/// applies the result to the session, saves the merged rules through
/// [`RuleStore`], and records each imported source's provenance through
/// [`ImportManifestStore`].
pub struct ImportRimSort<Importer, Rules, Manifest> {
    importer: Importer,
    rule_store: Rules,
    manifest_store: Manifest,
}

impl<Importer: RimSortImporter, Rules: RuleStore, Manifest: ImportManifestStore>
    ImportRimSort<Importer, Rules, Manifest>
{
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(importer: Importer, rule_store: Rules, manifest_store: Manifest) -> Self {
        Self {
            importer,
            rule_store,
            manifest_store,
        }
    }

    /// Returns the [`ImportedRules`] summary (for reporting per-file
    /// counts) after applying it to `session`.
    ///
    /// **Ordering is load-bearing**: the import manifest is written *only
    /// after* `rule_store.save` has already succeeded, in this method —
    /// never by `RimSortImporter::import` itself. If the adapter wrote the
    /// manifest as part of `import`, before this method even applied the
    /// result to the session, a `rule_store.save` failure afterward would
    /// roll the session's rules back via `restore_rules` but leave the
    /// manifest's already-written claim on disk untouched, so the manifest
    /// could go on to record a sha256 for rules that were never actually
    /// persisted — exactly the state the re-import badge ("cache sha
    /// differs from the sha recorded for the profile's last import") must
    /// never see, since a stale-vs-current comparison is only honest when
    /// "recorded" means "actually saved." Writing here, after a successful
    /// save, makes that state unreachable rather than merely rare.
    ///
    /// A manifest-write failure, unlike a rules-save failure, does
    /// **not** roll the import back: the rules it describes are already
    /// durably saved and the session already reflects them, so undoing
    /// that over a failure to write a secondary, best-effort provenance
    /// record would trade a correct state for a worse one. The manifest
    /// is simply stale until the next successful import, the same
    /// "a snapshot losing itself costs nothing but a badge" reasoning
    /// [`ImportManifestStore::load`] documents for a missing/corrupt file.
    ///
    /// # Errors
    ///
    /// Returns [`ImportRimSortError::Import`] when a database file can't
    /// be read, [`ImportRimSortError::Store`] when saving the rules fails
    /// (in which case the import is rolled back so the session never runs
    /// ahead of disk), or [`ImportRimSortError::Manifest`] when recording
    /// the import manifest fails (rules already saved; not rolled back —
    /// see above, and see that variant's own doc comment for why it's a
    /// distinct variant rather than folded into [`ImportRimSortError::Store`]).
    pub fn execute(
        &self,
        session: &mut Session,
        paths: &RimSortPaths,
    ) -> Result<ImportedRules, ImportRimSortError> {
        let active = session.active_mods_by_base_id();
        let imported = self.importer.import(paths, &active)?;
        let snapshot = session.rules_snapshot();
        session.apply_import(imported.clone());
        if let Err(error) = self
            .rule_store
            .save(&session.paths().profile_dir, session.rules())
        {
            session.restore_rules(snapshot);
            return Err(ImportRimSortError::Store(error));
        }

        if !imported.provenance.is_empty() {
            self.manifest_store
                .save(&session.paths().profile_dir, &imported.provenance)
                .map_err(ImportRimSortError::Manifest)?;
        }

        Ok(imported)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{PairRule, Rule, RuleOrigin};

    use super::*;
    use crate::ports::{IMPORT_SOURCE_USER_RULES, ImportRecord};
    use crate::test_support::{
        FakeRimSortImporter, InMemoryImportManifestStore, InMemoryRuleStore, session_fixture,
    };

    fn rimsort_paths() -> RimSortPaths {
        RimSortPaths {
            user_rules: Some("userRules.json".into()),
            community_rules: Some("communityRules.json".into()),
            steam_db: Some("steamDB.json".into()),
        }
    }

    fn provenance_record() -> ImportRecord {
        ImportRecord {
            file: "userRules.json".to_string(),
            sha256: "abc123".to_string(),
            bytes: 42,
            imported_at: "2026-09-09T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn applies_the_imported_rules_and_persists_them() {
        let mut session = session_fixture(&["a", "b"]);
        let imported = ImportedRules {
            user_rules: Some(vec![Rule::Pair(PairRule {
                after: ModId::new("a"),
                before: ModId::new("b"),
                origin: RuleOrigin::RimSortUser,
                comment: None,
                overrides_declared: false,
            })]),
            community_rules: Some(Vec::new()),
            steam_dependencies: Some(Vec::new()),
            skipped_inactive_rules: 3,
            skipped_inactive_steam: 7,
            provenance: BTreeMap::new(),
        };
        let use_case = ImportRimSort::new(
            FakeRimSortImporter::new(imported),
            InMemoryRuleStore::new(),
            InMemoryImportManifestStore::new(),
        );

        let result = use_case
            .execute(&mut session, &rimsort_paths())
            .expect("import must succeed");

        assert_eq!(result.skipped_inactive_rules, 3);
        assert_eq!(result.skipped_inactive_steam, 7);
        assert_eq!(session.rules().pairs.len(), 1);
        assert_eq!(
            use_case
                .rule_store
                .last_saved()
                .expect("must have saved")
                .pairs
                .len(),
            1
        );
    }

    /// The manifest write must happen strictly after a successful rule
    /// save — asserting the manifest too, not only
    /// the rules: a `rule_store` failure must mean the manifest store is
    /// never even called, since `ImportRimSort::execute` only reaches
    /// that step once `rule_store.save` has already returned `Ok`.
    #[test]
    fn a_failed_save_rolls_back_the_import_and_never_touches_the_manifest() {
        let mut session = session_fixture(&["a", "b"]);
        let imported = ImportedRules {
            user_rules: Some(vec![Rule::Pair(PairRule {
                after: ModId::new("a"),
                before: ModId::new("b"),
                origin: RuleOrigin::RimSortUser,
                comment: None,
                overrides_declared: false,
            })]),
            community_rules: Some(Vec::new()),
            steam_dependencies: Some(Vec::new()),
            skipped_inactive_rules: 0,
            skipped_inactive_steam: 0,
            provenance: BTreeMap::from([(
                IMPORT_SOURCE_USER_RULES.to_string(),
                provenance_record(),
            )]),
        };
        let rule_store = InMemoryRuleStore::new();
        rule_store.fail_next_save();
        let manifest_store = InMemoryImportManifestStore::new();
        let use_case = ImportRimSort::new(
            FakeRimSortImporter::new(imported),
            rule_store,
            manifest_store,
        );

        let result = use_case.execute(&mut session, &rimsort_paths());

        assert!(
            matches!(result, Err(ImportRimSortError::Store(_))),
            "a rule_store failure must surface as Store, not Manifest: {result:?}"
        );
        assert!(
            session.rules().pairs.is_empty(),
            "the imported rules must be rolled back when the save fails"
        );
        assert!(
            use_case.manifest_store.saved().is_empty(),
            "the manifest store must never be called when rule_store.save fails first"
        );
    }

    /// The other half of the same ordering fix: a *successful* import
    /// really does record the provenance `RimSortImporter::import`
    /// returned, into the manifest store, after the rules are saved.
    #[test]
    fn a_successful_import_records_its_provenance_in_the_manifest() {
        let mut session = session_fixture(&["a", "b"]);
        let imported = ImportedRules {
            user_rules: Some(vec![Rule::Pair(PairRule {
                after: ModId::new("a"),
                before: ModId::new("b"),
                origin: RuleOrigin::RimSortUser,
                comment: None,
                overrides_declared: false,
            })]),
            community_rules: None,
            steam_dependencies: None,
            skipped_inactive_rules: 0,
            skipped_inactive_steam: 0,
            provenance: BTreeMap::from([(
                IMPORT_SOURCE_USER_RULES.to_string(),
                provenance_record(),
            )]),
        };
        let use_case = ImportRimSort::new(
            FakeRimSortImporter::new(imported),
            InMemoryRuleStore::new(),
            InMemoryImportManifestStore::new(),
        );

        use_case
            .execute(&mut session, &rimsort_paths())
            .expect("import must succeed");

        let saved = use_case.manifest_store.saved();
        assert_eq!(
            saved.get(IMPORT_SOURCE_USER_RULES),
            Some(&provenance_record())
        );
    }

    /// A manifest-write failure gets its own
    /// [`ImportRimSortError::Manifest`] variant, distinct from
    /// [`ImportRimSortError::Store`] — and, unlike a `Store` failure,
    /// does **not** roll the already-saved rules back. The rules were
    /// genuinely persisted (`rule_store.save` succeeded) and the session
    /// already reflects them; only the secondary provenance record
    /// failed to write.
    #[test]
    fn a_manifest_write_failure_is_its_own_error_variant_and_does_not_roll_back_the_rules() {
        let mut session = session_fixture(&["a", "b"]);
        let imported = ImportedRules {
            user_rules: Some(vec![Rule::Pair(PairRule {
                after: ModId::new("a"),
                before: ModId::new("b"),
                origin: RuleOrigin::RimSortUser,
                comment: None,
                overrides_declared: false,
            })]),
            community_rules: None,
            steam_dependencies: None,
            skipped_inactive_rules: 0,
            skipped_inactive_steam: 0,
            provenance: BTreeMap::from([(
                IMPORT_SOURCE_USER_RULES.to_string(),
                provenance_record(),
            )]),
        };
        let manifest_store = InMemoryImportManifestStore::new();
        manifest_store.fail_next_save();
        let use_case = ImportRimSort::new(
            FakeRimSortImporter::new(imported),
            InMemoryRuleStore::new(),
            manifest_store,
        );

        let result = use_case.execute(&mut session, &rimsort_paths());

        assert!(
            matches!(result, Err(ImportRimSortError::Manifest(_))),
            "a manifest-store failure must surface as Manifest, not Store: {result:?}"
        );
        assert_eq!(
            session.rules().pairs.len(),
            1,
            "the rules must stay applied — rule_store.save already succeeded before the \
             manifest write was even attempted"
        );
        assert_eq!(
            use_case
                .rule_store
                .last_saved()
                .expect("must have saved")
                .pairs
                .len(),
            1,
            "the saved rules on disk must also be unaffected"
        );
    }
}
