//! [`JsonAssignmentProjectStore`]: `<profile>/assignments/<id>.json`, one
//! versioned envelope per patch maker project — mirrors
//! `patches.rs::JsonPatchProjectStore`'s own shape exactly
//! One file per project, so two
//! projects never contend for one file.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    AssignmentId, AssignmentProject, AssignmentRow, AssignmentSchema, PatchModIdentity, RowKey,
    Section, StoredAssignmentProject, TargetRef,
};
use rim_session::ports::{AssignmentProjectStore, StoreError};
use serde::{Deserialize, Serialize};

use crate::atomic::write_atomically;

/// The subdirectory under a profile directory every project's file lives
/// under.
const DIR_NAME: &str = "assignments";
/// Current on-disk schema version: a project is one or more [`Section`]s
/// rather than a single `schema`/`rows` pair. Version 1 is still read (and
/// migrated on load, never rewritten in place until the next explicit
/// `save`); every write produces version 2.
const SCHEMA_VERSION: u32 = 2;

/// One row's on-disk shape in the version-1 (single-section) record: the
/// target it was matched through plus the row itself, both already fully
/// `Serialize`/`Deserialize` on their own. A `Vec`, not a map, because
/// [`TargetRef`] is a struct: `serde_json` can only use a string as an
/// object key.
#[derive(Debug, Serialize, Deserialize)]
struct RowRecord {
    target: TargetRef,
    row: AssignmentRow,
}

/// One project's version-1, single-section persisted shape — read (and
/// migrated) only; `save` never writes this shape again.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignmentRecordV1 {
    id: String,
    name: String,
    package_id: String,
    display_name: String,
    author: String,
    description: String,
    refs: Vec<String>,
    excluded_refs: Vec<String>,
    targets: Vec<String>,
    schema: AssignmentSchema,
    rows: Vec<RowRecord>,
    #[serde(default)]
    standalone_rows: Vec<AssignmentRow>,
    export_dir: Option<PathBuf>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AssignmentFileV1 {
    version: u32,
    assignment: AssignmentRecordV1,
}

/// One row's on-disk shape in the version-2 (multi-section) record:
/// [`RowKey`] already derives `Serialize`/`Deserialize` as an ordinary
/// value (never as a `serde_json` map key, which a struct-shaped variant
/// can't be), so this is just a plain `{key, row}` pair, one `Vec` per
/// section for the same reason `RowRecord` above is a `Vec`.
#[derive(Debug, Serialize, Deserialize)]
struct RowRecordV2 {
    key: RowKey,
    row: AssignmentRow,
}

/// One section's on-disk shape: `def_type` is carried explicitly (not
/// just as a `Vec` position) so a hand-edited or reordered file still
/// resolves correctly.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SectionRecord {
    def_type: String,
    schema: AssignmentSchema,
    rows: Vec<RowRecordV2>,
}

/// One project's version-2, multi-section persisted shape
/// camelCase keys at this top
/// level, matching `patches.rs::PatchRecordV1`'s own convention — a
/// section's nested `schema` keeps its own already-shipped (snake_case)
/// shape from `rim-resolve`'s own `Serialize` derive unchanged.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignmentRecordV2 {
    id: String,
    name: String,
    package_id: String,
    display_name: String,
    author: String,
    description: String,
    refs: Vec<String>,
    excluded_refs: Vec<String>,
    targets: Vec<String>,
    sections: Vec<SectionRecord>,
    export_dir: Option<PathBuf>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AssignmentFileV2 {
    version: u32,
    assignment: AssignmentRecordV2,
}

/// Just enough of the envelope to check the schema version before
/// attempting to deserialize the full, version-specific shape — the same
/// pattern `patches.rs`/`decisions.rs`/`rules.rs` all use.
#[derive(Debug, Deserialize)]
struct VersionProbe {
    version: u32,
}

fn to_store_error(path: &Path, error: impl std::fmt::Display) -> StoreError {
    StoreError(format!("{}: {error}", path.display()))
}

fn assignments_dir(profile_dir: &Path) -> PathBuf {
    profile_dir.join(DIR_NAME)
}

fn assignment_path(profile_dir: &Path, id: &AssignmentId) -> PathBuf {
    assignments_dir(profile_dir).join(format!("{id}.json"))
}

/// The identity/timestamp fields both record versions share, parsed and
/// validated the same way — extracted so `from_record_v1`/`from_record_v2`
/// don't duplicate it.
struct CommonFields {
    id: AssignmentId,
    identity: PatchModIdentity,
    created_at: jiff::Timestamp,
    updated_at: jiff::Timestamp,
}

fn parse_common(
    path: &Path,
    id: &str,
    package_id: &str,
    display_name: &str,
    created_at: &str,
    updated_at: &str,
) -> Result<CommonFields, StoreError> {
    Ok(CommonFields {
        id: id
            .parse()
            .map_err(|e| to_store_error(path, format!("assignment id {id:?}: {e}")))?,
        identity: PatchModIdentity::new(package_id, display_name)
            .map_err(|e| to_store_error(path, format!("identity: {e}")))?,
        created_at: created_at
            .parse()
            .map_err(|e| to_store_error(path, format!("created_at {created_at:?}: {e}")))?,
        updated_at: updated_at
            .parse()
            .map_err(|e| to_store_error(path, format!("updated_at {updated_at:?}: {e}")))?,
    })
}

fn to_record(project: &AssignmentProject) -> AssignmentRecordV2 {
    let stored = project.to_stored();
    let sections = stored
        .sections
        .into_iter()
        .map(|(def_type, section)| SectionRecord {
            def_type,
            schema: section.schema,
            rows: section
                .rows
                .into_iter()
                .map(|(key, row)| RowRecordV2 { key, row })
                .collect(),
        })
        .collect();
    AssignmentRecordV2 {
        id: stored.id.to_string(),
        name: stored.name,
        package_id: stored.identity.package_id().to_string(),
        display_name: stored.identity.display_name().to_string(),
        author: stored.author,
        description: stored.description,
        refs: stored.refs.iter().map(ModId::to_string).collect(),
        excluded_refs: stored.excluded_refs.iter().map(ModId::to_string).collect(),
        targets: stored.targets.iter().map(ModId::to_string).collect(),
        sections,
        export_dir: stored.export_dir,
        created_at: stored.created_at.to_string(),
        updated_at: stored.updated_at.to_string(),
    }
}

/// Migrates a version-1 (single-section) record into an
/// [`AssignmentProject`]: `record.schema`'s own def type becomes the one
/// section's key, `record.rows` become [`RowKey::Target`] entries and
/// `record.standalone_rows` become [`RowKey::Own`] ones — the two never
/// coexist in practice (a schema was always either target-keyed or
/// free-standing, never both), but nothing here assumes that.
fn from_record_v1(
    path: &Path,
    record: AssignmentRecordV1,
) -> Result<AssignmentProject, StoreError> {
    let common = parse_common(
        path,
        &record.id,
        &record.package_id,
        &record.display_name,
        &record.created_at,
        &record.updated_at,
    )?;

    let mut rows: BTreeMap<RowKey, AssignmentRow> = BTreeMap::new();
    for entry in record.rows {
        rows.insert(RowKey::Target(entry.target), entry.row);
    }
    for row in record.standalone_rows {
        rows.insert(RowKey::Own(row.def_name.clone()), row);
    }
    let mut sections = BTreeMap::new();
    sections.insert(
        record.schema.def_type.clone(),
        Section {
            schema: record.schema,
            rows,
        },
    );

    Ok(AssignmentProject::from_stored(StoredAssignmentProject {
        id: common.id,
        name: record.name,
        identity: common.identity,
        author: record.author,
        description: record.description,
        refs: record.refs.iter().map(ModId::new).collect(),
        excluded_refs: record.excluded_refs.iter().map(ModId::new).collect(),
        targets: record.targets.iter().map(ModId::new).collect(),
        sections,
        export_dir: record.export_dir,
        created_at: common.created_at,
        updated_at: common.updated_at,
    }))
}

fn from_record_v2(
    path: &Path,
    record: AssignmentRecordV2,
) -> Result<AssignmentProject, StoreError> {
    let common = parse_common(
        path,
        &record.id,
        &record.package_id,
        &record.display_name,
        &record.created_at,
        &record.updated_at,
    )?;

    let sections = record
        .sections
        .into_iter()
        .map(|section| {
            let rows = section
                .rows
                .into_iter()
                .map(|entry| (entry.key, entry.row))
                .collect();
            (
                section.def_type,
                Section {
                    schema: section.schema,
                    rows,
                },
            )
        })
        .collect();

    Ok(AssignmentProject::from_stored(StoredAssignmentProject {
        id: common.id,
        name: record.name,
        identity: common.identity,
        author: record.author,
        description: record.description,
        refs: record.refs.iter().map(ModId::new).collect(),
        excluded_refs: record.excluded_refs.iter().map(ModId::new).collect(),
        targets: record.targets.iter().map(ModId::new).collect(),
        sections,
        export_dir: record.export_dir,
        created_at: common.created_at,
        updated_at: common.updated_at,
    }))
}

/// Reads one file's bytes, dispatching on its own `version` probe —
/// version 1 is migrated through [`from_record_v1`], version 2 read
/// directly through [`from_record_v2`]; anything else is an error.
fn read_project(path: &Path, bytes: &[u8]) -> Result<AssignmentProject, StoreError> {
    let probe: VersionProbe = serde_json::from_slice(bytes).map_err(|e| to_store_error(path, e))?;
    match probe.version {
        1 => {
            let file: AssignmentFileV1 =
                serde_json::from_slice(bytes).map_err(|e| to_store_error(path, e))?;
            from_record_v1(path, file.assignment)
        }
        2 => {
            let file: AssignmentFileV2 =
                serde_json::from_slice(bytes).map_err(|e| to_store_error(path, e))?;
            from_record_v2(path, file.assignment)
        }
        other => Err(to_store_error(
            path,
            format!("unsupported schema version {other}"),
        )),
    }
}

/// Reads and writes patch maker projects: `<profile>/assignments/<id>.json`.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonAssignmentProjectStore;

impl JsonAssignmentProjectStore {
    /// Builds the store. Stateless — every call re-reads/writes the
    /// directory it's given.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl AssignmentProjectStore for JsonAssignmentProjectStore {
    fn load_all(&self, profile_dir: &Path) -> Result<Vec<AssignmentProject>, StoreError> {
        let dir = assignments_dir(profile_dir);
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let entries = fs::read_dir(&dir).map_err(|e| to_store_error(&dir, e))?;

        let mut projects = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| to_store_error(&dir, e))?;
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(|e| to_store_error(&path, e))?;
            let project = read_project(&path, &bytes)?;
            // The file name is never trusted as the id — mirrors
            // `patches.rs`'s own guard against a hand-renamed/copied file
            // silently breaking `assignment_path`'s "one file per id"
            // invariant on the next `save`/`delete`.
            let stem = path.file_stem().and_then(|stem| stem.to_str());
            if stem != Some(project.id().as_str()) {
                return Err(to_store_error(
                    &path,
                    format!(
                        "file name doesn't match its own assignment id {}",
                        project.id()
                    ),
                ));
            }
            projects.push(project);
        }
        // Deterministic regardless of the directory listing's own order.
        projects.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(projects)
    }

    fn save(&self, profile_dir: &Path, project: &AssignmentProject) -> Result<(), StoreError> {
        let dir = assignments_dir(profile_dir);
        fs::create_dir_all(&dir).map_err(|e| to_store_error(&dir, e))?;

        let path = assignment_path(profile_dir, project.id());
        let file = AssignmentFileV2 {
            version: SCHEMA_VERSION,
            assignment: to_record(project),
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| to_store_error(&path, e))?;
        write_atomically(&path, json.as_bytes()).map_err(|e| to_store_error(&path, e))
    }

    fn delete(&self, profile_dir: &Path, id: &AssignmentId) -> Result<(), StoreError> {
        let path = assignment_path(profile_dir, id);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(to_store_error(&path, error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rim_resolve::domain::{Cardinality, FieldPath, FieldRole, FieldSpec, PathSegment};
    use tempfile::tempdir;

    use super::*;

    fn identity() -> PatchModIdentity {
        PatchModIdentity::new("mypatch.parts", "Sample Part Patch").expect("valid identity")
    }

    fn field(tag: &str) -> FieldPath {
        FieldPath::new(vec![PathSegment::Child(tag.to_string())])
    }

    fn schema() -> AssignmentSchema {
        let mut fields = BTreeMap::new();
        fields.insert(
            field("speciesNames"),
            FieldSpec {
                role: FieldRole::TargetKey {
                    def_type: "ThingDef".to_string(),
                },
                cardinality: Cardinality::List,
                observed: (1, 1),
                inferred_role: None,
            },
        );
        AssignmentSchema {
            def_type: "example.PartAssignmentDef".to_string(),
            refs: [ModId::new("example.framework")].into_iter().collect(),
            fields,
            target_shapes: BTreeMap::new(),
        }
    }

    fn standalone_schema() -> AssignmentSchema {
        AssignmentSchema {
            def_type: "example.PartDef".to_string(),
            refs: std::collections::BTreeSet::new(),
            fields: BTreeMap::new(),
            target_shapes: BTreeMap::new(),
        }
    }

    fn project() -> AssignmentProject {
        AssignmentProject::new(
            AssignmentId::derive(
                "abc123",
                identity().package_id(),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "Example race patch".to_string(),
            identity(),
            [ModId::new("example.framework")].into_iter().collect(),
            [ModId::new("some.race.mod")].into_iter().collect(),
            schema(),
            jiff::Timestamp::UNIX_EPOCH,
        )
    }

    /// A [`rim_resolve::domain::KnownDefs`] fake answering "nothing is
    /// already active" — the right answer whenever a test doesn't care
    /// about item-slot or def-name-collision validation (mirrors
    /// `rim-resolve`'s own `NoneKnown` fixture).
    struct NoneKnown;
    impl rim_resolve::domain::KnownDefs for NoneKnown {
        fn contains(&self, _def_type: &str, _name: &str) -> bool {
            false
        }
    }

    #[test]
    fn load_all_on_a_missing_directory_is_empty() {
        let dir = tempdir().expect("tempdir");

        let projects = JsonAssignmentProjectStore::new()
            .load_all(dir.path())
            .expect("load_all must succeed");

        assert!(projects.is_empty());
    }

    #[test]
    fn round_trips_a_project_with_a_row_through_save_and_load_all() {
        let dir = tempdir().expect("tempdir");
        let mut original = project();
        original
            .set_row(
                "example.PartAssignmentDef",
                RowKey::Target(TargetRef {
                    key_field: field("speciesNames"),
                    def: rim_resolve::domain::DefKey {
                        def_type: "ThingDef".to_string(),
                        def_name: "Human".to_string(),
                    },
                }),
                AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "mypatch_parts_Human".to_string(),
                    note: Some("a note".to_string()),
                },
                &NoneKnown,
            )
            .expect("valid row");

        let store = JsonAssignmentProjectStore::new();
        store
            .save(dir.path(), &original)
            .expect("save must succeed");
        let mut loaded = store
            .load_all(dir.path())
            .expect("load_all must succeed after a save");

        assert_eq!(loaded.len(), 1);
        let loaded = loaded.remove(0);
        assert_eq!(loaded, original);
    }

    #[test]
    fn round_trips_a_project_with_two_sections() {
        let dir = tempdir().expect("tempdir");
        let mut original = project();
        original
            .add_section(standalone_schema())
            .expect("add a second section");
        original
            .set_row(
                "example.PartDef",
                RowKey::Own("mypatch_parts_PartA".to_string()),
                AssignmentRow {
                    values: BTreeMap::new(),
                    def_name: "mypatch_parts_PartA".to_string(),
                    note: None,
                },
                &NoneKnown,
            )
            .expect("valid own row");

        let store = JsonAssignmentProjectStore::new();
        store
            .save(dir.path(), &original)
            .expect("save must succeed");
        let mut loaded = store
            .load_all(dir.path())
            .expect("load_all must succeed after a save");

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded.remove(0), original);
    }

    #[test]
    fn save_then_delete_removes_the_project_from_load_all() {
        let dir = tempdir().expect("tempdir");
        let project = project();
        let store = JsonAssignmentProjectStore::new();
        store.save(dir.path(), &project).expect("save must succeed");

        store
            .delete(dir.path(), project.id())
            .expect("delete must succeed");

        assert!(
            store
                .load_all(dir.path())
                .expect("load_all must succeed")
                .is_empty()
        );
    }

    #[test]
    fn deleting_an_id_with_no_file_on_disk_is_not_an_error() {
        let dir = tempdir().expect("tempdir");
        let bogus: AssignmentId = "abcdef012345".parse().expect("valid id");

        let result = JsonAssignmentProjectStore::new().delete(dir.path(), &bogus);

        assert!(result.is_ok());
    }

    #[test]
    fn load_all_is_sorted_by_id_regardless_of_directory_order() {
        let dir = tempdir().expect("tempdir");
        let store = JsonAssignmentProjectStore::new();
        let a = AssignmentProject::new(
            AssignmentId::derive(
                "abc123",
                identity().package_id(),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "First".to_string(),
            identity(),
            std::collections::BTreeSet::new(),
            std::collections::BTreeSet::new(),
            schema(),
            jiff::Timestamp::UNIX_EPOCH,
        );
        let other_identity = PatchModIdentity::new("other.patch", "Other").expect("valid identity");
        let b = AssignmentProject::new(
            AssignmentId::derive(
                "abc123",
                other_identity.package_id(),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "Second".to_string(),
            other_identity,
            std::collections::BTreeSet::new(),
            std::collections::BTreeSet::new(),
            schema(),
            jiff::Timestamp::UNIX_EPOCH,
        );
        store.save(dir.path(), &b).expect("save b must succeed");
        store.save(dir.path(), &a).expect("save a must succeed");

        let loaded = store.load_all(dir.path()).expect("load_all must succeed");

        let ids: Vec<_> = loaded.iter().map(AssignmentProject::id).cloned().collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn an_unsupported_schema_version_is_an_error_naming_the_file() {
        let dir = tempdir().expect("tempdir");
        let assignments_dir = dir.path().join(DIR_NAME);
        fs::create_dir_all(&assignments_dir).expect("create assignments dir");
        fs::write(
            assignments_dir.join("abcdef012345.json"),
            r#"{"version":99,"assignment":{}}"#,
        )
        .expect("seed a bogus file");

        let result = JsonAssignmentProjectStore::new().load_all(dir.path());

        match result {
            Err(error) => assert!(
                error.to_string().contains("unsupported schema version 99"),
                "unexpected error message: {error}"
            ),
            Ok(_) => panic!("a v99 payload must not be accepted"),
        }
    }

    #[test]
    fn load_all_errors_when_a_files_stem_does_not_match_its_own_assignment_id() {
        let dir = tempdir().expect("tempdir");
        let project = project();
        let store = JsonAssignmentProjectStore::new();
        store.save(dir.path(), &project).expect("save must succeed");
        let assignments_dir = dir.path().join(DIR_NAME);
        let original_path = assignments_dir.join(format!("{}.json", project.id()));
        let renamed_path = assignments_dir.join("renamed.json");
        fs::rename(&original_path, &renamed_path)
            .expect("rename the file out from under its own id");

        let result = store.load_all(dir.path());

        match result {
            Err(error) => assert!(
                error.to_string().contains(project.id().as_str()),
                "error must name the mismatched assignment id: {error}"
            ),
            Ok(_) => {
                panic!("a file whose name doesn't match its own assignment id must be rejected")
            }
        }
    }

    #[test]
    fn a_single_unreadable_file_fails_load_all_naming_it_rather_than_being_skipped() {
        let dir = tempdir().expect("tempdir");
        let assignments_dir = dir.path().join(DIR_NAME);
        fs::create_dir_all(&assignments_dir).expect("create assignments dir");
        fs::write(
            assignments_dir.join("abcdef012345.json"),
            b"not json at all",
        )
        .expect("seed a corrupt file");

        let result = JsonAssignmentProjectStore::new().load_all(dir.path());

        assert!(result.is_err(), "a corrupt file must surface as an error");
    }

    #[test]
    fn a_file_missing_a_required_field_is_reported_not_silently_defaulted() {
        let dir = tempdir().expect("tempdir");
        let assignments_dir = dir.path().join(DIR_NAME);
        fs::create_dir_all(&assignments_dir).expect("create assignments dir");
        fs::write(
            assignments_dir.join("abcdef012345.json"),
            r#"{"version":2,"assignment":{"id":"abcdef012345"}}"#,
        )
        .expect("seed a truncated file");

        let result = JsonAssignmentProjectStore::new().load_all(dir.path());

        assert!(result.is_err(), "a truncated record must not load");
    }

    /// A hand-written version-1, target-keyed file (the single-schema
    /// shape) must still load, becoming a
    /// project with one section keyed by the old top-level schema's own
    /// def type, its `rows` entries as [`RowKey::Target`] rows.
    #[test]
    fn a_version_1_target_keyed_file_migrates_into_one_section() {
        let dir = tempdir().expect("tempdir");
        let assignments_dir = dir.path().join(DIR_NAME);
        fs::create_dir_all(&assignments_dir).expect("create assignments dir");
        let v1 = serde_json::json!({
            "version": 1,
            "assignment": {
                "id": "abcdef012345",
                "name": "Example race patch",
                "packageId": "mypatch.parts",
                "displayName": "Sample Part Patch",
                "author": "rimmerge",
                "description": "",
                "refs": ["example.framework"],
                "excludedRefs": [],
                "targets": ["some.race.mod"],
                "schema": schema(),
                "rows": [{
                    "target": {
                        "key_field": field("speciesNames"),
                        "def": {"def_type": "ThingDef", "def_name": "Human"},
                    },
                    "row": {
                        "values": {},
                        "def_name": "mypatch_parts_Human",
                        "note": null,
                    },
                }],
                "exportDir": null,
                "createdAt": jiff::Timestamp::UNIX_EPOCH.to_string(),
                "updatedAt": jiff::Timestamp::UNIX_EPOCH.to_string(),
            },
        });
        fs::write(
            assignments_dir.join("abcdef012345.json"),
            serde_json::to_vec(&v1).expect("serialize v1 fixture"),
        )
        .expect("seed a v1 file");

        let mut loaded = JsonAssignmentProjectStore::new()
            .load_all(dir.path())
            .expect("a v1 file must still load");

        assert_eq!(loaded.len(), 1);
        let project = loaded.remove(0);
        let section = project
            .section("example.PartAssignmentDef")
            .expect("migrated section present");
        assert_eq!(section.rows.len(), 1);
        assert!(section.rows.contains_key(&RowKey::Target(TargetRef {
            key_field: field("speciesNames"),
            def: rim_resolve::domain::DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Human".to_string(),
            },
        })));
    }

    /// An even older version-1 file with no `standaloneRows` key on
    /// `AssignmentRecordV1` at all (entirely absent, not merely an empty
    /// array) must still load, its own free-standing rows defaulting to
    /// empty via `#[serde(default)]` rather than failing to
    /// parse. Distinct in intent from
    /// `a_version_1_target_keyed_file_migrates_into_one_section` above
    /// (which happens to omit the same key but exists to prove the
    /// `rows` -> `RowKey::Target` migration, not this tolerance).
    #[test]
    fn a_version_1_file_predating_standalone_rows_still_loads() {
        let dir = tempdir().expect("tempdir");
        let assignments_dir = dir.path().join(DIR_NAME);
        fs::create_dir_all(&assignments_dir).expect("create assignments dir");
        let v1 = serde_json::json!({
            "version": 1,
            "assignment": {
                "id": "abcdef012345",
                "name": "Example race patch",
                "packageId": "mypatch.parts",
                "displayName": "Sample Part Patch",
                "author": "rimmerge",
                "description": "",
                "refs": ["example.framework"],
                "excludedRefs": [],
                "targets": ["some.race.mod"],
                "schema": schema(),
                "rows": [],
                // No "standaloneRows" key at all — the pre-standalone-mode
                // shape, not just an empty array of it.
                "exportDir": null,
                "createdAt": jiff::Timestamp::UNIX_EPOCH.to_string(),
                "updatedAt": jiff::Timestamp::UNIX_EPOCH.to_string(),
            },
        });
        fs::write(
            assignments_dir.join("abcdef012345.json"),
            serde_json::to_vec(&v1).expect("serialize v1 fixture"),
        )
        .expect("seed a pre-standalone-rows v1 file");

        let mut loaded = JsonAssignmentProjectStore::new()
            .load_all(dir.path())
            .expect("a file predating standaloneRows must still load");

        assert_eq!(loaded.len(), 1);
        let project = loaded.remove(0);
        let section = project
            .section("example.PartAssignmentDef")
            .expect("migrated section present");
        assert!(
            section.rows.is_empty(),
            "no rows of either shape were ever recorded: {section:?}"
        );
    }

    /// A hand-written version-1, standalone ("new def") file — the
    /// pre-6b `standalone_rows` shape — migrates its rows into
    /// [`RowKey::Own`] entries of the same one section.
    #[test]
    fn a_version_1_standalone_file_migrates_own_rows() {
        let dir = tempdir().expect("tempdir");
        let assignments_dir = dir.path().join(DIR_NAME);
        fs::create_dir_all(&assignments_dir).expect("create assignments dir");
        let v1 = serde_json::json!({
            "version": 1,
            "assignment": {
                "id": "abcdef012345",
                "name": "New Part",
                "packageId": "sample.newpart",
                "displayName": "Sample's New Part",
                "author": "rimmerge",
                "description": "",
                "refs": ["example.framework"],
                "excludedRefs": [],
                "targets": [],
                "schema": standalone_schema(),
                "rows": [],
                "standaloneRows": [{
                    "values": {},
                    "def_name": "mypatch_newpart_Tail",
                    "note": null,
                }],
                "exportDir": null,
                "createdAt": jiff::Timestamp::UNIX_EPOCH.to_string(),
                "updatedAt": jiff::Timestamp::UNIX_EPOCH.to_string(),
            },
        });
        fs::write(
            assignments_dir.join("abcdef012345.json"),
            serde_json::to_vec(&v1).expect("serialize v1 fixture"),
        )
        .expect("seed a v1 file");

        let mut loaded = JsonAssignmentProjectStore::new()
            .load_all(dir.path())
            .expect("a v1 standalone file must still load");

        assert_eq!(loaded.len(), 1);
        let project = loaded.remove(0);
        let section = project
            .section("example.PartDef")
            .expect("migrated section present");
        assert_eq!(
            section
                .rows
                .get(&RowKey::Own("mypatch_newpart_Tail".to_string())),
            Some(&AssignmentRow {
                values: BTreeMap::new(),
                def_name: "mypatch_newpart_Tail".to_string(),
                note: None,
            })
        );
    }

    /// A v1 file saved by the store is never rewritten just by being
    /// loaded — but a subsequent `save` on the migrated project always
    /// produces a version-2 file.
    #[test]
    fn saving_a_migrated_v1_project_writes_version_2() {
        let dir = tempdir().expect("tempdir");
        let assignments_dir = dir.path().join(DIR_NAME);
        fs::create_dir_all(&assignments_dir).expect("create assignments dir");
        let v1 = serde_json::json!({
            "version": 1,
            "assignment": {
                "id": "abcdef012345",
                "name": "Example race patch",
                "packageId": "mypatch.parts",
                "displayName": "Sample Part Patch",
                "author": "rimmerge",
                "description": "",
                "refs": ["example.framework"],
                "excludedRefs": [],
                "targets": ["some.race.mod"],
                "schema": schema(),
                "rows": [],
                "exportDir": null,
                "createdAt": jiff::Timestamp::UNIX_EPOCH.to_string(),
                "updatedAt": jiff::Timestamp::UNIX_EPOCH.to_string(),
            },
        });
        let path = assignments_dir.join("abcdef012345.json");
        fs::write(
            &path,
            serde_json::to_vec(&v1).expect("serialize v1 fixture"),
        )
        .expect("seed a v1 file");
        let store = JsonAssignmentProjectStore::new();
        let mut loaded = store.load_all(dir.path()).expect("load v1");
        let project = loaded.remove(0);

        store.save(dir.path(), &project).expect("save must succeed");

        let bytes = fs::read(&path).expect("read back the saved file");
        let probe: VersionProbe = serde_json::from_slice(&bytes).expect("valid json");
        assert_eq!(probe.version, 2);
    }
}
