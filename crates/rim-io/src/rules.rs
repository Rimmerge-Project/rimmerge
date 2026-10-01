//! [`JsonRuleStore`]: `rules.json`, a versioned envelope over pairs,
//! placements, incompatibles, tag rules, manual tags, and settings.
//!
//! **Schema version 2** drops a fourth rule shape, tag-based clusters, which
//! a v1 envelope may still carry and this envelope's Rust side does not
//! model. (`suggest_merge_when_clean` is an additive, defaulted field under
//! the *same* v1 envelope, not a version bump — see that field's own doc
//! comment below.) A v1 file still loads
//! (`MIN_SUPPORTED_VERSION..=SCHEMA_VERSION`): a `"clusters"` array on it is
//! read generically (there is no `ClusterRule` Rust type) and surfaced as one
//! [`rim_session::ports::RulesLoadWarning::DroppedClusterRules`] naming
//! every dropped rule's own id, rather than silently ignored; the next
//! [`JsonRuleStore::save`] always writes the current version with no
//! `clusters` key at all, since [`RulesFile`] never declares one.
//!
//! **Schema version 3** switches `TagSignal` from serde's *internally* tagged
//! representation to the *adjacently* tagged one
//! (`{"kind": "...", "value": "..."}`). That is a wire-format change to
//! `tag_rules`, which is exactly what forces a version bump — the same rule
//! `decisions.json` follows for a new `FindingKey` text form. In practice
//! **no file on disk can carry the old shape**: the old one could not be
//! *written* at all (`serde_json::to_string` failed at runtime on a tagged
//! newtype variant over a string — see `TagSignal`'s own doc comment and its
//! round-trip test), and no tag rule ships, so every real `rules.json` has
//! `"tag_rules": []`. The bump is therefore defensive: it exists so a
//! hand-written v1/v2 file carrying the old shape fails loudly on its
//! `tag_rules` rather than half-loading, and so the version number stays an
//! honest record of the format. v1 and v2 both still load
//! (`MIN_SUPPORTED_VERSION..=SCHEMA_VERSION`) — their empty `tag_rules` array
//! is valid under either representation.

use std::fs;
use std::path::Path;

use rim_resolve::domain::{
    Confidence, IncompatibleRule, ManualTag, PairRule, PlacementRule, TagRule,
};
use rim_resolve::sort::{EnforcedLayers, TieBreak};
use rim_session::Settings;
use rim_session::ports::{LoadedRules, RuleStore, RulesLoadWarning, StoreError, StoredRules};
use serde::{Deserialize, Serialize};

use crate::atomic::write_atomically;

const FILE_NAME: &str = "rules.json";
const SCHEMA_VERSION: u32 = 3;
const MIN_SUPPORTED_VERSION: u32 = 1;

/// Just enough of the envelope to check the schema version before
/// attempting to deserialize the full shape.
#[derive(Debug, Deserialize)]
struct VersionProbe {
    version: u32,
}

/// `#[serde(default)]`'s fallback for [`SettingsDto::suggest_merge_when_clean`]
/// — an older `rules.json` has no such key at all, and the default is on,
/// so a v1 file missing the field must load exactly as if it had been
/// written with `true`. This field never bumped the schema version — the
/// envelope tolerates an added, defaulted field — the same reasoning
/// `Action::Merge`'s own `#[serde(default)]` on `choices` follows.
const fn default_suggest_merge_when_clean() -> bool {
    true
}

/// Imported placements stay on by default.
const fn default_use_imported_placements() -> bool {
    true
}

/// Imported pairs stay on by default too, matching
/// [`rim_session::Settings::default`] — an absent `rules.json` field must
/// default the same way a fresh `Settings::default()` does.
const fn default_use_imported_pairs() -> bool {
    true
}

/// Legacy-key defaults for `SettingsDto`'s four now-unused network
/// fields — see that struct's own doc comment for why they're read but
/// never written.
const fn default_fetch_community_rules() -> bool {
    true
}

/// See [`default_fetch_community_rules`].
const fn default_fetch_steam_workshop() -> bool {
    false
}

/// See [`default_fetch_community_rules`].
const fn default_allow_network_refresh() -> bool {
    true
}

/// See [`default_fetch_community_rules`].
const fn default_fetch_rimmerge_rules() -> bool {
    true
}

/// Inferred edges are enforced by default. Absent on an older file —
/// defaults to `true`, matching `EnforcedLayers::default` and
/// [`Settings::default`].
const fn default_enforce_inferred() -> bool {
    true
}

/// Off by default — see [`Settings::show_dangling_def_references`]'s own
/// doc comment for the measured false-positive rate behind this default.
/// Absent on an older file — defaults to `false`, matching
/// [`Settings::default`].
const fn default_show_dangling_def_references() -> bool {
    false
}

/// Serde-able mirror of [`TieBreak`], which doesn't derive `Serialize`/
/// `Deserialize` itself (a `rim-resolve` domain type staying serde-free) —
/// the same pattern [`SettingsDto`]'s `enforce_soft`/`enforce_awareness`
/// already uses for [`EnforcedLayers`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TieBreakDto {
    PreserveCurrent,
    #[default]
    Rebuild,
}

impl From<TieBreak> for TieBreakDto {
    fn from(value: TieBreak) -> Self {
        match value {
            TieBreak::PreserveCurrent => Self::PreserveCurrent,
            TieBreak::Rebuild => Self::Rebuild,
        }
    }
}

impl From<TieBreakDto> for TieBreak {
    fn from(value: TieBreakDto) -> Self {
        match value {
            TieBreakDto::PreserveCurrent => Self::PreserveCurrent,
            TieBreakDto::Rebuild => Self::Rebuild,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SettingsDto {
    threshold: u8,
    enforce_soft: bool,
    enforce_awareness: bool,
    #[serde(default = "default_suggest_merge_when_clean")]
    suggest_merge_when_clean: bool,
    /// Absent on an older file — defaults to [`TieBreak::Rebuild`],
    /// matching [`Settings::default`].
    #[serde(default)]
    tie_break: TieBreakDto,
    /// Absent on an older file — defaults to `true`, matching
    /// [`Settings::default`].
    #[serde(default = "default_use_imported_pairs")]
    use_imported_pairs: bool,
    /// Absent on an older file — defaults to `true`, matching
    /// [`Settings::default`].
    #[serde(default = "default_use_imported_placements")]
    use_imported_placements: bool,
    /// **Legacy, read-only.** These four fields moved to
    /// `rim_session::NetworkPolicy` (an app-global setting, no longer
    /// per-profile — see that type's own doc comment). Kept here,
    /// `#[serde(default)]`, purely so an existing `rules.json` written
    /// before the move still loads without error; `#[serde(skip_serializing)]`
    /// means a fresh save never writes them back, and neither
    /// [`From<Settings>`] nor [`Self::into_settings`] reads or writes this
    /// field — it exists solely to be parsed and then discarded.
    #[serde(default = "default_fetch_community_rules", skip_serializing)]
    #[allow(dead_code, reason = "kept only for backward-compatible parsing")]
    fetch_community_rules: bool,
    /// See [`Self::fetch_community_rules`].
    #[serde(default = "default_fetch_steam_workshop", skip_serializing)]
    #[allow(dead_code, reason = "kept only for backward-compatible parsing")]
    fetch_steam_workshop: bool,
    /// See [`Self::fetch_community_rules`].
    #[serde(default = "default_allow_network_refresh", skip_serializing)]
    #[allow(dead_code, reason = "kept only for backward-compatible parsing")]
    allow_network_refresh: bool,
    /// See [`Self::fetch_community_rules`].
    #[serde(default = "default_fetch_rimmerge_rules", skip_serializing)]
    #[allow(dead_code, reason = "kept only for backward-compatible parsing")]
    fetch_rimmerge_rules: bool,
    /// Absent on an older file — defaults to `true`, matching
    /// [`Settings::default`].
    #[serde(default = "default_enforce_inferred")]
    enforce_inferred: bool,
    /// Absent on an older file — defaults to `false`, matching
    /// [`Settings::default`].
    #[serde(default = "default_show_dangling_def_references")]
    show_dangling_def_references: bool,
}

impl From<Settings> for SettingsDto {
    fn from(settings: Settings) -> Self {
        Self {
            threshold: settings.threshold.percent(),
            enforce_soft: settings.enforce.soft,
            enforce_awareness: settings.enforce.awareness,
            suggest_merge_when_clean: settings.suggest_merge_when_clean,
            tie_break: settings.tie_break.into(),
            use_imported_pairs: settings.use_imported_pairs,
            use_imported_placements: settings.use_imported_placements,
            // Legacy fields: never written. The values here are inert —
            // `skip_serializing` means they never reach the file — but a
            // struct literal still needs something to put in them.
            fetch_community_rules: false,
            fetch_steam_workshop: false,
            allow_network_refresh: false,
            fetch_rimmerge_rules: false,
            enforce_inferred: settings.enforce.inferred,
            show_dangling_def_references: settings.show_dangling_def_references,
        }
    }
}

impl SettingsDto {
    fn into_settings(self) -> Result<Settings, String> {
        Ok(Settings {
            threshold: Confidence::new(self.threshold).map_err(|e| e.to_string())?,
            enforce: EnforcedLayers {
                soft: self.enforce_soft,
                awareness: self.enforce_awareness,
                inferred: self.enforce_inferred,
            },
            suggest_merge_when_clean: self.suggest_merge_when_clean,
            tie_break: self.tie_break.into(),
            use_imported_pairs: self.use_imported_pairs,
            use_imported_placements: self.use_imported_placements,
            show_dangling_def_references: self.show_dangling_def_references,
        })
    }
}

/// The envelope's rule/setting shape — read for every version in
/// `MIN_SUPPORTED_VERSION..=SCHEMA_VERSION` (every field added since v1
/// is additive and `#[serde(default)]`-backed, so one struct serves all)
/// and always written back at [`SCHEMA_VERSION`]. Deliberately has no
/// `clusters` field: since neither struct in this file sets
/// `#[serde(deny_unknown_fields)]`, a v1 file that still
/// carries a `"clusters": [...]` array simply has it ignored by this
/// struct's own deserialization — [`dropped_cluster_rule_ids`] reads it
/// back separately, generically, for the warning.
#[derive(Debug, Serialize, Deserialize)]
struct RulesFile {
    version: u32,
    pairs: Vec<PairRule>,
    placements: Vec<PlacementRule>,
    incompatibles: Vec<IncompatibleRule>,
    tag_rules: Vec<TagRule>,
    manual_tags: Vec<ManualTag>,
    settings: SettingsDto,
}

fn to_store_error(path: &Path, error: impl std::fmt::Display) -> StoreError {
    StoreError(format!("{}: {error}", path.display()))
}

/// A cluster rule's own id, read generically off pre-migration JSON — the
/// `ClusterRule` Rust type doesn't exist, but its `id` field's own wire
/// shape is a plain string. Falls back to the entry's
/// 0-based index when the field is missing or isn't a string, so the
/// warning always names something concrete.
fn cluster_rule_label(entry: &serde_json::Value, index: usize) -> String {
    entry
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("#{index}"))
}

/// Every cluster rule id `bytes` still carries under a top-level
/// `"clusters"` array, read generically (see [`cluster_rule_label`])
/// rather than through a typed field. Empty when the file has no such
/// key, an empty array, or isn't itself valid JSON (the caller has
/// already parsed `bytes` once by the time this runs, so a genuine parse
/// failure is reported through that path instead).
fn dropped_cluster_rule_ids(bytes: &[u8]) -> Vec<String> {
    let Ok(serde_json::Value::Object(root)) = serde_json::from_slice(bytes) else {
        return Vec::new();
    };
    let Some(serde_json::Value::Array(clusters)) = root.get("clusters") else {
        return Vec::new();
    };
    clusters
        .iter()
        .enumerate()
        .map(|(index, entry)| cluster_rule_label(entry, index))
        .collect()
}

/// Reads and writes `rules.json`.
#[derive(Debug, Default, Clone, Copy)]
pub struct JsonRuleStore;

impl JsonRuleStore {
    /// Builds the store. Stateless — every call re-reads/writes the
    /// directory it's given.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl RuleStore for JsonRuleStore {
    fn load(&self, dir: &Path) -> Result<LoadedRules, StoreError> {
        let path = dir.join(FILE_NAME);
        if !path.is_file() {
            return Ok(LoadedRules::default());
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
        let dropped_cluster_ids = dropped_cluster_rule_ids(&bytes);
        let file: RulesFile =
            serde_json::from_slice(&bytes).map_err(|e| to_store_error(&path, e))?;
        let settings = file
            .settings
            .into_settings()
            .map_err(|e| to_store_error(&path, e))?;
        let warnings = if dropped_cluster_ids.is_empty() {
            Vec::new()
        } else {
            vec![RulesLoadWarning::DroppedClusterRules {
                rule_ids: dropped_cluster_ids,
            }]
        };
        Ok(LoadedRules {
            rules: StoredRules {
                pairs: file.pairs,
                placements: file.placements,
                incompatibles: file.incompatibles,
                tag_rules: file.tag_rules,
                manual_tags: file.manual_tags,
                settings,
            },
            warnings,
        })
    }

    fn save(&self, dir: &Path, rules: &StoredRules) -> Result<(), StoreError> {
        let path = dir.join(FILE_NAME);
        fs::create_dir_all(dir).map_err(|e| to_store_error(dir, e))?;

        let file = RulesFile {
            version: SCHEMA_VERSION,
            pairs: rules.pairs.clone(),
            placements: rules.placements.clone(),
            incompatibles: rules.incompatibles.clone(),
            tag_rules: rules.tag_rules.clone(),
            manual_tags: rules.manual_tags.clone(),
            settings: rules.settings.into(),
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| to_store_error(&path, e))?;
        write_atomically(&path, json.as_bytes()).map_err(|e| to_store_error(&path, e))
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Placement, RuleOrigin};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn load_returns_defaults_when_no_file_exists_yet() {
        let dir = tempdir().expect("tempdir");
        let loaded = JsonRuleStore::new()
            .load(dir.path())
            .expect("load must succeed");
        assert!(loaded.rules.pairs.is_empty());
        assert_eq!(loaded.rules.settings.threshold.percent(), 80);
        assert!(
            loaded.rules.settings.use_imported_pairs,
            "the imported-pairs toggle's own default is true "
        );
        assert!(loaded.warnings.is_empty());
    }

    /// A user who explicitly persisted `use_imported_pairs: false` must
    /// keep that value on the next load. Only an *absent* field (an older
    /// v1 file, covered by
    /// [`a_v1_file_missing_import_settings_defaults_like_settings_default`])
    /// picks up the default; an explicit value on file is never
    /// second-guessed.
    #[test]
    fn an_explicitly_persisted_use_imported_pairs_false_survives_a_load() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":2,"pairs":[],"placements":[],"incompatibles":[],"tag_rules":[],"manual_tags":[],"settings":{"threshold":80,"enforce_soft":false,"enforce_awareness":false,"suggest_merge_when_clean":true,"tie_break":"rebuild","use_imported_pairs":false,"use_imported_placements":true}}"#)
        .expect("write");

        let loaded = JsonRuleStore::new()
            .load(dir.path())
            .expect("load must succeed");

        assert!(
            !loaded.rules.settings.use_imported_pairs,
            "an explicitly-persisted false must survive even though the default is now true"
        );
    }

    #[test]
    fn round_trips_every_rule_shape_and_settings() {
        let dir = tempdir().expect("tempdir");
        let mut rules = StoredRules::default();
        rules.placements.push(PlacementRule {
            mod_id: ModId::new("a"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: Some("pinned".to_string()),
        });
        rules.settings.threshold = Confidence::new(65).expect("valid");
        rules.settings.enforce.soft = true;
        rules.settings.suggest_merge_when_clean = false;
        rules.settings.tie_break = TieBreak::PreserveCurrent;
        rules.settings.use_imported_pairs = true;
        rules.settings.use_imported_placements = false;
        rules.settings.enforce.inferred = false;

        let store = JsonRuleStore::new();
        store.save(dir.path(), &rules).expect("save must succeed");
        let loaded = store.load(dir.path()).expect("load must succeed");

        assert_eq!(loaded.rules.placements.len(), 1);
        assert_eq!(
            loaded.rules.placements[0].comment.as_deref(),
            Some("pinned")
        );
        assert_eq!(loaded.rules.settings.threshold.percent(), 65);
        assert!(loaded.rules.settings.enforce.soft);
        assert!(!loaded.rules.settings.enforce.awareness);
        assert!(!loaded.rules.settings.suggest_merge_when_clean);
        assert_eq!(loaded.rules.settings.tie_break, TieBreak::PreserveCurrent);
        assert!(loaded.rules.settings.use_imported_pairs);
        assert!(!loaded.rules.settings.use_imported_placements);
        assert!(!loaded.rules.settings.enforce.inferred);
        assert!(loaded.warnings.is_empty());
    }

    /// An older `rules.json` has no `suggest_merge_when_clean` key at all — it
    /// must still load, with the field defaulting to `true`, under schema
    /// version 1 unchanged.
    #[test]
    fn a_v1_settings_object_missing_suggest_merge_when_clean_defaults_to_true() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":1,"pairs":[],"placements":[],"incompatibles":[],"clusters":[],"tag_rules":[],"manual_tags":[],"settings":{"threshold":80,"enforce_soft":false,"enforce_awareness":false}}"#)
        .expect("write");

        let loaded = JsonRuleStore::new()
            .load(dir.path())
            .expect("an older rules.json with no suggest_merge_when_clean key must still load");

        assert!(loaded.rules.settings.suggest_merge_when_clean);
    }

    /// The same v1 file also has none of the `tie_break`/
    /// `use_imported_pairs`/`use_imported_placements` settings fields —
    /// every one must default exactly like [`Settings::default`], and an
    /// *empty* `"clusters"` array must not produce a warning (nothing was
    /// actually dropped). The legacy network keys (see
    /// [`legacy_network_keys_in_the_file_are_read_without_error_and_never_reach_settings`])
    /// aren't part of [`Settings`] any more, so this test no longer
    /// covers them.
    #[test]
    fn a_v1_file_missing_import_settings_defaults_like_settings_default() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":1,"pairs":[],"placements":[],"incompatibles":[],"clusters":[],"tag_rules":[],"manual_tags":[],"settings":{"threshold":80,"enforce_soft":false,"enforce_awareness":false}}"#)
        .expect("write");

        let loaded = JsonRuleStore::new()
            .load(dir.path())
            .expect("an older rules.json with no import settings keys must still load");

        assert_eq!(
            loaded.rules.settings.tie_break,
            Settings::default().tie_break
        );
        assert_eq!(
            loaded.rules.settings.use_imported_pairs,
            Settings::default().use_imported_pairs
        );
        assert_eq!(
            loaded.rules.settings.use_imported_placements,
            Settings::default().use_imported_placements
        );
        assert!(
            loaded.warnings.is_empty(),
            "an empty clusters array drops nothing"
        );
    }

    /// The four legacy network keys (`fetch_community_rules`/
    /// `fetch_steam_workshop`/`allow_network_refresh`/
    /// `fetch_rimmerge_rules`) moved to `rim_session::NetworkPolicy`.
    /// A `rules.json` written before that move still carries them — this
    /// asserts the file still loads without error (the legacy `SettingsDto`
    /// fields absorb and discard them) and that a fresh save never writes
    /// them back.
    #[test]
    fn legacy_network_keys_in_the_file_are_read_without_error_and_never_reach_settings() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":2,"pairs":[],"placements":[],"incompatibles":[],"tag_rules":[],"manual_tags":[],"settings":{"threshold":80,"enforce_soft":false,"enforce_awareness":false,"suggest_merge_when_clean":true,"tie_break":"rebuild","use_imported_pairs":true,"use_imported_placements":true,"fetch_community_rules":false,"fetch_steam_workshop":true,"allow_network_refresh":false,"fetch_rimmerge_rules":false}}"#)
        .expect("write");
        let store = JsonRuleStore::new();

        let loaded = store
            .load(dir.path())
            .expect("a file carrying the legacy network keys must still load");
        store
            .save(dir.path(), &loaded.rules)
            .expect("save must succeed");

        let raw = fs::read_to_string(dir.path().join(FILE_NAME)).expect("read saved file");
        for legacy_key in [
            "fetch_community_rules",
            "fetch_steam_workshop",
            "allow_network_refresh",
            "fetch_rimmerge_rules",
        ] {
            assert!(
                !raw.contains(legacy_key),
                "a fresh save must never write the legacy key {legacy_key}"
            );
        }
    }

    // Additive fields never bump `SCHEMA_VERSION`; that is covered by
    // `save_always_writes_the_current_schema_version` below,
    // unconditionally of any particular setting's value — no separate
    // version test is needed for these fields.

    /// A `rules.json` with no `enforce_inferred` key at all (an older
    /// file) still loads, the
    /// field defaulting to `true` exactly like a fresh `Settings::default()`.
    #[test]
    fn a_file_missing_enforce_inferred_defaults_like_settings_default() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":2,"pairs":[],"placements":[],"incompatibles":[],"tag_rules":[],"manual_tags":[],"settings":{"threshold":80,"enforce_soft":false,"enforce_awareness":false,"suggest_merge_when_clean":true,"tie_break":"rebuild","use_imported_pairs":true,"use_imported_placements":true,"fetch_community_rules":true,"fetch_steam_workshop":false,"allow_network_refresh":true}}"#)
        .expect("write");

        let loaded = JsonRuleStore::new()
            .load(dir.path())
            .expect("load must succeed");

        assert_eq!(
            loaded.rules.settings.enforce.inferred,
            Settings::default().enforce.inferred
        );
    }

    /// An explicitly-persisted `enforce_inferred: false`
    /// survives a load unchanged — the same "an explicit value on file is
    /// never second-guessed" contract every other settings toggle here
    /// already gets.
    #[test]
    fn explicitly_persisted_enforce_inferred_false_survives_a_load() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":2,"pairs":[],"placements":[],"incompatibles":[],"tag_rules":[],"manual_tags":[],"settings":{"threshold":80,"enforce_soft":false,"enforce_awareness":false,"suggest_merge_when_clean":true,"tie_break":"rebuild","use_imported_pairs":true,"use_imported_placements":true,"fetch_community_rules":true,"fetch_steam_workshop":false,"allow_network_refresh":true,"enforce_inferred":false}}"#)
        .expect("write");

        let loaded = JsonRuleStore::new()
            .load(dir.path())
            .expect("load must succeed");

        assert!(!loaded.rules.settings.enforce.inferred);
    }

    /// The v1 -> v2 migration: a file with named, non-empty cluster rules
    /// loads with a [`RulesLoadWarning::DroppedClusterRules`] naming them,
    /// and the very next save leaves no trace of `"clusters"` at all.
    #[test]
    fn a_file_with_named_clusters_warns_and_is_rewritten_without_them_on_save() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":1,"pairs":[],"placements":[],"incompatibles":[],"clusters":[{"id":"framework","tag":"framework"},{"id":"informational"}],"tag_rules":[],"manual_tags":[],"settings":{"threshold":80,"enforce_soft":false,"enforce_awareness":false}}"#)
        .expect("write");

        let store = JsonRuleStore::new();
        let loaded = store.load(dir.path()).expect("load must succeed");

        assert_eq!(
            loaded.warnings,
            vec![RulesLoadWarning::DroppedClusterRules {
                rule_ids: vec!["framework".to_string(), "informational".to_string()],
            }]
        );

        store
            .save(dir.path(), &loaded.rules)
            .expect("save must succeed");
        let raw = fs::read_to_string(dir.path().join(FILE_NAME)).expect("read back");
        assert!(
            !raw.contains("clusters"),
            "the rewritten file must carry no clusters key at all: {raw}"
        );
        let reloaded = store.load(dir.path()).expect("reload must succeed");
        assert!(
            reloaded.warnings.is_empty(),
            "nothing left to warn about after the rewrite"
        );
    }

    #[test]
    fn a_cluster_entry_with_no_id_field_is_named_by_its_position() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":1,"pairs":[],"placements":[],"incompatibles":[],"clusters":[{"tag":"no-id-here"}],"tag_rules":[],"manual_tags":[],"settings":{"threshold":80,"enforce_soft":false,"enforce_awareness":false}}"#)
        .expect("write");

        let loaded = JsonRuleStore::new()
            .load(dir.path())
            .expect("load must succeed");

        assert_eq!(
            loaded.warnings,
            vec![RulesLoadWarning::DroppedClusterRules {
                rule_ids: vec!["#0".to_string()],
            }]
        );
    }

    #[test]
    fn save_always_writes_the_current_schema_version() {
        let dir = tempdir().expect("tempdir");
        JsonRuleStore::new()
            .save(dir.path(), &StoredRules::default())
            .expect("save must succeed");

        let raw = fs::read_to_string(dir.path().join(FILE_NAME)).expect("read back");
        assert!(raw.contains("\"version\": 3"), "unexpected contents: {raw}");
    }

    /// The v3 migration. A real v2 file's `tag_rules` is always `[]` — the old
    /// internally tagged `TagSignal` could not be serialised at all — so the
    /// whole migration is "it still loads, and the next save stamps 3". A v3
    /// file's own adjacently-tagged signal round-trips through the store.
    #[test]
    fn a_v2_file_still_loads_and_the_next_save_stamps_version_3() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":2,"pairs":[],"placements":[],"incompatibles":[],"tag_rules":[],"manual_tags":[],"settings":{"threshold":80,"enforce_soft":false,"enforce_awareness":false}}"#)
        .expect("write");
        let store = JsonRuleStore::new();

        let loaded = store.load(dir.path()).expect("a v2 file must still load");
        assert!(loaded.rules.tag_rules.is_empty());

        store.save(dir.path(), &loaded.rules).expect("save");
        let raw = fs::read_to_string(dir.path().join(FILE_NAME)).expect("read back");
        assert!(raw.contains("\"version\": 3"), "unexpected contents: {raw}");
    }

    /// A non-empty tag rule round-trips through the real store at v3 — the
    /// case `rules.json` never had until now, and the reason the
    /// version moved at all.
    #[test]
    fn a_non_empty_tag_rule_round_trips_through_the_store() {
        use rim_resolve::domain::{Tag, TagSignal};

        let dir = tempdir().expect("tempdir");
        let store = JsonRuleStore::new();
        let rules = StoredRules {
            tag_rules: vec![TagRule {
                tag: Tag::new("framework").expect("valid tag"),
                any_of: vec![
                    TagSignal::UrlContains("mods.example".to_string()),
                    TagSignal::DependsOn(ModId::new("example.framework")),
                ],
            }],
            ..StoredRules::default()
        };

        store.save(dir.path(), &rules).expect("save must succeed");
        let raw = fs::read_to_string(dir.path().join(FILE_NAME)).expect("read back");
        assert!(
            raw.contains("\"kind\": \"url_contains\"")
                && raw.contains("\"value\": \"mods.example\""),
            "the adjacently-tagged wire shape is what v3 stores: {raw}"
        );

        let loaded = store.load(dir.path()).expect("load must succeed");
        assert_eq!(loaded.rules.tag_rules, rules.tag_rules);
    }

    #[test]
    fn unknown_schema_version_is_an_error() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME),
            r#"{"version":4,"pairs":[],"placements":[],"incompatibles":[],"tag_rules":[],"manual_tags":[],"settings":{"threshold":80,"enforce_soft":false,"enforce_awareness":false}}"#)
        .expect("write");

        let result = JsonRuleStore::new().load(dir.path());

        assert!(result.is_err());
    }

    /// A differently-shaped payload at an unrecognized version must still
    /// fail with a clean "unsupported schema version" message, because the
    /// version is checked (via [`VersionProbe`]) before the full,
    /// version-specific shape is ever parsed.
    #[test]
    fn a_differently_shaped_unsupported_version_payload_fails_with_a_clean_message() {
        let dir = tempdir().expect("tempdir");
        fs::write(
            dir.path().join(FILE_NAME),
            r#"{"version":4,"entries":{"nested":"totally different shape"}}"#,
        )
        .expect("write");

        let result = JsonRuleStore::new().load(dir.path());

        match result {
            Err(error) => assert!(
                error.to_string().contains("unsupported schema version 4"),
                "unexpected error message: {error}"
            ),
            Ok(_) => panic!("an unsupported-version payload must not be accepted"),
        }
    }

    #[test]
    fn malformed_json_is_an_error() {
        let dir = tempdir().expect("tempdir");
        fs::write(dir.path().join(FILE_NAME), b"not json at all").expect("write");

        let result = JsonRuleStore::new().load(dir.path());

        assert!(result.is_err());
    }

    /// A file occupying the *directory* path makes `create_dir_all`
    /// fail — a portable stand-in for "the profile directory can't be
    /// written to".
    #[test]
    fn save_fails_when_the_profile_directory_cannot_be_created() {
        let dir = tempdir().expect("tempdir");
        let blocker = dir.path().join("profile");
        fs::write(&blocker, b"not a directory").expect("seed blocker file");

        let result = JsonRuleStore::new().save(&blocker, &StoredRules::default());

        assert!(result.is_err());
    }
}
