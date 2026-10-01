//! Real-install verification for the def inspector — the same
//! `RIMMERGE_PERF_PROFILE_DIR` convention as `real_install_timing.rs` (a
//! *copy* of a real profile directory; never the live one, since the
//! session may write `rules.json`/`decisions.json` there).
//!
//! Every subject (which mods, which defs) is picked *from the inventory
//! itself* — `SourceIndex`'s own maps — rather than hardcoded, so this
//! test keeps measuring the right shapes ("the biggest owner", "the most
//! contested def") even as the real install's mod list drifts between
//! runs.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop --all-features --release --run-ignored ignored-only -E 'test(def_inspector_timing)'`

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{ModId, Selector};
use rim_merge::effective::Completeness;
use rim_resolve::domain::{DefKey, DefRef, FindingKey, OrderSource};
use rim_session::use_cases::InspectDef;
use rim_session::{ChangeFilter, Session};

/// This file's own "Run with" invocation (the module doc comment above),
/// named in every guard message so it points at the exact command for
/// this tier rather than a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop \
     --all-features --release --run-ignored ignored-only";

/// Builds the same real-install [`Session`] `real_install_timing.rs`
/// does, plus the def-source reader `InspectDef` needs (kept alongside
/// the session, since `Session` has no getter for the one it holds for
/// its own internal use), or `None` (with a skip message) when this is a
/// genuinely install-less machine — never touches the real profile
/// directory. On a machine that does have the real install, an unset
/// `RIMMERGE_PERF_PROFILE_DIR` panics instead of returning `None`
/// (`crate::real_install_support::require_profile_dir`) — see
/// `docs/testing.md`'s three-state rule for why.
fn load_real_session() -> Option<(
    Session,
    std::sync::Arc<dyn rim_session::ports::DefSourceReader + Send + Sync>,
)> {
    let profile_dir = crate::real_install_support::require_profile_dir(RERUN_COMMAND)?;
    let paths = crate::real_install_support::real_install_paths(profile_dir, RERUN_COMMAND);

    let state = crate::state::AppState::default();
    let adapters = state.adapters.clone();
    let use_case = rim_session::use_cases::LoadProject::new(
        adapters.scanner,
        adapters.config_store,
        adapters.decision_store,
        adapters.rule_store,
        adapters.patch_store,
        adapters.assignment_store,
        rim_io::FsModKnowledgeStore::vendored(),
        true,
    );
    let mut session = use_case.execute(paths, &mut |_| {}).expect("load project");
    session.set_def_source_reader(adapters.def_reader.clone());
    Some((session, adapters.def_reader))
}

fn report_ms(label: &str, elapsed: Duration, detail: &str) {
    eprintln!(
        "{label:<52} {:>9.3} ms   {detail}",
        elapsed.as_secs_f64() * 1e3
    );
}

/// The mod (other than `exclude`) that owns the most defs/templates —
/// "a big content mod", picked by the inventory rather than named by hand.
fn biggest_owner(sources: &SourceIndex, exclude: &ModId) -> (ModId, usize) {
    let mut owned: BTreeMap<ModId, usize> = BTreeMap::new();
    for (mod_id, _) in sources.defs.keys() {
        *owned.entry(mod_id.clone()).or_default() += 1;
    }
    owned
        .into_iter()
        .filter(|(mod_id, _)| mod_id != exclude)
        .max_by_key(|(_, count)| *count)
        .expect("at least one owning mod")
}

/// The mod with the most total top-level patch operations — "a
/// patch-heavy mod".
fn most_patch_heavy(sources: &SourceIndex) -> (ModId, usize) {
    sources
        .ops_by_mod
        .iter()
        .map(|(mod_id, ops)| (mod_id.clone(), ops.values().sum::<usize>()))
        .max_by_key(|(_, total)| *total)
        .expect("at least one patching mod")
}

/// The `(def_type, def_name)` with the most owners — "the most-owned
/// def" (the most contested ownership in the install).
fn most_owned_def(sources: &SourceIndex) -> (DefRef, usize) {
    let (key, owners) = sources
        .owners_by_def
        .iter()
        .max_by_key(|(_, owners)| owners.len())
        .expect("at least one owned def");
    let def_ref = DefRef::new(
        DefKey {
            def_type: key.0.clone(),
            def_name: key.1.clone(),
        },
        Selector::DefName,
    );
    (def_ref, owners.len())
}

/// The `(def_type, def_name, selector)` with the most total patch
/// operations across every mod that targets it — "the most-patched def".
fn most_patched_def(sources: &SourceIndex) -> (DefRef, usize) {
    let mut totals: BTreeMap<(String, String, Selector), usize> = BTreeMap::new();
    for per_mod in sources.ops_by_mod.values() {
        for (key, count) in per_mod {
            *totals.entry(key.clone()).or_default() += count;
        }
    }
    let ((def_type, def_name, selector), total) = totals
        .into_iter()
        .max_by_key(|(_, total)| *total)
        .expect("at least one patched def");
    (DefRef::new(DefKey { def_type, def_name }, selector), total)
}

/// The `Name`-attributed template with the most direct children —
/// "a `@Name` template with many children".
fn most_registered_template(sources: &SourceIndex) -> (DefRef, usize) {
    let ((def_type, name), children) = sources
        .children_by_template
        .iter()
        .max_by_key(|(_, children)| children.len())
        .expect("at least one template with children");
    let def_ref = DefRef::new(
        DefKey {
            def_type: def_type.clone(),
            def_name: name.clone(),
        },
        Selector::NameAttr,
    );
    (def_ref, children.len())
}

/// The concrete def with the deepest `ParentName` chain — "a heavily
/// inherited def". Depth is computed over `sources`' own name→parent map
/// (a cheap approximation: real inheritance ties break by load order the
/// same way `InspectDef` does — see `crates/rim-analyzer/CLAUDE.md` — but
/// a probe only needs *a* long chain, not the canonically correct one).
/// The chosen def's real chain length is re-measured below via a genuine
/// `InspectDef::execute` call, so the reported number is always ground
/// truth regardless of this approximation.
fn most_deeply_inherited_def(sources: &SourceIndex) -> DefRef {
    let mut parent_of: BTreeMap<String, Option<String>> = BTreeMap::new();
    for ((_, name), owners) in &sources.templates {
        if let Some((_, entry)) = owners.first() {
            parent_of
                .entry(name.clone())
                .or_insert_with(|| entry.parent_name.clone());
        }
    }

    fn depth(
        name: &str,
        parent_of: &BTreeMap<String, Option<String>>,
        memo: &mut BTreeMap<String, usize>,
    ) -> usize {
        if let Some(&cached) = memo.get(name) {
            return cached;
        }
        // Cycle guard: seed 0 before recursing so a self-referential chain
        // bottoms out instead of recursing forever.
        memo.insert(name.to_string(), 0);
        let computed = match parent_of.get(name).and_then(Clone::clone) {
            Some(parent) => 1 + depth(&parent, parent_of, memo),
            None => 0,
        };
        memo.insert(name.to_string(), computed);
        computed
    }

    let mut memo = BTreeMap::new();
    let mut best: Option<(DefKey, usize)> = None;
    for ((_mod_id, (def_type, def_name)), entries) in &sources.defs {
        let Some(entry) = entries.first() else {
            continue;
        };
        let Some(parent) = &entry.parent_name else {
            continue;
        };
        let chain_len = 1 + depth(parent, &parent_of, &mut memo);
        if best
            .as_ref()
            .is_none_or(|(_, best_len)| chain_len > *best_len)
        {
            best = Some((
                DefKey {
                    def_type: def_type.clone(),
                    def_name: def_name.clone(),
                },
                chain_len,
            ));
        }
    }
    let (key, _) = best.expect("at least one def with a ParentName");
    DefRef::new(key, Selector::DefName)
}

/// A def whose inspection comes back `Partial`, found by scanning the
/// ledger's own `PatchCollision` findings (a replay never skips a
/// stopper and continues — a collision this crate can't fully replay is
/// exactly where `Partial` shows up) rather than guessed at.
fn a_partial_def(
    session: &mut Session,
    inspector: &InspectDef<std::sync::Arc<dyn rim_session::ports::DefSourceReader + Send + Sync>>,
) -> Option<DefRef> {
    let candidates: Vec<DefRef> = session
        .ledger(OrderSource::Current)
        .entries
        .iter()
        .filter_map(|entry| match &entry.key {
            FindingKey::PatchCollision { .. } => entry.key.def_ref(),
            _ => None,
        })
        .collect();

    for def_ref in candidates {
        if let Ok(inspection) = inspector.execute(session, &def_ref)
            && matches!(
                inspection.effective.completeness,
                Completeness::Partial { .. }
            )
        {
            return Some(def_ref);
        }
    }
    None
}

#[tokio::test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
async fn def_inspector_against_the_real_install() {
    let Some((mut session, def_reader)) = load_real_session() else {
        return;
    };
    eprintln!(
        "loaded {} mods; changes/inspect/search timings follow",
        session.report().mods.len()
    );

    // --- `defs changes` for three mods, picked from the inventory. -----
    let core = ModId::new("ludeon.rimworld");
    let (big_content_mod, owned_count) = biggest_owner(session.sources(), &core);
    let (patch_heavy_mod, op_count) = most_patch_heavy(session.sources());

    eprintln!();
    eprintln!("defs changes — three mods:");
    for (label, mod_id) in [
        ("Core", core.clone()),
        ("big content mod", big_content_mod.clone()),
        ("patch-heavy mod", patch_heavy_mod.clone()),
    ] {
        let start = Instant::now();
        let page = session.changes(&mod_id, &ChangeFilter::default());
        let elapsed = start.elapsed();
        let kinds: Vec<String> = page
            .kind_counts
            .iter()
            .map(|(kind, count)| format!("{kind:?}={count}"))
            .collect();
        report_ms(
            &format!("changes({label} = {mod_id})"),
            elapsed,
            &format!("total={} kinds=[{}]", page.total, kinds.join(", ")),
        );
    }
    eprintln!(
        "  (big content mod owns {owned_count} defs; patch-heavy mod has {op_count} top-level ops)"
    );

    // --- `defs inspect` on five picked defs. ----------------------------
    let inspector = InspectDef::new(def_reader);

    let (most_owned, owner_count) = most_owned_def(session.sources());
    let (most_patched, patch_count) = most_patched_def(session.sources());
    let heavily_inherited = most_deeply_inherited_def(session.sources());
    let (most_children, child_count) = most_registered_template(session.sources());
    let partial = a_partial_def(&mut session, &inspector);

    eprintln!();
    eprintln!("defs inspect — five picked defs (cold, budget 500 ms each):");
    let mut budget_exceeded = Vec::new();
    for (label, def_ref, note) in [
        (
            "most-owned def",
            Some(most_owned),
            format!("{owner_count} owners"),
        ),
        (
            "most-patched def",
            Some(most_patched),
            format!("{patch_count} total ops"),
        ),
        (
            "heavily inherited def",
            Some(heavily_inherited),
            String::new(),
        ),
        (
            "@Name template, many children",
            Some(most_children),
            format!("{child_count} direct children"),
        ),
        ("a Partial def", partial, String::new()),
    ] {
        let Some(def_ref) = def_ref else {
            eprintln!("  {label:<32} — no candidate found, skipped");
            continue;
        };
        let start = Instant::now();
        let inspection = match inspector.execute(&mut session, &def_ref) {
            Ok(inspection) => inspection,
            Err(error) => {
                eprintln!("  {label:<32} {def_ref} — inspect failed: {error}");
                continue;
            }
        };
        let elapsed = start.elapsed();
        let completeness = match inspection.effective.completeness {
            Completeness::Complete => "complete".to_string(),
            Completeness::Partial { .. } => "PARTIAL".to_string(),
        };
        report_ms(
            &format!("inspect({label})"),
            elapsed,
            &format!(
                "{def_ref}  owners={} patchers={} completeness={completeness}  {note}",
                inspection.owners.len(),
                inspection.patchers.len()
            ),
        );
        if elapsed > Duration::from_millis(500) {
            budget_exceeded.push((label.to_string(), def_ref.to_string(), elapsed));
        }
    }
    for (label, def_ref, elapsed) in &budget_exceeded {
        eprintln!(
            "  BUDGET EXCEEDED: {label} ({def_ref}) took {:.1} ms (budget 500 ms) — recorded, not chased blind",
            elapsed.as_secs_f64() * 1e3
        );
    }

    // --- cache: a second inspect of the same ref must not re-read files.
    // Picks the alphabetically-first owned def — distinct from every one
    // of the five above — so this really is a first, cold call.
    let warm_ref = {
        let (key, _) = session
            .sources()
            .owners_by_def
            .iter()
            .next()
            .expect("at least one owned def");
        DefRef::new(
            DefKey {
                def_type: key.0.clone(),
                def_name: key.1.clone(),
            },
            Selector::DefName,
        )
    };
    let cold_start = Instant::now();
    let _ = inspector
        .execute(&mut session, &warm_ref)
        .expect("first inspect");
    let cold_elapsed = cold_start.elapsed();
    let warm_start = Instant::now();
    let _ = inspector
        .execute(&mut session, &warm_ref)
        .expect("second inspect (cache hit)");
    let warm_elapsed = warm_start.elapsed();
    eprintln!();
    report_ms("inspect cache — cold", cold_elapsed, &warm_ref.to_string());
    report_ms("inspect cache — warm (cache hit)", warm_elapsed, "");
    assert!(
        warm_elapsed < cold_elapsed,
        "a cache hit must not be slower than the first, file-reading call: cold={cold_elapsed:?} warm={warm_elapsed:?}"
    );

    // --- `defs search` for a common prefix. -----------------------------
    let start = Instant::now();
    let hits = session.search_defs("Wall", rim_session::MAX_PAGE_SIZE);
    let elapsed = start.elapsed();
    report_ms("search(\"Wall\")", elapsed, &format!("{} hits", hits.len()));
    assert!(
        elapsed < Duration::from_millis(100),
        "search must stay under the 100 ms budget: took {elapsed:?}"
    );
}
