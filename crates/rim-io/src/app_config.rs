//! [`AppConfig`]: `config.json`, the app-global record of where this
//! machine's RimWorld install, workshop folder and `ModsConfig.xml` are.
//!
//! **Why this is not [`rim_session::Settings`].** `Settings` lives inside
//! the profile's own `rules.json`, and the profile directory is *derived
//! from the `ModsConfig.xml` path* ([`crate::profile_dir`] hashes it). A
//! setting that says where `ModsConfig.xml` is therefore cannot live in a
//! file whose location that answer determines — it is a bootstrap
//! problem, not a preference. So these three paths get their own file,
//! one directory up: `<base>/config.json`, a sibling of `profiles/` and
//! `databases/`, where `base` is the same `%LOCALAPPDATA%\rimmerge` both
//! interfaces already compute (and which the desktop's existing
//! `RIMMERGE_PROFILE_DIR` override already redirects, so a scratch base
//! gets a scratch `config.json` for free).
//!
//! Everything here is optional: an absent file, an absent key, an
//! unreadable file and an unparseable file all load as
//! [`AppConfig::default`]. `config.json` is the *third* rung of
//! [`crate::resolve_project_paths`]'s ladder, below an explicit flag and
//! the `RIMMERGE_*` environment variables and above detection — a broken
//! one must degrade to "detect instead", never to a hard error at
//! startup.

use std::path::{Path, PathBuf};

use rim_session::ports::StoreError;
use serde::{Deserialize, Serialize};

use crate::atomic::write_atomically;

const FILE_NAME: &str = "config.json";

/// The only schema version written today. Read leniently: a file
/// declaring any other version still loads (every field is optional and
/// every unknown key is ignored), because the failure mode of refusing it
/// is "the app cannot find your install any more" for what is, at worst,
/// three stale strings.
const SCHEMA_VERSION: u32 = 1;

/// See [`AppConfigFile::schema`].
const fn default_schema() -> u32 {
    SCHEMA_VERSION
}

/// `<base>/config.json` — the app-global path config, a sibling of
/// `<base>/profiles/` and `<base>/databases/`.
#[must_use]
pub fn app_config_path(base: &Path) -> PathBuf {
    base.join(FILE_NAME)
}

/// The three machine-specific paths a user can pin so they never have to
/// pass `--game-dir` again. `None` means "not pinned, fall through to the
/// next rung of the ladder" — never "empty".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppConfig {
    /// The RimWorld install directory.
    pub game_dir: Option<PathBuf>,
    /// The Steam workshop content folder for RimWorld (app id `294100`).
    pub workshop_dir: Option<PathBuf>,
    /// The active `ModsConfig.xml`.
    pub mods_config: Option<PathBuf>,
}

/// The on-disk shape: `{"schema":1,"game_dir":…,"workshop_dir":…,"mods_config":…}`.
/// Deliberately **not** `deny_unknown_fields` — a file written by a newer
/// build must still load on an older one.
#[derive(Debug, Serialize, Deserialize)]
struct AppConfigFile {
    /// `#[serde(default)]` because this file is one a user hand-edits: a
    /// `{"game_dir": "..."}` written by hand, with no `schema` key, must
    /// load rather than silently coming back empty and sending the app
    /// off to detection instead (the failure mode is invisible — the app
    /// simply ignores what you wrote). Always *written* as
    /// [`SCHEMA_VERSION`], so a hand-written file gains the key on the
    /// next save.
    #[serde(default = "default_schema")]
    schema: u32,
    #[serde(default)]
    game_dir: Option<String>,
    #[serde(default)]
    workshop_dir: Option<String>,
    #[serde(default)]
    mods_config: Option<String>,
}

impl AppConfig {
    /// Reads `<base>/config.json`. A missing, unreadable or unparseable
    /// file is [`AppConfig::default`], never an error — see this module's
    /// own doc comment for why.
    #[must_use]
    pub fn load(base: &Path) -> Self {
        let Ok(text) = std::fs::read_to_string(app_config_path(base)) else {
            return Self::default();
        };
        let Ok(file) = serde_json::from_str::<AppConfigFile>(&text) else {
            return Self::default();
        };
        Self {
            game_dir: file.game_dir.map(PathBuf::from),
            workshop_dir: file.workshop_dir.map(PathBuf::from),
            mods_config: file.mods_config.map(PathBuf::from),
        }
    }

    /// Writes `<base>/config.json` through the same atomic
    /// temp-file-then-rename writer every other file in this crate uses,
    /// creating `base` if it doesn't exist yet.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when serializing or writing fails.
    pub fn save(&self, base: &Path) -> Result<(), StoreError> {
        let file = AppConfigFile {
            schema: SCHEMA_VERSION,
            game_dir: path_to_string(self.game_dir.as_deref()),
            workshop_dir: path_to_string(self.workshop_dir.as_deref()),
            mods_config: path_to_string(self.mods_config.as_deref()),
        };
        let bytes = serde_json::to_vec_pretty(&file)
            .map_err(|error| StoreError(format!("serializing {FILE_NAME}: {error}")))?;
        let path = app_config_path(base);
        write_atomically(&path, &bytes)
            .map_err(|error| StoreError(format!("writing {}: {error}", path.display())))
    }

    /// True when no path at all is pinned — what `rimmerge config clear`
    /// leaves behind, and what a first run starts from.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.game_dir.is_none() && self.workshop_dir.is_none() && self.mods_config.is_none()
    }
}

fn path_to_string(path: Option<&Path>) -> Option<String> {
    path.map(|path| path.display().to_string())
}

/// Normalizes a user-supplied path before it is *pinned*: absolutized
/// against the current directory, and canonicalized when it already
/// exists (`crate::resolve_user_dir`, which works on a file as well as a
/// directory).
///
/// Pinning a relative path is a trap: `config.json` is read from
/// `%LOCALAPPDATA%\rimmerge` by a process whose working directory is
/// wherever it happened to be launched, so `rimmerge config set
/// --game-dir ./RimWorld` would resolve to a different install every
/// time — or to nothing. Absolutizing at the moment of pinning is the
/// only point where the user's intent ("this directory, here, now") is
/// still knowable.
///
/// Falls back to the path as given when absolutizing fails (an empty
/// path, a permissions error on an existing directory): a pin is a
/// convenience, and refusing one over a normalization failure would be
/// worse than storing what the user typed.
#[must_use]
pub fn pinned_path(raw: &Path) -> PathBuf {
    crate::resolve_user_dir(raw).unwrap_or_else(|_| raw.to_path_buf())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn the_config_sits_beside_profiles_and_databases() {
        let base = Path::new(r"C:\base");

        assert_eq!(app_config_path(base), base.join("config.json"));
        assert_eq!(
            app_config_path(base).parent(),
            crate::profile_dir(base, Path::new("x"))
                .parent()
                .and_then(Path::parent),
            "config.json must be a sibling of profiles/, not inside one"
        );
    }

    #[test]
    fn an_absent_file_loads_as_the_default() {
        let dir = tempdir().expect("tempdir");

        let config = AppConfig::load(dir.path());

        assert_eq!(config, AppConfig::default());
        assert!(config.is_empty());
    }

    #[test]
    fn an_unparseable_file_loads_as_the_default_rather_than_failing() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(app_config_path(dir.path()), b"{ not json at all").expect("seed");

        assert_eq!(AppConfig::load(dir.path()), AppConfig::default());
    }

    #[test]
    fn a_saved_config_round_trips() {
        let dir = tempdir().expect("tempdir");
        let config = AppConfig {
            game_dir: Some(PathBuf::from(r"Q:\Steam\steamapps\common\RimWorld")),
            workshop_dir: None,
            mods_config: Some(PathBuf::from(r"Q:\Config\ModsConfig.xml")),
        };

        config.save(dir.path()).expect("save must succeed");

        assert_eq!(AppConfig::load(dir.path()), config);
    }

    #[test]
    fn the_written_shape_is_the_documented_one() {
        let dir = tempdir().expect("tempdir");
        AppConfig {
            game_dir: Some(PathBuf::from("/games/RimWorld")),
            workshop_dir: None,
            mods_config: None,
        }
        .save(dir.path())
        .expect("save must succeed");

        let text = std::fs::read_to_string(app_config_path(dir.path())).expect("read back");
        let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");

        assert_eq!(value["schema"], 1);
        assert_eq!(value["game_dir"], "/games/RimWorld");
        assert!(value["workshop_dir"].is_null());
        assert!(value["mods_config"].is_null());
    }

    #[test]
    fn an_unknown_key_and_a_newer_schema_still_load() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            app_config_path(dir.path()),
            br#"{"schema":99,"game_dir":"/g","future_key":{"a":1}}"#,
        )
        .expect("seed");

        assert_eq!(
            AppConfig::load(dir.path()).game_dir.as_deref(),
            Some(Path::new("/g")),
            "an older build must not be locked out by a newer file"
        );
    }

    #[test]
    fn a_hand_written_file_with_no_schema_key_still_loads() {
        // This file is meant to be hand-editable. Without
        // `#[serde(default)]` on `schema`, the whole object fails to
        // deserialize and `load` returns the default — the app then
        // ignores what the user wrote, with no diagnostic anywhere.
        let dir = tempdir().expect("tempdir");
        std::fs::write(
            app_config_path(dir.path()),
            br#"{"game_dir": "/hand/written"}"#,
        )
        .expect("seed");

        assert_eq!(
            AppConfig::load(dir.path()).game_dir.as_deref(),
            Some(Path::new("/hand/written"))
        );
    }

    #[test]
    fn a_save_after_a_schemaless_load_writes_the_current_schema() {
        let dir = tempdir().expect("tempdir");
        std::fs::write(app_config_path(dir.path()), br#"{"game_dir": "/g"}"#).expect("seed");

        AppConfig::load(dir.path())
            .save(dir.path())
            .expect("save must succeed");

        let text = std::fs::read_to_string(app_config_path(dir.path())).expect("read back");
        let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(value["schema"], SCHEMA_VERSION);
    }

    #[test]
    fn a_relative_path_is_absolutized_before_it_is_pinned() {
        let relative = Path::new("RimWorld");

        let pinned = pinned_path(relative);

        assert!(
            pinned.is_absolute(),
            "a pinned path must not depend on the working directory of whoever reads config.json \
             next: {}",
            pinned.display()
        );
        assert!(pinned.ends_with("RimWorld"), "{}", pinned.display());
    }

    #[test]
    fn an_absolute_path_that_exists_survives_pinning_unchanged_in_meaning() {
        let dir = tempdir().expect("tempdir");
        let pinned = pinned_path(dir.path());

        assert!(pinned.is_absolute());
        assert_eq!(
            pinned.canonicalize().ok(),
            dir.path().canonicalize().ok(),
            "pinning must not change which directory is meant"
        );
    }

    #[test]
    fn saving_creates_the_base_directory() {
        let dir = tempdir().expect("tempdir");
        let base = dir.path().join("not").join("created").join("yet");

        AppConfig {
            game_dir: Some(PathBuf::from("/g")),
            ..AppConfig::default()
        }
        .save(&base)
        .expect("save must succeed");

        assert!(app_config_path(&base).is_file());
    }
}
