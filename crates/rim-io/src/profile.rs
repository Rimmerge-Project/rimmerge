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

/// Canonicalizes an existing config path before hashing. Windows also strips
/// the extended prefix, normalizes separators and folds case. Unix preserves
/// the original bytes, case and literal backslashes. UTF-8 path IDs stay unchanged.
/// If the file does not exist, its original spelling is used.
fn normalize(path: &Path) -> Vec<u8> {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    #[cfg(windows)]
    {
        let text = resolved.to_string_lossy();
        let stripped = text.strip_prefix(EXTENDED_PREFIX).unwrap_or(&text);
        stripped.replace('\\', "/").to_lowercase().into_bytes()
    }
    #[cfg(not(windows))]
    {
        resolved.as_os_str().as_encoded_bytes().to_vec()
    }
}

/// `<base>/profiles/<first 12 hex chars of sha256(normalized config path)>/`.
/// Stable for aliases of the same existing file. Case/separator variants are
/// equivalent only on Windows; Unix paths remain distinct.
#[must_use]
pub fn profile_dir(base: &Path, mods_config: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(normalize(mods_config));
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

/// Resolves a user-supplied directory to an absolute physical location.
/// Existing ancestors are canonicalized even when the destination is new;
/// missing components are appended without creating them. Windows' extended
/// prefix is stripped. No other filesystem errors are treated as absence.
///
/// # Errors
/// Returns filesystem errors, including broken links and inaccessible ancestors.
pub fn resolve_user_dir(path: &Path) -> io::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    match std::fs::symlink_metadata(&absolute) {
        Ok(_) => absolute
            .canonicalize()
            .map(|path| strip_extended_prefix(&path)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let parent = absolute.parent().ok_or(error)?;
            let mut resolved = resolve_user_dir(parent)?;
            match absolute.components().next_back() {
                Some(std::path::Component::Normal(name)) => resolved.push(name),
                Some(std::path::Component::ParentDir) => {
                    resolved.pop();
                }
                _ => return Err(io::Error::other("directory has no final component")),
            }
            // A missing segment followed by `..` can lead back to an existing alias.
            match std::fs::symlink_metadata(&resolved) {
                Ok(_) => resolved
                    .canonicalize()
                    .map(|path| strip_extended_prefix(&path)),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(resolved),
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

/// A destination that cannot safely be used for exporting a project.
#[derive(Debug, thiserror::Error)]
pub enum ExportDirError {
    /// Only the explicit install operation may write into Mods.
    #[error("{0} is inside the game's Mods folder; use install instead")]
    InsideMods(PathBuf),
    /// Resolving either location failed before any export writes.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Resolves an export destination and refuses the game's Mods directory and
/// descendants, including symlink/junction aliases and relative game paths.
///
/// # Errors
/// Returns [`ExportDirError::InsideMods`] for protected locations, or an I/O
/// error when either the destination or Mods cannot be resolved safely.
pub fn resolve_export_dir(out_dir: &Path, game_dir: &Path) -> Result<PathBuf, ExportDirError> {
    let resolved = resolve_user_dir(out_dir)?;
    let mods = std::path::absolute(game_dir.join("Mods"))?;
    let physical_mods = resolve_user_dir(&mods)?;
    if path_is_inside(&resolved, &mods) || path_is_inside(&resolved, &physical_mods) {
        return Err(ExportDirError::InsideMods(resolved));
    }
    Ok(resolved)
}

fn path_is_inside(path: &Path, ancestor: &Path) -> bool {
    #[cfg(windows)]
    {
        let key = |path: &Path| {
            PathBuf::from(
                strip_extended_prefix(path)
                    .to_string_lossy()
                    .replace('\\', "/")
                    .to_lowercase(),
            )
        };
        key(path).starts_with(key(ancestor))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(ancestor)
    }
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
    #[cfg(windows)]
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
    #[cfg(windows)]
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

    #[cfg(unix)]
    #[test]
    fn unix_profiles_distinguish_case_and_literal_backslashes() {
        let dir = tempdir().expect("scratch");
        let upper = dir.path().join("ModsConfig.xml");
        let lower = dir.path().join("modsconfig.xml");
        for file in [&upper, &lower] {
            std::fs::write(file, b"<ModsConfigData/>").expect("config");
        }
        let base = dir.path().join("profiles-base");
        assert_ne!(profile_dir(&base, &upper), profile_dir(&base, &lower));
        assert_ne!(
            profile_dir(&base, Path::new("Folder/ModsConfig.xml")),
            profile_dir(&base, Path::new("folder/ModsConfig.xml"))
        );
        assert_ne!(
            profile_dir(&base, Path::new(r"Folder\ModsConfig.xml")),
            profile_dir(&base, Path::new("Folder/ModsConfig.xml"))
        );
        let alias = dir.path().join("config-alias");
        std::os::unix::fs::symlink(&upper, &alias).expect("alias");
        assert_eq!(profile_dir(&base, &upper), profile_dir(&base, &alias));
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
    #[cfg(unix)]
    #[test]
    fn profiles_preserve_non_utf8_bytes_and_existing_utf8_identifiers() {
        use std::os::unix::ffi::OsStringExt;
        let scratch = tempdir().expect("scratch");
        let base = scratch.path().join("base");
        let configs: Vec<_> = [0xfe, 0xff]
            .into_iter()
            .map(|byte| {
                let config = scratch
                    .path()
                    .join(std::ffi::OsString::from_vec(vec![byte]));
                std::fs::write(&config, b"<ModsConfigData/>").expect("config");
                config
            })
            .collect();
        assert_ne!(
            profile_dir(&base, &configs[0]),
            profile_dir(&base, &configs[1])
        );
        let alias = scratch.path().join("alias");
        std::os::unix::fs::symlink(&configs[0], &alias).expect("alias");
        assert_eq!(profile_dir(&base, &configs[0]), profile_dir(&base, &alias));
        let utf8 = scratch.path().join("ModsConfig.xml");
        std::fs::write(&utf8, b"<ModsConfigData/>").expect("UTF-8 config");
        for path in [&utf8, Path::new("profile-utf8-stability/ModsConfig.xml")] {
            let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let previous: String = Sha256::digest(resolved.to_string_lossy().as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            assert_eq!(
                profile_dir(&base, path),
                base.join("profiles").join(&previous[..12])
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn new_directory_under_a_symlink_resolves_its_existing_ancestor() {
        let scratch = tempdir().expect("scratch");
        let target = scratch.path().join("target");
        std::fs::create_dir(&target).expect("target");
        let alias = scratch.path().join("alias");
        std::os::unix::fs::symlink(&target, &alias).expect("alias");
        assert_eq!(
            resolve_user_dir(&alias.join("new/nested")).expect("resolved"),
            target.join("new/nested")
        );
        assert!(!target.join("new").exists());
    }

    #[cfg(unix)]
    #[test]
    fn broken_links_and_non_directory_ancestors_are_errors() {
        let scratch = tempdir().expect("scratch");
        let broken = scratch.path().join("broken");
        std::os::unix::fs::symlink(scratch.path().join("absent"), &broken).expect("broken link");
        assert!(resolve_user_dir(&broken).is_err());
        assert!(resolve_user_dir(&broken.join("new")).is_err());
        let file = scratch.path().join("file");
        std::fs::write(&file, b"file").expect("file");
        assert!(resolve_user_dir(&file.join("new")).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn export_guard_checks_new_destinations_aliases_and_linked_mods() {
        use std::os::unix::fs::symlink;
        let scratch = tempdir().expect("scratch");
        let game = scratch.path().join("game");
        let mods = game.join("Mods");
        std::fs::create_dir_all(&mods).expect("Mods");
        let alias = scratch.path().join("alias");
        symlink(&mods, &alias).expect("alias");
        for out in [
            mods.clone(),
            mods.join("new/nested"),
            alias.join("new/nested"),
        ] {
            assert!(matches!(
                resolve_export_dir(&out, &game),
                Err(ExportDirError::InsideMods(_))
            ));
        }
        let outside = scratch.path().join("Mods-other/new");
        assert_eq!(
            resolve_export_dir(&outside, &game).expect("outside allowed"),
            outside
        );
        let target = scratch.path().join("external-mods");
        std::fs::rename(&mods, &target).expect("move Mods");
        symlink(&target, &mods).expect("linked Mods");
        assert!(matches!(
            resolve_export_dir(&target.join("new"), &game),
            Err(ExportDirError::InsideMods(_))
        ));
        assert!(!target.join("new").exists());
        std::fs::remove_file(&mods).expect("remove link");
        symlink(scratch.path().join("absent"), &mods).expect("broken Mods");
        assert!(matches!(
            resolve_export_dir(&outside, &game),
            Err(ExportDirError::Io(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn missing_segments_with_parent_traversal_cannot_hide_a_later_alias() {
        let scratch = tempdir().expect("scratch");
        let target = scratch.path().join("target");
        std::fs::create_dir(&target).expect("target");
        std::os::unix::fs::symlink(&target, scratch.path().join("alias")).expect("alias");
        let path = scratch.path().join("missing/../alias/new");
        assert_eq!(
            resolve_user_dir(&path).expect("resolved traversal"),
            target.join("new")
        );
    }

    #[cfg(unix)]
    #[test]
    fn inaccessible_ancestors_are_not_treated_as_missing() {
        use std::os::unix::fs::PermissionsExt;
        let scratch = tempdir().expect("scratch");
        let denied = scratch.path().join("denied");
        std::fs::create_dir(&denied).expect("denied directory");
        std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o000))
            .expect("deny access");
        let privileged = std::fs::read_dir(&denied).is_ok();
        let result = resolve_user_dir(&denied.join("new/nested"));
        std::fs::set_permissions(&denied, std::fs::Permissions::from_mode(0o700))
            .expect("restore permissions");
        if privileged {
            eprintln!("permission denial cannot be exercised by a privileged test process");
            return;
        }
        assert_eq!(
            result.expect_err("must propagate access failure").kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[cfg(windows)]
    #[test]
    fn export_comparison_keeps_windows_case_and_separator_rules() {
        assert!(path_is_inside(
            Path::new(r"C:\GAME\mods\new"),
            Path::new("c:/game/Mods")
        ));
        assert!(path_is_inside(
            Path::new(r"\\?\C:\GAME\Mods"),
            Path::new("c:/game/Mods")
        ));
        assert!(!path_is_inside(
            Path::new("c:/game/Mods-other"),
            Path::new("c:/game/Mods")
        ));
    }
}
