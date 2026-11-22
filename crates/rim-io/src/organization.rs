//! [`JsonOrganizationStore`]: `<profile>/organization.json`, the library
//! organization (groups and per-mod colours).
//!
//! The file is user data the app owns but a user can also edit, so the
//! reader treats it as untrusted: it reads at most
//! [`OrganizationLimits::MAX_FILE_BYTES`], parses into plain records, turns
//! every value into a domain value object, and hands the result to
//! [`ModOrganization::from_parts`], which rejects the whole document on any
//! broken invariant. Nothing here is ever "repaired": a document that does
//! not pass is reported as an [`OrganizationIssue`] and left on disk.
//!
//! The writer emits groups in id order and members sorted, so the same
//! state always produces the same bytes.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rim_analyzer::domain::ModId;
use rim_session::organization::{
    ColorToken, GroupId, GroupName, GroupParts, ModOrganization, OrganizationIssue,
    OrganizationLimits, OrganizationLoad, OrganizedModId,
};
use rim_session::ports::{OrganizationSaveError, OrganizationStore, StoreError};
use serde::{Deserialize, Serialize};

use crate::atomic::write_atomically;

const FILE_NAME: &str = "organization.json";
const ARCHIVE_PREFIX: &str = "organization.json.unusable-";
/// The only schema version so far. A larger one is a file from a newer
/// build and is refused (never read as a partial document).
const SCHEMA_VERSION: u32 = 1;
/// How many `-<n>` suffixes an archive name tries before giving up.
const MAX_ARCHIVE_NAME_ATTEMPTS: u32 = 1_000;

/// One group as the file writes it.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupRecord {
    id: u32,
    name: String,
    parent: Option<u32>,
    color: Option<String>,
    members: Vec<String>,
}

/// The whole document.
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OrganizationFile {
    version: u32,
    groups: Vec<GroupRecord>,
    mod_colors: BTreeMap<String, String>,
    /// The id the next created group gets; persisted so a deleted group's
    /// id is never handed out again after a reload.
    next_group_id: u32,
}

/// Just the version, read before the rest: a newer file's other fields may
/// be shaped differently, so it is refused by version, not by shape.
#[derive(Debug, Deserialize)]
struct VersionProbe {
    version: u32,
}

/// Reads and writes `organization.json`. Stateless: every call works on the
/// profile directory it is given.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonOrganizationStore;

impl JsonOrganizationStore {
    /// Builds the store.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// Reads the file's bytes, at most the limit.
fn read_bounded_file(path: &Path) -> Result<Option<Vec<u8>>, std::io::Error> {
    let limit = OrganizationLimits::MAX_FILE_BYTES;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok((bytes.len() <= limit).then_some(bytes))
}

fn parse_color(text: &str) -> Result<ColorToken, OrganizationIssue> {
    text.parse().map_err(|_| OrganizationIssue::Malformed)
}

fn parse_mod(text: &str) -> Result<OrganizedModId, OrganizationIssue> {
    OrganizedModId::new(&ModId::new(text)).map_err(|_| OrganizationIssue::Malformed)
}

fn group_parts(record: GroupRecord) -> Result<GroupParts, OrganizationIssue> {
    let malformed = || OrganizationIssue::Malformed;
    let members: Result<BTreeSet<_>, _> = record.members.iter().map(|id| parse_mod(id)).collect();
    Ok(GroupParts {
        id: GroupId::new(record.id).ok_or_else(malformed)?,
        name: GroupName::new(&record.name).map_err(|_| malformed())?,
        parent: record
            .parent
            .map(|parent| GroupId::new(parent).ok_or_else(malformed))
            .transpose()?,
        color: record.color.as_deref().map(parse_color).transpose()?,
        members: members?,
    })
}

/// Turns the parsed file into an organization, or says why it cannot be.
fn organization_from(file: OrganizationFile) -> Result<ModOrganization, OrganizationIssue> {
    let parts = file
        .groups
        .into_iter()
        .map(group_parts)
        .collect::<Result<Vec<_>, _>>()?;
    let mut colors = BTreeMap::new();
    for (id, color) in &file.mod_colors {
        colors.insert(parse_mod(id)?, parse_color(color)?);
    }
    ModOrganization::from_parts(parts, colors, file.next_group_id)
        .map_err(OrganizationIssue::Invalid)
}

fn parse_document(bytes: &[u8]) -> Result<ModOrganization, OrganizationIssue> {
    let probe: VersionProbe =
        serde_json::from_slice(bytes).map_err(|_| OrganizationIssue::Malformed)?;
    if probe.version > SCHEMA_VERSION {
        return Err(OrganizationIssue::NewerVersion {
            found: probe.version,
        });
    }
    if probe.version != SCHEMA_VERSION {
        return Err(OrganizationIssue::Malformed);
    }
    let file: OrganizationFile =
        serde_json::from_slice(bytes).map_err(|_| OrganizationIssue::Malformed)?;
    organization_from(file)
}

fn document_of(organization: &ModOrganization) -> OrganizationFile {
    let groups = organization
        .groups()
        .map(|(id, group)| GroupRecord {
            id: id.get(),
            name: group.name().to_string(),
            parent: group.parent().map(GroupId::get),
            color: group.color().map(|color| color.as_str().to_owned()),
            members: group
                .members()
                .iter()
                .map(|member| member.as_str().to_owned())
                .collect(),
        })
        .collect();
    let mod_colors = organization
        .mod_colors()
        .map(|(id, color)| (id.as_str().to_owned(), color.as_str().to_owned()))
        .collect();
    OrganizationFile {
        version: SCHEMA_VERSION,
        groups,
        mod_colors,
        next_group_id: organization.next_group_id(),
    }
}

fn to_store_error(path: &Path, error: impl std::fmt::Display) -> StoreError {
    StoreError(format!("{}: {error}", path.display()))
}

/// The first free `organization.json.unusable-<seconds>[-n]` beside `dir`'s
/// file, so a second "start over" in the same second never overwrites the
/// first archive.
fn free_archive_path(dir: &Path, seconds: u64) -> Option<PathBuf> {
    let base = format!("{ARCHIVE_PREFIX}{seconds}");
    let first = dir.join(&base);
    if !first.exists() {
        return Some(first);
    }
    (1..=MAX_ARCHIVE_NAME_ATTEMPTS)
        .map(|attempt| dir.join(format!("{base}-{attempt}")))
        .find(|candidate| !candidate.exists())
}

impl OrganizationStore for JsonOrganizationStore {
    fn load(&self, dir: &Path) -> OrganizationLoad {
        let path = dir.join(FILE_NAME);
        // A named pipe or device would block `open` while the session lock is
        // held, so only a regular file is ever opened. `metadata` follows a
        // link, so a link to a regular file still reads.
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return OrganizationLoad::Unusable(OrganizationIssue::Unreadable),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return OrganizationLoad::Missing;
            }
            Err(_) => return OrganizationLoad::Unusable(OrganizationIssue::Unreadable),
        }
        let bytes = match read_bounded_file(&path) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                return OrganizationLoad::Unusable(OrganizationIssue::TooLarge {
                    limit_bytes: OrganizationLimits::MAX_FILE_BYTES,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return OrganizationLoad::Missing;
            }
            Err(_) => return OrganizationLoad::Unusable(OrganizationIssue::Unreadable),
        };
        match parse_document(&bytes) {
            Ok(organization) => OrganizationLoad::Loaded(organization),
            Err(issue) => OrganizationLoad::Unusable(issue),
        }
    }

    fn save(
        &self,
        dir: &Path,
        organization: &ModOrganization,
    ) -> Result<(), OrganizationSaveError> {
        let path = dir.join(FILE_NAME);
        let mut json = serde_json::to_string_pretty(&document_of(organization))
            .map_err(|error| to_store_error(&path, error))?;
        json.push('\n');
        // Never write what `load` would refuse: an over-size file would
        // come back as `TooLarge` and turn the whole organization read-only.
        // The caller rolls the edit back on this error.
        if json.len() > OrganizationLimits::MAX_FILE_BYTES {
            return Err(OrganizationSaveError::TooLarge {
                bytes: json.len(),
                limit: OrganizationLimits::MAX_FILE_BYTES,
            });
        }
        write_atomically(&path, json.as_bytes())
            .map_err(|error| to_store_error(&path, error).into())
    }

    fn archive_unusable(&self, dir: &Path) -> Result<(), StoreError> {
        let path = dir.join(FILE_NAME);
        // Seconds since the epoch only name the copy; a clock set before
        // 1970 just yields 0.
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let target = free_archive_path(dir, seconds)
            .ok_or_else(|| to_store_error(dir, "no free name for the archive copy"))?;
        let partial = dir.join(format!("{FILE_NAME}.archive-tmp-{}", std::process::id()));
        match fs::copy(&path, &partial) {
            Ok(_) => {}
            // Nothing there, nothing to keep: the same answer the fake gives.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(to_store_error(&path, error)),
        }
        fs::rename(&partial, &target).map_err(|error| to_store_error(&target, error))
    }
}

#[cfg(test)]
mod tests {
    use rim_session::organization::{OrganizationDocumentError, OrganizationError};
    use rim_session::test_support::organized;

    use super::*;

    fn name(text: &str) -> GroupName {
        GroupName::new(text).expect("valid fixture name")
    }

    fn sample() -> ModOrganization {
        let mut organization = ModOrganization::new();
        let top = organization
            .create_group(name("Top"), None)
            .expect("create");
        let kid = organization
            .create_group(name("Kid"), Some(top))
            .expect("create");
        organization
            .add_members(
                kid,
                &BTreeSet::from([organized("b.mod"), organized("a.mod")]),
            )
            .expect("add");
        organization
            .set_group_color(top, Some(ColorToken::Indigo))
            .expect("colour");
        organization
            .set_mod_colors(&BTreeSet::from([organized("a.mod")]), Some(ColorToken::Red))
            .expect("colour");
        organization
    }

    fn write_raw(dir: &Path, text: &str) {
        fs::write(dir.join(FILE_NAME), text).expect("write fixture file");
    }

    fn unusable(dir: &Path) -> OrganizationIssue {
        match JsonOrganizationStore.load(dir) {
            OrganizationLoad::Unusable(issue) => issue,
            other => panic!("expected an unusable file, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_file_loads_as_missing() {
        let dir = tempfile::tempdir().expect("tempdir");

        assert_eq!(
            JsonOrganizationStore.load(dir.path()),
            OrganizationLoad::Missing
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_named_pipe_where_the_file_should_be_is_unreadable_and_never_opened() {
        let dir = tempfile::tempdir().expect("tempdir");
        let status = std::process::Command::new("mkfifo")
            .arg(dir.path().join(FILE_NAME))
            .status()
            .expect("mkfifo runs");
        assert!(status.success(), "mkfifo failed");
        let folder = dir.path().to_path_buf();
        let (sender, receiver) = std::sync::mpsc::channel();
        // Opening a pipe with no writer blocks forever, so a regression would
        // hang the load; the timeout turns that hang into a failure.
        std::thread::spawn(move || {
            let _ = sender.send(JsonOrganizationStore.load(&folder));
        });

        let loaded = receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("load returned instead of blocking on the pipe");

        assert_eq!(
            loaded,
            OrganizationLoad::Unusable(OrganizationIssue::Unreadable)
        );
    }

    #[test]
    fn a_saved_organization_loads_back_the_same() {
        let dir = tempfile::tempdir().expect("tempdir");
        let organization = sample();

        JsonOrganizationStore
            .save(dir.path(), &organization)
            .expect("save");

        assert_eq!(
            JsonOrganizationStore.load(dir.path()),
            OrganizationLoad::Loaded(organization)
        );
    }

    #[test]
    fn the_same_state_always_writes_the_same_bytes() {
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        let organization = sample();

        JsonOrganizationStore
            .save(first.path(), &organization)
            .expect("save");
        JsonOrganizationStore
            .save(second.path(), &organization)
            .expect("save");

        let bytes = fs::read(first.path().join(FILE_NAME)).expect("read");
        assert_eq!(
            bytes,
            fs::read(second.path().join(FILE_NAME)).expect("read")
        );
        let text = String::from_utf8(bytes).expect("utf-8");
        assert!(
            text.find("a.mod").expect("a") < text.find("b.mod").expect("b"),
            "members are written sorted"
        );
    }

    #[test]
    fn a_save_leaves_no_temp_file_beside_the_document() {
        let dir = tempfile::tempdir().expect("tempdir");

        JsonOrganizationStore
            .save(dir.path(), &sample())
            .expect("save");

        let names: Vec<String> = fs::read_dir(dir.path())
            .expect("list")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(names, [FILE_NAME]);
    }

    #[test]
    fn a_save_into_a_missing_profile_directory_creates_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let profile = dir.path().join("not").join("yet");

        JsonOrganizationStore
            .save(&profile, &sample())
            .expect("save creates the folder");

        assert!(profile.join(FILE_NAME).is_file());
    }

    #[test]
    fn an_oversize_file_is_unusable_and_never_fully_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let padding = " ".repeat(OrganizationLimits::MAX_FILE_BYTES + 1);
        write_raw(
            dir.path(),
            &format!(
                "{{\"version\":1,\"groups\":[],\"mod_colors\":{{}},\"next_group_id\":2000}}{padding}"
            ),
        );

        assert_eq!(
            unusable(dir.path()),
            OrganizationIssue::TooLarge {
                limit_bytes: OrganizationLimits::MAX_FILE_BYTES
            }
        );
    }

    #[test]
    fn a_file_exactly_at_the_limit_is_still_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let body = "{\"version\":1,\"groups\":[],\"mod_colors\":{},\"next_group_id\":1}";
        let padding = " ".repeat(OrganizationLimits::MAX_FILE_BYTES - body.len());
        write_raw(dir.path(), &format!("{body}{padding}"));

        assert_eq!(
            JsonOrganizationStore.load(dir.path()),
            OrganizationLoad::Loaded(ModOrganization::new())
        );
    }

    #[test]
    fn malformed_json_and_the_wrong_shape_are_malformed() {
        let dir = tempfile::tempdir().expect("tempdir");

        for text in [
            "not json",
            "",
            "[]",
            "{\"version\":1}",
            "{\"version\":1,\"groups\":[],\"mod_colors\":{},\"next_group_id\":2000,\"extra\":1}",
            "{\"version\":0,\"groups\":[],\"mod_colors\":{},\"next_group_id\":2000}",
        ] {
            write_raw(dir.path(), text);
            assert_eq!(
                unusable(dir.path()),
                OrganizationIssue::Malformed,
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_newer_version_is_refused_by_version_whatever_else_it_holds() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_raw(dir.path(), "{\"version\":2,\"layout\":\"something new\"}");

        assert_eq!(
            unusable(dir.path()),
            OrganizationIssue::NewerVersion { found: 2 }
        );
    }

    #[test]
    fn a_value_that_fails_its_value_object_is_malformed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let group = |extra: &str| {
            format!(
                "{{\"version\":1,\"groups\":[{{\"id\":1,\"name\":\"G\",\"parent\":null,\"color\":null,\"members\":[]{extra}}}],\"mod_colors\":{{}},\"next_group_id\":2000}}"
            )
        };
        let cases = [
            group("").replace("\"id\":1", "\"id\":0"),
            group("").replace("\"name\":\"G\"", "\"name\":\"  \""),
            group("").replace("\"color\":null", "\"color\":\"#ff0000\""),
            group("").replace("\"members\":[]", "\"members\":[\"\"]"),
            "{\"version\":1,\"groups\":[],\"mod_colors\":{\"a.b\":\"mauve\"},\"next_group_id\":2000}".to_string(),
        ];

        for text in cases {
            write_raw(dir.path(), &text);
            assert_eq!(unusable(dir.path()), OrganizationIssue::Malformed, "{text}");
        }
    }

    #[test]
    fn a_cycle_a_dangling_parent_and_a_duplicate_name_are_unusable_with_the_rule() {
        let dir = tempfile::tempdir().expect("tempdir");
        let doc = |groups: &str| {
            format!(
                "{{\"version\":1,\"groups\":[{groups}],\"mod_colors\":{{}},\"next_group_id\":2000}}"
            )
        };
        let group = |id: u32, name: &str, parent: &str| {
            format!(
                "{{\"id\":{id},\"name\":\"{name}\",\"parent\":{parent},\"color\":null,\"members\":[]}}"
            )
        };

        write_raw(
            dir.path(),
            &doc(&format!("{},{}", group(1, "A", "2"), group(2, "B", "1"))),
        );
        assert!(matches!(
            unusable(dir.path()),
            OrganizationIssue::Invalid(OrganizationDocumentError::Rule(
                OrganizationError::WouldCreateCycle
            ))
        ));

        write_raw(dir.path(), &doc(&group(1, "A", "9")));
        assert!(matches!(
            unusable(dir.path()),
            OrganizationIssue::Invalid(OrganizationDocumentError::DanglingParent { .. })
        ));

        write_raw(
            dir.path(),
            &doc(&format!(
                "{},{}",
                group(1, "A", "null"),
                group(2, "a", "null")
            )),
        );
        assert!(matches!(
            unusable(dir.path()),
            OrganizationIssue::Invalid(OrganizationDocumentError::Rule(
                OrganizationError::NameTaken { .. }
            ))
        ));
    }

    #[test]
    fn a_hostile_file_of_many_groups_is_bounded_by_the_group_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let groups: Vec<String> = (1..=OrganizationLimits::MAX_GROUPS + 1)
            .map(|id| {
                format!(
                    "{{\"id\":{id},\"name\":\"g{id}\",\"parent\":null,\"color\":null,\"members\":[]}}"
                )
            })
            .collect();
        write_raw(
            dir.path(),
            &format!(
                "{{\"version\":1,\"groups\":[{}],\"mod_colors\":{{}},\"next_group_id\":2000}}",
                groups.join(",")
            ),
        );

        assert!(matches!(
            unusable(dir.path()),
            OrganizationIssue::Invalid(OrganizationDocumentError::Rule(
                OrganizationError::TooManyGroups { .. }
            ))
        ));
    }

    #[test]
    fn a_steam_suffixed_member_in_a_hand_edited_file_is_read_as_its_base_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_raw(
            dir.path(),
            "{\"version\":1,\"groups\":[{\"id\":1,\"name\":\"G\",\"parent\":null,\"color\":null,\"members\":[\"x.y_steam\"]}],\"mod_colors\":{},\"next_group_id\":2000}",
        );

        let OrganizationLoad::Loaded(organization) = JsonOrganizationStore.load(dir.path()) else {
            panic!("the file is valid");
        };

        assert_eq!(
            organization.groups_of(&organized("x.y")).len(),
            1,
            "x.y_steam and x.y are one mod"
        );
    }

    #[test]
    fn a_directory_where_the_file_should_be_is_unreadable_not_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir(dir.path().join(FILE_NAME)).expect("mkdir");

        assert_eq!(unusable(dir.path()), OrganizationIssue::Unreadable);
    }

    #[test]
    fn archiving_keeps_a_copy_beside_the_file_and_leaves_the_file_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_raw(dir.path(), "garbage the user cares about");

        JsonOrganizationStore
            .archive_unusable(dir.path())
            .expect("archive");

        let archives: Vec<PathBuf> = fs::read_dir(dir.path())
            .expect("list")
            .map(|entry| entry.expect("entry").path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(ARCHIVE_PREFIX))
            })
            .collect();
        assert_eq!(archives.len(), 1);
        assert_eq!(
            fs::read_to_string(&archives[0]).expect("read archive"),
            "garbage the user cares about"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join(FILE_NAME)).expect("read original"),
            "garbage the user cares about",
            "the archive step alone never touches the file"
        );
    }

    #[test]
    fn a_second_archive_in_the_same_second_never_overwrites_the_first() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_raw(dir.path(), "first");
        JsonOrganizationStore
            .archive_unusable(dir.path())
            .expect("first");
        write_raw(dir.path(), "second");
        JsonOrganizationStore
            .archive_unusable(dir.path())
            .expect("second");

        let mut contents: Vec<String> = fs::read_dir(dir.path())
            .expect("list")
            .map(|entry| entry.expect("entry").path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(ARCHIVE_PREFIX))
            })
            .map(|path| fs::read_to_string(path).expect("read"))
            .collect();
        contents.sort();

        assert_eq!(contents, ["first", "second"]);
    }

    #[test]
    fn archiving_without_a_file_has_nothing_to_keep_and_succeeds() {
        let dir = tempfile::tempdir().expect("tempdir");

        JsonOrganizationStore
            .archive_unusable(dir.path())
            .expect("a missing file is nothing to archive");

        assert_eq!(
            fs::read_dir(dir.path()).expect("list").count(),
            0,
            "no archive copy and no temp file is left behind"
        );
    }

    #[test]
    fn a_deleted_groups_id_is_not_reused_after_a_save_and_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut organization = ModOrganization::new();
        organization.create_group(name("Keep"), None).expect("one");
        let doomed = organization.create_group(name("Gone"), None).expect("two");
        organization
            .delete_group(doomed, rim_session::organization::DeleteMode::WithSubgroups)
            .expect("delete");
        JsonOrganizationStore
            .save(dir.path(), &organization)
            .expect("save");

        let OrganizationLoad::Loaded(mut reloaded) = JsonOrganizationStore.load(dir.path()) else {
            panic!("the saved file is valid");
        };
        let fresh = reloaded.create_group(name("Fresh"), None).expect("three");

        assert_eq!(fresh.get(), 3, "id 2 belonged to the deleted group");
    }

    #[test]
    fn a_next_group_id_behind_the_groups_or_out_of_range_is_refused_clearly() {
        let dir = tempfile::tempdir().expect("tempdir");
        let doc = |id: u32, next: u32| {
            format!(
                "{{\"version\":1,\"groups\":[{{\"id\":{id},\"name\":\"G\",\"parent\":null,\"color\":null,\"members\":[]}}],\"mod_colors\":{{}},\"next_group_id\":{next}}}"
            )
        };

        write_raw(dir.path(), &doc(7, 3));
        assert!(matches!(
            unusable(dir.path()),
            OrganizationIssue::Invalid(OrganizationDocumentError::NextIdBehind { .. })
        ));

        write_raw(dir.path(), &doc(1, u32::MAX));
        assert!(matches!(
            unusable(dir.path()),
            OrganizationIssue::Invalid(OrganizationDocumentError::GroupIdOutOfRange { .. })
        ));

        write_raw(dir.path(), &doc(u32::MAX, u32::MAX));
        assert!(
            matches!(
                unusable(dir.path()),
                OrganizationIssue::Invalid(OrganizationDocumentError::GroupIdOutOfRange { .. })
            ),
            "a hostile id is an id error, not a group-count error"
        );
    }

    /// An organization at every limit with the longest ids, the worst case
    /// the aggregate allows.
    fn organization_at_its_limits() -> ModOrganization {
        let mut organization = ModOrganization::new();
        let per_group =
            OrganizationLimits::MAX_TOTAL_MEMBERSHIPS / OrganizationLimits::MAX_MEMBERS_PER_GROUP;
        for group_index in 0..per_group {
            let group = organization
                .create_group(name(&format!("group {group_index}")), None)
                .expect("create");
            let members: BTreeSet<_> = (0..OrganizationLimits::MAX_MEMBERS_PER_GROUP)
                .map(|member| {
                    organized(&format!("{group_index:02}.{member:05}.{}", "x".repeat(110)))
                })
                .collect();
            organization.add_members(group, &members).expect("add");
        }
        let colored: BTreeSet<_> = (0..OrganizationLimits::MAX_MOD_COLORS)
            .map(|index| organized(&format!("c.{index:05}.{}", "y".repeat(110))))
            .collect();
        organization
            .set_mod_colors(&colored, Some(ColorToken::Teal))
            .expect("colour");
        organization
    }

    #[test]
    fn an_organization_too_big_for_the_reader_is_refused_on_save_and_the_file_is_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        JsonOrganizationStore
            .save(dir.path(), &sample())
            .expect("a small one saves");
        let before = fs::read(dir.path().join(FILE_NAME)).expect("read");

        let result = JsonOrganizationStore.save(dir.path(), &organization_at_its_limits());

        assert!(
            matches!(result, Err(OrganizationSaveError::TooLarge { .. })),
            "the worst case exceeds the reader's cap, as a typed limit"
        );
        assert_eq!(
            fs::read(dir.path().join(FILE_NAME)).expect("read"),
            before,
            "a refused save leaves the previous file"
        );
        assert_eq!(
            JsonOrganizationStore.load(dir.path()),
            OrganizationLoad::Loaded(sample()),
            "so the next load still works"
        );
    }
}
