//! Shared test-only [`rim_session::Session`] builders for `src-tauri`'s
//! command tests.
//!
//! `AppState::default()`'s [`crate::state::Adapters`] are always the real
//! file-based `rim-io` ones (see that type's own doc comment), so any
//! test whose command actually reaches a store's `save` — not just one
//! that errors out before ever calling it — performs a real write.
//! [`rim_session::test_support::session_fixture`]'s relative
//! `"ModsConfig.xml"`/`"profile"` paths resolve against the process's
//! current directory, which for `cargo test` is this crate's own
//! `src-tauri/`, so such a test silently drops `rules.json`/
//! `decisions.json` into the source tree instead of a throwaway
//! location. The builders here put every real file under a fresh
//! `tempfile::tempdir()` instead; the returned [`tempfile::TempDir`]
//! deletes its directory on drop, so callers must keep it alive for the
//! whole test.

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{ModId, Report};
use rim_resolve::domain::DecisionSet;
use rim_session::LaunchUnavailable;
use rim_session::mod_info::ExternalUrl;
use rim_session::ports::{
    GameExecutable, GameLauncher, LaunchFailure, LaunchRoute, ModsConfigFile, StoredRules,
};
use rim_session::{ProjectPaths, Session};

use crate::state::{LinkOpenError, LinkOpener};

/// A [`LinkOpener`] that records every URL it was asked to open instead
/// of launching a real browser — the seam every `open_mod_link`/
/// `open_app_link` command test swaps [`crate::state::AppState::link_opener`]
/// for, so a default-gate test never opens anything real.
#[derive(Debug, Default)]
pub(crate) struct RecordingLinkOpener {
    calls: std::sync::Mutex<Vec<String>>,
}

impl RecordingLinkOpener {
    /// Every URL this fake was asked to open, in call order.
    pub(crate) fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl LinkOpener for RecordingLinkOpener {
    fn open(&self, url: &ExternalUrl) -> Result<(), LinkOpenError> {
        self.calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(url.as_str().to_string());
        Ok(())
    }
}

/// Builds a [`Session`] over [`rim_session::test_support::report_fixture`]'s
/// mods, an empty [`SourceIndex`], and a `ModsConfig.xml` active list
/// matching `ids` exactly — the temp-directory equivalent of
/// [`rim_session::test_support::session_fixture`]. Use
/// [`session_with_temp_paths`] instead when a test needs real conflict
/// data (a non-default `Report`/`SourceIndex`), e.g. the merge command
/// tests.
pub(crate) fn session_fixture_with_temp_paths(ids: &[&str]) -> (tempfile::TempDir, Session) {
    session_with_temp_paths(
        rim_session::test_support::report_fixture(ids),
        SourceIndex::default(),
        ids,
    )
}

/// Builds a [`Session`] whose `ModsConfig.xml` and `profile/` directory
/// are real files under a fresh `tempfile::tempdir()` — never the real
/// game install or the real RimSort/profile directories. `active_mod_ids`
/// seeds both the on-disk `ModsConfig.xml`'s `<activeMods>` and the
/// returned session's `ModsConfigFile.active_mods`.
pub(crate) fn session_with_temp_paths(
    report: Report,
    sources: SourceIndex,
    active_mod_ids: &[&str],
) -> (tempfile::TempDir, Session) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let mods_config_path = temp_dir.path().join("ModsConfig.xml");
    let active_mods_xml: String = active_mod_ids
        .iter()
        .map(|id| format!("<li>{id}</li>"))
        .collect();
    std::fs::write(
        &mods_config_path,
        format!(
            "<ModsConfigData><version>1.6.0</version><activeMods>{active_mods_xml}</activeMods></ModsConfigData>"
        ),
    )
    .expect("seed ModsConfig.xml");
    let profile_dir = temp_dir.path().join("profile");
    std::fs::create_dir_all(&profile_dir).expect("create profile dir");

    let session = Session::new(
        ProjectPaths {
            game_dir: temp_dir.path().join("game"),
            workshop_dir: temp_dir.path().join("workshop"),
            mods_config: mods_config_path,
            profile_dir,
        },
        report,
        Vec::new(),
        sources,
        StoredRules::default(),
        DecisionSet::new(),
        ModsConfigFile {
            version: "1.6.0".to_string(),
            active_mods: active_mod_ids.iter().map(|id| ModId::new(*id)).collect(),
            known_expansions: Vec::new(),
        },
        Vec::new(),
        Vec::new(),
    );
    (temp_dir, session)
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Request paths over a fresh copy of the analyzer's checked-in sample
/// game tree under a new temp directory: every path (game, mods config,
/// profile) lives inside it, so a real `load_project`/`rescan_project`
/// scan reads and writes nothing outside it. Keep the returned
/// [`tempfile::TempDir`] alive for the whole test.
pub(crate) fn scratch_sample_game() -> (tempfile::TempDir, crate::dto::project::ProjectPathsDto) {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let game_dir = temp_dir.path().join("game");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("crates")
        .join("rim-analyzer")
        .join("tests")
        .join("fixtures")
        .join("sample_game");
    copy_dir_recursive(&fixture, &game_dir).expect("copy the sample game fixture");
    // The shared fixture ships no `Version.txt`; the real scanner needs one.
    std::fs::write(game_dir.join("Version.txt"), "1.6.4871 rev590").expect("seed Version.txt");
    let paths = crate::dto::project::ProjectPathsDto {
        mods_config: game_dir.join("ModsConfig.xml").display().to_string(),
        workshop_dir: temp_dir
            .path()
            .join("workshop_does_not_exist")
            .display()
            .to_string(),
        profile_dir: temp_dir.path().join("profile").display().to_string(),
        game_dir: game_dir.display().to_string(),
    };
    (temp_dir, paths)
}

/// Scans `paths` the way the load and rescan commands do (a [`rim_session::use_cases::LoadProject`]
/// over `adapters`), but builds nothing around the result and swaps nothing
/// in: the session a test then holds in its hand is exactly the one a scan
/// produces *before* the swap, so an edit the test saves afterwards is the
/// "edit that landed while the scan ran". `active` is the active list to
/// scan, or `None` for the file's own.
pub(crate) fn scanned_session(
    adapters: &crate::state::Adapters,
    paths: &ProjectPaths,
    active: Option<Vec<ModId>>,
) -> Session {
    let adapters = adapters.clone();
    let use_case = rim_session::use_cases::LoadProject::new(
        adapters.scanner,
        adapters.config_store,
        adapters.decision_store,
        adapters.rule_store,
        adapters.patch_store,
        adapters.assignment_store,
        adapters.mod_knowledge_store.clone(),
        false,
    )
    .with_organization_store(adapters.organization_store.clone());
    match active {
        Some(active) => use_case.execute_with_active_set(paths.clone(), active, &mut |_| {}),
        None => use_case.execute(paths.clone(), &mut |_| {}),
    }
    .expect("scanning the scratch sample game must succeed")
}

/// What a late edit saves: a Top placement for `mod_id` in `rules.json`
/// and an Ignore decision on that mod's unsupported-version finding in
/// `decisions.json`, both written through the real stores into
/// `profile_dir`, exactly as `upsert_rule`/`decide` would have saved them.
/// Returns the decision's key.
pub(crate) fn save_late_rule_and_decision(
    profile_dir: &std::path::Path,
    mod_id: &ModId,
) -> rim_resolve::domain::FindingKey {
    use rim_resolve::domain::{Action, Decision, FindingKey, Placement, PlacementRule, RuleOrigin};
    use rim_session::ports::{DecisionStore, RuleStore};

    let rules = StoredRules {
        placements: vec![PlacementRule {
            mod_id: mod_id.clone(),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }],
        ..StoredRules::default()
    };
    rim_io::JsonRuleStore
        .save(profile_dir, &rules)
        .expect("save the late rule");
    let key = FindingKey::UnsupportedVersion {
        mod_id: mod_id.clone(),
    };
    let mut decisions = DecisionSet::new();
    decisions
        .insert(Decision {
            key: key.clone(),
            action: Action::Ignore,
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("a valid decision");
    rim_io::JsonDecisionStore
        .save(profile_dir, &decisions)
        .expect("save the late decision");
    key
}

/// A [`GameLauncher`] that never starts anything: `route` answers a
/// scripted result and `launch` records the route it was asked to start,
/// failing instead when a failure was scripted. The seam every
/// `get_game_launch_status`/`launch_game` test swaps
/// [`crate::state::AppState::game_launcher`] for, so a default-gate test
/// never opens `steam://` or spawns `RimWorldWin64.exe`.
#[derive(Debug)]
pub(crate) struct RecordingGameLauncher {
    route: Result<LaunchRoute, LaunchUnavailable>,
    failure: Option<LaunchFailure>,
    launched: std::sync::Mutex<Vec<LaunchRoute>>,
    queried: std::sync::Mutex<Vec<std::path::PathBuf>>,
}

impl RecordingGameLauncher {
    fn with_route(route: Result<LaunchRoute, LaunchUnavailable>) -> Self {
        Self {
            route,
            failure: None,
            launched: std::sync::Mutex::new(Vec::new()),
            queried: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Reports the Steam route.
    pub(crate) fn steam() -> Self {
        Self::with_route(Ok(LaunchRoute::Steam))
    }

    /// Reports the executable route for an invented install folder.
    pub(crate) fn executable() -> Self {
        let install = std::path::Path::new("recording-launcher-install");
        Self::with_route(Ok(LaunchRoute::Executable(GameExecutable::of_install(
            install,
        ))))
    }

    /// Reports that the install can't be started from here.
    pub(crate) fn unavailable() -> Self {
        Self::with_route(Err(LaunchUnavailable::ExecutableMissing))
    }

    /// Makes every `launch` call fail with `failure`.
    pub(crate) fn failing_with(mut self, failure: LaunchFailure) -> Self {
        self.failure = Some(failure);
        self
    }

    /// The install folder of every `route` query, in order.
    pub(crate) fn route_queries(&self) -> Vec<std::path::PathBuf> {
        self.queried
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The route of every successful `launch` call, in order.
    pub(crate) fn launched(&self) -> Vec<LaunchRoute> {
        self.launched
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl GameLauncher for RecordingGameLauncher {
    fn route(&self, game_dir: &std::path::Path) -> Result<LaunchRoute, LaunchUnavailable> {
        self.queried
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(game_dir.to_path_buf());
        self.route.clone()
    }

    fn launch(&self, route: &LaunchRoute) -> Result<(), LaunchFailure> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        self.launched
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(route.clone());
        Ok(())
    }
}

/// A slow read, made deterministic: the reader calls [`Gate::block`] from
/// inside the blocking task and stays there until the test calls
/// [`Gate::release`]. A test starts the command under test, waits with
/// [`Gate::wait_until_blocked`] until the read is in flight, runs another
/// session command meanwhile and releases the read afterwards. If the
/// command held the session lock across the read, that other command could
/// never run and the read would give up on its own timeout, which
/// [`Gate::was_released`] reports.
pub(crate) struct Gate {
    started_tx: std::sync::mpsc::Sender<()>,
    started_rx: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    release_tx: std::sync::mpsc::Sender<()>,
    release_rx: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    was_released: std::sync::atomic::AtomicBool,
}

impl Gate {
    /// How long either side waits for the other before giving up.
    const PATIENCE: std::time::Duration = std::time::Duration::from_secs(10);

    pub(crate) fn new() -> std::sync::Arc<Self> {
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        std::sync::Arc::new(Self {
            started_tx,
            started_rx: std::sync::Mutex::new(started_rx),
            release_tx,
            release_rx: std::sync::Mutex::new(release_rx),
            was_released: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Reader side: announces the read and waits for [`Self::release`].
    pub(crate) fn block(&self) {
        self.started_tx.send(()).expect("the test is listening");
        let released = self
            .release_rx
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv_timeout(Self::PATIENCE)
            .is_ok();
        self.was_released
            .store(released, std::sync::atomic::Ordering::SeqCst);
    }

    /// Test side: returns once a reader is inside [`Self::block`].
    pub(crate) async fn wait_until_blocked(self: &std::sync::Arc<Self>) {
        let gate = std::sync::Arc::clone(self);
        tauri::async_runtime::spawn_blocking(move || {
            gate.started_rx
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .recv_timeout(Self::PATIENCE)
        })
        .await
        .expect("join")
        .expect("the read started");
    }

    /// Test side: lets the blocked reader go on.
    pub(crate) fn release(&self) {
        self.release_tx.send(()).expect("the read is waiting");
    }

    /// Whether the reader was released by the test and not by its timeout.
    pub(crate) fn was_released(&self) -> bool {
        self.was_released.load(std::sync::atomic::Ordering::SeqCst)
    }
}
