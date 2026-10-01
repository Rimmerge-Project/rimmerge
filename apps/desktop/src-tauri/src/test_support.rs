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
use rim_session::mod_info::ExternalUrl;
use rim_session::ports::{ModsConfigFile, StoredRules};
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
