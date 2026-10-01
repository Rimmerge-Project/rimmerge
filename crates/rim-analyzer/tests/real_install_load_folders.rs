//! Real-install invariants for `LoadFolders.xml` resolution — no under-scan
//! (version-block selection) and no over-scan (`IfModActiveAll` gating) —
//! checked against the real install rather than a fixture. Both tests
//! re-derive their own expectation independently from the raw XML rather than
//! asking the code under test to validate itself — a fresh, small
//! reimplementation of the relevant rules, not a call into
//! `extract::load_folders`'s own private selection functions.
//!
//! `#[ignore]`d: reads the real game/workshop install and the real
//! `ModsConfig.xml` (read-only — nothing here writes anywhere) and
//! requires both `RIMMERGE_GAME_DIR` (which is what puts this machine in
//! the real-install tier at all -- unset, every test here skips with an
//! honest `skipping:` line) and `RIMMERGE_PERF_PROFILE_DIR`. This crate's own scan
//! needs no profile directory at all (that's a `rim-session`/`rim-io`
//! concept), so `common::require_scratch_profile_dir_is_set` is a pure
//! safety rail matching every other test in this documented tier (root
//! `CLAUDE.md`, `docs/testing.md`) — it **fails loudly, by panicking**,
//! when the variable is unset, rather than following
//! `apps/cli/tests/real_install_assign.rs`'s soft-skip-on-unset pattern,
//! which both call out as a trap that reports a tier "passed" while
//! asserting nothing.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features --release --run-ignored ignored-only`

use std::collections::HashSet;

use rim_analyzer::domain::{GameVersion, ModId, ScanOutput};
use rim_analyzer::extract::load_folders::{has_exact_version_block, resolve_version_entry};
use rim_analyzer::infra;

mod common;

/// This file's own "Run with" invocation (the module doc comment above),
/// named in every guard message so it points at the exact command for
/// this tier rather than a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features \
     --release --run-ignored ignored-only";

/// Scans the real, current install, read-only. `None` on a machine
/// outside the real-install tier (`RIMMERGE_GAME_DIR` unset); a
/// misconfigured one panics inside [`common::real_scan_config`] instead.
/// Panics with a clear message on any scan failure — this test only ever
/// runs by hand, so a loud panic is more useful here than threading a
/// `Result` through a `#[test]` fn.
fn scan_real_install() -> Option<(ScanOutput, GameVersion)> {
    let (config, game_version) = common::real_scan_config(RERUN_COMMAND)?;
    let output =
        infra::scan(&config).unwrap_or_else(|error| panic!("scanning the real install: {error}"));
    Some((output, game_version))
}

/// `extract::xml_util::decode_lossy` is `pub(crate)`, unreachable from
/// this integration-test crate — this is the same 3-line BOM-strip logic,
/// deliberately duplicated rather than widening that module's visibility
/// for one test file.
fn decode_lossy(bytes: &[u8]) -> String {
    const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
    let bytes = bytes.strip_prefix(&UTF8_BOM).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// A `LoadFolders.xml` block element's lookup key: lowercased, one leading
/// `v` stripped. A fresh, independent reimplementation of the
/// key-normalization rule for this test's own oracle — not a call into
/// `extract::load_folders`'s own private `block_key`.
fn normalized_block_key(tag_name: &str) -> String {
    let lower = tag_name.to_lowercase();
    match lower.strip_prefix('v') {
        Some(rest) => rest.to_string(),
        None => lower,
    }
}

/// A fresh, independent `(major, minor)` parse of a block key, deliberately
/// duplicating the rule rather than calling `extract::load_folders`'s own
/// private `parse_engine_version_key` — this test is meant to be a genuine
/// second opinion, not the code under test checking itself.
fn independent_major_minor(key: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = key.split('.').collect();
    if parts.len() > 3 {
        return None;
    }
    let mut numbers = Vec::with_capacity(parts.len());
    for part in &parts {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        numbers.push(part.parse::<u32>().ok()?);
    }
    let major = *numbers.first()?;
    let minor = numbers.get(1).copied().unwrap_or(0);
    Some((major, minor))
}

/// Whether `root` (a parsed `<loadFolders>` document) has *some* legitimate
/// fallback RimWorld could use for `game_version` besides an exact `<vX.Y>`
/// block: a `<default>` element, or any other element whose normalized key
/// parses ([`independent_major_minor`]) to a version `<=` `game_version`.
/// Mirrors the engine's own candidate filter (`default`/empty/no-`.` keys
/// excluded) independently.
fn has_any_fallback_candidate(root: roxmltree::Node, game_version: GameVersion) -> bool {
    root.children()
        .filter(roxmltree::Node::is_element)
        .any(|child| {
            let key = normalized_block_key(child.tag_name().name());
            if key == "default" {
                return true;
            }
            if key.is_empty() || !key.contains('.') {
                return false;
            }
            independent_major_minor(&key).is_some_and(|(major, minor)| {
                (major, minor) <= (game_version.major, game_version.minor)
            })
        })
}

/// Every `<li IfModActiveAll="...">` anywhere in the document (not only the
/// block the analyzer would actually select — see this function's own return
/// value), paired with its own normalized folder text and required ids.
/// Deliberately whole-document, not block-scoped: scoping correctly would
/// mean re-deriving the version-block selection here too, which would stop
/// this from being an independent check. Scanning the whole document only
/// risks a false *pass* (a coincidentally-identical folder name in a block
/// that was never selected), never a false *failure* — this test's own
/// invariant ("no active mod scans a folder whose `IfModActiveAll` set is
/// unsatisfied") still holds exactly when every such pair it finds is
/// honoured.
fn if_mod_active_all_entries(root: roxmltree::Node) -> Vec<(String, Vec<String>)> {
    let mut entries = Vec::new();
    for block in root.children().filter(roxmltree::Node::is_element) {
        for li in block
            .children()
            .filter(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case("li"))
        {
            let Some(raw_ids) = li.attribute("IfModActiveAll") else {
                continue;
            };
            let ids: Vec<String> = raw_ids
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_lowercase)
                .collect();
            if ids.is_empty() {
                continue; // vacuously satisfied — nothing to violate
            }
            let raw_text = li.text().unwrap_or("").trim();
            let folder = raw_text.trim_start_matches(['/', '\\']).to_string();
            entries.push((folder, ids));
        }
    }
    entries
}

/// No under-scan: for every active mod shipping a `LoadFolders.xml` with no
/// exact block for the running game version: if the file has *no* legitimate
/// fallback at all (independently re-derived —
/// [`has_any_fallback_candidate`]), `resolve_version_entry` returning `None`
/// is correct (the mod falls to the default *directory* rule). If it *does*
/// have a legitimate fallback, `resolve_version_entry` must not return `None`
/// — an exact-then-default-only fallthrough would, for exactly this shape.
/// Also asserts, in aggregate, that at least one such mod's resolved folder
/// actually exists on disk, proving resolution produces real, scannable
/// folders rather than merely a non-`None` return value. Asserts the
/// invariant, not specific mod ids — nothing here names a specific mod or
/// hardcodes a folder set.
#[test]
#[ignore = "needs the real install and RIMMERGE_PERF_PROFILE_DIR — see this file's own doc comment"]
fn under_scanned_mods_now_resolve_via_a_version_block() {
    let Some((scan, game_version)) = scan_real_install() else {
        return;
    };
    let active_bases: HashSet<ModId> = scan.load_order.as_slice().iter().map(ModId::base).collect();

    let mut candidates_checked = 0usize;
    let mut real_folder_resolutions = 0usize;

    for scanned_mod in &scan.scanned_mods {
        let load_folders_path = scanned_mod.info.path.join("LoadFolders.xml");
        let Ok(bytes) = std::fs::read(&load_folders_path) else {
            continue; // no LoadFolders.xml at all — out of scope for this test
        };
        let Ok(false) = has_exact_version_block(&bytes, game_version) else {
            continue; // has an exact block, or malformed — not the under-scan shape
        };
        candidates_checked += 1;

        let text = decode_lossy(&bytes);
        let doc = roxmltree::Document::parse(&text).unwrap_or_else(|error| {
            panic!(
                "{}: has_exact_version_block just parsed this file successfully, so a second \
                 parse of the same bytes must too: {error}",
                scanned_mod.info.id
            )
        });
        let has_fallback = has_any_fallback_candidate(doc.root_element(), game_version);

        let resolved = resolve_version_entry(&bytes, game_version, &active_bases)
            .unwrap_or_else(|error| panic!("{}: {error}", scanned_mod.info.id));

        assert!(
            has_fallback || resolved.is_none(),
            "{}: this test's own independent check found no <default>/eligible-lower-version \
             block, but resolve_version_entry still returned Some — either this check or the \
             code under test is wrong",
            scanned_mod.info.id
        );
        assert!(
            resolved.is_some() || !has_fallback,
            "{}: LoadFolders.xml has no exact v{}.{} block but does have a legitimate fallback \
             (a <default> or an eligible lower version block) — resolve_version_entry must not \
             return None for this shape (the exact under-scan bug this test guards against)",
            scanned_mod.info.id,
            game_version.major,
            game_version.minor
        );

        if let Some(entry) = resolved {
            let resolves_a_real_folder = entry.folders.iter().any(|folder| {
                let path = if folder.relative.is_empty() {
                    scanned_mod.info.path.clone()
                } else {
                    scanned_mod.info.path.join(&folder.relative)
                };
                path.is_dir()
            });
            if resolves_a_real_folder {
                real_folder_resolutions += 1;
            }
        }
    }

    eprintln!(
        "under-scan class (test 13): {candidates_checked} active mods ship a LoadFolders.xml \
         with no exact block; {real_folder_resolutions} of them resolve at least one real, \
         on-disk folder from a version block"
    );
    assert!(
        candidates_checked > 0,
        "no active mod on this install currently has the under-scan shape (a LoadFolders.xml \
         with no exact block) — this test can't exercise the fix on this install right now"
    );
    assert!(
        real_folder_resolutions > 0,
        "the under-scan class exists on this install, but not one such mod resolved a real, \
         on-disk folder from a version block — the fix looks reverted or broken"
    );
}

/// No over-scan: for every active mod shipping a `LoadFolders.xml` that
/// resolves to a version entry, every `<li IfModActiveAll="...">` this test's
/// own independent walk finds ([`if_mod_active_all_entries`]) whose required
/// ids are not *all* active must have its own normalized folder text absent
/// from the resolved `entry.folders`. Asserts the invariant, not a specific
/// entry count — nothing here hardcodes a count or names a specific mod; the
/// aggregate "found at least one real, currently-unsatisfied gate" check only
/// proves this test actually exercises the invariant on this install, not
/// what the invariant's answer should be.
#[test]
#[ignore = "needs the real install and RIMMERGE_PERF_PROFILE_DIR — see this file's own doc comment"]
fn no_active_mod_scans_a_folder_gated_by_an_unsatisfied_if_mod_active_all() {
    let Some((scan, game_version)) = scan_real_install() else {
        return;
    };
    let active_bases: HashSet<ModId> = scan.load_order.as_slice().iter().map(ModId::base).collect();

    let mut mods_checked = 0usize;
    let mut unsatisfied_gates_found = 0usize;

    for scanned_mod in &scan.scanned_mods {
        let load_folders_path = scanned_mod.info.path.join("LoadFolders.xml");
        let Ok(bytes) = std::fs::read(&load_folders_path) else {
            continue;
        };
        let Ok(Some(entry)) = resolve_version_entry(&bytes, game_version, &active_bases) else {
            continue; // no version entry resolved — nothing from LoadFolders.xml is scanned
        };
        mods_checked += 1;

        let text = decode_lossy(&bytes);
        let Ok(doc) = roxmltree::Document::parse(&text) else {
            continue; // resolve_version_entry just parsed this, so unreachable in practice
        };
        for (folder, required_ids) in if_mod_active_all_entries(doc.root_element()) {
            let all_active = required_ids
                .iter()
                .all(|id| active_bases.contains(&ModId::new(id).base()));
            if all_active {
                continue; // satisfied — this folder is allowed to be scanned
            }
            unsatisfied_gates_found += 1;
            assert!(
                !entry.folders.iter().any(|f| f.relative == folder),
                "{}: folder {folder:?} is gated by IfModActiveAll=\"{}\" (not all active) but \
                 was still resolved for scanning — the over-scan bug this test guards against",
                scanned_mod.info.id,
                required_ids.join(",")
            );
        }
    }

    eprintln!(
        "over-scan class (test 14): {mods_checked} active mods resolve a LoadFolders.xml \
         version entry; {unsatisfied_gates_found} currently-unsatisfied IfModActiveAll gates \
         found across them, none scanned"
    );
    assert!(
        unsatisfied_gates_found > 0,
        "no currently-unsatisfied IfModActiveAll gate was found on this install — this test \
         can't exercise the invariant on this install right now"
    );
}
