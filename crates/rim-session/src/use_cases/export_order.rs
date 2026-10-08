//! [`ExportOrder`]: builds the list to share from the order in
//! `ModsConfig.xml`, and writes it as a RimWorld mod list (`.rml`).
//!
//! The source is the file's order re-read at export time, never the
//! session's selected order: the file is what RimWorld loads now, and the
//! desktop opens on Suggested, an order the player may never have run.

use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::mod_inventory::ModInventory;
use crate::mod_list::{ExportListError, ExportedModList, SharedModList};
use crate::ports::{ConfigError, ModListFileError, ModListFileStore, ModsConfigStore};

/// What an export reads from the receiver's install: where the file is,
/// what is known about each mod, and what to leave out.
#[derive(Debug, Clone)]
pub struct ExportSource {
    /// The `ModsConfig.xml` to re-read.
    pub mods_config: PathBuf,
    /// Names and Workshop ids. From the scan on the desktop, from
    /// discovery in the CLI.
    pub inventory: ModInventory,
    /// This machine's generated merge mod, which a receiver cannot get.
    pub own_merge_mod: ModId,
    /// The installed game's version text, when known.
    pub game_version: Option<String>,
}

impl ExportSource {
    /// The source for a loaded session: its paths, its report's inventory
    /// and version, and its profile's own merge mod.
    #[must_use]
    pub fn from_session(session: &Session) -> Self {
        Self {
            mods_config: session.paths().mods_config.clone(),
            inventory: ModInventory::from_report(session.report()),
            own_merge_mod: session.own_merge_mod_id(),
            game_version: session.game_version(),
        }
    }
}

/// Why an export produced nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExportOrderError {
    /// `ModsConfig.xml` could not be read.
    #[error("cannot read ModsConfig.xml: {0}")]
    Config(#[from] ConfigError),
    /// The active order holds nothing that can be shared.
    #[error(transparent)]
    List(#[from] ExportListError),
    /// The mod-list file could not be written.
    #[error("cannot write the mod list: {0}")]
    Write(#[from] ModListFileError),
}

/// Builds and writes the shareable list.
pub struct ExportOrder<Config, Store> {
    config: Config,
    store: Store,
}

impl<Config, Store> ExportOrder<Config, Store>
where
    Config: ModsConfigStore,
    Store: ModListFileStore,
{
    /// Wires the ports: the `ModsConfig.xml` store re-read on every call,
    /// and the mod-list file store.
    #[must_use]
    pub fn new(config: Config, store: Store) -> Self {
        Self { config, store }
    }

    /// Re-reads `ModsConfig.xml` and builds the list. Writes nothing: the
    /// "Copy as text" path renders the result with
    /// [`crate::mod_list::render_text`].
    ///
    /// # Errors
    ///
    /// See [`ExportOrderError`]; never [`ExportOrderError::Write`].
    pub fn build(&self, source: &ExportSource) -> Result<ExportedModList, ExportOrderError> {
        let file = self.config.read(&source.mods_config)?;
        Ok(SharedModList::from_active(
            &file.active_mods,
            &source.inventory,
            &source.own_merge_mod,
            source.game_version.as_deref(),
        )?)
    }

    /// Builds the list and writes it to `path` as an `.rml`, replacing a
    /// file there. Returns what was exported, so the caller can report the
    /// count and the ids that could not be put in the list.
    ///
    /// # Errors
    ///
    /// See [`ExportOrderError`]. Nothing is written unless the list built.
    pub fn write_file(
        &self,
        source: &ExportSource,
        path: &Path,
    ) -> Result<ExportedModList, ExportOrderError> {
        let exported = self.build(source)?;
        self.store.write(path, &exported.list)?;
        Ok(exported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mod_list::SharedModList;
    use crate::ports::ModsConfigFile;
    use crate::test_support::{InMemoryModListFileStore, InMemoryModsConfigStore};
    use rim_resolve::test_support::ReportBuilder;

    const OWN_MERGE: &str = "rimmerge.merge.3f9a1c2b7d5e";

    fn ids(raws: &[&str]) -> Vec<ModId> {
        raws.iter().map(ModId::new).collect()
    }

    fn config(active: &[&str]) -> InMemoryModsConfigStore {
        InMemoryModsConfigStore::new(ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: ids(active),
            known_expansions: Vec::new(),
        })
    }

    fn source() -> ExportSource {
        let report = ReportBuilder::new()
            .core("ludeon.rimworld")
            .mod_with("example.framework", |m| {
                m.name = "Example Framework".to_string();
                m.workshop_id = Some(1_234_567_890);
            })
            .mod_("someone.localmod")
            .mod_(OWN_MERGE)
            .build();
        ExportSource {
            mods_config: "ModsConfig.xml".into(),
            inventory: ModInventory::from_report(&report),
            own_merge_mod: ModId::new(OWN_MERGE),
            game_version: Some("1.6.4871 rev590".to_string()),
        }
    }

    fn exported_ids(list: &SharedModList) -> Vec<&str> {
        list.entries().iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn build_exports_the_file_order_without_the_own_merge_mod() {
        let use_case = ExportOrder::new(
            config(&[
                "someone.localmod",
                "ludeon.rimworld",
                "example.framework_steam",
                OWN_MERGE,
            ]),
            InMemoryModListFileStore::new(),
        );

        let exported = use_case.build(&source()).expect("builds");

        assert_eq!(
            exported_ids(&exported.list),
            ["someone.localmod", "ludeon.rimworld", "example.framework"]
        );
        assert!(exported.unrepresentable.is_empty());
    }

    #[test]
    fn build_exports_the_file_as_it_is_now_not_as_scanned() {
        // The inventory knows three active mods; the file now lists one.
        let use_case = ExportOrder::new(
            config(&["ludeon.rimworld"]),
            InMemoryModListFileStore::new(),
        );

        let exported = use_case.build(&source()).expect("builds");

        assert_eq!(exported_ids(&exported.list), ["ludeon.rimworld"]);
    }

    #[test]
    fn build_reports_nothing_active_for_an_empty_file_list() {
        let use_case = ExportOrder::new(config(&[]), InMemoryModListFileStore::new());

        let error = use_case.build(&source()).expect_err("nothing to export");

        assert_eq!(
            error,
            ExportOrderError::List(ExportListError::NothingActive)
        );
    }

    #[test]
    fn build_reports_nothing_active_when_only_the_own_merge_mod_is_active() {
        let use_case = ExportOrder::new(config(&[OWN_MERGE]), InMemoryModListFileStore::new());

        let error = use_case.build(&source()).expect_err("nothing to export");

        assert_eq!(
            error,
            ExportOrderError::List(ExportListError::NothingActive)
        );
    }

    #[test]
    fn build_reports_an_unreadable_mods_config() {
        let failing = FailingConfig;
        let use_case = ExportOrder::new(failing, InMemoryModListFileStore::new());

        let error = use_case.build(&source()).expect_err("unreadable");

        assert_eq!(
            error,
            ExportOrderError::Config(ConfigError("no such file".to_string()))
        );
    }

    #[test]
    fn write_file_hands_the_built_list_to_the_store() {
        let store = InMemoryModListFileStore::new();
        let use_case = ExportOrder::new(config(&["ludeon.rimworld", "someone.localmod"]), store);

        let exported = use_case
            .write_file(&source(), Path::new("out/list.rml"))
            .expect("writes");

        let writes = use_case.store.writes();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0, Path::new("out/list.rml"));
        assert_eq!(writes[0].1, exported.list);
    }

    #[test]
    fn write_file_surfaces_a_write_failure() {
        let store = InMemoryModListFileStore::new();
        store.fail_next_write();
        let use_case = ExportOrder::new(config(&["ludeon.rimworld"]), store);

        let error = use_case
            .write_file(&source(), Path::new("out/list.rml"))
            .expect_err("write fails");

        assert!(matches!(error, ExportOrderError::Write(_)), "{error:?}");
        assert!(use_case.store.writes().is_empty());
    }

    #[test]
    fn write_file_writes_nothing_when_there_is_nothing_to_export() {
        let use_case = ExportOrder::new(config(&[]), InMemoryModListFileStore::new());

        let error = use_case
            .write_file(&source(), Path::new("out/list.rml"))
            .expect_err("nothing to export");

        assert!(matches!(error, ExportOrderError::List(_)), "{error:?}");
        assert!(use_case.store.writes().is_empty());
    }

    #[test]
    fn from_session_uses_the_session_paths_report_version_and_own_merge_mod() {
        let session = crate::test_support::session_fixture(&["a"]);

        let source = ExportSource::from_session(&session);

        assert_eq!(source.mods_config, session.paths().mods_config);
        assert_eq!(
            source.own_merge_mod,
            rim_resolve::domain::GeneratedModIdentity::for_profile(session.paths().profile_hash())
                .package_id
        );
        assert!(source.inventory.contains(&ModId::new("a")));
        assert_eq!(source.game_version.as_deref(), Some("1.6"));
    }

    #[test]
    fn from_session_has_no_game_version_when_the_report_has_none() {
        let mut report = ReportBuilder::new().core("ludeon.rimworld").build();
        report.metadata.game_version = String::new();
        let session = crate::test_support::session_with_sources_and_mods(
            rim_analyzer::analysis::SourceIndex::default(),
            report,
            &["ludeon.rimworld"],
        );

        assert_eq!(ExportSource::from_session(&session).game_version, None);
    }

    /// A `ModsConfig.xml` store whose read always fails.
    struct FailingConfig;

    impl ModsConfigStore for FailingConfig {
        fn read(&self, _path: &Path) -> Result<ModsConfigFile, ConfigError> {
            Err(ConfigError("no such file".to_string()))
        }

        fn write_with_backup(
            &self,
            _path: &Path,
            _file: &ModsConfigFile,
        ) -> Result<PathBuf, ConfigError> {
            Err(ConfigError("read only".to_string()))
        }
    }
}
