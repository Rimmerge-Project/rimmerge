//! [`list_saves`]: the `.rws` files in RimWorld's `Saves` folder, newest
//! first, for the desktop's "Import from a save" list.
//!
//! Directory metadata only. A save is 200 MB or more and this never opens
//! one (the header is read later, by `RmlFileStore::read`, only for the
//! file the user picks). No decision lives here (which folder to list is the
//! caller's), so it is a plain function, not a port.
//!
//! - **What counts**: a regular file whose name ends in `.rws` (ASCII
//!   case-insensitive). The entry is classified with the directory entry's
//!   own metadata, which never follows a link: a symlink, a junction and a
//!   folder named `x.rws` are all skipped, so a link planted in the folder
//!   cannot point the list at a file elsewhere. The `Saves` folder itself
//!   is opened as given: a player who moved it behind a junction still gets
//!   their list.
//! - **Bounded work**: at most [`MAX_SAVE_DIR_ENTRIES`] directory entries are
//!   examined and at most [`MAX_LISTED_SAVES`] are returned; the rest are
//!   counted in [`SaveListing::omitted`]. Past the examination bound the
//!   listing covers the first entries the file system hands out and
//!   `omitted` counts only what was seen.
//! - **Deterministic order**: newest modified time first, an implausible or
//!   missing time (see `plausible_time`) last, ties by file name ordinal.

use std::cmp::Ordering;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};

use crate::plausible_time::modified_seconds;

/// The most directory entries one listing examines.
pub const MAX_SAVE_DIR_ENTRIES: usize = 10_000;

/// The most saves one listing returns.
pub const MAX_LISTED_SAVES: usize = 200;

const SAVE_EXTENSION: &str = "rws";

/// One save file found in the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveFile {
    /// The file's full path.
    pub path: PathBuf,
    /// The file name without its extension. Untrusted text: it is the
    /// player's own save name, shown as text only.
    pub name: String,
    /// Modified time in whole seconds since the Unix epoch; `None` when the
    /// platform gave none or it is not a plausible time.
    pub saved_at: Option<u64>,
}

/// What [`list_saves`] found.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveListing {
    /// At most [`MAX_LISTED_SAVES`] saves, newest first.
    pub saves: Vec<SaveFile>,
    /// How many further saves were found but not listed.
    pub omitted: usize,
}

/// The two bounds, a value so tests can use small ones.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    max_examined: usize,
    max_listed: usize,
}

/// Lists the `.rws` files directly in `folder`, newest first.
///
/// # Errors
///
/// Returns the I/O error when `folder` cannot be listed (missing, not a
/// folder, no permission). A single entry that cannot be inspected (it
/// vanished between the listing and the inspection) is skipped.
pub fn list_saves(folder: &Path) -> io::Result<SaveListing> {
    list_saves_within(
        folder,
        Bounds {
            max_examined: MAX_SAVE_DIR_ENTRIES,
            max_listed: MAX_LISTED_SAVES,
        },
    )
}

fn list_saves_within(folder: &Path, bounds: Bounds) -> io::Result<SaveListing> {
    let mut found: Vec<(OsString, SaveFile)> = std::fs::read_dir(folder)?
        .take(bounds.max_examined)
        .filter_map(|entry| save_of(&entry.ok()?))
        .collect();
    found.sort_by(|(left_name, left), (right_name, right)| {
        order_newest_first(left.saved_at, right.saved_at).then_with(|| left_name.cmp(right_name))
    });
    let omitted = found.len().saturating_sub(bounds.max_listed);
    found.truncate(bounds.max_listed);
    Ok(SaveListing {
        saves: found.into_iter().map(|(_, save)| save).collect(),
        omitted,
    })
}

/// A later time sorts first; no time sorts after every time.
fn order_newest_first(left: Option<u64>, right: Option<u64>) -> Ordering {
    right.cmp(&left)
}

/// The save a directory entry is, or `None` when it is not a regular
/// `.rws` file. Uses the entry's own metadata (no link is followed).
fn save_of(entry: &std::fs::DirEntry) -> Option<(OsString, SaveFile)> {
    let file_name = entry.file_name();
    let path = Path::new(&file_name);
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(OsStr::new(SAVE_EXTENSION)))
    {
        return None;
    }
    let metadata = entry.metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    let name = path.file_stem()?.to_string_lossy().into_owned();
    let save = SaveFile {
        path: entry.path(),
        name,
        saved_at: modified_seconds(&metadata),
    };
    Some((file_name, save))
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    use super::*;

    fn write_save(folder: &Path, name: &str, modified_seconds: Option<u64>) {
        let file = File::create(folder.join(name)).expect("create");
        if let Some(seconds) = modified_seconds {
            file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
                .expect("set mtime");
        }
    }

    fn names(listing: &SaveListing) -> Vec<&str> {
        listing
            .saves
            .iter()
            .map(|save| save.name.as_str())
            .collect()
    }

    #[test]
    fn lists_only_regular_rws_files_newest_first() {
        let folder = tempfile::tempdir().expect("tempdir");
        write_save(folder.path(), "Old Colony.rws", Some(1_600_000_000));
        write_save(folder.path(), "Newest.rws", Some(1_700_000_300));
        write_save(folder.path(), "Upper.RWS", Some(1_700_000_200));
        write_save(folder.path(), "notes.txt", Some(1_700_000_900));
        write_save(folder.path(), "Autosave.rws.bak", Some(1_700_000_900));
        std::fs::create_dir(folder.path().join("dir.rws")).expect("folder named like a save");

        let listing = list_saves(folder.path()).expect("listing");

        assert_eq!(names(&listing), ["Newest", "Upper", "Old Colony"]);
        assert_eq!(listing.saves[0].saved_at, Some(1_700_000_300));
        assert_eq!(listing.saves[0].path, folder.path().join("Newest.rws"));
        assert_eq!(listing.omitted, 0);
    }

    #[test]
    fn equal_times_are_ordered_by_file_name() {
        let folder = tempfile::tempdir().expect("tempdir");
        for name in ["b.rws", "c.rws", "a.rws"] {
            write_save(folder.path(), name, Some(1_700_000_000));
        }

        let listing = list_saves(folder.path()).expect("listing");

        assert_eq!(names(&listing), ["a", "b", "c"]);
    }

    #[test]
    fn an_implausible_time_has_no_value_and_sorts_last() {
        let folder = tempfile::tempdir().expect("tempdir");
        write_save(folder.path(), "aaa.rws", Some(300_000_000_000));
        write_save(folder.path(), "bbb.rws", Some(1_600_000_000));

        let listing = list_saves(folder.path()).expect("listing");

        assert_eq!(names(&listing), ["bbb", "aaa"]);
        assert_eq!(listing.saves[1].saved_at, None);
    }

    #[test]
    fn more_saves_than_the_cap_are_counted_as_omitted() {
        let folder = tempfile::tempdir().expect("tempdir");
        for index in 0..=MAX_LISTED_SAVES {
            write_save(
                folder.path(),
                &format!("save-{index:03}.rws"),
                Some(1_700_000_000 + index as u64),
            );
        }

        let listing = list_saves(folder.path()).expect("listing");

        assert_eq!(listing.saves.len(), MAX_LISTED_SAVES);
        assert_eq!(listing.omitted, 1);
        assert_eq!(listing.saves[0].name, format!("save-{MAX_LISTED_SAVES:03}"));
        assert!(!names(&listing).contains(&"save-000"));
    }

    #[test]
    fn examination_stops_at_its_bound() {
        let folder = tempfile::tempdir().expect("tempdir");
        for index in 0..10 {
            write_save(
                folder.path(),
                &format!("save-{index}.rws"),
                Some(1_700_000_000),
            );
        }

        let listing = list_saves_within(
            folder.path(),
            Bounds {
                max_examined: 4,
                max_listed: 3,
            },
        )
        .expect("listing");

        assert_eq!(listing.saves.len(), 3);
        assert_eq!(listing.omitted, 1);
    }

    #[test]
    fn a_missing_folder_is_an_error() {
        let folder = tempfile::tempdir().expect("tempdir");

        let error = list_saves(&folder.path().join("absent")).expect_err("no such folder");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_link_named_like_a_save_is_not_listed() {
        let folder = tempfile::tempdir().expect("tempdir");
        let elsewhere = tempfile::tempdir().expect("tempdir");
        let target = elsewhere.path().join("outside.rws");
        write_save(elsewhere.path(), "outside.rws", Some(1_700_000_000));
        write_save(folder.path(), "inside.rws", Some(1_600_000_000));
        let link = folder.path().join("link.rws");
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&target, &link);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&target, &link);
        if made.is_err() {
            // Creating a symlink needs a privilege on Windows; without it
            // there is nothing to assert about.
            eprintln!("skipping: this platform or user cannot create a symlink");
            return;
        }

        let listing = list_saves(folder.path()).expect("listing");

        assert_eq!(names(&listing), ["inside"]);
    }
}
