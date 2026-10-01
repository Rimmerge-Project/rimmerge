//! [`JsonPatchProjectStore`]: `<profile>/patches/<patch-id>.json`, one
//! versioned envelope per compat patch project — mirrors
//! `decisions.rs::JsonDecisionStore`'s own shape. One file per project, so
//! two projects never contend for one file. Deleting a project also removes
//! its own `<profile>/patches/<id>.prev/` and `<id>.installed.prev/` backup
//! folders (`ExportPatch`'s own `patch_backup_dir`, `rim-session`) — a
//! deleted project leaves no trace behind for a future project id to collide
//! with.

use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{
    Action, Decision, FindingKey, PatchId, PatchModIdentity, PatchProject, PatchScope,
    StoredPatchProject,
};
use rim_session::ports::{PatchProjectStore, StoreError};
use serde::{Deserialize, Serialize};

use crate::atomic::write_atomically;

/// The subdirectory under a profile directory every project's file lives
/// under.
const DIR_NAME: &str = "patches";
/// **Why a new `FindingKey` text form bumps this version.** Mirrors
/// `decisions.rs::SCHEMA_VERSION`'s own reasoning: `load_all`'s own
/// `entry.key.parse()` (in `from_record` below) treats an unrecognized key
/// string as a hard, whole-*file* parse error, not a silently-skipped
/// unknown field — and unlike `decisions.json`'s many small per-decision
/// records, one project file bundles every decision for that patch, so
/// one renamed key fails the whole project's load, not just one decision.
///
/// - **2**: `FindingKey::RuntimePatchCollision`'s own text form changed
///   from `harmony_patch_collision:...` to `runtime_patch_collision:...`
///   (the runtime-method-patching concept rename — see
///   `crates/rim-analyzer/src/domain/assembly.rs`). [`PatchFileV1`]'s own
///   JSON *shape* is unchanged across this bump — only the finding-key
///   *text* inside a [`DecisionRecord`]'s `key` field differs — so
///   `MIN_SUPPORTED_VERSION` stays `1`: an old v1 file with the old text
///   still parses structurally, but if it names the renamed finding,
///   `from_record`'s own `entry.key.parse()` now fails with the same
///   "unrecognized finding key kind" error `decisions.rs` gives (and the
///   same recovery: delete the one offending decision, or the whole
///   project file, and redo it — `rim_resolve`'s own
///   `FindingKeyParseError::UnknownKind` now says so directly). `save`
///   always writes the current version regardless.
const SCHEMA_VERSION: u32 = 2;
const MIN_SUPPORTED_VERSION: u32 = 1;

/// One decision's on-disk shape — identical to `decisions.rs`'s own
/// `DecisionRecord` (the canonical [`FindingKey`] text as the key, `action`
/// as [`Action`]'s own tagged JSON shape), duplicated rather than shared
/// since `decisions.rs`'s own struct is private to that module.
#[derive(Debug, Serialize, Deserialize)]
struct DecisionRecord {
    key: String,
    action: Action,
    note: Option<String>,
    decided_at: String,
}

/// One project's persisted shape:
/// camelCase keys, matching every other field Rimmerge writes into a
/// mod's own `rimmerge.json`, except [`DecisionRecord`]'s own fields,
/// which stay exactly `decisions.json`'s shape.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchRecordV1 {
    id: String,
    name: String,
    package_id: String,
    display_name: String,
    author: String,
    description: String,
    scope: Vec<String>,
    decisions: Vec<DecisionRecord>,
    export_dir: Option<PathBuf>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct PatchFileV1 {
    version: u32,
    patch: PatchRecordV1,
}

/// Just enough of the envelope to check the schema version before
/// attempting to deserialize the full, version-specific shape — the same
/// pattern `decisions.rs`/`rules.rs` both use.
#[derive(Debug, Deserialize)]
struct VersionProbe {
    version: u32,
}

fn to_store_error(path: &Path, error: impl std::fmt::Display) -> StoreError {
    StoreError(format!("{}: {error}", path.display()))
}

fn patches_dir(profile_dir: &Path) -> PathBuf {
    profile_dir.join(DIR_NAME)
}

fn patch_path(profile_dir: &Path, id: &PatchId) -> PathBuf {
    patches_dir(profile_dir).join(format!("{id}.json"))
}

fn to_record(project: &PatchProject) -> PatchRecordV1 {
    PatchRecordV1 {
        id: project.id().to_string(),
        name: project.name().to_string(),
        package_id: project.identity().package_id().to_string(),
        display_name: project.identity().display_name().to_string(),
        author: project.author().to_string(),
        description: project.description().to_string(),
        scope: project
            .scope()
            .members()
            .iter()
            .map(ModId::to_string)
            .collect(),
        decisions: project
            .decisions()
            .iter()
            .map(|decision| DecisionRecord {
                key: decision.key.to_string(),
                action: decision.action.clone(),
                note: decision.note.clone(),
                decided_at: decision.decided_at.to_string(),
            })
            .collect(),
        export_dir: project.export_dir().map(Path::to_path_buf),
        created_at: project.created_at().to_string(),
        updated_at: project.updated_at().to_string(),
    }
}

fn from_record(path: &Path, record: PatchRecordV1) -> Result<PatchProject, StoreError> {
    let id: PatchId = record
        .id
        .parse()
        .map_err(|e| to_store_error(path, format!("patch id {:?}: {e}", record.id)))?;
    let identity = PatchModIdentity::new(&record.package_id, &record.display_name)
        .map_err(|e| to_store_error(path, format!("identity: {e}")))?;
    let scope = PatchScope::new(record.scope.iter().map(ModId::new))
        .map_err(|e| to_store_error(path, format!("scope: {e}")))?;
    let created_at: jiff::Timestamp = record
        .created_at
        .parse()
        .map_err(|e| to_store_error(path, format!("created_at {:?}: {e}", record.created_at)))?;
    let updated_at: jiff::Timestamp = record
        .updated_at
        .parse()
        .map_err(|e| to_store_error(path, format!("updated_at {:?}: {e}", record.updated_at)))?;

    let decisions = record
        .decisions
        .into_iter()
        .map(|entry| {
            let key: FindingKey = entry
                .key
                .parse()
                .map_err(|e| to_store_error(path, format!("finding key {:?}: {e}", entry.key)))?;
            let decided_at = entry.decided_at.parse().map_err(|e| {
                to_store_error(path, format!("decided_at {:?}: {e}", entry.decided_at))
            })?;
            Ok(Decision {
                key,
                action: entry.action,
                note: entry.note,
                decided_at,
            })
        })
        .collect::<Result<Vec<Decision>, StoreError>>()?;

    // `PatchProject::from_stored`, not `PatchProject::decide` per
    // decision: a decision on file can legitimately name a key the
    // project's *current* scope no longer admits (a scope shrink orphans
    // it rather than deleting it — `ScopeChange`'s own contract), and
    // `decide`'s scope check would reject the whole file over that one
    // decision.
    PatchProject::from_stored(StoredPatchProject {
        id,
        name: record.name,
        identity,
        author: record.author,
        description: record.description,
        scope,
        decisions,
        export_dir: record.export_dir,
        created_at,
        updated_at,
    })
    .map_err(|e| to_store_error(path, e))
}

/// Reads and writes compat patch projects: `<profile>/patches/<patch-id>.json`.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonPatchProjectStore;

impl JsonPatchProjectStore {
    /// Builds the store. Stateless — every call re-reads/writes the
    /// directory it's given.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl PatchProjectStore for JsonPatchProjectStore {
    fn load_all(&self, profile_dir: &Path) -> Result<Vec<PatchProject>, StoreError> {
        let dir = patches_dir(profile_dir);
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
            let probe: VersionProbe =
                serde_json::from_slice(&bytes).map_err(|e| to_store_error(&path, e))?;
            if !(MIN_SUPPORTED_VERSION..=SCHEMA_VERSION).contains(&probe.version) {
                return Err(to_store_error(
                    &path,
                    format!("unsupported schema version {}", probe.version),
                ));
            }
            let file: PatchFileV1 =
                serde_json::from_slice(&bytes).map_err(|e| to_store_error(&path, e))?;
            let project = from_record(&path, file.patch)?;
            // The file name is never trusted as the id (`from_record`
            // parses `patch.id` from the payload itself) — but a file
            // renamed or copied by hand now names a different patch than
            // its own id says, silently breaking `patch_path`'s own
            // "one file per id" invariant on the next `save`/`delete`.
            // Caught here, naming the file, rather than a corrupt-looking
            // divergence discovered later.
            let stem = path.file_stem().and_then(|stem| stem.to_str());
            if stem != Some(project.id().as_str()) {
                return Err(to_store_error(
                    &path,
                    format!("file name doesn't match its own patch id {}", project.id()),
                ));
            }
            projects.push(project);
        }
        // Deterministic regardless of the directory listing's own order.
        projects.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(projects)
    }

    fn save(&self, profile_dir: &Path, project: &PatchProject) -> Result<(), StoreError> {
        let dir = patches_dir(profile_dir);
        fs::create_dir_all(&dir).map_err(|e| to_store_error(&dir, e))?;

        let path = patch_path(profile_dir, project.id());
        let file = PatchFileV1 {
            version: SCHEMA_VERSION,
            patch: to_record(project),
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| to_store_error(&path, e))?;
        write_atomically(&path, json.as_bytes()).map_err(|e| to_store_error(&path, e))
    }

    fn delete(&self, profile_dir: &Path, id: &PatchId) -> Result<(), StoreError> {
        let path = patch_path(profile_dir, id);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(to_store_error(&path, error)),
        }

        // `ExportPatch`'s own backup folders for this id
        // (`<id>.prev`/`<id>.installed.prev`, `rim-session`'s
        // `patch_backup_dir`) — never left behind for a future project to
        // collide with, or to be mistaken for a still-live project's own
        // backup by anything scanning `patches/`.
        for suffix in ["prev", "installed.prev"] {
            let backup_dir = patches_dir(profile_dir).join(format!("{id}.{suffix}"));
            if let Err(error) = fs::remove_dir_all(&backup_dir)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(to_store_error(&backup_dir, error));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use tempfile::tempdir;

    use super::*;

    fn identity() -> PatchModIdentity {
        PatchModIdentity::new("sample.abcompat", "A + B Compatibility").expect("valid identity")
    }

    fn scope() -> PatchScope {
        PatchScope::new([ModId::new("fixture.moda"), ModId::new("fixture.modb")])
            .expect("two distinct members")
    }

    fn project() -> PatchProject {
        PatchProject::new(
            PatchId::derive(
                "abc123",
                identity().package_id(),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "AB compat".to_string(),
            identity(),
            scope(),
            jiff::Timestamp::UNIX_EPOCH,
        )
    }

    #[test]
    fn load_all_on_a_missing_directory_is_empty() {
        let dir = tempdir().expect("tempdir");

        let projects = JsonPatchProjectStore::new()
            .load_all(dir.path())
            .expect("load_all must succeed");

        assert!(projects.is_empty());
    }

    #[test]
    fn round_trips_a_project_through_save_and_load_all() {
        let dir = tempdir().expect("tempdir");
        // Built via `from_stored` (rather than `new` + setters) so the
        // test can also cover `updated_at` differing from `created_at` —
        // `PatchProject::new` has no other way to produce that, by design
        // (see `PatchProject::from_stored`'s own doc comment).
        let original = PatchProject::from_stored(StoredPatchProject {
            id: PatchId::derive(
                "abc123",
                identity().package_id(),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            name: "AB compat".to_string(),
            identity: identity(),
            author: "sample".to_string(),
            description: "a description".to_string(),
            scope: scope(),
            decisions: vec![Decision {
                key: FindingKey::DefOverride {
                    key: rim_resolve::domain::DefKey {
                        def_type: "ThingDef".to_string(),
                        def_name: "Fixture_Wall".to_string(),
                    },
                    owners: [ModId::new("fixture.moda"), ModId::new("fixture.modb")]
                        .into_iter()
                        .collect(),
                },
                action: Action::Ignore,
                note: Some("keep as-is".to_string()),
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            }],
            export_dir: Some(PathBuf::from("D:/mods/ab-compat")),
            created_at: jiff::Timestamp::UNIX_EPOCH,
            updated_at: jiff::Timestamp::from_second(1000).expect("valid timestamp"),
        })
        .expect("Ignore is always a valid patch action");

        let store = JsonPatchProjectStore::new();
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
    fn save_then_delete_removes_the_project_from_load_all() {
        let dir = tempdir().expect("tempdir");
        let project = project();
        let store = JsonPatchProjectStore::new();
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
    fn delete_also_removes_the_prev_and_installed_prev_backup_folders() {
        let dir = tempdir().expect("tempdir");
        let project = project();
        let store = JsonPatchProjectStore::new();
        store.save(dir.path(), &project).expect("save must succeed");
        // `ExportPatch`'s own backup folders (`rim-session`'s
        // `patch_backup_dir`), seeded by hand since this crate's own
        // tests never run the writer.
        let patches_dir = dir.path().join(DIR_NAME);
        let prev = patches_dir.join(format!("{}.prev", project.id()));
        let installed_prev = patches_dir.join(format!("{}.installed.prev", project.id()));
        fs::create_dir_all(prev.join("About")).expect("seed the export backup");
        fs::create_dir_all(installed_prev.join("About")).expect("seed the install backup");

        store
            .delete(dir.path(), project.id())
            .expect("delete must succeed");

        assert!(!prev.exists(), "the export backup must be removed too");
        assert!(
            !installed_prev.exists(),
            "the install backup must be removed too"
        );
    }

    #[test]
    fn deleting_an_id_with_no_file_on_disk_is_not_an_error() {
        let dir = tempdir().expect("tempdir");
        let bogus: PatchId = "abcdef012345".parse().expect("valid id");

        let result = JsonPatchProjectStore::new().delete(dir.path(), &bogus);

        assert!(result.is_ok());
    }

    #[test]
    fn load_all_is_sorted_by_id_regardless_of_directory_order() {
        let dir = tempdir().expect("tempdir");
        let store = JsonPatchProjectStore::new();
        let a = PatchProject::new(
            PatchId::derive(
                "abc123",
                identity().package_id(),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "First".to_string(),
            identity(),
            scope(),
            jiff::Timestamp::UNIX_EPOCH,
        );
        let other_identity =
            PatchModIdentity::new("other.compat", "Other").expect("valid identity");
        let b = PatchProject::new(
            PatchId::derive(
                "abc123",
                other_identity.package_id(),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "Second".to_string(),
            other_identity,
            scope(),
            jiff::Timestamp::UNIX_EPOCH,
        );
        store.save(dir.path(), &b).expect("save b must succeed");
        store.save(dir.path(), &a).expect("save a must succeed");

        let loaded = store.load_all(dir.path()).expect("load_all must succeed");

        let ids: Vec<_> = loaded.iter().map(PatchProject::id).cloned().collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn an_unsupported_schema_version_is_an_error_naming_the_file() {
        let dir = tempdir().expect("tempdir");
        let patches_dir = dir.path().join(DIR_NAME);
        fs::create_dir_all(&patches_dir).expect("create patches dir");
        fs::write(
            patches_dir.join("abcdef012345.json"),
            r#"{"version":99,"patch":{}}"#,
        )
        .expect("seed a bogus file");

        let result = JsonPatchProjectStore::new().load_all(dir.path());

        match result {
            Err(error) => assert!(
                error.to_string().contains("unsupported schema version 99"),
                "unexpected error message: {error}"
            ),
            Ok(_) => panic!("a v99 payload must not be accepted"),
        }
    }

    /// `PatchFileV1`'s own JSON *shape* is unchanged by the 1 → 2 bump
    /// (`SCHEMA_VERSION`'s own doc comment) — only the finding-key text
    /// inside a decision may differ — so `MIN_SUPPORTED_VERSION` staying
    /// `1` must mean a genuine pre-rename file, with an ordinary
    /// (never-renamed) finding key, still loads.
    #[test]
    fn a_v1_file_with_an_ordinary_finding_key_still_loads() {
        let dir = tempdir().expect("tempdir");
        let project = project();
        let store = JsonPatchProjectStore::new();
        store.save(dir.path(), &project).expect("save must succeed");

        let path = patch_path(dir.path(), project.id());
        let bytes = fs::read(&path).expect("read back the saved file");
        let mut value: serde_json::Value =
            serde_json::from_slice(&bytes).expect("saved file must be valid json");
        assert_eq!(value["version"], SCHEMA_VERSION);
        value["version"] = serde_json::json!(1);
        fs::write(
            &path,
            serde_json::to_vec_pretty(&value).expect("re-serialize"),
        )
        .expect("write the downgraded file back");

        let loaded = store
            .load_all(dir.path())
            .expect("a v1 file must still load");

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id(), project.id());
    }

    /// A decision keyed on the pre-rename `harmony_patch_collision:...`
    /// text fails the whole file's load with a clear, recoverable error —
    /// not a silently-dropped decision — the same "unrecognized finding
    /// key kind" shape `decisions.rs` gives, now with a recovery hint
    /// (see `FindingKeyParseError::UnknownKind`'s own doc comment).
    #[test]
    fn a_stale_renamed_finding_key_fails_the_whole_file_with_a_recovery_hint() {
        let dir = tempdir().expect("tempdir");
        let mut original = project();
        original
            .decide(Decision {
                key: FindingKey::RuntimePatchCollision {
                    target_type: "Verse.Pawn".to_string(),
                    target_method: "Kill".to_string(),
                    owners: [ModId::new("fixture.moda"), ModId::new("fixture.modb")]
                        .into_iter()
                        .collect(),
                },
                action: Action::Ignore,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("Ignore is always a valid runtime-patch-collision action");
        let store = JsonPatchProjectStore::new();
        store
            .save(dir.path(), &original)
            .expect("save must succeed");

        // Rewrite the just-saved, current-vocabulary key text back to the
        // pre-rename spelling — reproducing exactly what a genuine old
        // file would contain, without hand-building the rest of the
        // record's own JSON shape.
        let path = patch_path(dir.path(), original.id());
        let bytes = fs::read_to_string(&path).expect("read back the saved file");
        let stale = bytes.replace("runtime_patch_collision:", "harmony_patch_collision:");
        assert_ne!(
            stale, bytes,
            "the key text must actually have been rewritten"
        );
        fs::write(&path, stale).expect("write the stale-key file back");

        let result = JsonPatchProjectStore::new().load_all(dir.path());

        match result {
            Err(error) => assert!(
                error.to_string().contains("unrecognized finding key kind"),
                "unexpected error message: {error}"
            ),
            Ok(_) => panic!("a file with a stale, renamed finding key must not load"),
        }
    }

    #[test]
    fn load_all_errors_when_a_files_stem_does_not_match_its_own_patch_id() {
        let dir = tempdir().expect("tempdir");
        let project = project();
        let store = JsonPatchProjectStore::new();
        store.save(dir.path(), &project).expect("save must succeed");
        let patches_dir = dir.path().join(DIR_NAME);
        let original_path = patches_dir.join(format!("{}.json", project.id()));
        let renamed_path = patches_dir.join("renamed.json");
        fs::rename(&original_path, &renamed_path)
            .expect("rename the file out from under its own id");

        let result = store.load_all(dir.path());

        match result {
            Err(error) => assert!(
                error.to_string().contains(project.id().as_str()),
                "error must name the mismatched patch id: {error}"
            ),
            Ok(_) => panic!("a file whose name doesn't match its own patch id must be rejected"),
        }
    }

    /// `set_scope` legitimately shrinks a
    /// project's scope and leaves an admitted decision orphaned rather
    /// than deleting it (`PatchProject::set_scope`'s own `ScopeChange`
    /// contract). `load_all` must round-trip that file, not reject the
    /// whole project because the orphaned decision no longer replays
    /// through `PatchProject::decide`'s scope check.
    #[test]
    fn load_all_returns_a_project_whose_scope_shrink_orphaned_a_decision() {
        let dir = tempdir().expect("tempdir");
        let mut original = project();
        let key = FindingKey::DefOverride {
            key: rim_resolve::domain::DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Fixture_Wall".to_string(),
            },
            owners: [ModId::new("fixture.moda"), ModId::new("fixture.modb")]
                .into_iter()
                .collect(),
        };
        original
            .decide(Decision {
                key: key.clone(),
                action: Action::Ignore,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("Ignore is valid while the key is still admitted");

        let shrunk = PatchScope::new([ModId::new("fixture.moda"), ModId::new("fixture.modc")])
            .expect("two distinct members");
        let change = original.set_scope(shrunk);
        assert_eq!(
            change.now_orphaned,
            vec![key.clone()],
            "sanity: the scope shrink must orphan exactly this decision"
        );

        let store = JsonPatchProjectStore::new();
        store
            .save(dir.path(), &original)
            .expect("save must succeed even with an orphaned decision on file");

        let mut loaded = store
            .load_all(dir.path())
            .expect("load_all must not reject a file holding a legitimately orphaned decision");
        assert_eq!(loaded.len(), 1);
        let mut loaded = loaded.remove(0);

        assert_eq!(
            loaded.decisions().get(&key),
            Some(&Decision {
                key: key.clone(),
                action: Action::Ignore,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            }),
            "the orphaned decision must survive the round trip intact"
        );

        // Still prunable through the domain's own API — nothing about
        // trusting the stored decision disables `prune_orphaned`.
        let no_longer_live: BTreeSet<FindingKey> = BTreeSet::new();
        let pruned = loaded.prune_orphaned(&no_longer_live);
        assert_eq!(
            pruned,
            vec![Decision {
                key: key.clone(),
                action: Action::Ignore,
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            }]
        );
        assert!(loaded.decisions().get(&key).is_none());
    }

    #[test]
    fn a_single_unreadable_file_fails_load_all_naming_it_rather_than_being_skipped() {
        let dir = tempdir().expect("tempdir");
        let patches_dir = dir.path().join(DIR_NAME);
        fs::create_dir_all(&patches_dir).expect("create patches dir");
        fs::write(patches_dir.join("abcdef012345.json"), b"not json at all")
            .expect("seed a corrupt file");

        let result = JsonPatchProjectStore::new().load_all(dir.path());

        assert!(result.is_err(), "a corrupt file must surface as an error");
    }
}
