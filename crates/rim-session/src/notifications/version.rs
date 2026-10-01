//! [`AppVersion`]: the running or a published Rimmerge version, compared
//! by full semver precedence so a pre-release build correctly sorts older
//! than the stable release it precedes. [`LatestRelease`]: one parsed
//! `GET /releases/latest` result. [`GameMajorMinor`]: the RimWorld
//! version truncated to `major.minor`, for
//! [`crate::notifications::Notification::GameVersionChanged`].

use std::fmt;

/// A Rimmerge version — either the binary currently running
/// ([`Self::running`], which accepts a pre-release/build suffix) or one
/// GitHub reports as published ([`Self::published`], which rejects one).
/// `Ord` is full semver precedence (`semver::Version`'s own), so a
/// pre-release like `0.2.0-rc.1` is correctly older than the stable
/// `0.2.0` it precedes — a notice only ever fires when a published
/// version is strictly newer than the running one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct AppVersion(semver::Version);

/// The longest tag string this app ever expects to parse — a bound
/// checked before the string ever reaches the semver parser.
const MAX_LEN: usize = 32;

/// A version string [`AppVersion`] couldn't accept.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AppVersionError {
    /// Longer than the 32-character cap (`MAX_LEN`, this module's own
    /// private constant).
    #[error("version string is longer than {MAX_LEN} characters")]
    TooLong,
    /// Not a valid `MAJOR.MINOR.PATCH[-pre][+build]` string (after
    /// stripping one leading `v`, when present). Carries the parser's own
    /// message as a plain `String` rather than the source error itself —
    /// `semver::Error` implements neither `Clone` nor `PartialEq`, which
    /// this enum needs for its own tests and callers.
    #[error("{0}")]
    NotSemver(String),
    /// [`AppVersion::published`] only: the version carries a pre-release
    /// or build-metadata suffix, so it cannot be a stable published
    /// release.
    #[error("{0} is a pre-release or build version, not a stable release")]
    NotAStableVersion(String),
}

fn strip_leading_v(raw: &str) -> &str {
    raw.strip_prefix('v').unwrap_or(raw)
}

impl AppVersion {
    /// Parses the binary's own running version — accepts a pre-release or
    /// build suffix (a dev/rc build), so a dev build ahead of the latest
    /// stable release still compares correctly against it.
    ///
    /// # Errors
    ///
    /// Returns [`AppVersionError`] when `raw` is over 32 characters or
    /// isn't valid semver (after stripping one leading `v`).
    pub fn running(raw: &str) -> Result<Self, AppVersionError> {
        if raw.len() > MAX_LEN {
            return Err(AppVersionError::TooLong);
        }
        let version = semver::Version::parse(strip_leading_v(raw))
            .map_err(|error| AppVersionError::NotSemver(error.to_string()))?;
        Ok(Self(version))
    }

    /// Parses a version GitHub reports as published — rejects a
    /// pre-release or build suffix, since `/releases/latest` is
    /// documented to return only a stable, published release (this check
    /// is defence in depth, not the primary guarantee — see
    /// `rim_io::release_feed`'s own doc comment).
    ///
    /// # Errors
    ///
    /// As [`Self::running`], plus [`AppVersionError::NotAStableVersion`]
    /// when the parsed version carries a pre-release or build suffix.
    pub fn published(raw: &str) -> Result<Self, AppVersionError> {
        let version = Self::running(raw)?;
        if version.0.pre.is_empty() && version.0.build.is_empty() {
            Ok(version)
        } else {
            Err(AppVersionError::NotAStableVersion(raw.to_string()))
        }
    }
}

impl fmt::Display for AppVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One parsed, validated `GET /releases/latest` result: a stable,
/// published version and when it was published. Deliberately narrow —
/// never carries the response's own `body`/`html_url`/`assets`; the
/// release link shown to the user is built from a `const` prefix plus
/// [`Self::version`] on the app side (see `docs/privacy-and-network.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestRelease {
    /// The published version.
    pub version: AppVersion,
    /// When GitHub recorded this release as published.
    pub published_at: jiff::Timestamp,
}

/// A RimWorld version truncated to `major.minor` — mods declare
/// `supportedVersions` at this granularity (`render_merge_mod.rs`'s own
/// `EmitInput::game_version`), so a change to the build number alone
/// must never trigger [`crate::notifications::Notification::GameVersionChanged`].
/// Mirrors `use_cases::render_merge_mod`'s own private `major_minor`
/// truncation exactly, kept as an independent copy rather than a shared
/// export: that helper's module is private to `use_cases` (unreachable
/// from `notifications`), and it returns a plain rendering `String`
/// where this is a domain value object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameMajorMinor(String);

impl GameMajorMinor {
    /// Truncates a full game version string (e.g. `"1.6.4525 rev123"`)
    /// to its `major.minor` form (`"1.6"`).
    #[must_use]
    pub fn parse(game_version: &str) -> Self {
        let mut parts = game_version.split(|c: char| !c.is_ascii_digit() && c != '.');
        let truncated = parts
            .next()
            .unwrap_or(game_version)
            .splitn(3, '.')
            .take(2)
            .collect::<Vec<_>>()
            .join(".");
        Self(truncated)
    }
}

/// The longest digit group the strict [`GameMajorMinor`] parse accepts. A
/// fingerprint arriving from outside the process is persisted, so its size
/// is bounded; real versions have one or two digits per group.
const MAX_VERSION_GROUP_DIGITS: usize = 6;

/// A string [`GameMajorMinor`]'s strict parse rejected: not a `major` or
/// `major.minor` made of 1 to `MAX_VERSION_GROUP_DIGITS` ASCII digits per
/// group.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a game version in major.minor form")]
pub struct GameMajorMinorParseError(String);

impl std::str::FromStr for GameMajorMinor {
    type Err = GameMajorMinorParseError;

    /// Parses exactly what [`GameMajorMinor`]'s `Display` produces
    /// (`"1.6"`, or a bare `"1"`): one or two dot-separated groups of
    /// ASCII digits, each at most `MAX_VERSION_GROUP_DIGITS` long. Unlike
    /// [`GameMajorMinor::parse`], which truncates a full version string leniently, this refuses anything else, so a
    /// fingerprint arriving from outside the process can never be
    /// persisted as a garbage acknowledgement.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let is_digits = |part: &str| {
            (1..=MAX_VERSION_GROUP_DIGITS).contains(&part.len())
                && part.bytes().all(|b| b.is_ascii_digit())
        };
        let mut parts = text.split('.');
        let valid = parts.next().is_some_and(is_digits)
            && parts.next().is_none_or(is_digits)
            && parts.next().is_none();
        if valid {
            Ok(Self(text.to_string()))
        } else {
            Err(GameMajorMinorParseError(text.to_string()))
        }
    }
}

impl fmt::Display for GameMajorMinor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_major_minor_truncates_a_full_version_string() {
        assert_eq!(GameMajorMinor::parse("1.6.4525 rev123").to_string(), "1.6");
    }

    #[test]
    fn game_major_minor_from_str_round_trips_its_own_display() {
        let parsed: GameMajorMinor = "1.6".parse().expect("major.minor parses");
        assert_eq!(parsed.to_string(), "1.6");
        assert_eq!(parsed, GameMajorMinor::parse("1.6.4525 rev123"));
        assert!("7".parse::<GameMajorMinor>().is_ok());
    }

    #[test]
    fn game_major_minor_from_str_rejects_anything_but_major_minor() {
        for garbage in [
            "", "abc", "1.", ".6", "1.6.4525", "1.6 rev1", "1..6", " 1.6", "-1.6",
        ] {
            assert!(
                garbage.parse::<GameMajorMinor>().is_err(),
                "{garbage:?} must be rejected"
            );
        }
    }

    #[test]
    fn game_major_minor_from_str_bounds_the_length_of_each_digit_group() {
        let longest = "9".repeat(MAX_VERSION_GROUP_DIGITS);
        assert!(
            format!("{longest}.{longest}")
                .parse::<GameMajorMinor>()
                .is_ok()
        );

        let too_long = "9".repeat(MAX_VERSION_GROUP_DIGITS + 1);
        assert!(too_long.parse::<GameMajorMinor>().is_err());
        assert!(format!("1.{too_long}").parse::<GameMajorMinor>().is_err());
    }

    #[test]
    fn game_major_minor_ignores_the_build_number() {
        assert_eq!(
            GameMajorMinor::parse("1.6.4525"),
            GameMajorMinor::parse("1.6.9999")
        );
    }

    #[test]
    fn published_rejects_a_pre_release_suffix() {
        let err = AppVersion::published("0.2.0-rc.1").unwrap_err();
        assert!(matches!(err, AppVersionError::NotAStableVersion(_)));
    }

    #[test]
    fn published_rejects_a_build_metadata_suffix() {
        let err = AppVersion::published("0.2.0+build.5").unwrap_err();
        assert!(matches!(err, AppVersionError::NotAStableVersion(_)));
    }

    #[test]
    fn published_rejects_a_string_over_the_length_cap() {
        let too_long = "1.".to_string() + &"0".repeat(40);
        let err = AppVersion::published(&too_long).unwrap_err();
        assert!(matches!(err, AppVersionError::TooLong));
    }

    #[test]
    fn published_accepts_one_leading_v() {
        assert_eq!(
            AppVersion::published("v0.2.0").unwrap(),
            AppVersion::published("0.2.0").unwrap()
        );
    }

    #[test]
    fn a_pre_release_running_version_is_older_than_the_stable_release_it_precedes() {
        let running = AppVersion::running("0.2.0-rc.1").unwrap();
        let published = AppVersion::published("0.2.0").unwrap();
        assert!(running < published);
    }

    #[test]
    fn an_equal_or_older_published_version_is_not_newer_than_running() {
        let running = AppVersion::running("0.2.0").unwrap();
        let same = AppVersion::published("0.2.0").unwrap();
        let older = AppVersion::published("0.1.9").unwrap();
        assert!(!(same > running));
        assert!(!(older > running));
    }

    #[test]
    fn a_newer_published_version_is_newer_than_running() {
        let running = AppVersion::running("0.2.0").unwrap();
        let newer = AppVersion::published("0.3.0").unwrap();
        assert!(newer > running);
    }
}
