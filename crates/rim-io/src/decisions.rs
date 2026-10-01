//! [`JsonDecisionStore`]: `decisions.json`, a versioned envelope over
//! every [`Decision`] on file.

use std::fs;
use std::path::Path;

use rim_resolve::domain::{Action, Decision, DecisionSet, FindingKey};
use rim_session::ports::{DecisionStore, StoreError};
use serde::{Deserialize, Serialize};

use crate::atomic::write_atomically;

const FILE_NAME: &str = "decisions.json";
/// **Why a new `FindingKey` text form bumps this version.** Unlike
/// `rules.json`'s own additive, `#[serde(default)]`-backed fields — which
/// an older struct simply ignores on read — a new canonical key text form
/// (a new decidable `FindingKey` variant, or a new `EdgeKind` embedded in
/// one) is not that kind of change: `load`'s own `record.key.parse()`
/// treats an unrecognized key string as a **hard, whole-file** parse error
/// (see this file's own `load` below), not a silently-skipped unknown
/// field. So a `decisions.json` a newer build writes, once it contains even
/// one newly-keyed decision, would make every *older* build's `load` fail
/// outright — not just on that one decision, on every decision in the
/// profile, since `load` bails at the first unparsable key. Bumping the
/// schema version turns that into the same clean, actionable "unsupported
/// schema version" refusal `rules.json`/`patches/<id>.json` already give an
/// old build reading a newer file, instead of a confusing "finding key ...:
/// unrecognized finding key kind" error while the envelope's own `version`
/// still claims to be one the old build understands. No migration is needed
/// alongside a bump (`MIN_SUPPORTED_VERSION` stays `1`): an older file
/// could never contain the new text in the first place, so there is nothing
/// to translate on load — `save` always writes the current version, the
/// same one-way "read old, always write current" shape `rules.json`
/// follows.
///
/// Each version's new key text:
///
/// - **2**: `FindingKey::PatchWillFail` (`patch_will_fail:...`).
/// - **3**: `EdgeKind::PatchSelectsInjectedNode`
///   (`patch_selects_injected_node`) inside an
///   `edge_dropped:`/`declaration_questioned:`/`placement_questioned:` key.
/// - **4**: `FindingKey::TranspilerCollision` (`transpiler_collision:...`).
/// - **5**: `FindingKey::DeclarationOverridden`
///   (`declaration_overridden:...`). `PairRule.overrides_declared` itself
///   needed no `rules.json` bump — additive, `#[serde(default)]` (see
///   `PairRule::overrides_declared`'s own doc comment).
/// - **6**: `EdgeKind::PatchInvalidatesPredicate`
///   (`patch_invalidates_predicate`) inside any `FindingKey` variant
///   embedding an `EdgeKind` —
///   `edge_dropped:`/`declaration_questioned:`/`placement_questioned:`/
///   `declaration_overridden:`.
/// - **7**: `FindingKey::RuntimePatchCollision`'s own text form changed
///   from `harmony_patch_collision:...` to `runtime_patch_collision:...`
///   (the runtime-method-patching concept rename — see
///   `crates/rim-analyzer/src/domain/assembly.rs`). Unlike every version
///   above, this is not an additive new-text case: it renames an
///   *existing* variant's text, so an old file's own
///   `harmony_patch_collision:...` key fails to parse under this or any
///   later version, with a clear "unrecognized finding key kind" error
///   naming the exact string — deliberately not aliased for backward
///   compatibility, since doing so would mean keeping a real third-party
///   mod's name in this store's parser permanently, the opposite of what
///   the rename is for. Recovery is deleting that one `decisions.json`
///   entry (or the whole file, which just re-empties the profile's
///   decision set) and re-deciding.
/// - **8**: `FindingKey::UndecodableTexture` (`undecodable_texture:...`).
/// - **9**: `FindingKey::BrokenInheritance` (`broken_inheritance:...`) and
///   `FindingKey::NearMissModReference` (`near_miss_mod_reference:...`).
/// - **10**: `EdgeKind::PatchRemovedNodeCosmetic`
///   (`patch_removed_node_cosmetic`) inside any `FindingKey` variant
///   embedding an `EdgeKind` —
///   `edge_dropped:`/`declaration_questioned:`/`placement_questioned:`/
///   `declaration_overridden:`. An existing `edge_dropped:...:patch_removed_node`
///   decision for a pair the analyzer now classifies as cosmetic is
///   orphaned by this change (the key no longer resolves, so the finding
///   reopens under its new `patch_removed_node_cosmetic` key) — expected,
///   not a bug, and the reopened finding auto-accepts at 95 with no
///   `Reorder` alternative, so it needs no re-deciding.
/// - **11**: `EdgeKind::ReplaceDiscardsAddition`
///   (`replace_discards_addition`) inside any `FindingKey` variant
///   embedding an `EdgeKind`, and `FindingKey::DiscardedAddition`
///   (`discarded_addition:...`).
/// - **12**: `FindingKey::DanglingDefReference`
///   (`dangling_def_reference:...`).
const SCHEMA_VERSION: u32 = 12;
const MIN_SUPPORTED_VERSION: u32 = 1;

/// One decision's on-disk shape: the canonical [`FindingKey`] text as the
/// key, `action` as [`Action`]'s own tagged JSON shape, and everything
/// else a [`Decision`] carries.
#[derive(Debug, Serialize, Deserialize)]
struct DecisionRecord {
    key: String,
    action: Action,
    note: Option<String>,
    decided_at: String,
}

/// The envelope's own shape — read for every version in
/// `MIN_SUPPORTED_VERSION..=SCHEMA_VERSION` (nothing about `decisions`'
/// own wire shape changed with the v1->v2 bump above, only which
/// `FindingKey` text forms `load` can parse) and always written back at
/// `SCHEMA_VERSION`.
#[derive(Debug, Serialize, Deserialize)]
struct DecisionsFile {
    version: u32,
    decisions: Vec<DecisionRecord>,
}

/// Just enough of the envelope to check the schema version before
/// attempting to deserialize the full, version-specific shape — a
/// different version's `decisions` field could be shaped completely
/// differently, so checking the version first gives a clean "unsupported
/// schema version" error instead of a confusing field-shape mismatch.
#[derive(Debug, Deserialize)]
struct VersionProbe {
    version: u32,
}

fn to_store_error(path: &Path, error: impl std::fmt::Display) -> StoreError {
    StoreError(format!("{}: {error}", path.display()))
}

/// Reads and writes `decisions.json`.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonDecisionStore;

impl JsonDecisionStore {
    /// Builds the store. Stateless — every call re-reads/writes the
    /// directory it's given.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl DecisionStore for JsonDecisionStore {
    fn load(&self, dir: &Path) -> Result<DecisionSet, StoreError> {
        let path = dir.join(FILE_NAME);
        if !path.is_file() {
            return Ok(DecisionSet::new());
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
        let file: DecisionsFile =
            serde_json::from_slice(&bytes).map_err(|e| to_store_error(&path, e))?;

        let mut set = DecisionSet::new();
        for record in file.decisions {
            let key: FindingKey = record
                .key
                .parse()
                .map_err(|e| to_store_error(&path, format!("finding key {:?}: {e}", record.key)))?;
            let decided_at = record.decided_at.parse().map_err(|e| {
                to_store_error(&path, format!("decided_at {:?}: {e}", record.decided_at))
            })?;
            set.insert(Decision {
                key,
                action: record.action,
                note: record.note,
                decided_at,
            })
            .map_err(|e| to_store_error(&path, e))?;
        }
        Ok(set)
    }

    fn save(&self, dir: &Path, set: &DecisionSet) -> Result<(), StoreError> {
        let path = dir.join(FILE_NAME);
        fs::create_dir_all(dir).map_err(|e| to_store_error(dir, e))?;

        let decisions: Vec<DecisionRecord> = set
            .iter()
            .map(|decision| DecisionRecord {
                key: decision.key.to_string(),
                action: decision.action.clone(),
                note: decision.note.clone(),
                decided_at: decision.decided_at.to_string(),
            })
            .collect();

        let file = DecisionsFile {
            version: SCHEMA_VERSION,
            decisions,
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| to_store_error(&path, e))?;
        write_atomically(&path, json.as_bytes()).map_err(|e| to_store_error(&path, e))
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn load_returns_an_empty_set_when_no_file_exists_yet() {
        let dir = tempdir().expect("tempdir");
        let set = JsonDecisionStore::new()
            .load(dir.path())
            .expect("load must succeed");
        assert!(
            set.get(&FindingKey::MissingMod {
                mod_id: ModId::new("a")
            })
            .is_none()
        );
    }

    #[test]
    fn round_trips_a_decision_through_save_and_load() {
        let dir = tempdir().expect("tempdir");
        let mut set = DecisionSet::new();
        let key = FindingKey::UndeclaredHardDependency {
            after: ModId::new("a"),
            before: ModId::new("b"),
        };
        set.insert(Decision {
            key: key.clone(),
            action: Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
            note: Some("because reasons".to_string()),
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("reorder is always a valid action");

        let store = JsonDecisionStore::new();
        store.save(dir.path(), &set).expect("save must succeed");
        let loaded = store.load(dir.path()).expect("load must succeed");

        let decision = loaded.get(&key).expect("decision must round-trip");
        assert_eq!(
            decision.action,
            Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            }
        );
        assert_eq!(decision.note.as_deref(), Some("because reasons"));
        assert_eq!(decision.decided_at, jiff::Timestamp::UNIX_EPOCH);
    }

    #[test]
    fn unknown_schema_version_is_an_error() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join(FILE_NAME),
            r#"{"version":99,"decisions":[]}"#,
        )
        .expect("write");

        let result = JsonDecisionStore::new().load(dir.path());

        assert!(result.is_err());
    }

    /// A payload one version past [`SCHEMA_VERSION`], with a completely
    /// different internal shape, must still fail with a clean "unsupported
    /// schema version" message — not a confusing field-shape
    /// deserialization error — because the version is checked (via
    /// [`VersionProbe`]) before the full, version-specific shape is ever
    /// parsed.
    #[test]
    fn a_differently_shaped_future_version_payload_fails_with_an_unsupported_version_message() {
        let dir = tempdir().expect("tempdir");
        let future_version = SCHEMA_VERSION + 1;
        fs::write(
            dir.path().join(FILE_NAME),
            format!(
                r#"{{"version":{future_version},"entries":{{"nested":"totally different shape"}}}}"#
            ),
        )
        .expect("write");

        let result = JsonDecisionStore::new().load(dir.path());

        match result {
            Err(error) => assert!(
                error
                    .to_string()
                    .contains(&format!("unsupported schema version {future_version}")),
                "unexpected error message: {error}"
            ),
            Ok(_) => panic!("a future-version payload must not be accepted"),
        }
    }

    /// A v1 file (written before `PatchWillFail`'s v1->v2 bump — see
    /// `SCHEMA_VERSION`'s own doc comment) still loads: nothing about
    /// `decisions`' own wire shape changed, so `MIN_SUPPORTED_VERSION..=
    /// SCHEMA_VERSION` accepts it, same as `rules.json`'s own v1
    /// tolerance.
    #[test]
    fn a_v1_file_still_loads() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":1,"decisions":[{"key":"missing_mod:a","action":{"action":"ignore"},"note":null,"decided_at":"1970-01-01T00:00:00Z"}]}"#)
        .expect("write");

        let loaded = JsonDecisionStore::new()
            .load(dir.path())
            .expect("a v1 file must still load");

        let key = FindingKey::MissingMod {
            mod_id: ModId::new("a"),
        };
        let decision = loaded.get(&key).expect("decision must round-trip");
        assert_eq!(decision.action, Action::Ignore);
    }

    #[test]
    fn malformed_json_is_an_error() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME), b"not json at all").expect("write");

        let result = JsonDecisionStore::new().load(dir.path());

        assert!(result.is_err());
    }

    /// A file occupying the *directory* path makes `create_dir_all` fail
    /// — a portable stand-in for "the profile directory can't be
    /// written to" that doesn't depend on OS-specific permission APIs.
    #[test]
    fn save_fails_when_the_profile_directory_cannot_be_created() {
        let dir = tempdir().expect("tempdir");
        let blocker = dir.path().join("profile");
        fs::write(&blocker, b"not a directory").expect("seed blocker file");

        let result = JsonDecisionStore::new().save(&blocker, &DecisionSet::new());

        assert!(result.is_err());
    }
}
