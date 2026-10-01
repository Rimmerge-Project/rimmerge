//! `rim-io`: infrastructure adapters implementing `rim-session`'s ports —
//! `ModsConfig.xml` I/O, decisions/rules JSON files, the RimSort importer,
//! the analyzer-backed scanner, [`databases`] (fetching RimSort's two
//! published rule databases and this project's own rules file straight
//! from GitHub over HTTPS into an app-global cache
//! ([`databases_dir`]), so community rules don't require a local RimSort
//! install), and [`release_feed`] (the version-check call to GitHub's
//! Releases API). Both network adapters share one transport seam, `net`
//! (crate-internal) — the closed host allowlist and the one place every
//! request's headers, timeouts, and byte cap are set. Composition roots
//! (`apps/cli`,
//! `apps/desktop`) construct these and inject them into `rim-session`'s
//! use cases.

pub mod app_config;
pub mod app_settings;
pub mod asset_locator;
pub mod assignment_store;
pub(crate) mod atomic;
pub mod databases;
pub mod decisions;
pub mod def_cache;
pub mod def_source;
pub mod game_log;
pub mod merge_mod;
pub mod mod_about;
pub mod mod_knowledge;
pub mod mods_config;
pub(crate) mod net;
pub mod notifications;
pub mod patches;
pub mod process;
pub mod profile;
pub mod project_paths;
pub mod release_feed;
pub mod rimsort;
pub mod rules;
pub mod scanner;

pub use app_config::{AppConfig, app_config_path, pinned_path};
pub use app_settings::{JsonAppSettingsStore, app_settings_path};
pub use asset_locator::FileAssetLocator;
pub use assignment_store::JsonAssignmentProjectStore;
pub use databases::GithubRuleDatabaseFetcher;
pub use decisions::JsonDecisionStore;
pub use def_cache::FsDefCacheCarrierProbe;
pub use def_source::FileDefSourceReader;
pub use game_log::FileGameLogReader;
pub use merge_mod::MergeModFolderWriter;
pub use mod_about::FileModAboutReader;
pub use mod_knowledge::{FsModKnowledgeStore, embedded_bundle_sha256, vendored_knowledge};
pub use mods_config::ModsConfigFileStore;
pub use notifications::{
    JsonNotificationStateStore, JsonProfileNotificationStateStore, notifications_path,
};
pub use patches::JsonPatchProjectStore;
pub use process::{GameProcessProbe, SysinfoGameProcessProbe};
pub use profile::{databases_dir, profile_dir, resolve_user_dir};
pub use project_paths::{
    GAME_DIR_VAR, MODS_CONFIG_VAR, PathOverrides, PathResolutionError, ResolvedPaths,
    WORKSHOP_DIR_VAR, not_an_install_warning, resolve_project_paths,
};
pub use release_feed::GithubReleaseFeed;
pub use rimsort::RimSortImporter;
pub use rimsort::import_manifest::JsonImportManifestStore;
pub use rules::JsonRuleStore;
pub use scanner::{AnalyzerScanner, discover_inventory};
