//! [`write_atomically`]: writes bytes to a file via a temp file next to
//! it, then an atomic rename — so a failure partway through a write (a
//! full disk, a permission error, a crash) never leaves the target
//! half-written. Used by every writer in this crate: `ModsConfig.xml`
//! (and its pre-write backup copy), `decisions.json`, `rules.json`, and
//! the RimSort import snapshots.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Writes `bytes` to `path` atomically: creates a sibling temp file,
/// writes and flushes it, then renames it over `path` (an atomic replace
/// on both Windows and POSIX). If creating or writing the temp file
/// fails, `path` itself is never touched.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let temp_path = temp_path_for(path);
    {
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&temp_path, path)?;
    Ok(())
}

/// Whether [`write_if_absent`] created the file or found one already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CreateOutcome {
    /// The file did not exist and now holds the bytes.
    Created,
    /// A file was already there (whoever wrote it, valid or not); it is
    /// untouched.
    AlreadyExisted,
}

/// Creates `path` with `bytes` only if nothing exists there, in one
/// operation (`create_new`, `O_EXCL` / `CREATE_NEW`): two callers racing
/// to create the same file cannot both succeed, and the loser never
/// replaces or truncates the winner's. Unlike [`write_atomically`] this
/// writes the final path directly, so a crash between create and write can
/// leave a short file; callers use it only for a small file whose loader
/// treats an unreadable one as a safe, repairable state.
pub(crate) fn write_if_absent(path: &Path, bytes: &[u8]) -> std::io::Result<CreateOutcome> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Ok(CreateOutcome::AlreadyExisted);
        }
        Err(error) => return Err(error),
    };
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(CreateOutcome::Created)
}

fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    path.with_file_name(format!("{file_name}.tmp-{}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn writes_bytes_and_creates_parent_directories() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("file.txt");

        write_atomically(&path, b"hello").expect("write must succeed");

        assert_eq!(fs::read(&path).expect("read"), b"hello");
    }

    #[test]
    fn overwrites_existing_content() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("file.txt");
        fs::write(&path, b"old").expect("seed");

        write_atomically(&path, b"new").expect("write must succeed");

        assert_eq!(fs::read(&path).expect("read"), b"new");
    }

    #[test]
    fn a_failed_write_leaves_the_original_file_intact() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("target.txt");
        fs::write(&path, b"original").expect("seed original");

        // Force the temp-file creation to fail deterministically: a
        // directory already occupies exactly the path `write_atomically`
        // will try to create a file at, so `File::create` fails on both
        // Windows and POSIX without relying on ACLs/permissions.
        let blocking_temp = temp_path_for(&path);
        fs::create_dir(&blocking_temp).expect("create blocking dir");

        let result = write_atomically(&path, b"new content");

        assert!(result.is_err(), "the simulated failure must surface");
        assert_eq!(
            fs::read(&path).expect("original must still be readable"),
            b"original",
            "a failed write must never touch the original file"
        );
    }

    #[test]
    fn write_if_absent_creates_a_missing_file_and_its_parents() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("file.txt");

        let outcome = write_if_absent(&path, b"first").expect("create");

        assert_eq!(outcome, CreateOutcome::Created);
        assert_eq!(fs::read(&path).expect("read"), b"first");
    }

    #[test]
    fn write_if_absent_never_replaces_an_existing_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("file.txt");
        fs::write(&path, b"winner").expect("seed");

        let outcome = write_if_absent(&path, b"loser").expect("losing is not an error");

        assert_eq!(outcome, CreateOutcome::AlreadyExisted);
        assert_eq!(fs::read(&path).expect("read"), b"winner");
    }

    #[test]
    fn no_leftover_temp_file_remains_after_a_successful_write() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("file.txt");

        write_atomically(&path, b"content").expect("write must succeed");

        assert!(!temp_path_for(&path).exists());
    }
}
