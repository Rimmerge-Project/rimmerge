//! `get_mods_config_baseline`/`refresh_mods_config_baseline`: the live
//! baseline the Mods page marks pending changes against.
//!
//! `refresh_mods_config_baseline` has the same lock exception as
//! `game_launch`: a short snapshot under the session lock (the path and the
//! file epoch), the read and parse on a bare [`spawn_blocking`] with the
//! lock released (the frontend polls it every few seconds, and a slow disk
//! must never make another command wait), then a second short lock to swap
//! the result in. It only reads: it never writes the file and never touches
//! the working set or the orders. `session://changed` is emitted only when
//! the list or the status actually changed, so a quiet poll invalidates
//! nothing.

use rim_session::BaselineChange;
use rim_session::ports::ModsConfigStore;
use rim_session::use_cases::RefreshFileBaseline;
use tauri::async_runtime::spawn_blocking;

use crate::commands::emit_session_changed;
use crate::dto::file_baseline::FileBaselineDto;
use crate::dto::project::SessionChangeReasonDto;
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// What the baseline says now: its status and what someone else changed in
/// the file's membership. Read-only.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn get_mods_config_baseline_inner(
    state: &AppState,
) -> Result<FileBaselineDto, CommandError> {
    with_session(state, |session| Ok(FileBaselineDto::from(&*session))).await
}

/// See `get_mods_config_baseline_inner`.
///
/// # Errors
///
/// See `get_mods_config_baseline_inner`.
#[tauri::command]
pub async fn get_mods_config_baseline(
    state: tauri::State<'_, AppState>,
) -> Result<FileBaselineDto, CommandError> {
    get_mods_config_baseline_inner(&state).await
}

/// Re-reads the file through `store` (generic so a test can slow it down)
/// and swaps the result into the session. Returns the baseline afterwards
/// and whether anything changed.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::internal`] when the blocking read panics. A failed
/// read is not an error: it is the baseline's own `Stale` state.
pub(crate) async fn refresh_mods_config_baseline_inner<S>(
    state: &AppState,
    store: S,
) -> Result<(FileBaselineDto, BaselineChange), CommandError>
where
    S: ModsConfigStore + Send + 'static,
{
    let probe = with_session(state, |session| Ok(session.file_probe())).await?;
    let observation = spawn_blocking(move || RefreshFileBaseline::new(store).fetch(probe))
        .await
        .map_err(|join_error| {
            tracing::warn!(error = %join_error, "baseline read task panicked");
            CommandError::internal("background task panicked")
        })?;
    with_session(state, move |session| {
        let change = observation.commit(session);
        Ok((FileBaselineDto::from(&*session), change))
    })
    .await
}

/// See `refresh_mods_config_baseline_inner`; also emits
/// `session://changed` with [`SessionChangeReasonDto::FileBaselineChanged`]
/// when the list or the status changed.
///
/// # Errors
///
/// See `refresh_mods_config_baseline_inner`.
#[tauri::command]
pub async fn refresh_mods_config_baseline(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<FileBaselineDto, CommandError> {
    let store = state.adapters.config_store;
    let (baseline, change) = refresh_mods_config_baseline_inner(&state, store).await?;
    if change == BaselineChange::Changed {
        emit_session_changed(&app, SessionChangeReasonDto::FileBaselineChanged);
    }
    Ok(baseline)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use rim_analyzer::domain::ModId;
    use rim_session::ports::{ConfigError, ConfigErrorKind, ModsConfigFile};
    use rim_session::test_support::{InMemoryModsConfigStore, session_fixture_with_inactive};

    use super::*;
    use crate::dto::file_baseline::{
        BaselineIssueDto, ExternalFileChangeDto, FileBaselineStateDto,
    };
    use crate::error::CommandErrorCode;

    fn file(active: &[&str]) -> ModsConfigFile {
        ModsConfigFile {
            version: "1.6".to_string(),
            active_mods: active.iter().map(|id| ModId::new(*id)).collect(),
            known_expansions: Vec::new(),
        }
    }

    fn state_with_session() -> AppState {
        let state = AppState::default();
        *state.session.write().expect("lock") =
            Some(session_fixture_with_inactive(&["a", "b"], &["x"]));
        state
    }

    #[tokio::test]
    async fn the_baseline_needs_a_loaded_project() {
        let state = AppState::default();

        let status = get_mods_config_baseline_inner(&state).await;
        let refresh =
            refresh_mods_config_baseline_inner(&state, InMemoryModsConfigStore::new(file(&[])))
                .await;

        assert_eq!(status.unwrap_err().code, CommandErrorCode::NoProjectLoaded);
        assert_eq!(refresh.unwrap_err().code, CommandErrorCode::NoProjectLoaded);
    }

    #[tokio::test]
    async fn an_outside_change_is_reported_once_and_a_quiet_poll_is_not_a_change() {
        let state = state_with_session();
        let store = InMemoryModsConfigStore::new(file(&["a", "b", "x"]));

        let (first, first_change) = refresh_mods_config_baseline_inner(&state, store)
            .await
            .expect("refresh");
        let quiet = InMemoryModsConfigStore::new(file(&["a", "b", "x"]));
        let (_, second_change) = refresh_mods_config_baseline_inner(&state, quiet)
            .await
            .expect("refresh");

        assert_eq!(first_change, BaselineChange::Changed);
        assert_eq!(
            first.external_change,
            ExternalFileChangeDto::Membership {
                added: 1,
                removed: 0
            }
        );
        assert_eq!(second_change, BaselineChange::Unchanged);
    }

    #[tokio::test]
    async fn an_outside_change_is_not_pending_work_of_our_own() {
        let state = state_with_session();
        let store = InMemoryModsConfigStore::new(file(&["a", "b", "x"]));

        refresh_mods_config_baseline_inner(&state, store)
            .await
            .expect("refresh");
        let pending = with_session(&state, |session| Ok(session.pending_changes()))
            .await
            .expect("pending changes");

        assert!(pending.unapplied.is_empty());
        assert!(!pending.order_unapplied);
    }

    #[tokio::test]
    async fn an_unreadable_file_is_a_stale_state_not_a_command_error() {
        let state = state_with_session();
        let store = InMemoryModsConfigStore::new(file(&["a", "b"]));
        store.fail_reads(ConfigErrorKind::Malformed);

        let (baseline, change) = refresh_mods_config_baseline_inner(&state, store)
            .await
            .expect("a failed read is data");

        assert_eq!(change, BaselineChange::Changed);
        assert_eq!(
            baseline.state,
            FileBaselineStateDto::Stale {
                reason: BaselineIssueDto::Malformed
            }
        );
    }

    #[tokio::test]
    async fn a_refresh_leaves_the_working_set_alone() {
        let state = state_with_session();
        let before = {
            let guard = state.session.read().expect("lock");
            let session = guard.as_ref().expect("session");
            session.working().ids().to_vec()
        };

        refresh_mods_config_baseline_inner(&state, InMemoryModsConfigStore::new(file(&["x", "a"])))
            .await
            .expect("refresh");

        let guard = state.session.read().expect("lock");
        let session = guard.as_ref().expect("session");
        assert_eq!(session.working().ids(), before.as_slice());
    }

    /// A store whose read announces itself and then waits to be released.
    struct GatedStore {
        started: Sender<()>,
        release: Receiver<()>,
        was_released: Arc<AtomicBool>,
    }

    impl ModsConfigStore for GatedStore {
        fn read(&self, _path: &Path) -> Result<ModsConfigFile, ConfigError> {
            self.started.send(()).expect("test is listening");
            let released = self.release.recv_timeout(Duration::from_secs(10)).is_ok();
            self.was_released.store(released, Ordering::SeqCst);
            Ok(file(&["a", "b"]))
        }

        fn write_with_backup(
            &self,
            _path: &Path,
            _file: &ModsConfigFile,
        ) -> Result<PathBuf, ConfigError> {
            panic!("a baseline refresh never writes");
        }
    }

    /// While the read is in flight another command must be able to take the
    /// session lock. Were the lock held across the read, the second command
    /// would wait for a release that only it can send, and the read would
    /// give up on its own timeout.
    #[tokio::test]
    async fn the_session_lock_is_free_while_the_file_is_read() {
        let state = state_with_session();
        let (started_tx, started_rx) = channel();
        let (release_tx, release_rx) = channel();
        let was_released = Arc::new(AtomicBool::new(false));
        let store = GatedStore {
            started: started_tx,
            release: release_rx,
            was_released: Arc::clone(&was_released),
        };
        let started_rx = Mutex::new(started_rx);

        let refresh = refresh_mods_config_baseline_inner(&state, store);
        let other_command = async {
            let wait = spawn_blocking(move || {
                started_rx
                    .lock()
                    .expect("lock")
                    .recv_timeout(Duration::from_secs(10))
            })
            .await
            .expect("join");
            wait.expect("the read started");
            let result = get_mods_config_baseline_inner(&state).await;
            release_tx.send(()).expect("the read is waiting");
            result
        };
        let (refreshed, other) = tokio::join!(refresh, other_command);

        assert!(other.is_ok());
        assert!(refreshed.is_ok());
        assert!(
            was_released.load(Ordering::SeqCst),
            "the other command could not run while the file was being read"
        );
    }
}
