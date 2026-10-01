//! [`profile_dir`]: derives a stable, per-`ModsConfig.xml` profile
//! directory from that path's hash. [`resolve_user_dir`]: the shared
//! normalization both interfaces (`apps/cli`, `apps/desktop/src-tauri`)
//! apply to a user-supplied directory before it reaches a use case.

use std::io;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Windows' extended-length path prefix, stripped before hashing so a
/// canonicalized (`\\?\C:\...`) and a plain (`C:\...`) spelling of the
/// same path hash identically.
const EXTENDED_PREFIX: &str = r"\\?\";

/// Normalizes `path` into the text [`profile_dir`] hashes: canonicalized
/// when it exists on disk (resolving `.`/`..` and symlinks), with the
/// Windows extended-length prefix stripped, backslashes turned to
/// forward slashes, and lowercased — so two spellings of the same
/// `ModsConfig.xml` (different case, different slash direction, a
/// not-yet-canonical relative path) always hash to the same profile.
/// Falling back to `path` unchanged (still normalized the same way) when
/// canonicalization fails (the path doesn't exist yet) is a best effort:
/// it can't undo `.`/`..`/symlinks it can't see, but it does at least
/// unify case and slash direction.
fn normalize(path: &Path) -> String {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let text = resolved.to_string_lossy();
    let stripped = text.strip_prefix(EXTENDED_PREFIX).unwrap_or(&text);
    let forward_slashes = stripped.replace('\\', "/");
    forward_slashes.to_lowercase()
}

/// `<base>/profiles/<first 12 hex chars of sha256(normalized mods_config
/// path)>/` — stable for a given `mods_config` path regardless of case,
/// slash direction, or `\\?\`-prefixed canonical spelling (so re-running
/// against the same install always finds the same profile), and distinct
/// across installs.
#[must_use]
pub fn profile_dir(base: &Path, mods_config: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(normalize(mods_config).as_bytes());
    let digest = hasher.finalize();
    let hex: String = digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    base.join("profiles").join(hex)
}

/// `<base>/databases/` — the app-global cache `GithubRuleDatabaseFetcher`
/// fetches `communityRules.json`/`steamDB.json` into. Deliberately sibling to
/// `profiles/`, not inside any one profile: the two rule databases are shared
/// across every `ModsConfig.xml` this `base` directory ever sees, so
/// downloading them again per profile hash would be absurd. Resolved by the
/// same `%LOCALAPPDATA%\rimmerge` `base` both interfaces already compute for
/// [`profile_dir`].
#[must_use]
pub fn databases_dir(base: &Path) -> PathBuf {
    base.join("databases")
}

/// Strips [`EXTENDED_PREFIX`] from `path` if present, leaving every other
/// component untouched (unlike [`normalize`], this never lowercases or
/// changes slash direction — the result is a real path callers go on to
/// use, not a hash key).
fn strip_extended_prefix(path: &Path) -> PathBuf {
    match path.to_string_lossy().strip_prefix(EXTENDED_PREFIX) {
        Some(stripped) => PathBuf::from(stripped),
        None => path.to_path_buf(),
    }
}

/// Resolves a user-supplied directory (a CLI `--out` flag, a Tauri
/// folder-picker result, or a hand-typed path) to what a writer should
/// actually use: first absolutized against the process's current
/// directory ([`std::path::absolute`] — a pure computation, no
/// filesystem access, so a relative path always has a well-defined
/// location), then, only when that path already exists on disk,
/// canonicalized to resolve `.`/`..` segments, symlinks, and junctions,
/// with Windows' extended-length `\\?\` prefix stripped back off (the
/// same normalization [`profile_dir`]'s own [`normalize`] applies before
/// hashing). A folder that doesn't exist yet is returned merely
/// absolutized — there is nothing on disk yet to canonicalize, and the
/// caller (an exporter) is the one about to create it.
///
/// Both `apps/cli` and `apps/desktop/src-tauri` must call this on every
/// user-supplied directory before it reaches `rim_session`'s use cases:
/// those use cases do no filesystem I/O of their own (see
/// `rim-session`'s `CLAUDE.md`) and so can only check a path
/// *lexically* — a symlink, junction, or 8.3 short name that lexically
/// looks clear of the game's `Mods` folder but resolves inside it on
/// disk would otherwise slip past `ExportPatch`'s own refusal.
///
/// # Errors
///
/// Returns an error when absolutizing or canonicalizing fails (e.g. an
/// empty path, or a permissions error reading an existing directory).
pub fn resolve_user_dir(path: &Path) -> io::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    if !absolute.exists() {
        return Ok(absolute);
    }
    let canonical = absolute.canonicalize()?;
    Ok(strip_extended_prefix(&canonical))
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn is_stable_for_the_same_path() {
        let base = Path::new("base");
        let mods_config = Path::new("C:/Users/x/ModsConfig.xml");
        assert_eq!(
            profile_dir(base, mods_config),
            profile_dir(base, mods_config)
        );
    }

    #[test]
    fn differs_across_paths() {
        let base = Path::new("base");
        assert_ne!(
            profile_dir(base, Path::new("a/ModsConfig.xml")),
            profile_dir(base, Path::new("b/ModsConfig.xml"))
        );
    }

    #[test]
    fn hash_segment_is_12_lowercase_hex_chars_under_base_profiles() {
        let dir = profile_dir(Path::new("base"), Path::new("ModsConfig.xml"));
        let hash_segment = dir
            .file_name()
            .and_then(|n| n.to_str())
            .expect("must have a final segment");
        assert_eq!(hash_segment.len(), 12);
        assert!(
            hash_segment
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
        assert!(dir.starts_with(Path::new("base").join("profiles")));
    }

    /// Case and slash-direction variants of a path that doesn't exist on
    /// disk can't be resolved by `canonicalize`, so this exercises the
    /// fallback normalization (lowercase + forward slashes) on its own.
    #[test]
    fn case_and_slash_variants_of_a_nonexistent_path_map_to_one_profile() {
        let base = Path::new("base");
        let backslash_upper = Path::new(r"C:\Users\X\ModsConfig.xml");
        let forward_lower = Path::new("c:/users/x/modsconfig.xml");

        assert_eq!(
            profile_dir(base, backslash_upper),
            profile_dir(base, forward_lower)
        );
    }

    /// For a path that *does* exist, `canonicalize` itself already
    /// resolves whatever case/slash spelling was given down to one
    /// on-disk form, so a case/slash variant hashes identically even
    /// before the fallback normalization ever runs.
    #[test]
    fn case_and_slash_variants_of_an_existing_path_map_to_one_profile() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("ModsConfig.xml");
        std::fs::write(&file, b"<ModsConfigData/>").expect("seed file");

        let lower_name = file
            .to_string_lossy()
            .to_lowercase()
            .replace('\\', "/")
            .replace('/', "\\");
        let variant = PathBuf::from(lower_name);

        assert_eq!(
            profile_dir(Path::new("base"), &file),
            profile_dir(Path::new("base"), &variant)
        );
    }

    // -- databases_dir --------------------------------------------------

    #[test]
    fn databases_dir_is_a_stable_sibling_of_profiles() {
        let base = Path::new("base");

        assert_eq!(databases_dir(base), base.join("databases"));
        assert_ne!(
            databases_dir(base),
            profile_dir(base, Path::new("ModsConfig.xml")),
            "the global cache must never collide with a per-profile directory"
        );
    }

    // -- resolve_user_dir ----------------------------------------------

    #[test]
    fn resolve_user_dir_absolutizes_a_relative_path() {
        let relative = Path::new("some_relative_export_dir_that_does_not_exist");

        let resolved = resolve_user_dir(relative).expect("must resolve");

        assert!(
            resolved.is_absolute(),
            "a relative path must come back absolute: {resolved:?}"
        );
    }

    #[test]
    fn resolve_user_dir_does_not_fail_for_a_not_yet_existing_directory() {
        let dir = tempdir().expect("tempdir");
        let brand_new = dir.path().join("brand_new_export");
        assert!(!brand_new.exists());

        let resolved = resolve_user_dir(&brand_new).expect("must not error on a missing folder");

        assert_eq!(
            resolved, brand_new,
            "an already-absolute, not-yet-existing path is returned unchanged"
        );
    }

    /// An existing directory reached through a `.`/`..`-laden spelling
    /// canonicalizes down to its real path — the behavior `ExportPatch`'s
    /// own lexical `is_inside` check can't provide on its own (see that
    /// use case's doc comment).
    #[test]
    fn resolve_user_dir_canonicalizes_an_existing_directory() {
        let dir = tempdir().expect("tempdir");
        let sub_dir = dir.path().join("sub");
        std::fs::create_dir(&sub_dir).expect("create sub dir");
        let messy = dir.path().join("sub").join("..").join("sub");

        let resolved = resolve_user_dir(&messy).expect("must resolve");

        assert_eq!(resolved, resolve_user_dir(&sub_dir).expect("resolve plain"));
        assert!(
            !resolved.to_string_lossy().contains(".."),
            "the `..` segment must be resolved away: {resolved:?}"
        );
    }

    #[test]
    fn resolve_user_dir_strips_the_extended_length_prefix() {
        let dir = tempdir().expect("tempdir");

        let resolved = resolve_user_dir(dir.path()).expect("must resolve");

        assert!(
            !resolved.to_string_lossy().starts_with(EXTENDED_PREFIX),
            "the \\\\?\\ prefix must be stripped: {resolved:?}"
        );
    }
}
