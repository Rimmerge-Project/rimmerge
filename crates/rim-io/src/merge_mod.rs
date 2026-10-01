//! [`MergeModFolderWriter`]: the atomic swap that installs the generated
//! merge mod into the game's `Mods/` folder:
//!
//! 1. Clear any stale `<mods_dir>/.rimmerge_tmp_<pid>/` left behind by an
//!    earlier write that died before its own step-4 cleanup ran, then
//!    render fresh into it, writing `About.xml` last — the tmp dir has no
//!    `About/About.xml` until the very end, so the game never sees a
//!    half-written folder even mid-render.
//! 2. If `<mods_dir>/<folder>` already exists: replace `<backup_dir>`
//!    (delete it, then recursively copy the current generation into it —
//!    a rename isn't an option since the profile directory is usually on
//!    another volume), then `remove_dir_all` the old folder.
//! 3. `fs::rename` the tmp dir into place — atomic on the same volume.
//! 4. On any failure at any step: remove the tmp dir and return a
//!    [`MergeModError`] naming the step; whatever the second step already reached
//!    is left as-is (a partially-replaced backup is recoverable by hand,
//!    and the old folder is never removed until the backup copy above it
//!    has already succeeded).

use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::GeneratedMarker;
use rim_analyzer::extract::rimmerge_marker;
use rim_merge::emit::{FileContent, RenderedFile, RenderedMod};
use rim_session::ports::{MergeModError, MergeModWriteReport, MergeModWriter};

use crate::atomic::write_atomically;

/// The file `render_into` always writes last, so its presence is what
/// makes a rendered folder "complete" from the game's point of view.
const ABOUT_XML: &str = "About/About.xml";

/// The marker file [`MergeModWriter::read_marker`] reads back.
const MARKER_FILE: &str = "rimmerge.json";

/// `rimmerge.json` is a handful of short fields; anything bigger than this
/// was not written by Rimmerge and is read as no marker at all — a matching
/// copy of the same reasoning `FileDefSourceReader`'s own `MAX_READ_BYTES`
/// documents (`rim_analyzer::infra::mod_scan::MAX_READ_BYTES` is
/// `pub(crate)` to that crate, so this is a separate, deliberately smaller
/// limit rather than a reused one).
const MAX_MARKER_BYTES: u64 = 1024 * 1024;

/// Installs the generated merge mod into `<mods_dir>/<folder_name>`
/// atomically, keeping exactly one previous generation under a caller-given
/// backup directory.
#[derive(Debug, Default, Clone, Copy)]
pub struct MergeModFolderWriter;

impl MergeModFolderWriter {
    /// Builds the writer. Stateless: every call works from the paths
    /// given to it.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

fn tmp_dir(mods_dir: &Path) -> PathBuf {
    mods_dir.join(format!(".rimmerge_tmp_{}", std::process::id()))
}

fn step_error(step: &str, error: std::io::Error) -> MergeModError {
    MergeModError(format!("{step}: {error}"))
}

/// Recursively copies every file under `from` into `to`, creating
/// directories as needed.
fn copy_dir_recursive(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

fn write_file(tmp: &Path, file: &RenderedFile) -> Result<(), MergeModError> {
    let dest = tmp.join(&file.relative_path);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|error| step_error("creating a directory", error))?;
    }
    match &file.content {
        FileContent::Text(text) => write_atomically(&dest, text.as_bytes())
            .map_err(|error| step_error("writing a file", error)),
        FileContent::CopyFrom(source) => fs::copy(source, &dest)
            .map(|_| ())
            .map_err(|error| step_error("copying an asset", error)),
    }
}

/// Renders every file of `rendered` into `tmp`, `About/About.xml` last.
fn render_into(tmp: &Path, rendered: &RenderedMod) -> Result<(), MergeModError> {
    fs::create_dir_all(tmp).map_err(|error| step_error("creating the temp directory", error))?;
    let (about, rest): (Vec<&RenderedFile>, Vec<&RenderedFile>) = rendered
        .files
        .iter()
        .partition(|file| file.relative_path == Path::new(ABOUT_XML));
    for file in rest.into_iter().chain(about) {
        write_file(tmp, file)?;
    }
    Ok(())
}

impl MergeModWriter for MergeModFolderWriter {
    fn write(
        &self,
        mods_dir: &Path,
        backup_dir: &Path,
        rendered: &RenderedMod,
    ) -> Result<MergeModWriteReport, MergeModError> {
        let tmp = tmp_dir(mods_dir);
        let final_dir = mods_dir.join(&rendered.folder_name);

        // A stale `.rimmerge_tmp_<pid>` can be left behind by an earlier
        // `write` for this same `mods_dir` that died before its own
        // cleanup ran (a panic skips every `let _ =
        // fs::remove_dir_all(&tmp)` guard below). `render_into`'s own
        // `fs::create_dir_all(tmp)` happily writes into an
        // already-existing directory, so without this, leftover files
        // from that earlier generation would silently survive into the
        // new one instead of being overwritten or absent.
        if let Err(error) = fs::remove_dir_all(&tmp)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            return Err(step_error("removing a stale temp directory", error));
        }

        if let Err(error) = render_into(&tmp, rendered) {
            let _ = fs::remove_dir_all(&tmp);
            return Err(error);
        }

        let mut backup_path = None;
        if final_dir.is_dir() {
            if let Err(error) = fs::remove_dir_all(backup_dir)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                let _ = fs::remove_dir_all(&tmp);
                return Err(step_error("removing the previous backup", error));
            }
            if let Err(error) = copy_dir_recursive(&final_dir, backup_dir) {
                let _ = fs::remove_dir_all(&tmp);
                return Err(step_error("backing up the previous generation", error));
            }
            backup_path = Some(backup_dir.to_path_buf());
            if let Err(error) = fs::remove_dir_all(&final_dir) {
                let _ = fs::remove_dir_all(&tmp);
                return Err(step_error("removing the previous generation", error));
            }
        }

        if let Err(error) = fs::rename(&tmp, &final_dir) {
            let _ = fs::remove_dir_all(&tmp);
            return Err(step_error("installing the new generation", error));
        }

        Ok(MergeModWriteReport {
            mod_path: final_dir,
            backup_path,
        })
    }

    fn remove(
        &self,
        mods_dir: &Path,
        _backup_dir: &Path,
        folder_name: &str,
    ) -> Result<Option<PathBuf>, MergeModError> {
        let final_dir = mods_dir.join(folder_name);
        if !final_dir.is_dir() {
            return Ok(None);
        }
        fs::remove_dir_all(&final_dir)
            .map_err(|error| step_error("removing the merge mod", error))?;
        Ok(Some(final_dir))
    }

    fn exists(&self, mods_dir: &Path, folder_name: &str) -> bool {
        mods_dir.join(folder_name).is_dir()
    }

    fn read_marker(
        &self,
        dir: &Path,
        folder_name: &str,
    ) -> Result<Option<GeneratedMarker>, MergeModError> {
        let marker_path = dir.join(folder_name).join(MARKER_FILE);
        let metadata = match fs::metadata(&marker_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(step_error("reading the rimmerge.json marker", error)),
        };
        if metadata.len() > MAX_MARKER_BYTES {
            return Ok(None);
        }
        let bytes = match fs::read(&marker_path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(step_error("reading the rimmerge.json marker", error)),
        };
        Ok(rimmerge_marker::parse(&bytes))
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn rendered(folder_name: &str, about_text: &str) -> RenderedMod {
        RenderedMod {
            folder_name: folder_name.to_string(),
            files: vec![
                RenderedFile {
                    relative_path: PathBuf::from("About/About.xml"),
                    content: FileContent::Text(about_text.to_string()),
                },
                RenderedFile {
                    relative_path: PathBuf::from("Patches/rimmerge_ThingDef.xml"),
                    content: FileContent::Text("<Patch></Patch>".to_string()),
                },
            ],
        }
    }

    #[test]
    fn a_fresh_write_creates_the_folder_and_leaves_no_tmp_dir() {
        let dir = tempdir().expect("tempdir");
        let mods_dir = dir.path().join("Mods");
        let backup_dir = dir.path().join("profile").join("merge-mod.prev");
        let writer = MergeModFolderWriter::new();

        let report = writer
            .write(
                &mods_dir,
                &backup_dir,
                &rendered("rimmerge_merge_abc", "v1"),
            )
            .expect("write must succeed");

        assert_eq!(report.mod_path, mods_dir.join("rimmerge_merge_abc"));
        assert!(report.backup_path.is_none());
        assert!(report.mod_path.join("About/About.xml").is_file());
        assert!(
            report
                .mod_path
                .join("Patches/rimmerge_ThingDef.xml")
                .is_file()
        );
        assert!(!tmp_dir(&mods_dir).exists());
    }

    /// `render_into`'s own
    /// `fs::create_dir_all(tmp)` happily writes into an already-existing
    /// directory, so a stale `.rimmerge_tmp_<pid>` left behind by an
    /// earlier write that died before its own cleanup ran (a panic skips
    /// every guard) must be cleared before this write renders into it —
    /// otherwise a leftover file from that earlier generation would
    /// silently survive into the new one.
    #[test]
    fn write_clears_a_stale_tmp_dir_before_rendering_into_it() {
        let dir = tempdir().expect("tempdir");
        let mods_dir = dir.path().join("Mods");
        let backup_dir = dir.path().join("profile").join("merge-mod.prev");
        let writer = MergeModFolderWriter::new();
        let stale_tmp = tmp_dir(&mods_dir);
        fs::create_dir_all(stale_tmp.join("Defs")).expect("seed a stale tmp dir");
        fs::write(stale_tmp.join("Defs").join("Leftover.xml"), b"<Defs/>")
            .expect("seed a leftover file from a previous, never-cleaned-up write");

        let report = writer
            .write(
                &mods_dir,
                &backup_dir,
                &rendered("rimmerge_merge_abc", "v1"),
            )
            .expect("write must succeed despite the stale tmp dir");

        assert!(
            !report.mod_path.join("Defs").exists(),
            "the stale generation's own files must not survive into the new one"
        );
        assert!(report.mod_path.join("About/About.xml").is_file());
        assert!(!tmp_dir(&mods_dir).exists());
    }

    #[test]
    fn overwriting_keeps_exactly_one_backup_generation() {
        let dir = tempdir().expect("tempdir");
        let mods_dir = dir.path().join("Mods");
        let backup_dir = dir.path().join("profile").join("merge-mod.prev");
        let writer = MergeModFolderWriter::new();

        writer
            .write(
                &mods_dir,
                &backup_dir,
                &rendered("rimmerge_merge_abc", "v1"),
            )
            .expect("first write must succeed");
        let second = writer
            .write(
                &mods_dir,
                &backup_dir,
                &rendered("rimmerge_merge_abc", "v2"),
            )
            .expect("second write must succeed");

        assert_eq!(second.backup_path, Some(backup_dir.clone()));
        let backed_up_about =
            fs::read_to_string(backup_dir.join("About/About.xml")).expect("backup must exist");
        assert_eq!(backed_up_about, "v1");
        let current_about =
            fs::read_to_string(mods_dir.join("rimmerge_merge_abc").join("About/About.xml"))
                .expect("current generation must exist");
        assert_eq!(current_about, "v2");
        assert!(!tmp_dir(&mods_dir).exists());
    }

    /// A simulated failure partway through rendering (a file already
    /// occupying the tmp dir's own path, the same technique
    /// `atomic.rs`'s own test uses) must leave the existing generation —
    /// and its backup — untouched.
    #[test]
    fn a_failed_render_leaves_the_existing_generation_and_backup_intact() {
        let dir = tempdir().expect("tempdir");
        let mods_dir = dir.path().join("Mods");
        let backup_dir = dir.path().join("profile").join("merge-mod.prev");
        let writer = MergeModFolderWriter::new();
        writer
            .write(
                &mods_dir,
                &backup_dir,
                &rendered("rimmerge_merge_abc", "v1"),
            )
            .expect("seed write must succeed");

        // Occupy the exact path the next write's tmp dir would use, with
        // a plain file — `fs::create_dir_all` on it fails deterministically
        // on both Windows and POSIX.
        fs::write(tmp_dir(&mods_dir), b"occupied").expect("occupy the tmp dir path");

        let result = writer.write(
            &mods_dir,
            &backup_dir,
            &rendered("rimmerge_merge_abc", "v2"),
        );

        assert!(result.is_err(), "the simulated failure must surface");
        let current_about =
            fs::read_to_string(mods_dir.join("rimmerge_merge_abc").join("About/About.xml"))
                .expect("the existing generation must still be readable");
        assert_eq!(
            current_about, "v1",
            "the existing generation must be untouched"
        );
        assert!(
            !backup_dir.exists(),
            "no backup was ever made for this failed attempt, so none should appear"
        );
    }

    #[test]
    fn remove_deletes_the_folder_and_reports_its_path() {
        let dir = tempdir().expect("tempdir");
        let mods_dir = dir.path().join("Mods");
        let backup_dir = dir.path().join("profile").join("merge-mod.prev");
        let writer = MergeModFolderWriter::new();
        writer
            .write(
                &mods_dir,
                &backup_dir,
                &rendered("rimmerge_merge_abc", "v1"),
            )
            .expect("seed write must succeed");

        let removed = writer
            .remove(&mods_dir, &backup_dir, "rimmerge_merge_abc")
            .expect("remove must succeed");

        assert_eq!(removed, Some(mods_dir.join("rimmerge_merge_abc")));
        assert!(!mods_dir.join("rimmerge_merge_abc").exists());
    }

    #[test]
    fn removing_a_folder_that_does_not_exist_is_a_no_op() {
        let dir = tempdir().expect("tempdir");
        let mods_dir = dir.path().join("Mods");
        let backup_dir = dir.path().join("profile").join("merge-mod.prev");
        let writer = MergeModFolderWriter::new();

        let removed = writer
            .remove(&mods_dir, &backup_dir, "nothing_here")
            .expect("remove must succeed even when there is nothing to remove");

        assert_eq!(removed, None);
    }

    #[test]
    fn read_marker_is_none_when_the_folder_does_not_exist() {
        let dir = tempdir().expect("tempdir");
        let writer = MergeModFolderWriter::new();

        let marker = writer
            .read_marker(dir.path(), "nothing_here")
            .expect("a missing folder is not an error");

        assert_eq!(marker, None);
    }

    #[test]
    fn read_marker_is_none_when_the_folder_exists_with_no_marker_file() {
        let dir = tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("some_mod")).expect("seed folder");
        let writer = MergeModFolderWriter::new();

        let marker = writer
            .read_marker(dir.path(), "some_mod")
            .expect("an absent marker file is not an error");

        assert_eq!(marker, None);
    }

    #[test]
    fn read_marker_is_none_for_unparsable_json() {
        let dir = tempdir().expect("tempdir");
        let mod_dir = dir.path().join("some_mod");
        fs::create_dir_all(&mod_dir).expect("seed folder");
        fs::write(mod_dir.join("rimmerge.json"), b"not json at all").expect("seed marker file");
        let writer = MergeModFolderWriter::new();

        let marker = writer
            .read_marker(dir.path(), "some_mod")
            .expect("unparsable content is not an error");

        assert_eq!(marker, None);
    }

    #[test]
    fn read_marker_parses_a_written_compat_patch_marker() {
        let dir = tempdir().expect("tempdir");
        let mod_dir = dir.path().join("sample_abcompat");
        fs::create_dir_all(&mod_dir).expect("seed folder");
        fs::write(mod_dir.join("rimmerge.json"),
            br#"{"kind":"patch","patchId":"3f9a1c02be77","profileHash":"abc","scope":["fixture.moda","fixture.modb"],"decisionsSha256":"def","rimmergeVersion":"0.1.0"}"#)
        .expect("seed marker file");
        let writer = MergeModFolderWriter::new();

        let marker = writer
            .read_marker(dir.path(), "sample_abcompat")
            .expect("a well-formed marker must read")
            .expect("must be Some");

        assert_eq!(marker.kind, rim_analyzer::domain::GeneratedKind::Patch);
        assert_eq!(marker.patch_id.as_deref(), Some("3f9a1c02be77"));
        assert_eq!(
            marker.scope,
            Some(
                [
                    rim_analyzer::domain::ModId::new("fixture.moda"),
                    rim_analyzer::domain::ModId::new("fixture.modb"),
                ]
                .into_iter()
                .collect()
            )
        );
    }

    #[test]
    fn read_marker_is_none_when_the_marker_file_is_oversized() {
        let dir = tempdir().expect("tempdir");
        let mod_dir = dir.path().join("some_mod");
        fs::create_dir_all(&mod_dir).expect("seed folder");
        // Larger than `MAX_MARKER_BYTES`, but still valid JSON so the only
        // thing this test can be proving is the size cap, not a parse
        // failure.
        let oversized = format!(
            r#"{{"kind":"merge","padding":"{}"}}"#,
            "a".repeat(MAX_MARKER_BYTES as usize + 1)
        );
        fs::write(mod_dir.join("rimmerge.json"), oversized.as_bytes()).expect("seed marker file");
        let writer = MergeModFolderWriter::new();

        let marker = writer
            .read_marker(dir.path(), "some_mod")
            .expect("an oversized file is not an error");

        assert_eq!(marker, None);
    }
}
