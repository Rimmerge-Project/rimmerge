//! Real-install consistency check for the def inspector: for a sample of
//! contested defs, `InspectDef`'s own patcher list must agree — same
//! mods, same order — with `PlanMerge`'s scoped preview for that same
//! `PatchCollision` finding. This is `rim-session`'s own
//! `inspect_defs_patchers_agree_with_plan_merges_own_contributions`
//! regression (`crates/rim-session/src/use_cases/inspect_def.rs`), which
//! proved the two use cases can't silently drift apart on one hand-built
//! fixture; this test proves the same thing at real-install scale.
//!
//! `#[ignore]`d: needs the real game/workshop install (read-only) and a
//! *copy* of a real profile directory in `RIMMERGE_PERF_PROFILE_DIR`
//! (never the real one — the session may write `rules.json`/
//! `decisions.json`/an inspections cache there). Builds its own
//! [`rim_session::Session`] directly (rather than reusing
//! `apps/cli/src/common.rs::build_session`, which is private to the
//! binary crate and unreachable from an integration test) — the same
//! ~15-line duplication `apps/desktop/src-tauri/src/real_install_timing.rs`
//! already accepts for the same reason.
//!
//! `mod def_conflict_view_real_install` shares [`load_real_session`] for
//! three more real-install cases:
//! `Session::def_conflict_view` on `BiomeDef/AridShrubland`'s
//! `plantDensity` collision, `HediffDef/BionicHeart`'s `comps` list, and
//! a freshly stored merge choice on a real DefOverride (named via RIMMERGE_EXPECTED_DEF_OVERRIDE_CASE).
//! `RIMMERGE_PERF_PROFILE_DIR` for those must point at an **empty**
//! scratch directory (never a copy of a real one): `Session::decide` is
//! in-memory only (no persisting use case wraps it here), so nothing is
//! ever written to it, but `LoadProject` still needs *a* writable
//! directory to hand `rim-io`'s stores, which happily default an empty
//! one to "no rules, no decisions yet".
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo test -p rimmerge-cli --release --test real_install_defs -- --ignored --nocapture`
//! or, for one of the three `def_conflict_view` cases specifically:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rimmerge-cli --release --run-ignored ignored-only -E 'test(arid_shrubland_plant_density_collision_has_no_unsupported_op)'`

mod common;

use std::path::PathBuf;
use std::sync::Arc;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::FindingKey;
use rim_session::Session;
use rim_session::use_cases::{InspectDef, PlanMerge};

const CONTESTED_DEF_SAMPLE: usize = 10;

/// This file's own "Run with" invocation (this module's own doc comment,
/// above) — named in [`common::require_profile_dir`]'s panic message.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo test -p rimmerge-cli \
     --release --test real_install_defs -- --ignored --nocapture";

fn load_real_session(profile_dir: PathBuf) -> Session {
    let paths = common::real_install_paths(profile_dir, RERUN_COMMAND);
    let use_case = rim_session::use_cases::LoadProject::new(
        rim_io::AnalyzerScanner::new(),
        rim_io::ModsConfigFileStore::new(),
        rim_io::JsonDecisionStore::new(),
        rim_io::JsonRuleStore::new(),
        rim_io::JsonPatchProjectStore::new(),
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FsModKnowledgeStore::vendored(),
        true,
    );
    let mut session = use_case
        .execute(paths, &mut |_| {})
        .unwrap_or_else(|error| panic!("load project: {error}"));
    session.set_def_source_reader(Arc::new(rim_io::FileDefSourceReader::new()));
    session
}

#[test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
fn inspect_def_patchers_agree_with_plan_merge_on_ten_contested_defs() {
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let mut session = load_real_session(profile_dir);

    // Ten distinct `PatchCollision` keys, in whatever order the ledger
    // reports them — a real, contested sample, not a hand-picked one.
    let sample: Vec<FindingKey> = session
        .ledger(rim_resolve::domain::OrderSource::Current)
        .entries
        .iter()
        .filter_map(|entry| match &entry.key {
            FindingKey::PatchCollision { .. } => Some(entry.key.clone()),
            _ => None,
        })
        .take(CONTESTED_DEF_SAMPLE)
        .collect();
    assert!(
        !sample.is_empty(),
        "expected at least one PatchCollision finding on the real install"
    );

    let planner = PlanMerge::new(rim_io::FileDefSourceReader::new());
    let inspector = InspectDef::new(rim_io::FileDefSourceReader::new());

    for key in &sample {
        let FindingKey::PatchCollision { mods, .. } = key else {
            unreachable!("filtered to PatchCollision above");
        };
        let def_ref = key
            .def_ref()
            .unwrap_or_else(|| panic!("PatchCollision {key} must have a def_ref"));

        let preview = planner
            .execute(&mut session, key)
            .unwrap_or_else(|error| panic!("planning {key}: {error}"));
        let plan_owners: Vec<ModId> = preview.owners.clone();

        let inspection = inspector
            .execute(&mut session, &def_ref)
            .unwrap_or_else(|error| panic!("inspecting {def_ref}: {error}"));
        let inspected_patchers: Vec<ModId> = inspection
            .patchers
            .iter()
            .filter(|patcher| mods.contains(&patcher.mod_id))
            .map(|patcher| patcher.mod_id.clone())
            .collect();

        eprintln!("{def_ref:<48} plan={plan_owners:?} inspect(filtered)={inspected_patchers:?}");
        assert_eq!(
            inspected_patchers, plan_owners,
            "InspectDef's own patcher list, filtered to {key}'s own mods, must agree \
             with PlanMerge's scoped preview — same mods, same order"
        );
    }

    eprintln!(
        "{} of {} requested contested defs checked, all agree",
        sample.len(),
        CONTESTED_DEF_SAMPLE
    );
}

/// Real-install verification of the def conflict view:
/// `InspectDef` + `PlanMerge` + `Session::def_conflict_view` on three
/// hand-picked, real contested defs, asserting the user-facing
/// properties each case exists to cover. Shares
/// [`load_real_session`] with the patcher-agreement test above; each case
/// is its own `#[ignore]`d test so a single case can be re-run with `cargo
/// nextest run -p rimmerge-cli --release --run-ignored ignored-only -E
/// 'test(<name>)'` without re-loading the whole install three times in
/// one process.
mod def_conflict_view_real_install {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use rim_merge::effective::Completeness;
    use rim_merge::tree::FieldPath;
    use rim_resolve::domain::{Action, Decision, FindingKey, MergeChoice, OrderSource};
    use rim_session::use_cases::{InspectDef, PlanMerge};
    use rim_session::{FieldRowKind, Preference, Problem, Session};

    use super::{ModId, load_real_session};

    /// Finds a live `DefOverride` finding for `def_type`/`def_name` in the
    /// current ledger — never hand-built, since `FindingKey`'s own field
    /// shapes (`owners: BTreeSet<ModId>`, in this case) are exactly what
    /// this test wants to avoid re-deriving by hand from the real
    /// install's own mod list.
    fn def_override_key(session: &mut Session, def_type: &str, def_name: &str) -> FindingKey {
        session
            .ledger(OrderSource::Current)
            .entries
            .iter()
            .find_map(|entry| match &entry.key {
                FindingKey::DefOverride { key, .. }
                    if key.def_type == def_type && key.def_name == def_name =>
                {
                    Some(entry.key.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "expected a live DefOverride finding for {def_type}/{def_name} on the real \
                     install — has the active mod list changed since this test was written?"
                )
            })
    }

    /// [`def_override_key`]'s `PatchCollision` sibling, additionally
    /// matched on `sub_path` since one def can carry several collisions
    /// (`AridShrubland` has several on the real install; this test wants
    /// the one contested field specifically, not whichever the ledger
    /// lists first).
    fn patch_collision_key(
        session: &mut Session,
        def_type: &str,
        def_name: &str,
        sub_path: &str,
    ) -> FindingKey {
        session
            .ledger(OrderSource::Current)
            .entries
            .iter()
            .find_map(|entry| match &entry.key {
                FindingKey::PatchCollision {
                    key,
                    sub_path: entry_sub_path,
                    ..
                } if key.def_type == def_type
                    && key.def_name == def_name
                    && entry_sub_path.as_deref() == Some(sub_path) =>
                {
                    Some(entry.key.clone())
                }
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "expected a live PatchCollision finding for {def_type}/{def_name}#{sub_path} \
                     on the real install — has the active mod list changed since this test was \
                     written?"
                )
            })
    }

    /// The tier gate, shared with every other real-install test in this
    /// crate: `None` (after an honest `skipping:` line) on a machine
    /// outside the tier, a panic naming the missing variable inside it.
    fn scratch_profile_dir() -> Option<PathBuf> {
        super::common::require_profile_dir(super::RERUN_COMMAND)
    }

    /// The two real mods this install's own `plantDensity` collision on
    /// `BiomeDef/AridShrubland` needs — named only through this env var
    /// format
    /// `<mod a>,<mod b>`. `None` (after an honest skip message) when
    /// unset; the test that uses it returns early in that case.
    fn expected_plant_density_pair() -> Option<(ModId, ModId)> {
        let Ok(raw) = std::env::var("RIMMERGE_EXPECTED_PLANT_DENSITY_PAIR") else {
            eprintln!(
                "RIMMERGE_EXPECTED_PLANT_DENSITY_PAIR not set -- skipping the AridShrubland \
                 plantDensity collision test (format: <mod a>,<mod b>)"
            );
            return None;
        };
        let (a, b) = raw.split_once(',').unwrap_or_else(|| {
            panic!("RIMMERGE_EXPECTED_PLANT_DENSITY_PAIR must be '<mod a>,<mod b>', got {raw}")
        });
        Some((ModId::new(a), ModId::new(b)))
    }

    /// A handful of `(wildPlants key, owning mod id)` spot checks for the
    /// same `AridShrubland` real install case — named only through this
    /// env var, format `<key path>:<mod id>,...`. Empty (after an honest
    /// skip message) when unset; the spot-check loop that uses it simply
    /// has nothing to check in that case.
    fn expected_wild_plants_sample() -> Vec<(String, String)> {
        let Some(raw) = super::common::require_pin_var(
            "RIMMERGE_EXPECTED_WILD_PLANTS_SAMPLE",
            "RIMMERGE_GAME_DIR=<install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> \
             RIMMERGE_EXPECTED_WILD_PLANTS_SAMPLE=<wildPlants key>:<mod id>,... cargo nextest \
             run -p rimmerge-cli --all-features --release --run-ignored ignored-only -E \
             'binary(real_install_defs)'",
        ) else {
            return Vec::new();
        };
        raw.split(',')
            .filter(|entry| !entry.trim().is_empty())
            .map(|entry| {
                let (key_name, mod_id) = entry.split_once(':').unwrap_or_else(|| {
                    panic!(
                        "RIMMERGE_EXPECTED_WILD_PLANTS_SAMPLE entries must be '<wildPlants \
                         key>:<mod id>', got {entry}"
                    )
                });
                (key_name.to_string(), mod_id.to_string())
            })
            .collect()
    }

    /// `BiomeDef/AridShrubland`'s `plantDensity` collision — two real
    /// content mods, a real, contested pair. Planning a def one of whose
    /// active patchers ships a bare `/Defs`-scoped root add elsewhere in
    /// its own patch file resolves that add as `XPathExpr::DocumentRoot`
    /// rather than failing with `PlanFailed`/`UnsupportedOp` and starving
    /// this panel — this pins that end to end, against the real install.
    ///
    /// **A real finding, not an assumption**: on this install both mods'
    /// own `PatchOperationReplace` happen to land on the *identical* final
    /// value — `DiffClass::Agreeing`, not `Conflict` (two touchers
    /// agreeing on one value is a clean merge, not a contested one,
    /// regardless of the analyzer's own "contested" *severity* label,
    /// which only means "more than one patcher touches this path" — it
    /// never inspects the values themselves). This test asserts the real,
    /// observed `CleanMerge` shape rather than an assumed `Conflict`.
    #[test]
    #[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
    fn arid_shrubland_plant_density_collision_has_no_unsupported_op() {
        let Some((mod_a, mod_b)) = expected_plant_density_pair() else {
            return;
        };
        let Some(profile_dir) = scratch_profile_dir() else {
            return;
        };
        let mut session = load_real_session(profile_dir);
        let key = patch_collision_key(&mut session, "BiomeDef", "AridShrubland", "plantDensity");
        let def_ref = key.def_ref().expect("PatchCollision always has a def_ref");

        InspectDef::new(rim_io::FileDefSourceReader::new())
            .execute(&mut session, &def_ref)
            .unwrap_or_else(|error| panic!("inspecting {def_ref}: {error}"));
        PlanMerge::new(rim_io::FileDefSourceReader::new())
            .execute(&mut session, &key)
            .unwrap_or_else(|error| panic!("planning {key}: {error}"));

        let view = session
            .def_conflict_view(&key)
            .unwrap_or_else(|error| panic!("building the conflict view for {key}: {error}"));

        assert!(
            !view.fields.is_empty(),
            "an empty panel is never acceptable — {key} produced no field rows"
        );
        assert!(
            view.problems.iter().all(|problem| !matches!(
                problem,
                Problem::UnsupportedOp { .. } | Problem::PlanFailed { .. }
            )),
            "the root-add grammar fix must leave this collision with no UnsupportedOp/PlanFailed \
             problems, got {:#?}",
            view.problems
        );
        assert_eq!(
            view.effective_completeness,
            Completeness::Complete,
            "the fold must reach the end for {key}"
        );

        let contested_path: FieldPath = "plantDensity".parse().expect("valid field path");
        let row = view
            .fields
            .iter()
            .find(|row| row.path == contested_path)
            .unwrap_or_else(|| panic!("expected a row for plantDensity, got {:#?}", view.fields));
        // Both mods replace to the same real value on this install — the
        // `Agreeing` rule, not `Conflict`. Still asserted explicitly
        // (never `Unchanged`/`ListEntry`) so a future value drift between
        // the two mods that turns this into a genuine `Conflict` is
        // equally covered by every assertion below.
        assert_ne!(
            row.kind,
            FieldRowKind::Unchanged,
            "plantDensity is touched by two active patchers on this install — never Unchanged: {row:?}"
        );
        let contesting_mods: std::collections::BTreeSet<ModId> = row
            .values
            .iter()
            .map(|(mod_id, _)| mod_id.clone())
            .collect();
        assert!(
            contesting_mods.contains(&mod_a) && contesting_mods.contains(&mod_b),
            "expected both {mod_a} and {mod_b} in plantDensity's own values, got \
             {contesting_mods:?}"
        );
        let (in_game_mod, _) = row
            .in_game
            .as_ref()
            .unwrap_or_else(|| panic!("plantDensity must have an in_game value"));
        assert!(
            contesting_mods.contains(in_game_mod),
            "in_game must credit one of plantDensity's own two contributors, got {in_game_mod} \
             among {contesting_mods:?}"
        );
        match row.kind {
            FieldRowKind::Conflict => match &row.preference {
                Preference::LoadOrder { winner } => assert_eq!(winner, in_game_mod, "{row:?}"),
                other => panic!(
                    "expected a LoadOrder preference (no decision/choice exists here), got \
                     {other:?}"
                ),
            },
            // A CleanMerge/ListEntry row carries no preference by design
            // (`FieldRow::preference`'s own doc comment) — "who wins and
            // why" is reserved for genuine conflicts.
            _ => assert_eq!(row.preference, Preference::None, "{row:?}"),
        }

        eprintln!(
            "AridShrubland/plantDensity: kind={:?} field rows={} touchers={} in_game={in_game_mod} \
             completeness={:?} problems={}",
            row.kind,
            view.fields.len(),
            view.touchers.len(),
            view.effective_completeness,
            view.problems.len()
        );
    }

    /// `BiomeDef/AridShrubland` carries no `wildAnimals` `PatchCollision`
    /// finding at all — pinned so no future reader "restores" a collision
    /// expectation. The real patch XML shape:
    ///
    /// ```xml
    /// <xpath>Defs/BiomeDef[defName="AridShrubland"]/wildAnimals</xpath>
    /// <value>
    ///   <XBM_Theropod>0.6</XBM_Theropod>
    ///   <XBM_Synapsid>0.3</XBM_Synapsid>
    ///   ...
    /// </value>
    /// ```
    ///
    /// `wildAnimals` is a tag-keyed map (`Dictionary<PawnKindDef, float>`),
    /// not an `li` list — three real content mods each inject their own, wholly
    /// disjoint top-level element names via a plain `PatchOperationAdd`,
    /// with no overlap with each other or with vanilla Core's keys. The
    /// analyzer keys such an `Add` by `sub_path` **plus** each top-level
    /// element name its own `<value>` injects
    /// (`analysis::conflicts::patches::split_injected_elements`), so these three mods'
    /// contributions land in three entirely separate keys instead of one
    /// shared `wildAnimals` bucket — no two mods ever touch the same key,
    /// so no collision is ever reported at all. The mods' entries all
    /// coexist with nothing to merge, so the collision vanishes rather than
    /// merely resolving cleanly. Contrast the sibling test below,
    /// `arid_shrubland_wild_plants_collision_unions_cleanly_despite_a_replace`,
    /// where a *genuine* keyed-map collision on the same def correctly
    /// still exists.
    #[test]
    #[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
    fn arid_shrubland_has_no_wild_animals_patch_collision_after_the_keyed_map_split() {
        let Some(profile_dir) = scratch_profile_dir() else {
            return;
        };
        let mut session = load_real_session(profile_dir);

        let wild_animals_collision =
            session
                .ledger(OrderSource::Current)
                .entries
                .iter()
                .find(|entry| {
                    matches!(&entry.key,
                        FindingKey::PatchCollision { key, sub_path, .. }
                            if key.def_type == "BiomeDef"
                                && key.def_name == "AridShrubland"
                                && sub_path.as_deref() == Some("wildAnimals")
                    )
                });

        assert!(
            wild_animals_collision.is_none(),
            "BiomeDef/AridShrubland must carry no whole-field `wildAnimals` PatchCollision \
             any more — the keyed-map split keys it per disjoint animal key instead, got {:#?}",
            wild_animals_collision.map(|entry| &entry.key)
        );
    }

    /// `BiomeDef/AridShrubland`'s `wildPlants` — a *genuine* keyed-map
    /// collision on the same def as the collision-free `wildAnimals` one
    /// above: unlike `wildAnimals`, one of the real contributors
    /// contributes a whole-container
    /// `PatchOperationReplace`:
    ///
    /// ```xml
    /// <li Class="PatchOperationReplace">
    ///   <xpath>/Defs/BiomeDef[defName = "AridShrubland"]/wildPlants</xpath>
    ///   <value>
    ///     <wildPlants>
    ///       <Plant_Dandelion>0.8</Plant_Dandelion>
    ///       ...
    ///       <EX_Plant_AridGrass>12.0</EX_Plant_AridGrass>
    ///     </wildPlants>
    ///   </value>
    /// </li>
    /// ```
    ///
    /// rewriting the entire `wildPlants` dictionary wholesale, while six
    /// *other* mods each `PatchOperationAdd` their own, mutually disjoint
    /// new plant keys onto the same `wildPlants` path. The analyzer's
    /// blocking rule (`analysis::conflicts::acts_on_the_node_itself`/`exact_blocked`)
    /// correctly keeps every one of these seven contributors in **one**
    /// `wildPlants` bucket rather than mis-splitting the `Replace` away
    /// from the `Add`s it destructively interacts with (a whole-container
    /// rewrite can silently discard whatever an earlier-loaded `Add`
    /// contributed) — the "Replace at root vs an Add" shape, real and on
    /// this exact def.
    ///
    /// **A real finding, not an assumption**: despite the coarse
    /// `Replace`/`Add` mix, `collision_fields` still decomposes the whole
    /// container key by key across every contributor's own final
    /// candidate — every non-vanilla key comes back
    /// `OneSided` (contributed by exactly one mod, no genuine value
    /// disagreement), so the preview still reaches `Complete` — "unions
    /// cleanly" here means *zero field-level conflicts*, regardless of
    /// `op_count`.
    ///
    /// **`op_count` is order-contingent, not hand-derived, and is never
    /// named here by mod identity** (see this crate's own pins-file
    /// convention — a real mod id belongs in the gitignored pins file,
    /// never in a committed test): the `Replace` means the *selected*
    /// load order decides whether each `Add`'s own contribution actually
    /// survives live today (whichever mods load after the `Replace`'s
    /// own mod keep their plants; whichever load before lose them). On
    /// this install's real, current `ModsConfig.xml`, the `Replace`'s own
    /// mod loads before every one of its six `wildPlants` `Add`
    /// co-contributors, so nothing is silently dropped and the merge mod
    /// needs **no** restore ops — `op_count: 0`, structurally like
    /// `wildAnimals` even though (unlike `wildAnimals`) a genuine
    /// collision still exists here and still needs the container's own
    /// per-key expansion. Had any `Add` loaded ahead of the `Replace`, it
    /// would lose its own contribution live and need real restore ops
    /// (`op_count > 0`) — re-measure this pin the same way after any
    /// reorder that moves one of the seven contributors across the
    /// `Replace`.
    #[test]
    #[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
    fn arid_shrubland_wild_plants_collision_unions_cleanly_despite_a_replace() {
        let Some(profile_dir) = scratch_profile_dir() else {
            return;
        };
        let mut session = load_real_session(profile_dir);
        let key = patch_collision_key(&mut session, "BiomeDef", "AridShrubland", "wildPlants");
        let def_ref = key.def_ref().expect("PatchCollision always has a def_ref");

        InspectDef::new(rim_io::FileDefSourceReader::new())
            .execute(&mut session, &def_ref)
            .unwrap_or_else(|error| panic!("inspecting {def_ref}: {error}"));
        // `PlanMerge::execute` returns `&MergePreview` borrowed from
        // `session`'s own cache, so every value this test needs from it
        // is pulled into an owned local *before* `session.def_conflict_view`
        // borrows `session` again below — mirroring
        // `arid_shrubland_plant_density_collision_has_no_unsupported_op`'s
        // own "call it, don't hold the reference" pattern above.
        let (op_count, diff_field_count, conflicting_fields) = {
            let preview =
                rim_session::use_cases::PlanMerge::new(rim_io::FileDefSourceReader::new())
                    .execute(&mut session, &key)
                    .unwrap_or_else(|error| panic!("planning {key}: {error}"));
            let op_count = match &preview.state {
                rim_resolve::domain::MergeState::Complete { op_count } => *op_count,
                other => panic!(
                    "expected Complete (a Replace-vs-Adds keyed map still auto-resolves at the \
                     field level), got {other:?}"
                ),
            };
            let conflicting_fields: Vec<String> = preview
                .diff
                .fields
                .iter()
                .filter(|field| matches!(field.class, rim_merge::diff::DiffClass::Conflict { .. }))
                .map(|field| field.path.to_string())
                .collect();
            (op_count, preview.diff.fields.len(), conflicting_fields)
        };

        // The core, user-facing claim: the whole-container `Replace`
        // doesn't turn this into a genuine per-key conflict — every key
        // is one-sided, so nothing here needs a user decision. Whether it
        // needs restore *ops* is order-contingent (see this test's own
        // doc comment): on this install's current real order, every
        // `Add` co-contributor loads after the `Replace`'s own mod, so
        // nothing is silently dropped and `op_count` is genuinely `0` —
        // re-derive alongside the doc comment's own note if a future
        // reorder changes which side of the `Replace` any of the six
        // `Add`s load on.
        assert_eq!(
            op_count, 0,
            "expected no restore ops on this install's current real order (every wildPlants Add \
             co-contributor loads after the Replace's own mod) — got op_count={op_count}, meaning \
             the load order changed again; re-derive this pin per this test's own doc comment"
        );
        assert!(
            diff_field_count > 20,
            "expected the container's own per-key expansion (grammar pass 4's collision_fields), \
             not a single whole-subtree field: {diff_field_count} fields"
        );
        assert!(
            conflicting_fields.is_empty(),
            "no key genuinely conflicts on this real install — every contributor's own plant \
             names are disjoint: {conflicting_fields:?}"
        );

        let view = session
            .def_conflict_view(&key)
            .unwrap_or_else(|error| panic!("building the conflict view for {key}: {error}"));

        // Per-key rows, not a single-line blob: each contested key is its
        // own `FieldRowKind::MapEntry` row — never a lone `Conflict`/
        // `CleanMerge` row for the whole `wildPlants` container itself,
        // even though a `Replace` (not only `Add`s) contributes to it.
        let map_entry_rows: Vec<_> = view
            .fields
            .iter()
            .filter(|row| row.kind == FieldRowKind::MapEntry)
            .collect();
        assert!(
            map_entry_rows.len() > 20,
            "expected one MapEntry row per contested key, got {} among {} total rows: {:#?}",
            map_entry_rows.len(),
            view.fields.len(),
            view.fields
        );
        let wild_plants_container: FieldPath = "wildPlants".parse().expect("valid field path");
        assert!(
            view.fields
                .iter()
                .all(|row| row.path != wild_plants_container),
            "the container itself must never appear as its own row once collision_fields has \
             expanded it per key: {:#?}",
            view.fields
        );
        assert!(
            view.fields
                .iter()
                .all(|row| row.kind != FieldRowKind::Conflict),
            "matches the engine-level assertion above — no key conflicts on this install: {:#?}",
            view.fields
                .iter()
                .filter(|row| row.kind == FieldRowKind::Conflict)
                .collect::<Vec<_>>()
        );

        // Spot-check a handful of real keys from the seven contributors —
        // including the Replace's own new addition — naming the exact
        // contributor, proving per-key attribution survived the
        // container's own expansion regardless of which op class
        // produced it. Named only through
        // RIMMERGE_EXPECTED_WILD_PLANTS_SAMPLE; an empty sample (the var
        // unset) makes this loop a no-op.
        for (key_name, mod_id) in expected_wild_plants_sample() {
            let path: FieldPath = key_name.parse().expect("valid field path");
            let row = view
                .fields
                .iter()
                .find(|row| row.path == path)
                .unwrap_or_else(|| panic!("expected a row for {key_name}, got {:#?}", view.fields));
            assert_eq!(row.kind, FieldRowKind::MapEntry, "{row:?}");
            assert!(
                row.values
                    .iter()
                    .any(|(owner, _)| owner.as_str() == mod_id.as_str()),
                "{key_name} must credit {mod_id}, got {:?}",
                row.values
            );
        }

        eprintln!(
            "AridShrubland/wildPlants: op_count={op_count} diff_fields={diff_field_count} \
             map_entry_rows={} total field rows={} touchers={}",
            map_entry_rows.len(),
            view.fields.len(),
            view.touchers.len()
        );
    }

    /// `HediffDef/BionicHeart`'s `comps` list (a bionics-fork mod over
    /// core) — asserts the list-entry shape holds on the real
    /// install too: each `comps` list item is its own `ListEntry` row
    /// naming the mod that added it, and the container path itself never
    /// shows a spurious top-level `Conflict` once its own children
    /// explain the apparent disagreement (`is_container_conflict_explained_by_list_children`).
    #[test]
    #[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
    fn bionic_heart_comps_list_entries_have_per_entry_sources() {
        let Some(profile_dir) = scratch_profile_dir() else {
            return;
        };
        let mut session = load_real_session(profile_dir);
        let key = def_override_key(&mut session, "HediffDef", "BionicHeart");
        let def_ref = key.def_ref().expect("DefOverride always has a def_ref");

        InspectDef::new(rim_io::FileDefSourceReader::new())
            .execute(&mut session, &def_ref)
            .unwrap_or_else(|error| panic!("inspecting {def_ref}: {error}"));
        PlanMerge::new(rim_io::FileDefSourceReader::new())
            .execute(&mut session, &key)
            .unwrap_or_else(|error| panic!("planning {key}: {error}"));

        let view = session
            .def_conflict_view(&key)
            .unwrap_or_else(|error| panic!("building the conflict view for {key}: {error}"));

        assert!(
            !view.fields.is_empty(),
            "an empty panel is never acceptable — {key} produced no field rows"
        );
        assert!(
            view.problems
                .iter()
                .all(|problem| !matches!(problem, Problem::UnsupportedOp { .. })),
            "expected no UnsupportedOp problems for BionicHeart, got {:#?}",
            view.problems
        );

        let comps_entries: Vec<_> = view
            .fields
            .iter()
            .filter(|row| {
                row.kind == FieldRowKind::ListEntry && row.path.to_string().starts_with("comps")
            })
            .collect();
        assert!(
            !comps_entries.is_empty(),
            "expected at least one comps ListEntry row, got {:#?}",
            view.fields
        );
        for row in &comps_entries {
            assert!(
                !row.values.is_empty(),
                "every ListEntry row must name at least one source mod, got {row:?}"
            );
        }

        let comps_container: FieldPath = "comps".parse().expect("valid field path");
        if let Some(container_row) = view.fields.iter().find(|row| row.path == comps_container) {
            assert_ne!(
                container_row.kind,
                FieldRowKind::Conflict,
                "the comps container must not show a spurious top-level Conflict once its own \
                 list children explain it, got {container_row:?}"
            );
        }

        eprintln!(
            "BionicHeart: {} field rows ({} comps list entries), {} touchers, completeness={:?}",
            view.fields.len(),
            comps_entries.len(),
            view.touchers.len(),
            view.effective_completeness
        );
    }

    /// The pinned real DefOverride case (four owners on the real
    /// install) with a freshly stored `Merge` decision naming a
    /// non-winning owner — asserts `in_game` keeps naming the real,
    /// load-order winner while `after_merge` follows the stored choice
    /// instead, diverging from it (`after_merge` is what a complete merge
    /// would produce, distinct from what the game runs today).
    ///
    /// **A real finding, not an assumption**: none of HeadNormal's real
    /// fields on this install come back `FieldRowKind::Conflict` — the
    /// four mods' own values never disagree pairwise on any one field
    /// (`OneSided`/`Agreeing` only). This test therefore stores its choice
    /// on the first row carrying two or more distinct candidate mods,
    /// regardless of `kind` — `after_merge_for`'s own owner attribution
    /// honors a stored [`MergeChoice`] for *any* row kind (only
    /// `FieldRow::preference` is gated to `Conflict` rows by design, per
    /// its own doc comment — see the `preference` assertion below).
    #[test]
    #[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
    fn head_normal_stored_merge_choice_diverges_from_in_game() {
        // Named only through this env var: a real, active `DefOverride`
        // with 4+ owners and no field-level conflicts, format
        // `<def type>,<def name>`.
        let Some(raw) = super::common::require_pin_var(
            "RIMMERGE_EXPECTED_DEF_OVERRIDE_CASE",
            super::RERUN_COMMAND,
        ) else {
            return;
        };
        let (def_type, def_name) = raw.split_once(',').unwrap_or_else(|| {
            panic!("RIMMERGE_EXPECTED_DEF_OVERRIDE_CASE must be '<def type>,<def name>', got {raw}")
        });
        let Some(profile_dir) = scratch_profile_dir() else {
            return;
        };
        let mut session = load_real_session(profile_dir);
        let key = def_override_key(&mut session, def_type, def_name);
        let def_key = match &key {
            FindingKey::DefOverride { key, .. } => key.clone(),
            _ => unreachable!("def_override_key only ever returns a DefOverride"),
        };
        let def_ref = key.def_ref().expect("DefOverride always has a def_ref");

        InspectDef::new(rim_io::FileDefSourceReader::new())
            .execute(&mut session, &def_ref)
            .unwrap_or_else(|error| panic!("inspecting {def_ref}: {error}"));
        PlanMerge::new(rim_io::FileDefSourceReader::new())
            .execute(&mut session, &key)
            .unwrap_or_else(|error| panic!("planning {key}: {error}"));

        let before = session
            .def_conflict_view(&key)
            .unwrap_or_else(|error| panic!("building the conflict view for {key}: {error}"));
        let target_row = before
            .fields
            .iter()
            .find(|row| {
                let distinct_mods: std::collections::BTreeSet<&ModId> =
                    row.values.iter().map(|(mod_id, _)| mod_id).collect();
                distinct_mods.len() >= 2
            })
            .unwrap_or_else(|| {
                panic!(
                    "expected at least one field with two or more distinct candidate mods on \
                     HeadNormal, got {:#?}",
                    before.fields
                )
            });
        let target_kind = target_row.kind;
        let path = target_row.path.clone();
        let (in_game_mod, _) = target_row
            .in_game
            .clone()
            .unwrap_or_else(|| panic!("{path} must have an in_game value"));
        let chosen_mod = target_row
            .values
            .iter()
            .map(|(mod_id, _)| mod_id.clone())
            .find(|mod_id| mod_id != &in_game_mod)
            .unwrap_or_else(|| {
                panic!(
                    "expected at least two distinct mods among {path}'s own values, got {:#?}",
                    target_row.values
                )
            });

        session
            .decide(Decision {
                key: key.clone(),
                action: Action::Merge {
                    key: def_key,
                    choices: BTreeMap::from([(
                        path.clone(),
                        MergeChoice::From {
                            mod_id: chosen_mod.clone(),
                        },
                    )]),
                },
                note: None,
                decided_at: jiff::Timestamp::now(),
            })
            .unwrap_or_else(|error| panic!("recording the merge decision on {path}: {error}"));
        PlanMerge::new(rim_io::FileDefSourceReader::new())
            .execute(&mut session, &key)
            .unwrap_or_else(|error| panic!("re-planning {key} after the decision: {error}"));

        let after = session
            .def_conflict_view(&key)
            .unwrap_or_else(|error| panic!("re-building the conflict view for {key}: {error}"));
        let row = after
            .fields
            .iter()
            .find(|row| row.path == path)
            .unwrap_or_else(|| panic!("{path} must still have a row after the decision"));

        let (after_in_game_mod, _) = row
            .in_game
            .as_ref()
            .unwrap_or_else(|| panic!("{path} must still have an in_game value"));
        assert_eq!(
            after_in_game_mod, &in_game_mod,
            "a stored Merge decision must never change what's in-game today"
        );
        let (after_merge_mod, _) = row
            .after_merge
            .as_ref()
            .unwrap_or_else(|| panic!("{path} must have after_merge once the preview is Complete"));
        assert_eq!(after_merge_mod, &chosen_mod, "{row:?}");
        assert_ne!(
            after_in_game_mod, after_merge_mod,
            "the stored merge choice must diverge from what's in-game today for this to be a \
             meaningful test — chosen mod and real winner coincided by chance"
        );
        assert_eq!(
            row.kind, target_kind,
            "a stored Merge decision must never reclassify a row's own kind: {row:?}"
        );
        match target_kind {
            // Only a Conflict row's own `preference` reflects a stored
            // choice (`FieldRow::preference`'s own doc comment); this
            // install's HeadNormal has none (see this test's own doc
            // comment), so the branch below is exercised by whichever
            // future install/fixture drift finally produces one.
            FieldRowKind::Conflict => assert_eq!(
                row.preference,
                Preference::MergeChoice {
                    choice: MergeChoice::From {
                        mod_id: chosen_mod.clone()
                    }
                },
                "{row:?}"
            ),
            _ => assert_eq!(
                row.preference,
                Preference::None,
                "a non-Conflict row's preference stays None regardless of a stored choice: {row:?}"
            ),
        }

        eprintln!(
            "HeadNormal: path={path} kind={target_kind:?} in_game={in_game_mod} \
             after_merge={after_merge_mod} ({} total field rows, {} touchers)",
            after.fields.len(),
            after.touchers.len()
        );
    }
}
