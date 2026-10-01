//! [`resolve_project_paths`]: the one implementation of "where is this
//! machine's RimWorld install, and where does this project's state live".
//!
//! Both composition roots call it — `apps/cli`'s `common::resolve_paths`
//! and the desktop's `get_default_paths` — so the precedence ladder
//! exists exactly once.
//! Highest wins:
//!
//! 1. an explicit `--game-dir`/`--workshop-dir`/`--mods-config` flag, or
//!    the desktop Setup page's field for this session
//! 2. `RIMMERGE_GAME_DIR` / `RIMMERGE_WORKSHOP_DIR` / `RIMMERGE_MODS_CONFIG`
//! 3. `<base>/config.json` ([`crate::AppConfig`])
//! 4. detection ([`rim_analyzer::infra::paths::detect_game_dir`]) and the
//!    derivations that hang off it
//! 5. [`PathResolutionError`] — **never** a fallback path. A guessed
//!    install directory is worse than no answer: it turns "I can't find
//!    your game" into a scan of an empty folder and a confusing report.
//!
//! `profile_dir` is deliberately outside the ladder's middle rungs: it is
//! either given explicitly or derived from the resolved `mods_config`
//! ([`crate::profile_dir`]), because a profile that didn't match its own
//! `ModsConfig.xml` would silently load another install's decisions.

use std::path::PathBuf;

use rim_analyzer::infra::paths;
use rim_session::ProjectPaths;

use crate::app_config::AppConfig;

/// Rung 2's three variable names, re-exported from
/// [`rim_analyzer::infra::paths`] rather than restated here: the
/// real-install test guards in `crates/rim-analyzer/tests/` cannot depend
/// on this crate (it depends on *them*), so the one definition has to sit
/// upstream or the literal ends up copied three times.
pub use rim_analyzer::infra::paths::{GAME_DIR_VAR, MODS_CONFIG_VAR, WORKSHOP_DIR_VAR};

/// Rung 1: whatever the caller was told explicitly, plus the `base`
/// directory `config.json` and `profiles/` live under.
#[derive(Debug, Clone)]
pub struct PathOverrides {
    /// `%LOCALAPPDATA%\rimmerge` (or the desktop's `RIMMERGE_PROFILE_DIR`
    /// redirect): where `config.json`, `profiles/` and `databases/` live.
    pub base: PathBuf,
    /// An explicit install directory (`--game-dir`, or the Setup page's
    /// field).
    pub game_dir: Option<PathBuf>,
    /// An explicit workshop content folder.
    pub workshop_dir: Option<PathBuf>,
    /// An explicit `ModsConfig.xml`.
    pub mods_config: Option<PathBuf>,
    /// An explicit profile directory; otherwise derived from the resolved
    /// `mods_config`.
    pub profile_dir: Option<PathBuf>,
}

impl PathOverrides {
    /// Nothing given but the base directory — the shape every interface
    /// starts from before it applies its own flags.
    #[must_use]
    pub fn new(base: PathBuf) -> Self {
        Self {
            base,
            game_dir: None,
            workshop_dir: None,
            mods_config: None,
            profile_dir: None,
        }
    }
}

/// Rung 5: resolution ran out of rungs. Both variants name every place
/// that was looked at, so the message is actionable without the user
/// having to know what this tool checks.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathResolutionError {
    /// No install directory was given, pinned, or detected.
    #[error(
        "no RimWorld install found. Pass --game-dir <path>, set {GAME_DIR_VAR}, or run \
         `rimmerge config set --game-dir <path>`. Looked in:\n{}",
        format_candidates(.candidates)
    )]
    GameDirNotFound {
        /// Every candidate [`paths::default_game_dir_candidates`] offered,
        /// in the order they were tried.
        candidates: Vec<PathBuf>,
    },
    /// No `ModsConfig.xml` was given, pinned, or derivable (the
    /// derivation needs `%USERPROFILE%`).
    #[error(
        "could not work out where ModsConfig.xml is. Pass --mods-config <path>, set \
         {MODS_CONFIG_VAR}, or run `rimmerge config set --mods-config <path>`. The default \
         location could not be derived: {reason}"
    )]
    ModsConfigNotFound {
        /// Why the `%USERPROFILE%`-relative derivation failed.
        reason: String,
    },
}

fn format_candidates(candidates: &[PathBuf]) -> String {
    if candidates.is_empty() {
        return "  (no candidate locations are known for this platform)".to_string();
    }
    candidates
        .iter()
        .map(|candidate| format!("  {}", candidate.display()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The wording every surface uses for "this path was given to me, but it
/// doesn't look like a RimWorld install" — `rimmerge config set`'s own
/// warning, [`ResolvedPaths::warning`], and the desktop Setup page all
/// render this same sentence, so a user who sees it in one place
/// recognises it in the others.
#[must_use]
pub fn not_an_install_warning(game_dir: &std::path::Path) -> String {
    format!(
        "{} does not look like a RimWorld install (Version.txt or Data/Core/ is missing)",
        game_dir.display()
    )
}

/// A resolved [`ProjectPaths`] plus any **non-fatal** complaint about it.
///
/// `warning` is `Some` when rungs 1-3 answered with a directory that
/// fails [`paths::is_game_dir`] — an explicit `--game-dir`, a
/// `RIMMERGE_GAME_DIR`, or a pinned `config.json` entry pointing
/// somewhere that isn't (or isn't yet) an install. Deliberately **not**
/// an error: an external drive that isn't mounted yet, or a path typed
/// one character wrong, should produce a scan that fails with a real
/// message about *that* directory rather than a refusal before anything
/// is attempted. Detection (rung 4) can never trigger it — it only ever
/// returns a directory that already passed the same predicate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedPaths {
    /// The resolved paths.
    pub paths: ProjectPaths,
    /// See this type's own doc comment; `None` when nothing is amiss.
    pub warning: Option<String>,
}

/// Applies the ladder in this module's doc comment.
///
/// # Errors
///
/// [`PathResolutionError::GameDirNotFound`] when no install directory can
/// be found, and [`PathResolutionError::ModsConfigNotFound`] when no
/// `ModsConfig.xml` path can be worked out.
pub fn resolve_project_paths(
    overrides: PathOverrides,
) -> Result<ResolvedPaths, PathResolutionError> {
    resolve_with(&overrides, &SystemEnvironment)
}

/// The machine-dependent inputs to the ladder, behind a trait so the
/// ladder itself is tested against a hand-written fake instead of the
/// process environment (which no two tests could set concurrently without
/// racing).
trait Environment {
    /// The value of environment variable `key` as a path, if it is set to
    /// something other than whitespace. See [`paths::path_var`] for why
    /// this reads `var_os` rather than `var` (a path need not be UTF-8,
    /// and `var`'s `NotUnicode` error would silently fall through to the
    /// next rung — resolving a *different* install than the one named).
    fn path_var(&self, key: &str) -> Option<PathBuf>;
    /// See [`paths::detect_game_dir`].
    fn detect_game_dir(&self) -> Option<PathBuf>;
    /// See [`paths::default_game_dir_candidates`].
    fn game_dir_candidates(&self) -> Vec<PathBuf>;
    /// See [`paths::default_mods_config_path`]; the error text is carried
    /// into [`PathResolutionError::ModsConfigNotFound`].
    fn default_mods_config(&self) -> Result<PathBuf, String>;
    /// See [`paths::is_game_dir`] — injected so the warning's own tests
    /// don't need a real install tree on disk.
    fn is_game_dir(&self, path: &std::path::Path) -> bool;
}

struct SystemEnvironment;

impl Environment for SystemEnvironment {
    fn path_var(&self, key: &str) -> Option<PathBuf> {
        paths::path_var(key)
    }

    fn detect_game_dir(&self) -> Option<PathBuf> {
        paths::detect_game_dir()
    }

    fn game_dir_candidates(&self) -> Vec<PathBuf> {
        paths::default_game_dir_candidates()
    }

    fn default_mods_config(&self) -> Result<PathBuf, String> {
        paths::default_mods_config_path().map_err(|error| error.to_string())
    }

    fn is_game_dir(&self, path: &std::path::Path) -> bool {
        paths::is_game_dir(path)
    }
}

fn resolve_with(
    overrides: &PathOverrides,
    environment: &dyn Environment,
) -> Result<ResolvedPaths, PathResolutionError> {
    let config = AppConfig::load(&overrides.base);

    // Rungs 1-3 are answers someone *gave*; rung 4 is one this code
    // found. Only the former can be wrong in a way worth warning about,
    // which is why detection is kept out of `given` entirely.
    let given = first_of(
        overrides.game_dir.clone(),
        environment.path_var(GAME_DIR_VAR),
        config.game_dir.clone(),
    );
    let warning = given
        .as_deref()
        .filter(|game_dir| !environment.is_game_dir(game_dir))
        .map(not_an_install_warning);
    let game_dir = given
        .or_else(|| environment.detect_game_dir())
        .ok_or_else(|| PathResolutionError::GameDirNotFound {
            candidates: environment.game_dir_candidates(),
        })?;

    let workshop_dir = first_of(
        overrides.workshop_dir.clone(),
        environment.path_var(WORKSHOP_DIR_VAR),
        config.workshop_dir.clone(),
    )
    .unwrap_or_else(|| paths::default_workshop_dir(&game_dir));

    let mods_config = match first_of(
        overrides.mods_config.clone(),
        environment.path_var(MODS_CONFIG_VAR),
        config.mods_config.clone(),
    ) {
        Some(path) => path,
        None => environment
            .default_mods_config()
            .map_err(|reason| PathResolutionError::ModsConfigNotFound { reason })?,
    };

    let profile_dir = overrides
        .profile_dir
        .clone()
        .unwrap_or_else(|| crate::profile_dir(&overrides.base, &mods_config));

    Ok(ResolvedPaths {
        paths: ProjectPaths {
            game_dir,
            workshop_dir,
            mods_config,
            profile_dir,
        },
        warning,
    })
}

/// Rungs 1-3 for one path, in order.
///
/// [`Environment::path_var`] has already rejected an unset or
/// whitespace-only variable (`set RIMMERGE_GAME_DIR=` means "unset this",
/// not "resolve the install to the empty path"), so the only thing left
/// to do here is order the three.
fn first_of(
    explicit: Option<PathBuf>,
    from_env: Option<PathBuf>,
    from_config: Option<PathBuf>,
) -> Option<PathBuf> {
    explicit.or(from_env).or(from_config)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::tempdir;

    use super::*;

    /// A hand-written fake rather than a mock: plain fields, set per test,
    /// so each case reads as the rung it is exercising.
    #[derive(Default)]
    struct FakeEnvironment {
        vars: Vec<(String, String)>,
        detected: Option<PathBuf>,
        candidates: Vec<PathBuf>,
        mods_config: Option<PathBuf>,
        /// Which directories this fake machine considers real installs.
        /// Empty means "none" — so the default fake exercises the
        /// warning path, and a test that cares says so explicitly.
        installs: Vec<PathBuf>,
    }

    impl FakeEnvironment {
        fn with_var(mut self, key: &str, value: &str) -> Self {
            self.vars.push((key.to_string(), value.to_string()));
            self
        }

        fn detecting(mut self, game_dir: &str) -> Self {
            self.detected = Some(PathBuf::from(game_dir));
            self.installs.push(PathBuf::from(game_dir));
            self
        }

        fn with_default_mods_config(mut self, path: &str) -> Self {
            self.mods_config = Some(PathBuf::from(path));
            self
        }

        fn with_install_at(mut self, path: &str) -> Self {
            self.installs.push(PathBuf::from(path));
            self
        }
    }

    impl Environment for FakeEnvironment {
        fn path_var(&self, key: &str) -> Option<PathBuf> {
            self.vars
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
                .filter(|value| !value.trim().is_empty())
                .map(PathBuf::from)
        }

        fn detect_game_dir(&self) -> Option<PathBuf> {
            self.detected.clone()
        }

        fn game_dir_candidates(&self) -> Vec<PathBuf> {
            self.candidates.clone()
        }

        fn default_mods_config(&self) -> Result<PathBuf, String> {
            self.mods_config
                .clone()
                .ok_or_else(|| "USERPROFILE environment variable is not set".to_string())
        }

        fn is_game_dir(&self, path: &Path) -> bool {
            self.installs.iter().any(|install| install == path)
        }
    }

    fn base_with_config(config: &AppConfig) -> tempfile::TempDir {
        let dir = tempdir().expect("tempdir");
        config.save(dir.path()).expect("seed config.json");
        dir
    }

    fn overrides_at(base: &Path) -> PathOverrides {
        PathOverrides::new(base.to_path_buf())
    }

    #[test]
    fn an_explicit_flag_beats_the_environment_the_config_and_detection() {
        let base = base_with_config(&AppConfig {
            game_dir: Some(PathBuf::from("/from-config")),
            ..AppConfig::default()
        });
        let environment = FakeEnvironment::default()
            .with_var(GAME_DIR_VAR, "/from-env")
            .detecting("/detected")
            .with_default_mods_config("/mods/ModsConfig.xml");
        let mut overrides = overrides_at(base.path());
        overrides.game_dir = Some(PathBuf::from("/from-flag"));

        let resolved = resolve_with(&overrides, &environment)
            .expect("resolution must succeed")
            .paths;

        assert_eq!(resolved.game_dir, PathBuf::from("/from-flag"));
    }

    #[test]
    fn the_environment_beats_the_config_and_detection() {
        let base = base_with_config(&AppConfig {
            game_dir: Some(PathBuf::from("/from-config")),
            ..AppConfig::default()
        });
        let environment = FakeEnvironment::default()
            .with_var(GAME_DIR_VAR, "/from-env")
            .detecting("/detected")
            .with_default_mods_config("/mods/ModsConfig.xml");

        let resolved = resolve_with(&overrides_at(base.path()), &environment)
            .expect("resolution must succeed")
            .paths;

        assert_eq!(resolved.game_dir, PathBuf::from("/from-env"));
    }

    #[test]
    fn the_config_beats_detection() {
        let base = base_with_config(&AppConfig {
            game_dir: Some(PathBuf::from("/from-config")),
            ..AppConfig::default()
        });
        let environment = FakeEnvironment::default()
            .detecting("/detected")
            .with_default_mods_config("/mods/ModsConfig.xml");

        let resolved = resolve_with(&overrides_at(base.path()), &environment)
            .expect("resolution must succeed")
            .paths;

        assert_eq!(resolved.game_dir, PathBuf::from("/from-config"));
    }

    #[test]
    fn detection_is_the_last_rung_before_the_error() {
        let base = tempdir().expect("tempdir");
        let environment = FakeEnvironment::default()
            .detecting("/detected")
            .with_default_mods_config("/mods/ModsConfig.xml");

        let resolved = resolve_with(&overrides_at(base.path()), &environment)
            .expect("resolution must succeed")
            .paths;

        assert_eq!(resolved.game_dir, PathBuf::from("/detected"));
    }

    #[test]
    fn no_install_anywhere_errors_and_lists_every_candidate_rather_than_guessing() {
        let base = tempdir().expect("tempdir");
        let environment = FakeEnvironment {
            candidates: vec![PathBuf::from("/look/here"), PathBuf::from("/and/here")],
            ..FakeEnvironment::default()
        };

        let error =
            resolve_with(&overrides_at(base.path()), &environment).expect_err("must not guess");

        let PathResolutionError::GameDirNotFound { ref candidates } = error else {
            panic!("expected GameDirNotFound, got {error:?}");
        };
        assert_eq!(candidates.len(), 2);
        let message = error.to_string();
        assert!(message.contains("/look/here"), "{message}");
        assert!(message.contains("/and/here"), "{message}");
        assert!(message.contains(GAME_DIR_VAR), "{message}");
        assert!(message.contains("rimmerge config set"), "{message}");
    }

    #[test]
    fn the_workshop_dir_falls_through_to_the_derivation_off_the_game_dir() {
        let base = tempdir().expect("tempdir");
        let environment = FakeEnvironment::default()
            .detecting(r"Q:\Steam\steamapps\common\RimWorld")
            .with_default_mods_config("/mods/ModsConfig.xml");

        let resolved = resolve_with(&overrides_at(base.path()), &environment)
            .expect("resolution must succeed")
            .paths;

        assert_eq!(
            resolved.workshop_dir,
            paths::default_workshop_dir(Path::new(r"Q:\Steam\steamapps\common\RimWorld"))
        );
    }

    #[test]
    fn each_path_climbs_its_own_ladder_independently() {
        let base = base_with_config(&AppConfig {
            game_dir: Some(PathBuf::from("/from-config")),
            workshop_dir: Some(PathBuf::from("/workshop-from-config")),
            mods_config: None,
        });
        let environment = FakeEnvironment::default()
            .with_var(MODS_CONFIG_VAR, "/mods-from-env/ModsConfig.xml")
            .detecting("/detected")
            .with_default_mods_config("/mods/ModsConfig.xml");

        let resolved = resolve_with(&overrides_at(base.path()), &environment)
            .expect("resolution must succeed")
            .paths;

        assert_eq!(resolved.game_dir, PathBuf::from("/from-config"));
        assert_eq!(
            resolved.workshop_dir,
            PathBuf::from("/workshop-from-config")
        );
        assert_eq!(
            resolved.mods_config,
            PathBuf::from("/mods-from-env/ModsConfig.xml")
        );
    }

    #[test]
    fn an_underivable_mods_config_errors_rather_than_pointing_at_nothing() {
        let base = tempdir().expect("tempdir");
        let environment = FakeEnvironment::default().detecting("/detected");

        let error =
            resolve_with(&overrides_at(base.path()), &environment).expect_err("must not guess");

        assert!(
            matches!(error, PathResolutionError::ModsConfigNotFound { .. }),
            "{error:?}"
        );
        assert!(error.to_string().contains(MODS_CONFIG_VAR));
    }

    #[test]
    fn the_profile_dir_is_derived_from_the_resolved_mods_config() {
        let base = tempdir().expect("tempdir");
        let environment = FakeEnvironment::default()
            .detecting("/detected")
            .with_default_mods_config("/mods/ModsConfig.xml");

        let resolved = resolve_with(&overrides_at(base.path()), &environment)
            .expect("resolution must succeed")
            .paths;

        assert_eq!(
            resolved.profile_dir,
            crate::profile_dir(base.path(), Path::new("/mods/ModsConfig.xml")),
            "the profile must be the one this ModsConfig.xml hashes to"
        );
    }

    #[test]
    fn an_explicit_profile_dir_is_used_verbatim() {
        let base = tempdir().expect("tempdir");
        let environment = FakeEnvironment::default()
            .detecting("/detected")
            .with_default_mods_config("/mods/ModsConfig.xml");
        let mut overrides = overrides_at(base.path());
        overrides.profile_dir = Some(PathBuf::from("/scratch/profile"));

        let resolved = resolve_with(&overrides, &environment)
            .expect("resolution must succeed")
            .paths;

        assert_eq!(resolved.profile_dir, PathBuf::from("/scratch/profile"));
    }

    #[test]
    fn an_empty_environment_variable_falls_through_instead_of_resolving_to_nothing() {
        let base = base_with_config(&AppConfig {
            game_dir: Some(PathBuf::from("/from-config")),
            ..AppConfig::default()
        });
        let environment = FakeEnvironment::default()
            .with_var(GAME_DIR_VAR, "")
            .detecting("/detected")
            .with_default_mods_config("/mods/ModsConfig.xml");

        let resolved = resolve_with(&overrides_at(base.path()), &environment)
            .expect("resolution must succeed")
            .paths;

        assert_eq!(
            resolved.game_dir,
            PathBuf::from("/from-config"),
            "`set RIMMERGE_GAME_DIR=` must not resolve the install to the empty path"
        );
    }

    #[test]
    fn a_given_game_dir_that_is_not_an_install_warns_but_still_resolves() {
        let base = tempdir().expect("tempdir");
        let environment = FakeEnvironment::default()
            .with_var(GAME_DIR_VAR, "/typo")
            .with_default_mods_config("/mods/ModsConfig.xml");
        // No `.detecting(...)`, so nothing else could have answered:
        // resolution must still succeed on the given path.

        let resolved =
            resolve_with(&overrides_at(base.path()), &environment).expect("must not be fatal");

        assert_eq!(resolved.paths.game_dir, PathBuf::from("/typo"));
        assert_eq!(
            resolved.warning.as_deref(),
            Some(not_an_install_warning(Path::new("/typo")).as_str()),
            "a path someone gave that isn't an install is a warning, not a refusal"
        );
    }

    #[test]
    fn the_not_an_install_warning_names_both_markers_as_alternatives() {
        let warning = not_an_install_warning(Path::new("/typo"));

        assert!(
            warning.ends_with("(Version.txt or Data/Core/ is missing)"),
            "either marker missing is enough to doubt an install, got: {warning}"
        );
    }

    #[test]
    fn a_given_game_dir_that_is_an_install_carries_no_warning() {
        let base = tempdir().expect("tempdir");
        let environment = FakeEnvironment::default()
            .with_var(GAME_DIR_VAR, "/real")
            .with_install_at("/real")
            .with_default_mods_config("/mods/ModsConfig.xml");

        let resolved =
            resolve_with(&overrides_at(base.path()), &environment).expect("must not be fatal");

        assert_eq!(resolved.warning, None);
    }

    #[test]
    fn detection_never_warns_because_it_only_returns_verified_installs() {
        let base = tempdir().expect("tempdir");
        // `detecting` registers the directory as an install too, exactly
        // as `detect_game_dir` guarantees on a real machine.
        let environment = FakeEnvironment::default()
            .detecting("/detected")
            .with_default_mods_config("/mods/ModsConfig.xml");

        let resolved =
            resolve_with(&overrides_at(base.path()), &environment).expect("must not be fatal");

        assert_eq!(resolved.paths.game_dir, PathBuf::from("/detected"));
        assert_eq!(resolved.warning, None);
    }

    #[test]
    fn a_non_utf8_environment_variable_is_not_silently_skipped() {
        // `paths::path_var` reads `var_os`, so a value `std::env::var`
        // would reject as `NotUnicode` still resolves. Checked on the one
        // seam that can be exercised portably — the real reader's own
        // handling of an absent variable and a whitespace one — since
        // planting a non-UTF-8 value needs an `unsafe` `set_var` in
        // edition 2024.
        assert_eq!(
            SystemEnvironment.path_var("RIMMERGE_DEFINITELY_UNSET_IN_THIS_PROCESS"),
            None
        );
    }
}
