//! [`RimSortImporter`]: reads RimSort's three database files and turns
//! every rule naming only active mods into a [`rim_resolve::domain::Rule`],
//! copying a snapshot of each source file into `<profile_dir>/imports/`.

// `pub(crate)`, not private: `crate::databases::cache` reuses both
// parsers verbatim as its own "parse before commit" validation
// — a plain `mod`
// here would keep them visible only within this module's own
// descendants, which `databases` (a sibling, not a child) isn't.
pub(crate) mod rules_file;
pub(crate) mod steam_db;

pub mod import_manifest;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{Rule, RuleOrigin};
use rim_session::ports::{
    IMPORT_SOURCE_COMMUNITY_RULES, IMPORT_SOURCE_STEAM_DEPENDENCIES, IMPORT_SOURCE_USER_RULES,
    ImportError, ImportRecord, ImportedRules, RimSortImporter as RimSortImporterPort, RimSortPaths,
};
use sha2::{Digest, Sha256};

use crate::atomic::write_atomically;

fn to_import_error(path: &Path, error: impl std::fmt::Display) -> ImportError {
    ImportError(format!("{}: {error}", path.display()))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Writes `bytes` (already read from `source` for parsing, so this
/// doesn't re-read the — potentially tens-of-megabytes — source file) to
/// `<profile_dir>/imports/<source's file name>`, atomically. A no-op
/// when `source` has no file name (nothing sensible to copy to).
fn snapshot(profile_dir: &Path, source: &Path, bytes: &[u8]) -> Result<(), ImportError> {
    let Some(file_name) = source.file_name() else {
        return Ok(());
    };
    let dest = profile_dir.join("imports").join(file_name);
    write_atomically(&dest, bytes).map_err(|e| to_import_error(&dest, e))
}

/// Imports RimSort's `userRules.json`/`communityRules.json`/`steamDB.json`,
/// copying a snapshot of each source file into `<profile_dir>/imports/` (a
/// snapshot copy rather than a live re-read keeps a sort reproducible after
/// RimSort's own databases change; the rules page can always re-import to
/// refresh).
#[derive(Debug, Clone)]
pub struct RimSortImporter {
    profile_dir: PathBuf,
}

impl RimSortImporter {
    /// Builds the importer. Snapshots are copied into
    /// `<profile_dir>/imports/`.
    #[must_use]
    pub fn new(profile_dir: PathBuf) -> Self {
        Self { profile_dir }
    }
}

/// One source's parsed result, kept in memory until every present source
/// has parsed successfully — see [`RimSortImporter::import`]'s own doc
/// comment for why nothing is written to disk until then.
struct ParsedSource {
    path: PathBuf,
    bytes: Vec<u8>,
    rules: Vec<Rule>,
    skipped: usize,
}

impl RimSortImporter {
    /// Parses one `userRules.json`/`communityRules.json`-shaped source:
    /// `None` (this source wasn't part of the import) is skipped
    /// outright — no read, no parse — and reported as `None`, never an
    /// imported zero. Reads and
    /// parses only; writes nothing to disk (see [`Self::import`]).
    fn parse_rule_file(
        path: Option<&Path>,
        origin: RuleOrigin,
        active_ids: &BTreeSet<ModId>,
    ) -> Result<Option<ParsedSource>, ImportError> {
        let Some(path) = path else {
            return Ok(None);
        };
        let bytes = fs::read(path).map_err(|e| to_import_error(path, e))?;
        let (rules, skipped) =
            rules_file::parse(&bytes, origin, active_ids).map_err(|e| to_import_error(path, e))?;
        Ok(Some(ParsedSource {
            path: path.to_path_buf(),
            bytes,
            rules,
            skipped,
        }))
    }

    /// Parses `steamDB.json` — same `None`-is-skipped, read-and-parse-only
    /// contract as [`Self::parse_rule_file`], for the one source keyed by
    /// workshop id rather than plain `packageId`.
    fn parse_steam_db(
        path: Option<&Path>,
        active: &BTreeMap<ModId, Option<u64>>,
    ) -> Result<Option<ParsedSource>, ImportError> {
        let Some(path) = path else {
            return Ok(None);
        };
        let bytes = fs::read(path).map_err(|e| to_import_error(path, e))?;
        let (rules, skipped) =
            steam_db::parse(&bytes, active).map_err(|e| to_import_error(path, e))?;
        Ok(Some(ParsedSource {
            path: path.to_path_buf(),
            bytes,
            rules,
            skipped,
        }))
    }

    /// Snapshots `parsed` into `<profile_dir>/imports/` and returns its
    /// provenance record — the write half of [`Self::import`]'s
    /// all-or-nothing phase 2, run once per present source. Does **not**
    /// touch any manifest file: recording provenance in
    /// `<profile>/imports/manifest.json` is
    /// [`rim_session::use_cases::ImportRimSort::execute`]'s own job, only
    /// after its `RuleStore::save` has already succeeded (see
    /// `rim_session::ports::ImportManifestStore`'s own doc comment for
    /// why this moved out of the adapter).
    fn commit(
        &self,
        parsed: &ParsedSource,
        imported_at: &str,
    ) -> Result<ImportRecord, ImportError> {
        snapshot(&self.profile_dir, &parsed.path, &parsed.bytes)?;
        let file = parsed
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(ImportRecord {
            file,
            sha256: sha256_hex(&parsed.bytes),
            bytes: parsed.bytes.len(),
            imported_at: imported_at.to_string(),
        })
    }
}

impl RimSortImporterPort for RimSortImporter {
    /// Reads and parses every present source *before* writing anything
    /// (phase 1), then — only once every one of them has parsed
    /// successfully — snapshots each into `<profile_dir>/imports/` and
    /// returns its provenance on [`ImportedRules::provenance`] (phase 2).
    /// This ordering is load-bearing, not incidental: reading, parsing,
    /// and snapshotting one source at a time would let a later source
    /// failing to parse leave an earlier source's snapshot on disk
    /// overwritten with bytes the rules this call never returned were
    /// built from — breaking `imports/`'s whole contract ("the exact bytes
    /// this profile's rules were built from") the moment a second or third
    /// source failed. Phase 2 is the same all-or-nothing phase this
    /// method's own snapshot writes use.
    fn import(
        &self,
        paths: &RimSortPaths,
        active: &BTreeMap<ModId, Option<u64>>,
    ) -> Result<ImportedRules, ImportError> {
        // `userRules.json`/`communityRules.json` are keyed by `packageId`
        // alone — no workshop id ambiguity to resolve — so they only ever
        // need the plain id set.
        let active_ids: BTreeSet<ModId> = active.keys().cloned().collect();

        // Phase 1: read + parse only. Nothing touches disk yet.
        let user = Self::parse_rule_file(
            paths.user_rules.as_deref(),
            RuleOrigin::RimSortUser,
            &active_ids,
        )?;
        let community = Self::parse_rule_file(
            paths.community_rules.as_deref(),
            RuleOrigin::RimSortCommunity,
            &active_ids,
        )?;
        let steam = Self::parse_steam_db(paths.steam_db.as_deref(), active)?;

        // Phase 2: every present source parsed — snapshot each one and
        // collect its provenance. A source that wasn't part of this
        // import (`None`) is skipped here too; its previous snapshot, if
        // any, is left exactly as it was.
        let imported_at = jiff::Timestamp::now().to_string();
        let mut provenance = BTreeMap::new();
        if let Some(parsed) = &user {
            provenance.insert(
                IMPORT_SOURCE_USER_RULES.to_string(),
                self.commit(parsed, &imported_at)?,
            );
        }
        if let Some(parsed) = &community {
            provenance.insert(
                IMPORT_SOURCE_COMMUNITY_RULES.to_string(),
                self.commit(parsed, &imported_at)?,
            );
        }
        if let Some(parsed) = &steam {
            provenance.insert(
                IMPORT_SOURCE_STEAM_DEPENDENCIES.to_string(),
                self.commit(parsed, &imported_at)?,
            );
        }

        Ok(ImportedRules {
            // Summed only over the sources actually imported this call —
            // a `None` source contributes nothing, not even zero.
            skipped_inactive_rules: user.as_ref().map_or(0, |p| p.skipped)
                + community.as_ref().map_or(0, |p| p.skipped),
            skipped_inactive_steam: steam.as_ref().map_or(0, |p| p.skipped),
            user_rules: user.map(|p| p.rules),
            community_rules: community.map(|p| p.rules),
            steam_dependencies: steam.map(|p| p.rules),
            provenance,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rim_resolve::domain::Rule;
    use tempfile::tempdir;

    use super::*;

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    fn fixture_paths() -> RimSortPaths {
        RimSortPaths {
            user_rules: Some(fixture("userRules.json")),
            community_rules: Some(fixture("communityRules.json")),
            steam_db: Some(fixture("steamDB.json")),
        }
    }

    #[test]
    fn imports_rules_for_active_mods_and_snapshots_every_source_file() {
        let profile_dir = tempdir().expect("tempdir");
        let active: BTreeMap<ModId, Option<u64>> = [
            "example.framework",
            "fixture.addon.a",
            "fixture.addon.b",
            "fixture.addon.c",
            "example.architect",
            "fixture.addon.d",
            "fixture.meru",
            "example.animation",
            "example.animation.experimental",
            "example.patchlib",
            "ludeon.rimworld.biotech",
        ]
        .into_iter()
        .map(|id| (ModId::new(id), None))
        .collect();

        let importer = RimSortImporter::new(profile_dir.path().to_path_buf());
        let imported = importer
            .import(&fixture_paths(), &active)
            .expect("import must succeed");

        assert!(
            imported
                .user_rules
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(|r| matches!(r, Rule::Pair(p) if p.after == ModId::new("fixture.addon.a"))),
            "userRules.json's loadAfter example.framework must produce a pair rule"
        );
        assert!(
            imported
                .community_rules
                .as_deref()
                .unwrap_or_default()
                .iter()
                .any(
                    |r| matches!(r, Rule::Incompatible(i) if i.a == ModId::new("example.architect"))
                ),
            "communityRules.json's incompatibleWith must produce an incompatible rule"
        );
        assert!(imported.steam_dependencies.as_deref().unwrap_or_default().iter().any(|r| matches!(r,
                Rule::Pair(p) if p.after == ModId::new("fixture.meru") && p.before == ModId::new("example.patchlib")
            )),
            "steamDB.json's dependency chain must produce a pair rule"
        );

        for name in ["userRules.json", "communityRules.json", "steamDB.json"] {
            assert!(
                profile_dir.path().join("imports").join(name).is_file(),
                "{name} must be snapshotted into <profile_dir>/imports/"
            );
        }
    }

    #[test]
    fn counts_entries_skipped_for_being_inactive() {
        let profile_dir = tempdir().expect("tempdir");
        let importer = RimSortImporter::new(profile_dir.path().to_path_buf());

        let imported = importer
            .import(&fixture_paths(), &BTreeMap::new())
            .expect("import must succeed even with nothing active");

        assert!(imported.user_rules.expect("was imported").is_empty());
        assert!(imported.community_rules.expect("was imported").is_empty());
        assert!(
            imported
                .steam_dependencies
                .expect("was imported")
                .is_empty()
        );
        assert!(imported.skipped_inactive_rules > 0);
        assert!(imported.skipped_inactive_steam > 0);
    }

    #[test]
    fn missing_rimsort_files_are_an_error() {
        let profile_dir = tempdir().expect("tempdir");
        let importer = RimSortImporter::new(profile_dir.path().to_path_buf());
        let missing_paths = RimSortPaths {
            user_rules: Some(profile_dir.path().join("does-not-exist-userRules.json")),
            community_rules: Some(
                profile_dir
                    .path()
                    .join("does-not-exist-communityRules.json"),
            ),
            steam_db: Some(profile_dir.path().join("does-not-exist-steamDB.json")),
        };

        let result = importer.import(&missing_paths, &BTreeMap::new());

        assert!(result.is_err());
    }

    /// A `None` path is skipped outright — no read, no parse, no snapshot
    /// copy — and its `ImportedRules` field comes back `None`, never an
    /// imported zero. The other two sources import normally in the same call.
    #[test]
    fn a_none_path_is_skipped_and_reported_as_not_imported() {
        let profile_dir = tempdir().expect("tempdir");
        let importer = RimSortImporter::new(profile_dir.path().to_path_buf());
        let paths = RimSortPaths {
            user_rules: None,
            ..fixture_paths()
        };

        let imported = importer
            .import(&paths, &BTreeMap::new())
            .expect("import must succeed with one source skipped");

        assert!(
            imported.user_rules.is_none(),
            "a None path must report None, not an imported empty Vec"
        );
        assert!(imported.community_rules.is_some());
        assert!(imported.steam_dependencies.is_some());
        assert!(
            !profile_dir
                .path()
                .join("imports")
                .join("userRules.json")
                .exists(),
            "a skipped source must never be snapshotted"
        );
    }

    /// `skipped_inactive_rules` is summed only over the sources actually
    /// imported this call — a `None` source contributes nothing, not
    /// even zero-as-a-value. Proven by comparing the full three-source
    /// total against the sum of two calls, each isolating one source, so
    /// this test makes no assumption about either fixture's own skip
    /// count.
    #[test]
    fn skip_counts_are_summed_only_over_sources_actually_imported() {
        let profile_dir = tempdir().expect("tempdir");
        let importer = RimSortImporter::new(profile_dir.path().to_path_buf());

        let all_three = importer
            .import(&fixture_paths(), &BTreeMap::new())
            .expect("import must succeed");
        let user_only = importer
            .import(
                &RimSortPaths {
                    community_rules: None,
                    steam_db: None,
                    ..fixture_paths()
                },
                &BTreeMap::new(),
            )
            .expect("import must succeed with two sources skipped");
        let community_only = importer
            .import(
                &RimSortPaths {
                    user_rules: None,
                    steam_db: None,
                    ..fixture_paths()
                },
                &BTreeMap::new(),
            )
            .expect("import must succeed with two sources skipped");

        assert_eq!(
            all_three.skipped_inactive_rules,
            user_only.skipped_inactive_rules + community_only.skipped_inactive_rules,
            "the combined total must equal exactly the sum of each source imported alone"
        );
    }

    /// A later source failing to parse must never leave an *earlier*
    /// source's `imports/` snapshot already overwritten with bytes the
    /// rules this call ultimately never returns (the whole call errors)
    /// were never actually built from. Regression test for the
    /// parse-everything-then-write-everything ordering — a version of
    /// `import` that snapshots one source at a time as
    /// it parses would leave `userRules.json`'s snapshot written here
    /// despite the call as a whole failing on `communityRules.json`.
    #[test]
    fn a_later_source_failing_to_parse_leaves_an_earlier_sources_snapshot_untouched() {
        let profile_dir = tempdir().expect("tempdir");
        let community_path = profile_dir.path().join("communityRules.json");
        fs::write(&community_path, b"not valid json").expect("write malformed fixture");
        let paths = RimSortPaths {
            user_rules: Some(fixture("userRules.json")),
            community_rules: Some(community_path),
            steam_db: None,
        };
        let importer = RimSortImporter::new(profile_dir.path().to_path_buf());

        let result = importer.import(&paths, &BTreeMap::new());

        assert!(
            result.is_err(),
            "a malformed communityRules.json must fail the whole import"
        );
        assert!(
            !profile_dir
                .path()
                .join("imports")
                .join("userRules.json")
                .exists(),
            "userRules.json parsed successfully but must never be snapshotted when a later \
             source in the same call fails to parse"
        );
    }

    /// A successful import returns provenance for each source it actually
    /// imported — `RimSortImporter::import` itself writes no manifest
    /// (that belongs to `rim_session::use_cases::ImportRimSort::execute`);
    /// this only proves the provenance it *hands back* is
    /// right.
    #[test]
    fn a_successful_import_returns_provenance_for_every_imported_source() {
        let profile_dir = tempdir().expect("tempdir");
        let importer = RimSortImporter::new(profile_dir.path().to_path_buf());

        let imported = importer
            .import(&fixture_paths(), &BTreeMap::new())
            .expect("import must succeed");

        let user_record = imported
            .provenance
            .get(IMPORT_SOURCE_USER_RULES)
            .expect("user_rules must have a provenance record");
        assert_eq!(user_record.file, "userRules.json");
        let on_disk = fs::read(profile_dir.path().join("imports").join("userRules.json"))
            .expect("snapshot must exist");
        assert_eq!(
            user_record.sha256,
            sha256_hex(&on_disk),
            "the recorded sha256 must match the exact bytes snapshotted"
        );
        assert_eq!(user_record.bytes, on_disk.len());
        assert!(
            imported
                .provenance
                .contains_key(IMPORT_SOURCE_COMMUNITY_RULES)
        );
        assert!(
            imported
                .provenance
                .contains_key(IMPORT_SOURCE_STEAM_DEPENDENCIES)
        );
    }

    /// A source that wasn't part of this import (`None` path) has no
    /// provenance record either — mirroring `user_rules`/`community_rules`/
    /// `steam_dependencies` themselves being `None`.
    #[test]
    fn a_none_source_has_no_provenance_record() {
        let profile_dir = tempdir().expect("tempdir");
        let importer = RimSortImporter::new(profile_dir.path().to_path_buf());

        let imported = importer
            .import(
                &RimSortPaths {
                    user_rules: None,
                    ..fixture_paths()
                },
                &BTreeMap::new(),
            )
            .expect("import must succeed with one source skipped");

        assert!(!imported.provenance.contains_key(IMPORT_SOURCE_USER_RULES));
        assert!(
            imported
                .provenance
                .contains_key(IMPORT_SOURCE_COMMUNITY_RULES)
        );
    }
}
