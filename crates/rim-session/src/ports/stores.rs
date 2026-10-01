//! Persistence ports: merge-mod writer, mods config, decisions, rules, patch/assignment projects,
//! mod knowledge.

use std::path::{Path, PathBuf};

use rim_analyzer::domain::{GeneratedMarker, ModId};
use rim_merge::emit::RenderedMod;
use rim_resolve::domain::{
    AssignmentId, AssignmentProject, DecisionSet, IncompatibleRule, ManualTag, PairRule, PatchId,
    PatchProject, PlacementRule, Rule, RuleSet, TagRule,
};

use crate::settings::Settings;

/// A [`MergeModWriter`] failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct MergeModError(pub String);

/// What [`MergeModWriter::write`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeModWriteReport {
    /// The generated mod's folder, on disk.
    pub mod_path: PathBuf,
    /// The previous generation's backup path, if one existed to back up.
    pub backup_path: Option<PathBuf>,
}

/// Renders the merge mod folder under `mods_dir`, atomically, keeping one
/// previous generation.
pub trait MergeModWriter {
    /// Writes `rendered` into `mods_dir` atomically (see
    /// the adapter's doc comment for the exact swap), backing
    /// up any existing folder of the same name into `backup_dir` first.
    ///
    /// # Errors
    ///
    /// Returns [`MergeModError`] when any step of the write fails.
    fn write(
        &self,
        mods_dir: &Path,
        backup_dir: &Path,
        rendered: &RenderedMod,
    ) -> Result<MergeModWriteReport, MergeModError>;

    /// Removes `folder_name` under `mods_dir` (and appends nothing) when no
    /// merge decision remains. Returns the removed folder's path, or
    /// `None` when it didn't exist.
    ///
    /// # Errors
    ///
    /// Returns [`MergeModError`] when the removal fails.
    fn remove(
        &self,
        mods_dir: &Path,
        backup_dir: &Path,
        folder_name: &str,
    ) -> Result<Option<PathBuf>, MergeModError>;

    /// Whether `folder_name` currently exists under `mods_dir` — the
    /// desktop `get_merge_mod` command uses this to report whether the
    /// mod has ever been written to disk, without writing or removing
    /// anything itself.
    #[must_use]
    fn exists(&self, mods_dir: &Path, folder_name: &str) -> bool;

    /// The `rimmerge.json` marker of `<dir>/<folder_name>`, parsed the same
    /// way `rim_analyzer`'s own discovery reads a mod's marker
    /// `Ok(None)` both when
    /// `<dir>/<folder_name>` doesn't exist at all and when it exists with
    /// no (or an unparsable) marker — a caller that needs to tell those two
    /// apart pairs this with [`Self::exists`] first, exactly as
    /// `crate::use_cases::ExportPatch` does to decide whether a target
    /// folder is safe to overwrite (this patch's own previous export) or
    /// foreign (someone else's folder, or none at all yet).
    ///
    /// # Errors
    ///
    /// Returns [`MergeModError`] when the folder exists but its marker
    /// file exists and can't be read (never for a missing file or a
    /// present-but-unparsable marker — both read as `Ok(None)`).
    fn read_marker(
        &self,
        dir: &Path,
        folder_name: &str,
    ) -> Result<Option<GeneratedMarker>, MergeModError>;
}

/// `ModsConfig.xml`'s content, parsed to and from the exact shape rim-io
/// writes back: `version` and `known_expansions` are preserved verbatim
/// from whatever was last read, so a Rimmerge write never drops fields
/// RimWorld itself put there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModsConfigFile {
    /// The `<version>` element's text, unchanged from the source file.
    pub version: String,
    /// The `<activeMods>` list, in file order.
    pub active_mods: Vec<ModId>,
    /// The `<knownExpansions>` list, unchanged from the source file.
    pub known_expansions: Vec<ModId>,
}

/// A `ModsConfig.xml` read/write failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ConfigError(pub String);

/// Reads and writes `ModsConfig.xml`.
pub trait ModsConfigStore {
    /// # Errors
    ///
    /// Returns [`ConfigError`] when `path` can't be read or parsed.
    fn read(&self, path: &Path) -> Result<ModsConfigFile, ConfigError>;

    /// Writes `file` to `path`, first copying the existing file to a
    /// timestamped backup. Returns the backup path.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the backup or write fails.
    fn write_with_backup(&self, path: &Path, file: &ModsConfigFile)
    -> Result<PathBuf, ConfigError>;
}

/// A decisions/rules file read/write failure, including an unrecognized
/// schema version.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct StoreError(pub String);

/// Loads and saves the decisions file.
pub trait DecisionStore {
    /// Returns an empty [`DecisionSet`] when `dir` has no decisions file
    /// yet.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file exists but can't be read or
    /// parsed, or names an unrecognized schema version.
    fn load(&self, dir: &Path) -> Result<DecisionSet, StoreError>;

    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    fn save(&self, dir: &Path, set: &DecisionSet) -> Result<(), StoreError>;
}

/// A non-fatal condition [`RuleStore::load`] noticed while reading the
/// rules file — surfaced to the caller (via [`LoadedRules::warnings`])
/// rather than silently swallowed, but never a reason to fail the load.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RulesLoadWarning {
    /// A `rules.json` written with the retired tag-based cluster rules
    /// still carried them; they were dropped on load and will not be
    /// written back by the next [`RuleStore::save`].
    #[error("dropped {} pre-migration cluster rule(s) on load: {}",
        .rule_ids.len(),
        .rule_ids.join(", ")
    )]
    DroppedClusterRules {
        /// The dropped rules' own ids, as recorded in the old file (or a
        /// positional placeholder, `#<index>`, for an entry that carried
        /// none).
        rule_ids: Vec<String>,
    },
    /// A row in the fetched mod-knowledge data named something this
    /// binary does not implement — a `behaviour` string, a `match` mode,
    /// or a `rule` discriminant added after this build. The row is
    /// ignored and everything else in the file still loads: **a newer
    /// data file must never break an older binary**.
    #[error("ignored an unknown {what} {value:?} in the {section} rules")]
    UnknownModKnowledgeValue {
        /// Which section of the data file the row came from.
        section: String,
        /// What kind of value was not understood.
        what: ModKnowledgeValueKind,
        /// The value itself, as written in the data.
        value: String,
    },
    /// A role of the fetched log-shapes data held more rows than a binary
    /// accepts; the rows past the limit were ignored unread.
    #[error("ignored {ignored} row(s) over the per-role limit in the {section} role {role}")]
    ModKnowledgeRowsOverRoleLimit {
        /// Which section of the data file the rows came from.
        section: String,
        /// The role whose limit was exceeded.
        role: String,
        /// How many rows were ignored.
        ignored: usize,
    },
    /// A `prefer_outside_framework` precedence rule named no framework, so
    /// it could not be applied and was ignored.
    #[error("ignored the {def_type} precedence rule: it names no framework")]
    PrecedenceRuleMissingFramework {
        /// The def type the rule was for.
        def_type: String,
    },
    /// The fetched mod-knowledge cache file exists but could not be read
    /// or parsed; the bundled snapshot was used instead.
    #[error("could not read the cached rimmerge rules ({0}); using the bundled snapshot")]
    ModKnowledgeCacheUnreadable(String),
}

/// What kind of value of a mod-knowledge row a binary did not understand
/// (see [`RulesLoadWarning::UnknownModKnowledgeValue`]). A closed set so the
/// frontend can translate it; the English [`Display`](std::fmt::Display) text
/// is for logs and the CLI only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKnowledgeValueKind {
    /// A precedence `rule` discriminant.
    PrecedenceRule,
    /// A patch-operation `behaviour`.
    Behaviour,
    /// A `match` mode of a patch-operation, def-cache or log-shape row.
    MatchMode,
    /// A `match` mode of a patch-operation gate.
    GateMatchMode,
    /// A `behaviour` of a patch-operation gate.
    GateBehaviour,
    /// A conditional operation's type.
    ConditionalType,
    /// A top-level section of the data file.
    TopLevelSection,
    /// A log-shape role.
    LogShapeRole,
    /// A template placeholder type.
    PlaceholderType,
    /// A log-shape template that is empty, too long, or malformed.
    TemplateInvalid,
    /// A log-shape template lacking a capture its role needs.
    CaptureMissing,
    /// A log-shape marker string outside its length bounds.
    MarkerLength,
    /// A log-shape row id over the length bound.
    IdTooLong,
    /// A log-shape template that does not compile within the size limits.
    TemplateUncompilable,
}

impl std::fmt::Display for ModKnowledgeValueKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::PrecedenceRule => "precedence rule",
            Self::Behaviour => "behaviour",
            Self::MatchMode => "match mode",
            Self::GateMatchMode => "gate match mode",
            Self::GateBehaviour => "gate behaviour",
            Self::ConditionalType => "conditional type",
            Self::TopLevelSection => "top-level section",
            Self::LogShapeRole => "log-shape role",
            Self::PlaceholderType => "placeholder type",
            Self::TemplateInvalid => "invalid template",
            Self::CaptureMissing => "missing capture",
            Self::MarkerLength => "marker length",
            Self::IdTooLong => "over-long id",
            Self::TemplateUncompilable => "uncompilable template",
        })
    }
}

/// [`RuleStore::load`]'s result: the persisted rules and settings, plus
/// any non-fatal warnings noticed while reading the file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LoadedRules {
    /// The persisted rules and settings.
    pub rules: StoredRules,
    /// Non-fatal warnings noticed while reading the file.
    pub warnings: Vec<RulesLoadWarning>,
}

/// Every rule and setting persisted with a profile: the three rule shapes,
/// tag rules, manual tag overrides, and [`Settings`] — all versioned and
/// saved together so a profile is one self-consistent file (there is no
/// separate settings port; settings ride along with the rules file rather
/// than growing another port for one small struct).
///
/// A retired fourth, tag-based cluster rule shape is not supported: a
/// `rules.json` that still carries a `"clusters"` array has it dropped on
/// load, surfaced as a [`RulesLoadWarning::DroppedClusterRules`] rather
/// than silently ignored, and is rewritten without it the next time
/// [`RuleStore::save`] runs — see `rim_io::JsonRuleStore`'s own doc
/// comment for the envelope version this migration bumped.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StoredRules {
    /// Pair (`loadAfter`/`loadBefore`) rules.
    pub pairs: Vec<PairRule>,
    /// Top/bottom placement rules.
    pub placements: Vec<PlacementRule>,
    /// Incompatibility rules.
    pub incompatibles: Vec<IncompatibleRule>,
    /// Tag inference rules.
    pub tag_rules: Vec<TagRule>,
    /// Manual per-mod tag overrides.
    pub manual_tags: Vec<ManualTag>,
    /// User-configurable sorter/ledger settings.
    pub settings: Settings,
}

impl StoredRules {
    /// Every pair/placement/incompatible rule as one merged [`RuleSet`],
    /// in [`RuleSet::merged`]'s stable precedence order.
    #[must_use]
    pub fn rule_set(&self) -> RuleSet {
        let mut rules =
            Vec::with_capacity(self.pairs.len() + self.placements.len() + self.incompatibles.len());
        rules.extend(self.pairs.iter().cloned().map(Rule::Pair));
        rules.extend(self.placements.iter().cloned().map(Rule::Placement));
        rules.extend(self.incompatibles.iter().cloned().map(Rule::Incompatible));
        RuleSet::merged([RuleSet::new(rules)])
    }
}

/// Loads and saves the rules file (pairs, placements, incompatibles, tag
/// rules, manual tags, and settings).
pub trait RuleStore {
    /// Returns [`LoadedRules::default`] (empty rules, no warnings) when
    /// `dir` has no rules file yet.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the file exists but can't be read or
    /// parsed, or names an unrecognized schema version.
    fn load(&self, dir: &Path) -> Result<LoadedRules, StoreError>;

    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    fn save(&self, dir: &Path, rules: &StoredRules) -> Result<(), StoreError>;
}

/// Loads and saves compat patch projects: `<profile>/patches/<patch-id>.json`,
/// one file per project, so two
/// projects never contend for one file and a deleted project is one unlink.
pub trait PatchProjectStore {
    /// Every project under `<profile>/patches/`. An absent directory is an
    /// empty list; a single unreadable or unrecognized-schema-version file
    /// is an error naming it — never silently skipped, so a corrupt project
    /// doesn't disappear without a trace.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a project file exists but can't be read,
    /// parsed, or names an unrecognized schema version.
    fn load_all(&self, profile_dir: &Path) -> Result<Vec<PatchProject>, StoreError>;

    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    fn save(&self, profile_dir: &Path, project: &PatchProject) -> Result<(), StoreError>;

    /// # Errors
    ///
    /// Returns [`StoreError`] when the removal fails. Removing an id with
    /// no file on disk is not an error (mirrors [`std::fs::remove_file`]'s
    /// own callers always checking existence first — an adapter is free to
    /// treat "already gone" as success).
    fn delete(&self, profile_dir: &Path, id: &PatchId) -> Result<(), StoreError>;
}

/// A no-op [`PatchProjectStore`]: `load_all` always returns empty,
/// `save`/`delete` silently succeed without persisting anything. Both
/// composition roots (`apps/cli`, `apps/desktop/src-tauri`) wire in the
/// real `rim-io::JsonPatchProjectStore` instead — this remains only as a
/// fixture for tests (mainly `rim-io`'s own end-to-end ones) that don't
/// need patches to persist. Not test-support-gated since it has no
/// dependency on the `test-support` feature to keep it that lightweight.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoPatchStore;

impl PatchProjectStore for NoPatchStore {
    fn load_all(&self, _profile_dir: &Path) -> Result<Vec<PatchProject>, StoreError> {
        Ok(Vec::new())
    }

    fn save(&self, _profile_dir: &Path, _project: &PatchProject) -> Result<(), StoreError> {
        Ok(())
    }

    fn delete(&self, _profile_dir: &Path, _id: &PatchId) -> Result<(), StoreError> {
        Ok(())
    }
}

/// Loads and saves patch maker projects: `<profile>/assignments/<id>.json`,
/// one file per project — the
/// exact [`PatchProjectStore`] shape, one file per project so two projects
/// never contend for one file and a deleted project is one unlink.
pub trait AssignmentProjectStore {
    /// Every project under `<profile>/assignments/`. An absent directory
    /// is an empty list; a single unreadable or unrecognized-schema-version
    /// file is an error naming it — never silently skipped, so a corrupt
    /// project doesn't disappear without a trace.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a project file exists but can't be read,
    /// parsed, or names an unrecognized schema version.
    fn load_all(&self, profile_dir: &Path) -> Result<Vec<AssignmentProject>, StoreError>;

    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    fn save(&self, profile_dir: &Path, project: &AssignmentProject) -> Result<(), StoreError>;

    /// # Errors
    ///
    /// Returns [`StoreError`] when the removal fails. Removing an id with
    /// no file on disk is not an error, mirroring [`PatchProjectStore::delete`].
    fn delete(&self, profile_dir: &Path, id: &AssignmentId) -> Result<(), StoreError>;
}

/// A no-op [`AssignmentProjectStore`], mirroring [`NoPatchStore`] exactly:
/// `load_all` always returns empty, `save`/`delete` silently succeed
/// without persisting anything. Both composition roots wire in the real
/// `rim-io::JsonAssignmentProjectStore` instead — this remains only as a
/// fixture for tests that don't need assignment projects to persist.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoAssignmentStore;

impl AssignmentProjectStore for NoAssignmentStore {
    fn load_all(&self, _profile_dir: &Path) -> Result<Vec<AssignmentProject>, StoreError> {
        Ok(Vec::new())
    }

    fn save(&self, _profile_dir: &Path, _project: &AssignmentProject) -> Result<(), StoreError> {
        Ok(())
    }

    fn delete(&self, _profile_dir: &Path, _id: &AssignmentId) -> Result<(), StoreError> {
        Ok(())
    }
}

// -- `Player.log` import ---------------------------------------------------
//
// These types are the port's own DTO, not `rim-io`'s pure-parser return
// type re-exported: `rim-io` depends on `rim-session` (root `CLAUDE.md`'s
// layering — `rim-io` implements this crate's ports), so a type this
// trait's signature names cannot live in `rim-io` without a circular
// dependency. `rim_io::game_log::parse` (pure, unit-tested directly on
// fixture text, no filesystem, no `ModId`, no `Report`) returns exactly
// this shape; `rim_io::FileGameLogReader` (the adapter implementing
// [`GameLogReader`] below) is the one place that shape crosses the port
// boundary, and it needs no field-by-field conversion at all, since
// there is only one definition of it.

/// Loads the mod-specific knowledge that lives in data rather than in
/// code: which def types have
/// a verified precedence rule, which third-party patch-operation classes
/// map onto which modelled behaviour, and which def-cache plugins exist.
///
/// Read once, at [`crate::use_cases::LoadProject`] time — **never on the
/// sort path**. The adapter's contract is "the fetched cache if it is
/// present and parses, else the bundled snapshot compiled into the
/// binary", so output stays deterministic offline and on a first run
/// (`rim_io::FsModKnowledgeStore`).
///
/// Deliberately **not** a source of tag rules: tag rules change sort
/// output, so they follow the RimSort import model instead (fetch ->
/// cache -> explicit import into the profile's `rules.json`), preserving
/// the guarantee that no sort path ever reads the cache. See
/// [`crate::ModKnowledge`]'s own doc comment.
pub trait ModKnowledgeStore {
    /// Loads the current knowledge, plus any non-fatal warnings noticed
    /// while reading it. Never an error: unreadable or unparseable data
    /// falls back to the bundled snapshot with a warning.
    ///
    /// `source_enabled` is [`crate::NetworkPolicy::fetch_rimmerge_rules`], and
    /// it gates **consumption**, not just fetching: off means the
    /// bundled snapshot even when a cached copy is sitting on disk. That
    /// is the same rule [`crate::should_import_from_cache`] applies to the
    /// other two sources — a source's own toggle decides whether its
    /// cached bytes are allowed to affect this run at all, not merely
    /// whether a refresh may replace them.
    fn load(&self, source_enabled: bool) -> LoadedModKnowledge;
}

/// [`ModKnowledgeStore::load`]'s result.
#[derive(Debug, Clone, Default)]
pub struct LoadedModKnowledge {
    /// The knowledge itself.
    pub knowledge: crate::ModKnowledge,
    /// Non-fatal warnings noticed while reading it.
    pub warnings: Vec<RulesLoadWarning>,
}
