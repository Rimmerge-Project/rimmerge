//! A patch's identity: its id, and the generated mod's package id, name, and author.

use std::fmt;
use std::str::FromStr;

use rim_analyzer::domain::ModId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::merge::GeneratedModIdentity;

/// RimWorld's own Core package id (see `ModMetaData.PackageIdFormatRegex`
/// for the format) — Core is never "outside" a patch
/// scope: it is always active and always the implicit base of any
/// [`FindingKey::DefOverride`](crate::domain::finding::FindingKey::DefOverride)/[`FindingKey::PatchCollision`](crate::domain::finding::FindingKey::PatchCollision) it owns (see
/// [`PatchScope::membership`](crate::domain::patch::scope::PatchScope::membership)). A DLC id (e.g.
/// `ludeon.rimworld.royalty`) is a different, unrelated string and is
/// therefore treated as an ordinary outside mod unless the user puts it in
/// scope.
pub(super) const CORE_MOD_ID: &str = "ludeon.rimworld";

/// The `packageId` prefix reserved for the profile merge mod
/// ([`GeneratedModIdentity::for_profile`]) — a compat patch may never claim
/// it, so the two kinds of generated mod can never collide.
const RESERVED_PACKAGE_PREFIX: &str = "rimmerge.merge.";

/// The longest `packageId` RimWorld's own validator
/// (`ModMetaData.PackageIdFormatRegex`) accepts.
const MAX_PACKAGE_ID_LEN: usize = 60;

/// The longest display name [`PatchModIdentity::new`] accepts. Not part of
/// RimWorld's own format rules — a policy limit keeping the identity form
/// and the rendered `About.xml` reasonable.
const MAX_DISPLAY_NAME_LEN: usize = 100;

/// The default `About.xml` author for a newly created patch: a fixed
/// constant rather than an OS user-name lookup, since this crate does no
/// IO. `rim-session`'s `UpdatePatch` is the one edit needed to change it.
pub(super) const DEFAULT_PATCH_AUTHOR: &str = "Rimmerge";

/// The stable identity of one [`PatchProject`](crate::domain::patch::project::PatchProject): `sha256(profile_hash ‖
/// package_id ‖ created_at)[..12]`, the same "12 hex chars of a sha256
/// digest" shape `rim_io::profile_dir` already uses for a profile
/// directory. Stable for the project's whole life (it's the file name
/// under `<profile>/patches/`) — a later package-id edit through
/// [`PatchProject::set_identity`](crate::domain::patch::project::PatchProject::set_identity) never changes it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PatchId(String);

/// [`PatchId`]'s canonical text form failed to parse: not exactly 12
/// lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid patch id {0:?}: expected exactly 12 lowercase hex characters")]
pub struct PatchIdParseError(String);

impl PatchId {
    /// Derives a new patch's id. Unique per creation (the timestamp and the
    /// user-chosen package id both feed the hash) and independent of a
    /// later package-id edit, since only the *creation-time* package id and
    /// timestamp are hashed.
    #[must_use]
    pub fn derive(profile_hash: &str, package_id: &ModId, created_at: jiff::Timestamp) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(profile_hash.as_bytes());
        hasher.update(package_id.as_str().as_bytes());
        hasher.update(created_at.to_string().as_bytes());
        let digest = hasher.finalize();
        let hex: String = digest
            .iter()
            .take(6)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Self(hex)
    }

    /// The raw 12-hex-char text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PatchId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for PatchId {
    type Err = PatchIdParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let is_lowercase_hex = text.len() == 12
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'));
        if is_lowercase_hex {
            Ok(Self(text.to_string()))
        } else {
            Err(PatchIdParseError(text.to_string()))
        }
    }
}

impl TryFrom<String> for PatchId {
    type Error = PatchIdParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<PatchId> for String {
    fn from(value: PatchId) -> Self {
        value.0
    }
}

/// Why [`PatchModIdentity::new`] rejected a proposed identity.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatchIdentityError {
    /// The package id fails RimWorld's own format rule:
    /// wrong length, an illegal character, or a leading/trailing/doubled
    /// `.`. The `&'static str` names which rule.
    #[error("package id {0:?}: {1}")]
    PackageIdFormat(String, &'static str),
    /// The package id has no `.` at all — this crate's own stricter policy
    /// (an author segment), not a RimWorld fact.
    #[error("package id {0:?} needs an author segment (`author.name`)")]
    MissingAuthorSegment(String),
    /// The package id uses the prefix reserved for the profile merge mod.
    #[error("package id {0:?} uses the prefix reserved for the profile merge mod")]
    ReservedPrefix(String),
    /// The display name is empty (after trimming).
    #[error("display name must not be empty")]
    EmptyDisplayName,
    /// The display name is over [`MAX_DISPLAY_NAME_LEN`] characters.
    #[error("display name must be at most {MAX_DISPLAY_NAME_LEN} characters")]
    DisplayNameTooLong,
}

/// Validates `raw`'s format against RimWorld's own rule alone (length, character
/// set, leading/trailing/doubled `.`) — never the author-segment or
/// reserved-prefix rules, which are this crate's own policy and get their
/// own [`PatchIdentityError`] variants.
fn validate_package_id_format(raw: &str) -> Result<(), PatchIdentityError> {
    if raw.is_empty() || raw.len() > MAX_PACKAGE_ID_LEN {
        return Err(PatchIdentityError::PackageIdFormat(
            raw.to_string(),
            "must be 1-60 characters",
        ));
    }
    if !raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
        return Err(PatchIdentityError::PackageIdFormat(
            raw.to_string(),
            "must contain only letters, digits, and `.`",
        ));
    }
    if raw.starts_with('.') || raw.ends_with('.') {
        return Err(PatchIdentityError::PackageIdFormat(
            raw.to_string(),
            "must not start or end with `.`",
        ));
    }
    if raw.contains("..") {
        return Err(PatchIdentityError::PackageIdFormat(
            raw.to_string(),
            "must not contain a doubled `.`",
        ));
    }
    Ok(())
}

/// A compat patch's user-chosen identity, validated at construction
/// (RimWorld's package id format plus this crate's own rules) — a
/// constructed value is always a valid
/// value: every field is private, so the only way to get a
/// [`PatchModIdentity`] at all is through [`PatchModIdentity::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchModIdentity {
    package_id: ModId,
    folder_name: String,
    display_name: String,
}

impl PatchModIdentity {
    /// Validates and builds a patch identity.
    ///
    /// `package_id`: 1-60 chars of `[A-Za-z0-9.]`, no leading/trailing/
    /// doubled `.`, at least one `.` (an author segment), not starting with
    /// `rimmerge.merge.` (reserved for the profile merge mod). Lowercased
    /// through [`ModId::new`]. `display_name`: non-empty after trimming,
    /// at most [`MAX_DISPLAY_NAME_LEN`] characters.
    ///
    /// `folder_name` is derived, never user-chosen:
    /// the package id with every `.` replaced by `_` (`sample.abcompat` ->
    /// `sample_abcompat`) — valid on every filesystem, short, and
    /// recognisably the id.
    ///
    /// # Errors
    ///
    /// Returns the [`PatchIdentityError`] variant naming the first rule
    /// `package_id`/`display_name` breaks, in the order documented on
    /// [`PatchIdentityError`] itself.
    pub fn new(package_id: &str, display_name: &str) -> Result<Self, PatchIdentityError> {
        let raw = package_id.trim();
        validate_package_id_format(raw)?;
        if !raw.contains('.') {
            return Err(PatchIdentityError::MissingAuthorSegment(raw.to_string()));
        }
        let package_id = ModId::new(raw);
        if package_id.as_str().starts_with(RESERVED_PACKAGE_PREFIX) {
            return Err(PatchIdentityError::ReservedPrefix(raw.to_string()));
        }

        let display_name = display_name.trim();
        if display_name.is_empty() {
            return Err(PatchIdentityError::EmptyDisplayName);
        }
        if display_name.chars().count() > MAX_DISPLAY_NAME_LEN {
            return Err(PatchIdentityError::DisplayNameTooLong);
        }

        let folder_name = package_id.as_str().replace('.', "_");
        Ok(Self {
            package_id,
            folder_name,
            display_name: display_name.to_string(),
        })
    }

    /// The validated, lowercased package id.
    #[must_use]
    pub fn package_id(&self) -> &ModId {
        &self.package_id
    }

    /// The derived folder name.
    #[must_use]
    pub fn folder_name(&self) -> &str {
        &self.folder_name
    }

    /// The display name shown in `About.xml` and every UI surface.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// The plain-data [`super::merge::GeneratedModIdentity`](crate::domain::merge::GeneratedModIdentity)
    /// `rim_merge::emit` actually renders — a compat patch's identity is
    /// user-chosen rather than hash-derived, but this is the same
    /// three-field shape either way.
    #[must_use]
    pub fn as_generated(&self) -> GeneratedModIdentity {
        GeneratedModIdentity {
            package_id: self.package_id.clone(),
            folder_name: self.folder_name.clone(),
            display_name: self.display_name.clone(),
        }
    }
}
