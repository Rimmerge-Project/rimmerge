//! [`AppState`]: the Tauri-managed state every command works through — a
//! shared, lockable slot for the loaded [`rim_session::Session`], the
//! `rim-io` adapters built once at startup, and the [`GameProcessProbe`]
//! `apply` refuses a write against.

use std::sync::{Arc, RwLock};

use rim_io::{
    AnalyzerScanner, FileAssetLocator, FileDefSourceReader, FileGameLogReader, FileModAboutReader,
    FsDefCacheCarrierProbe, FsModKnowledgeStore, GithubReleaseFeed, JsonAssignmentProjectStore,
    JsonDecisionStore, JsonNotificationStateStore, JsonPatchProjectStore, JsonRuleStore,
    MergeModFolderWriter, ModsConfigFileStore,
};
pub use rim_io::{GameProcessProbe, SysinfoGameProcessProbe};
use rim_session::Session;
use rim_session::mod_info::ExternalUrl;
use rim_session::ports::{
    AssetLocator, DefSourceReader, GameLauncher, ModAboutReader, ReleaseFeed,
};
use tauri::async_runtime::spawn_blocking;
use tokio::sync::Mutex as AsyncMutex;

use crate::error::CommandError;

/// The `rim-io` adapters this app injects into `rim-session`'s use cases.
/// Every field but [`Adapters::def_reader`] is stateless (a zero-sized
/// unit struct), so building the set once at startup is purely for the
/// composition root to own a single place they're constructed — not for
/// any resource they'd otherwise reacquire per call. Every command reads
/// its adapters from here rather than constructing `rim_io::*` types
/// inline.
///
/// `def_reader` carries a small file-text cache
/// (`FileDefSourceReader`'s own doc comment), so it isn't `Copy` — kept
/// behind an `Arc` (shared, cheap to clone) rather than rebuilt per
/// command, which would throw the cache away on every call. That's also
/// why this struct is `Clone` but not `Copy`: cloning an `Arc` is
/// cheap, but it isn't a bitwise copy.
///
/// `def_reader` is `Arc<dyn DefSourceReader>`, not `Arc<FileDefSourceReader>`,
/// so a test can build an [`AppState`] with an in-memory reader
/// (`rim_session::test_support::InMemoryDefSourceReader`) instead of one
/// backed by real files, while production wiring
/// ([`Adapters::default`]) still builds the real
/// [`rim_io::FileDefSourceReader`].
#[derive(Clone)]
pub struct Adapters {
    /// Scans a RimWorld install and runs the analyzer over it.
    pub scanner: AnalyzerScanner,
    /// Reads and writes `ModsConfig.xml`.
    pub config_store: ModsConfigFileStore,
    /// Loads and saves `decisions.json`.
    pub decision_store: JsonDecisionStore,
    /// Loads and saves `rules.json`.
    pub rule_store: JsonRuleStore,
    /// Loads and saves compat patch projects (`<profile>/patches/<id>.json`).
    pub patch_store: JsonPatchProjectStore,
    /// Loads and saves assignment (patch maker) projects
    /// (`<profile>/assignments/<id>.json`).
    pub assignment_store: JsonAssignmentProjectStore,
    /// Renders the generated merge mod into the game's `Mods/` folder.
    pub merge_mod_writer: MergeModFolderWriter,
    /// Reads a def/template/patch element's XML text back by locator, for
    /// the merge editor.
    pub def_reader: Arc<dyn DefSourceReader + Send + Sync>,
    /// Locates a mod's texture file (for a `ShipAsset` decision or the
    /// `read_texture` command) and reads it back.
    ///
    /// `Arc<dyn AssetLocator + Send + Sync>`, not `FileAssetLocator`
    /// directly, for the same reason [`Adapters::def_reader`] is boxed:
    /// lets a test build an [`AppState`] with an in-memory locator
    /// (`rim_session::test_support::FakeAssetLocator`) instead of one
    /// backed by real files, while production wiring
    /// ([`Adapters::default`]) still builds the real
    /// [`rim_io::FileAssetLocator`].
    pub asset_locator: Arc<dyn AssetLocator + Send + Sync>,
    /// Reads a mod's `About.xml` details (description, mod version, icon
    /// path) for the mod info panel — the same seam
    /// [`Adapters::asset_locator`] offers, so a test can inject
    /// `rim_session::test_support::FakeModAboutReader` instead of the
    /// real [`rim_io::FileModAboutReader`].
    pub mod_about_reader: Arc<dyn ModAboutReader + Send + Sync>,
    /// Reads and parses a `Player.log`.
    /// Stateless (no cache, unlike [`Self::def_reader`]), so it's kept
    /// bare rather than behind an `Arc` — cheap to copy per call.
    pub game_log_reader: FileGameLogReader,
    /// Probes whether an active mod carries a known def-cache plugin — one
    /// cheap directory walk per active
    /// mod, real filesystem IO this app's own `rim-session` layer can't
    /// do itself (see that port's own doc comment).
    pub def_cache_probe: FsDefCacheCarrierProbe,
    /// Reads the mod-specific knowledge that lives in data
    /// from the app-global
    /// rule-database cache, falling back to the vendored defaults.
    /// Cloned into `LoadProject`; holds only the cache directory.
    pub mod_knowledge_store: FsModKnowledgeStore,
    /// Loads and saves `<base>/notifications.json` (dismissal/mute
    /// bookkeeping, the update-check record).
    pub notification_state_store: JsonNotificationStateStore,
    /// Queries GitHub's Releases API for the latest published Rimmerge
    /// release. `Arc<dyn ReleaseFeed>`, not `GithubReleaseFeed` directly,
    /// for the same reason [`Adapters::def_reader`] is boxed: a test
    /// injects a fake feed instead of one that opens a real socket.
    pub release_feed: Arc<dyn ReleaseFeed + Send + Sync>,
}

impl Default for Adapters {
    fn default() -> Self {
        Self {
            scanner: AnalyzerScanner,
            config_store: ModsConfigFileStore,
            decision_store: JsonDecisionStore,
            rule_store: JsonRuleStore,
            patch_store: JsonPatchProjectStore,
            assignment_store: JsonAssignmentProjectStore,
            merge_mod_writer: MergeModFolderWriter,
            def_reader: Arc::new(FileDefSourceReader::new()),
            asset_locator: Arc::new(FileAssetLocator),
            mod_about_reader: Arc::new(FileModAboutReader::new()),
            game_log_reader: FileGameLogReader,
            def_cache_probe: FsDefCacheCarrierProbe::new(),
            mod_knowledge_store: crate::commands::rules_databases::cache_dir().map_or_else(
                |_| FsModKnowledgeStore::vendored(),
                FsModKnowledgeStore::new,
            ),
            notification_state_store: JsonNotificationStateStore::new(),
            release_feed: Arc::new(GithubReleaseFeed::new()),
        }
    }
}

impl std::fmt::Debug for Adapters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Adapters")
            .field("scanner", &self.scanner)
            .field("config_store", &self.config_store)
            .field("decision_store", &self.decision_store)
            .field("rule_store", &self.rule_store)
            .field("patch_store", &self.patch_store)
            .field("assignment_store", &self.assignment_store)
            .field("merge_mod_writer", &self.merge_mod_writer)
            .field("def_reader", &"<dyn DefSourceReader>")
            .field("asset_locator", &"<dyn AssetLocator>")
            .field("mod_about_reader", &"<dyn ModAboutReader>")
            .field("game_log_reader", &self.game_log_reader)
            .field("def_cache_probe", &self.def_cache_probe)
            .field("mod_knowledge_store", &self.mod_knowledge_store)
            .field("notification_state_store", &self.notification_state_store)
            .field("release_feed", &"<dyn ReleaseFeed>")
            .finish()
    }
}

/// Opens an [`ExternalUrl`] in the user's default system browser — the
/// only place any URL this app knows about ever reaches an outside
/// program. A port (not a direct `tauri_plugin_opener` call at every
/// call site) so a default-gate test never launches a real browser: it
/// injects `test_support::RecordingLinkOpener` instead (not a doc link:
/// `test_support` is `pub(crate)`, unreachable from this public item's
/// own rustdoc page). Takes
/// only [`ExternalUrl`], never a bare `String`/`&str` — the type itself
/// is the enforcement that only a parsed `http`/`https` URL with a host
/// ever reaches this trait, never arbitrary caller-supplied text.
pub trait LinkOpener: std::fmt::Debug {
    /// Opens `url`.
    ///
    /// # Errors
    ///
    /// Returns [`LinkOpenError`] when the OS couldn't open it.
    fn open(&self, url: &ExternalUrl) -> Result<(), LinkOpenError>;
}

/// [`LinkOpener::open`] failed — the OS refused or couldn't find a
/// program to open the link with.
#[derive(Debug, Clone, thiserror::Error)]
#[error("failed to open link: {0}")]
pub struct LinkOpenError(pub String);

/// The real [`LinkOpener`]: hands the URL to
/// `tauri_plugin_opener::open_url`, which shells out to the OS's own
/// "open with default program" mechanism directly — this crate never
/// registers the `tauri-plugin-opener` *plugin* itself
/// (`.plugin(tauri_plugin_opener::init())`), only calls its plain Rust
/// function, so there is no `opener:*` IPC command surface for the
/// frontend to reach at all, regardless of `capabilities/default.json`
/// (which also grants no such permission — see `capabilities.rs`'s own
/// regression test).
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemLinkOpener;

impl LinkOpener for SystemLinkOpener {
    fn open(&self, url: &ExternalUrl) -> Result<(), LinkOpenError> {
        tauri_plugin_opener::open_url(url.as_str(), None::<&str>)
            .map_err(|error| LinkOpenError(error.to_string()))
    }
}

/// Tauri-managed application state: the current session (`None` until
/// [`crate::commands::project::load_project`] succeeds), behind a lock a
/// composition root shares across every IPC command, the adapter set
/// injected into `rim-session`'s use cases, and the game-process probe
/// `apply` consults.
pub struct AppState {
    /// The loaded session, if any. `rim_session::Session` is
    /// single-writer (see its own docs), so every command that touches it
    /// takes the write half of this lock, even for read-only queries —
    /// several of `Session`'s own "read" methods (`ledger`, `findings`)
    /// lazily build and cache internal state on `&mut self`.
    pub session: Arc<RwLock<Option<Session>>>,
    /// The `rim-io` adapters this app was built with.
    pub adapters: Adapters,
    /// Whether RimWorld's own process is currently running.
    pub game_process_probe: Arc<dyn GameProcessProbe>,
    /// Decides how this install is started and starts it. A `dyn` port for
    /// the same reason as [`AppState::game_process_probe`]: tests inject
    /// `test_support::RecordingGameLauncher`, so no default-gate test ever
    /// starts the real game.
    pub game_launcher: Arc<dyn GameLauncher + Send + Sync>,
    /// Opens an external link (a mod's workshop/homepage page, or one of
    /// the app's own fixed links) in the system browser.
    pub link_opener: Arc<dyn LinkOpener + Send + Sync>,
    /// Serializes `load_project` calls: two concurrent loads racing to
    /// scan and then replace [`AppState::session`] could otherwise
    /// interleave, leaving the session built from one call's paths but
    /// the summary returned from the other's.
    pub load_lock: Arc<AsyncMutex<()>>,
    /// Whether `run_launch_network_checks` has already run once in this
    /// process — the once-per-launch flag
    /// `rim_session::use_cases::RunLaunchNetworkChecks::execute` needs
    /// (see that use case's own doc comment for why this lives on the
    /// interface's own state rather than in `rim-session`).
    pub launch_checks_ran: Arc<std::sync::atomic::AtomicBool>,
    /// Whether a "Get the recommended rules" run is in progress in this
    /// process: taken through `commands::recommended_rules::RunningGuard`,
    /// so a double click or a second window cannot start a second download.
    pub recommended_rules_running: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            session: Arc::new(RwLock::new(None)),
            adapters: Adapters::default(),
            game_process_probe: Arc::new(SysinfoGameProcessProbe),
            game_launcher: Arc::new(rim_io::SystemGameLauncher::new()),
            link_opener: Arc::new(SystemLinkOpener),
            load_lock: Arc::new(AsyncMutex::new(())),
            launch_checks_ran: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            recommended_rules_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState")
            .field("session", &self.session)
            .field("adapters", &self.adapters)
            .field("game_process_probe", &"<dyn GameProcessProbe>")
            .field("game_launcher", &"<dyn GameLauncher>")
            .field("link_opener", &"<dyn LinkOpener>")
            .field("load_lock", &"<tokio::sync::Mutex<()>>")
            .field(
                "launch_checks_ran",
                &self
                    .launch_checks_ran
                    .load(std::sync::atomic::Ordering::SeqCst),
            )
            .field(
                "recommended_rules_running",
                &self
                    .recommended_rules_running
                    .load(std::sync::atomic::Ordering::SeqCst),
            )
            .finish()
    }
}

/// Extracts a printable message from a caught panic's payload — `Box<dyn
/// Any + Send>` carries a `&'static str` for a `panic!("literal")` and a
/// `String` for a `panic!("{}", formatted)`, the two shapes
/// `std::panic::catch_unwind` actually produces in practice.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> &str {
    if let Some(message) = payload.downcast_ref::<&str>() {
        message
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.as_str()
    } else {
        "<non-string panic payload>"
    }
}

/// Runs `f` against the loaded session on a blocking thread, taking the
/// write half of [`AppState::session`] for the duration of the call —
/// every command runs its
/// session work in [`spawn_blocking`] so the lock guard never crosses an
/// `.await` (the closure given to `spawn_blocking` is fully synchronous,
/// so that's true by construction here).
///
/// A panic inside `f` is caught with [`std::panic::catch_unwind`] rather
/// than left to unwind through this function: `rim_session::Session` mutates
/// no clone-and-swap discipline, and its own cross-field invariants are
/// only guaranteed once a mutating method returns normally — e.g.
/// `Session::decide` records the new decision *before* recomputing the
/// sort/ledgers that must stay in sync with it. Left unwinding, a panic
/// there drops this guard mid-panic, which poisons the
/// [`std::sync::RwLock`] (a write guard's own `Drop` marks it poisoned
/// whenever it runs during an unwind) — so the *next*, unrelated command
/// would also see `Err(poisoned)` below and pay for a bug it never
/// touched. Catching it here means only the panicking call itself is
/// ever affected; whatever partial mutation `f` left behind still can't
/// be trusted (a caught panic doesn't undo it), so the session is
/// discarded in that same call, not lazily on whatever runs next.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, [`CommandError::session_lost`] when `f` panicked (the session
/// is discarded, in this same call) or — defensively, should the lock
/// ever end up poisoned by something other than `f` itself — when the
/// lock was already poisoned on entry, whatever error `f` itself
/// returns, or [`CommandError::internal`] when the blocking task panics
/// somewhere `catch_unwind` didn't (or can't) catch.
pub async fn with_session<T, F>(state: &AppState, f: F) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce(&mut Session) -> Result<T, CommandError> + Send + 'static,
{
    let session_lock = Arc::clone(&state.session);
    spawn_blocking(move || {
        let mut guard = match session_lock.write() {
            Ok(guard) => guard,
            Err(poisoned) => {
                {
                    let mut recovered = poisoned.into_inner();
                    *recovered = None;
                }
                session_lock.clear_poison();
                return Err(CommandError::session_lost());
            }
        };
        let session = guard.as_mut().ok_or_else(CommandError::no_project_loaded)?;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(session))) {
            Ok(result) => result,
            Err(panic_payload) => {
                *guard = None;
                tracing::warn!(
                    panic = panic_message(&*panic_payload),
                    "session command panicked; discarding this session, not the lock"
                );
                Err(CommandError::session_lost())
            }
        }
    })
    .await
    .unwrap_or_else(|join_error| {
        tracing::warn!(error = %join_error, "session task panicked");
        Err(CommandError::internal("background task panicked"))
    })
}

/// Replaces the loaded session with `session`, on a blocking thread for
/// the same reason [`with_session`] does.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the blocking task panics.
pub async fn replace_session(state: &AppState, session: Session) -> Result<(), CommandError> {
    let session_lock = Arc::clone(&state.session);
    spawn_blocking(move || {
        let mut guard = session_lock
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *guard = Some(session);
    })
    .await
    .map_err(|join_error| {
        tracing::warn!(error = %join_error, "replace-session task panicked");
        CommandError::internal("background task panicked")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn with_session_on_an_empty_state_returns_no_project_loaded() {
        let state = AppState::default();

        let result = with_session(&state, |_session| Ok(())).await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::NoProjectLoaded
        );
    }

    /// Pins `with_session`'s defensive fallback: an *already-poisoned*
    /// lock on entry (built here by poisoning it directly, bypassing
    /// `with_session` entirely) still recovers gracefully. A panic inside `f`
    /// never poisons the lock (see
    /// `a_panic_inside_one_command_does_not_poison_the_lock_for_the_next_command`
    /// below), so this exercises the belt-and-suspenders path for a poison
    /// from *outside* `with_session`.
    #[tokio::test]
    async fn a_poisoned_lock_yields_session_lost_and_clears_the_session() {
        let state = AppState::default();
        *state
            .session
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(rim_session::test_support::session_fixture(&["a"]));

        // Poison the lock from outside `with_session` (a raw thread, not
        // a command closure) — see this test's own doc comment above.
        let session_lock = Arc::clone(&state.session);
        let poisoning = std::thread::spawn(move || {
            let _guard = session_lock
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            panic!("simulated panic while holding the session lock");
        });
        let _ = poisoning.join();
        assert!(state.session.is_poisoned());

        let result = with_session(&state, |_session| Ok(())).await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::SessionLost
        );
        assert!(
            !state.session.is_poisoned(),
            "the lock must be un-poisoned for later calls"
        );
        assert!(
            state
                .session
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_none(),
            "the corrupted session must be discarded"
        );
    }

    /// A panic inside one command's own closure must not poison the lock for
    /// whatever command runs next. Without `catch_unwind` (calling
    /// `f(session)` directly), the panic unwinds past this guard, which (a)
    /// fails the panicking command with `Internal` (from the `spawn_blocking`
    /// `JoinError` path) instead of `SessionLost`, failing the first assertion
    /// below, and (b) poisons `state.session` on the way out, failing the
    /// second assertion (`state.session.is_poisoned()` is `true`) and making
    /// the final `with_session` call return `SessionLost` instead of
    /// `NoProjectLoaded`.
    #[tokio::test]
    async fn a_panic_inside_one_command_does_not_poison_the_lock_for_the_next_command() {
        let state = AppState::default();
        *state
            .session
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(rim_session::test_support::session_fixture(&["a"]));

        let panicking_command = with_session(&state, |_session| -> Result<(), CommandError> {
            panic!("simulated panic inside a command closure");
        })
        .await;

        assert_eq!(
            panicking_command.unwrap_err().code,
            crate::error::CommandErrorCode::SessionLost,
            "the panicking command itself must still fail"
        );
        assert!(
            !state.session.is_poisoned(),
            "a panic inside `f` must never poison the lock"
        );

        // The next command — any command, unrelated to the one that
        // panicked — must see a plain, honest "no project loaded"
        // (the panicking call already discarded it), never a second
        // `session_lost` surprise caused by lock poisoning.
        let next_command = with_session(&state, |_session| Ok(())).await;

        assert_eq!(
            next_command.unwrap_err().code,
            crate::error::CommandErrorCode::NoProjectLoaded,
            "the next command must fail for its own honest reason, not the previous panic"
        );
    }
}
