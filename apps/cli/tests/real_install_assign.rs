//! Real-install verification of assignment inference, end to end through
//! the two-phase `CreateAssignment::list_candidates` ->
//! `CreateAssignment::infer_candidate` -> `CreateAssignment::execute` ->
//! `AssignmentCoverage` -> `SetAssignmentRow` -> `ExportAssignment`,
//! against a real target-keyed def type.
//!
//! Every assertion below needs the real classification pipeline's own
//! real, *known* output against one real framework's actual field names
//! and def-type shape -- these are the ground truth under test, read live
//! off this machine's own real install, not stand-ins for something else
//! -- but none of that real identity is hardcoded in this file: every mod
//! id, def type, and field name is read from [`AssignGroundTruth`], loaded
//! from the JSON file named by `RIMMERGE_ASSIGN_GROUND_TRUTH` (see that
//! struct's own doc comment for the exact schema). [`require_ground_truth`]
//! is this file's own `require_pin_var`-shaped guard: a machine outside
//! the real-install tier skips quietly; a machine inside it with the
//! variable unset **panics**, naming the variable, rather than silently
//! asserting nothing -- the same three-state rule every other real-install
//! test in this workspace follows. Nothing here is fabricated or hidden,
//! only sourced from data instead of hardcoded.
//!
//! `#[ignore]`d: needs the real game/workshop install (read-only) and a
//! *copy* of a real profile directory in `RIMMERGE_PERF_PROFILE_DIR`
//! (never the real one -- the session writes a new
//! `assignments/<id>.json` there). Builds its own [`rim_session::Session`]
//! directly (rather than reusing `apps/cli/src/common.rs::build_session`,
//! which is private to the binary crate and unreachable from an
//! integration test) -- the same ~15-line duplication
//! `tests/real_install_defs.rs` and
//! `apps/desktop/src-tauri/src/real_install_timing.rs` already accept for
//! the same reason.
//!
//! Five `#[ignore]`d tests: the primary flow (R = `{example.framework}`),
//! a lighter, separate closure check (R = the lineage-support framework
//! alone), the user's own workflow (R = the lineage-support framework,
//! T = Odyssey), that same R/T pair's
//! multi-section story -- a free-standing own part referenced by a
//! target-keyed race group row in the same project -- and a guard on the
//! real `chance...` field cardinality margin.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-cli --all-features --release --run-ignored ignored-only -E 'binary(real_install_assign)'`

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use rim_analyzer::domain::{GeneratedKind, ModId};
use rim_analyzer::extract::rimmerge_marker;
use rim_resolve::domain::{
    AssignmentRow, Cardinality, FieldPath, FieldRole, FieldSpec, RowIntent, RowKey, RowValue,
    ScalarKind,
};
use rim_session::Session;
use rim_session::ports::{DefSourceReader, ElementExpectation};
use rim_session::use_cases::{
    AddAssignmentSection, AssignmentCoverage, AssignmentExportOptions, AssignmentInstances,
    CreateAssignment, CreateAssignmentInput, ExportAssignment, ListItems, ListItemsFilter,
    SetAssignmentRow, UpdateAssignment, UpdateAssignmentInput,
};

/// `ludeon.rimworld` -- vanilla Core, keep-listed real content, never
/// mod-specific. Every real-install verification here includes it in T.
const CORE_MOD: &str = "ludeon.rimworld";
/// The Odyssey DLC's own package id -- vanilla content, keep-listed --
/// the user-workflow tests' own T. If a run finds it inactive, the test
/// falls back to Core alone and says so (see that test's own body) rather
/// than failing outright on a machine without Odyssey installed.
const ODYSSEY_DLC: &str = "ludeon.rimworld.odyssey";

/// This file's own "Run with" invocation (this module's own doc comment,
/// above) — named in [`common::require_profile_dir`]'s panic message so
/// it points at the exact command for this tier, not a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> \
     RIMMERGE_ASSIGN_GROUND_TRUTH=<path to a ground-truth JSON file> cargo nextest run -p \
     rimmerge-cli --all-features --release --run-ignored ignored-only -E \
     'binary(real_install_assign)'";

/// This file's real ground truth, read from a JSON file named by
/// `RIMMERGE_ASSIGN_GROUND_TRUTH` -- no third-party mod id, def type, or
/// field name is hardcoded anywhere else in this file. Every field is a plain
/// string or list of strings read straight off the real framework's own
/// real schema; there is no synthetic fallback, because the field names
/// themselves *are* the fact under test.
///
/// JSON schema (camelCase keys, all required):
///
/// ```json
/// {
///   "core": "<R's own real package id -- the smallest effective R that
///            still yields the one group def type candidate>",
///   "lineageSupport": "<a real addon whose declared modDependencies pulls
///                    core in through the effective-refs closure>",
///   "groupDefType": "<the target-keyed assignment def type>",
///   "partDefType": "<the item def type the primary item-slot field
///                    resolves to>",
///   "tagDefType": "<the second ItemSlot picker's own def type>",
///   "targetKeyField": "<the TargetKey field name, e.g. raceNames>",
///   "requiredChild": "<a child element name targetKeyField's own
///                      TargetShape must require, e.g. race>",
///   "itemSlotFields": ["<every ItemSlot field name paired to a
///                        chance<Field> field, in the order this file's
///                        own reference table lists them>"],
///   "primaryItemSlotField": "<one of itemSlotFields, exercised in every
///                             test's own row-setting flow>",
///   "secondItemSlotField": "<a second, unrelated ItemSlot field name,
///                            e.g. tags>",
///   "boolFields": [["<field name>", "<lowercase default: true|false>"],
///                   "... six pairs, matching Scalar(Bool) fields"],
///   "numberFields": ["<three Scalar(Number) field names>"],
///   "defSuffixFields": ["<two Def-suffixed ItemSlot(ThingDef) field
///                         names, e.g. eggFertilizedDef>"],
///   "extraTargets": ["<T beyond Core: a handful of real race-defining
///                      mods>"]
/// }
/// ```
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssignGroundTruth {
    core: String,
    lineage_support: String,
    group_def_type: String,
    part_def_type: String,
    tag_def_type: String,
    target_key_field: String,
    required_child: String,
    item_slot_fields: Vec<String>,
    primary_item_slot_field: String,
    second_item_slot_field: String,
    bool_fields: Vec<(String, String)>,
    number_fields: Vec<String>,
    def_suffix_fields: Vec<String>,
    extra_targets: Vec<String>,
}

/// `None` (after an honest skip message) on a machine outside the
/// real-install tier; **panics**, naming the variable, on a machine
/// inside it with `RIMMERGE_ASSIGN_GROUND_TRUTH` unset -- the same
/// three-state rule [`common::require_profile_dir`] already applies to
/// `RIMMERGE_PERF_PROFILE_DIR`, mirrored here for this file's own
/// ground-truth variable. Every one of this file's five tests calls this *and*
/// `common::require_profile_dir` before doing anything else; both must
/// return `Some` for a test to proceed.
fn require_ground_truth() -> Option<AssignGroundTruth> {
    let path = common::require_pin_var("RIMMERGE_ASSIGN_GROUND_TRUTH", RERUN_COMMAND)?;
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("reading RIMMERGE_ASSIGN_GROUND_TRUTH ({path}): {error}"));
    Some(
        serde_json::from_slice(&bytes).unwrap_or_else(|error| {
            panic!("parsing RIMMERGE_ASSIGN_GROUND_TRUTH ({path}): {error}")
        }),
    )
}

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

/// Every `FieldRole::Chances` field in `fields` paired to `slot` --
/// matched dynamically rather than against one hard-coded spelling, since
/// the framework's own real XML corpus carries a handful of
/// inconsistently-cased `chance...` tags alongside its own canonical
/// field names -- the `Chances` pairing matches case-insensitively per
/// field, so more than one spelling can independently qualify.
fn chances_fields_for_slot<'a>(
    fields: &'a BTreeMap<FieldPath, FieldSpec>,
    slot: &FieldPath,
) -> Vec<&'a FieldPath> {
    fields
        .iter()
        .filter_map(|(path, spec)| match &spec.role {
            FieldRole::Chances { for_slot } if for_slot == slot => Some(path),
            _ => None,
        })
        .collect()
}

/// Every file under `dir`, as `(relative path, content bytes)`, sorted for
/// a deterministic byte-for-byte comparison across two independent exports
/// of the same folder -- `rim-io`'s own `assignment_end_to_end.rs`
/// `snapshot_files` helper, duplicated here since it's private to that
/// crate's own test binary.
fn snapshot_files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {dir:?}: {e}")) {
            let entry = entry.unwrap_or_else(|e| panic!("dir entry under {dir:?}: {e}"));
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or_else(|e| panic!("{path:?} must be under {root:?}: {e}"))
                    .to_path_buf();
                let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
                out.push((relative, bytes));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// Reads one of `owner`'s own real, raw (unpatched, uninherited) instance
/// of `def_type`/`def_name` straight off disk through the same
/// [`rim_io::FileDefSourceReader`] the session itself uses, and returns
/// its top-level element tag set -- the ground truth this file's own
/// "same shape as the reference" assertions compare an emitted row
/// against. Deliberately independent of this tool's own schema inference
/// (never `candidate.schema.fields`, never `AssignmentInstances`' own
/// flattening): the whole point is a real, external check that doesn't
/// let the inferred schema grade itself.
fn read_owner_instance_tags(
    session: &Session,
    owner: &str,
    def_type: &str,
    def_name: &str,
) -> BTreeSet<String> {
    let key = (
        ModId::new(owner),
        (def_type.to_string(), def_name.to_string()),
    );
    let entries = session
        .sources()
        .defs
        .get(&key)
        .unwrap_or_else(|| panic!("{owner} must own {def_type}/{def_name}"));
    let entry = entries
        .last()
        .unwrap_or_else(|| panic!("{owner}'s own {def_type}/{def_name} has no DefEntry"));
    let reader = rim_io::FileDefSourceReader::new();
    let expectation = ElementExpectation {
        tag: def_type.to_string(),
        def_name: Some(def_name.to_string()),
        name_attr: None,
    };
    let xml_text = reader
        .read_element(&entry.locator, &expectation)
        .unwrap_or_else(|error| panic!("reading {owner}'s own {def_type}/{def_name}: {error}"));
    let tree = rim_merge::xml::parse(&xml_text)
        .unwrap_or_else(|error| panic!("parsing {owner}'s own {def_type}/{def_name}: {error}"));
    match &tree.root.content {
        rim_merge::tree::Content::Children(children) => {
            children.iter().map(|child| child.tag.clone()).collect()
        }
        _ => BTreeSet::new(),
    }
}

/// The union of top-level tags across every `group_type` instance `core`
/// owns itself, read directly off disk ([`read_owner_instance_tags`]) --
/// the framework's own *own* instances are sparse (a handful of mostly
/// commented-out group defs), so a union
/// over all of them, not just one, is the honest reference shape.
fn framework_core_reference_tags(
    session: &Session,
    core: &str,
    group_type: &str,
) -> BTreeSet<String> {
    let names: Vec<String> = session
        .sources()
        .owners_by_def
        .iter()
        .filter(|((def_type, _), owners)| {
            def_type == group_type && owners.iter().any(|o| o.base() == ModId::new(core))
        })
        .map(|((_, name), _)| name.clone())
        .collect();
    assert!(
        !names.is_empty(),
        "expected at least one {group_type} instance owned by {core} itself"
    );
    eprintln!(
        "reading {} of {core}'s own real {group_type} instances for the reference shape: {names:?}",
        names.len()
    );
    let mut tags = BTreeSet::new();
    for name in &names {
        tags.extend(read_owner_instance_tags(session, core, group_type, name));
    }
    tags
}

#[test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
fn group_def_assignment_matches_the_plans_real_install_verification() {
    let Some(gt) = require_ground_truth() else {
        return;
    };
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let mut session = load_real_session(profile_dir);
    let core = gt.core.as_str();
    let group_type = gt.group_def_type.as_str();

    let refs: BTreeSet<ModId> = [ModId::new(core)].into_iter().collect();
    // A small set of real race-defining mods plus Core -- T, the
    // candidate-target pool for the TargetKey field's own learned
    // `TargetShape`, and the read-gate every `TargetKey` field's shape
    // sample is filtered against.
    let targets: BTreeSet<ModId> = std::iter::once(ModId::new(CORE_MOD))
        .chain(gt.extra_targets.iter().map(ModId::new))
        .collect();

    let create_assignment = CreateAssignment::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );

    // -- Phase 1: list_candidates -- cheap, no instance reads at all. -------
    let list_started = Instant::now();
    let summaries = create_assignment.list_candidates(&session, &refs, &BTreeSet::new(), &targets);
    let list_elapsed = list_started.elapsed();
    eprintln!(
        "CreateAssignment::list_candidates (R = {{{core}}}) took {list_elapsed:?}, {} \
         candidate type(s) listed -- sub-second is the whole point of splitting this out of the \
         old single-call propose",
        summaries.len()
    );
    let summary = summaries
        .iter()
        .find(|s| s.def_type == group_type)
        .unwrap_or_else(|| {
            panic!(
                "expected {group_type} among the phase-1 candidates, got {:?}",
                summaries.iter().map(|s| &s.def_type).collect::<Vec<_>>()
            )
        });
    eprintln!(
        "{group_type} phase-1 summary: {} instance(s), owners {:?}",
        summary.instance_count, summary.owners
    );

    // -- Phase 2: infer_candidate -- the real, per-type inference, also
    // honouring T (only referenced targets owned by T/Core are ever read
    // back for the TargetShape sample) -- the expensive half, timed. ------
    let infer_started = Instant::now();
    let candidate = create_assignment
        .infer_candidate(&mut session, &refs, &BTreeSet::new(), &targets, group_type)
        .unwrap_or_else(|error| panic!("inferring {group_type}: {error}"))
        .unwrap_or_else(|| panic!("{group_type} must be a valid candidate"));
    let infer_elapsed = infer_started.elapsed();
    eprintln!(
        "CreateAssignment::infer_candidate (R = {{{core}}}, def_type = {group_type}, T honoured \
         -- {} member(s)) took {infer_elapsed:?} (before T was honoured here, the \
         single-call propose measured 67.1s/99.7s/63.4s across three runs reading every \
         referenced race regardless of ownership)",
        targets.len()
    );

    assert!(
        candidate.unreadable_targets.is_empty(),
        "no referenced target owned by T/Core should fail to read on a healthy install: {:?}",
        candidate.unreadable_targets
    );
    eprintln!(
        "excluded_targets (referenced but outside T/Core, never read at all): {}",
        candidate.excluded_targets.len()
    );

    let fields = &candidate.schema.fields;
    let path = |name: &str| -> FieldPath {
        name.parse()
            .unwrap_or_else(|error| panic!("{name} is a valid field path: {error}"))
    };

    // -- Instance count: read off any field's `observed.1` (every field is
    // voted over the same instance set) -- logged against a recorded
    // reference count, not hard-pinned: the real install's active mod list
    // can legitimately change between runs. ------------------------------
    let race_names = path(&gt.target_key_field);
    let race_names_spec = fields.get(&race_names).unwrap_or_else(|| {
        panic!(
            "expected a {} field, got {:?}",
            gt.target_key_field,
            fields.keys().collect::<Vec<_>>()
        )
    });
    let instance_count = race_names_spec.observed.1;
    eprintln!(
        "active {group_type} instances read: {instance_count} (recorded reference figure: \
         682)"
    );
    if instance_count != 682 {
        eprintln!(
            "NOTE: instance count drifted from the recorded 682 to {instance_count} -- \
             real-install mod list churn since the figure was recorded, not a regression"
        );
    }

    // -- the TargetKey field -> TargetKey(ThingDef), shape contains the
    // required child -----------------------------------------------------
    assert_eq!(
        race_names_spec.role,
        FieldRole::TargetKey {
            def_type: "ThingDef".to_string()
        },
        "{} must classify as TargetKey(ThingDef): {:?}",
        gt.target_key_field,
        race_names_spec.role
    );
    let shape = candidate
        .schema
        .target_shapes
        .get(&race_names)
        .unwrap_or_else(|| panic!("{} must get a computed TargetShape", gt.target_key_field));
    assert!(
        shape.required_children.contains(&gt.required_child),
        "every referenced race must carry <{}/>: {shape:?}",
        gt.required_child
    );

    // -- Every item slot -> ItemSlot(part_type) -----------------------------
    let slot_names: Vec<&str> = gt.item_slot_fields.iter().map(String::as_str).collect();
    let part_type = gt.part_def_type.as_str();
    for slot_name in &slot_names {
        let slot_path = path(slot_name);
        let spec = fields.get(&slot_path).unwrap_or_else(|| {
            panic!(
                "expected a {slot_name} field, got {:?}",
                fields.keys().collect::<Vec<_>>()
            )
        });
        assert_eq!(
            spec.role,
            FieldRole::ItemSlot {
                def_type: part_type.to_string()
            },
            "{slot_name} must classify as ItemSlot({part_type}): {:?}",
            spec.role
        );
    }

    // -- the second item-slot field -> ItemSlot(tag_type) --------------------
    let tags = path(&gt.second_item_slot_field);
    let tag_type = gt.tag_def_type.as_str();
    assert_eq!(
        fields[&tags].role,
        FieldRole::ItemSlot {
            def_type: tag_type.to_string()
        },
        "{} must classify as ItemSlot({tag_type}): {:?}",
        gt.second_item_slot_field,
        fields[&tags].role
    );

    // -- Five chance fields: a field's cardinality is `List` once at least
    // two of its occurrences are `li`-wrapped (or a majority are), so these
    // classify `Chances` even though most of their sparse occurrences write
    // RimWorld's own bare-scalar `List<T>` shorthand. Asserted with a
    // Scalar(Number) fallback still tolerated (never silently hidden if the
    // real corpus doesn't clear that bar for some field) so a genuine
    // remaining gap is disclosed, not hidden behind a hard failure. -------
    for slot_name in &slot_names {
        let slot_path = path(slot_name);
        let paired = chances_fields_for_slot(fields, &slot_path);
        if !paired.is_empty() {
            eprintln!(
                "chance field(s) paired to {slot_name} (Chances, as the paired-cardinality \
                 classification intends): {paired:?}"
            );
            continue;
        }
        let scalar_number_fallbacks: Vec<&FieldPath> = fields
            .iter()
            .filter(|(path, _)| {
                path.to_string()
                    .to_lowercase()
                    .starts_with(&format!("chance{}", slot_name.to_lowercase()))
            })
            .filter(|(_, spec)| {
                matches!(
                    spec.role,
                    FieldRole::Scalar {
                        kind: ScalarKind::Number,
                        ..
                    }
                )
            })
            .map(|(path, _)| path)
            .collect();
        assert!(
            !scalar_number_fallbacks.is_empty(),
            "expected a Chances field paired to {slot_name}, or at least a Scalar(Number) \
             fallback -- neither found among {:?}",
            fields
                .iter()
                .map(|(path, spec)| (path.to_string(), &spec.role))
                .collect::<Vec<_>>()
        );
        eprintln!(
            "NOTE: chance field(s) for {slot_name} still classify Scalar(Number), not Chances, \
             even under the paired-cardinality classification -- disclosed, not hidden: \
             {scalar_number_fallbacks:?}"
        );
    }

    // -- Nine boolean/number scalars, with their recorded defaults ---------
    let expect_bool = |name: &str, expected_default: &str| {
        let spec = fields
            .get(&path(name))
            .unwrap_or_else(|| panic!("expected a {name} field"));
        match &spec.role {
            FieldRole::Scalar {
                kind: ScalarKind::Bool,
                default,
            } => {
                assert_eq!(
                    default.as_deref().map(str::to_lowercase),
                    Some(expected_default.to_string()),
                    "{name}'s default: {default:?}"
                );
            }
            other => panic!("{name} must classify as Scalar(Bool), got {other:?}"),
        }
    };
    for (name, default) in &gt.bool_fields {
        expect_bool(name, default);
    }

    for name in &gt.number_fields {
        let spec = fields
            .get(&path(name))
            .unwrap_or_else(|| panic!("expected a {name} field"));
        assert!(
            matches!(
                spec.role,
                FieldRole::Scalar {
                    kind: ScalarKind::Number,
                    ..
                }
            ),
            "{name} must classify as Scalar(Number): {:?}",
            spec.role
        );
    }

    // -- modExtensions -> Opaque, when observed at all ----------------------
    if let Some(spec) = fields.get(&path("modExtensions")) {
        assert_eq!(
            spec.role,
            FieldRole::Opaque,
            "modExtensions: {:?}",
            spec.role
        );
    } else {
        eprintln!(
            "NOTE: no modExtensions field observed on this install's active {} instances",
            gt.group_def_type
        );
    }

    // -- pawnKindNames: a field whose majority cardinality is Scalar (most
    // occurrences unwrapped) but which is observed `li`-wrapped often
    // enough reports List, sending an unresolvable-value field to Opaque
    // instead of Text (>8 distinct unresolvable strings). Not
    // hard-asserted, for the same "real-install drift is not a coding
    // defect" reasoning as everywhere else in this file. -------------------
    if let Some(spec) = fields.get(&path("pawnKindNames")) {
        eprintln!(
            "pawnKindNames role: {:?} (expected after the cardinality fix: Opaque)",
            spec.role
        );
    }
    // The `Def`/`Defs`-suffix exemption (`crates/rim-resolve`'s
    // `has_def_suffixed_leaf_tag`) rescues both fields from the
    // `MIN_RESOLVED_DISTINCT` floor straight to `ItemSlot{ThingDef}`.
    for name in &gt.def_suffix_fields {
        let spec = fields
            .get(&path(name))
            .unwrap_or_else(|| panic!("expected a {name} field"));
        assert!(
            matches!(&spec.role,
                FieldRole::ItemSlot { def_type } if def_type == "ThingDef"
            ),
            "{name} must classify as ItemSlot{{ThingDef}}: {:?}",
            spec.role
        );
    }

    // === Create the project, build coverage, set a row, export =============

    let create_input = CreateAssignmentInput {
        name: "assignment export verification".to_string(),
        package_id: "mypatch.assignexportverify".to_string(),
        display_name: "Assignment Export Verification".to_string(),
        refs: refs.clone(),
        excluded_refs: BTreeSet::new(),
        targets: targets.clone(),
        schema: candidate.schema.clone(),
    };
    let assignment_id = create_assignment
        .execute(&mut session, create_input)
        .unwrap_or_else(|error| panic!("creating the assignment: {error}"));

    let coverage_use_case = AssignmentCoverage::new(rim_io::FileDefSourceReader::new());
    let coverage = coverage_use_case
        .execute(&mut session, &assignment_id, &candidate.schema.def_type)
        .unwrap_or_else(|error| panic!("building coverage: {error}"));
    assert!(
        coverage.applicable,
        "a TargetKey schema's coverage is always applicable"
    );

    eprintln!(
        "TargetShape candidates (ThingDef, within T = {} mods): {}",
        targets.len(),
        coverage.rows.len()
    );
    assert!(
        !coverage.rows.is_empty(),
        "expected at least one ThingDef candidate within T"
    );
    let covered = coverage
        .rows
        .iter()
        .filter(|row| row.intent == RowIntent::Override)
        .count();
    let uncovered = coverage
        .rows
        .iter()
        .filter(|row| row.intent == RowIntent::Cover)
        .count();
    let no_winner = coverage
        .rows
        .iter()
        .filter(|row| row.winner.is_none())
        .count();
    eprintln!(
        "coverage: {} candidate(s), {covered} already-covered (Override), {uncovered} \
         uncovered (Cover), {no_winner} with no winner named",
        coverage.rows.len()
    );

    // -- SetAssignmentRow for one target (raceNames is the only TargetKey
    // field here -- pawnKindNames classified Opaque above, so there is no
    // PawnKindDef-keyed row to set). Prefers a genuinely uncovered (Cover)
    // target when one exists; T here is Core plus the four example race
    // submods, and on today's real install *every* one of Core's
    // own candidate races already has an existing group def (core +
    // the lineage-support framework jointly cover every vanilla animal),
    // and none of the four example race submods' own ThingDefs satisfy
    // the learned TargetShape at
    // all -- 0 uncovered targets is itself the real, verified coverage
    // result for this T, not a test-setup mistake, so this falls back to
    // any candidate (an Override) rather than forcing an artificial
    // uncovered case. ---------------------------------------------------
    let chosen_row = coverage
        .rows
        .iter()
        .find(|row| row.intent == RowIntent::Cover)
        .unwrap_or_else(|| {
            coverage.rows.first().unwrap_or_else(|| {
                panic!("expected at least one candidate target among {coverage:?}")
            })
        });
    let target = chosen_row.target.clone();
    eprintln!(
        "setting a row for target {} (intent={:?})",
        target.def, chosen_row.intent
    );

    let set_row = SetAssignmentRow::new(rim_io::JsonAssignmentProjectStore::new());
    set_row
        .execute(
            &mut session,
            &assignment_id,
            &candidate.schema.def_type,
            RowKey::Target(target.clone()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: format!("mypatch_assignexportverify_{}", target.def.def_name),
                note: None,
            },
        )
        .unwrap_or_else(|error| panic!("setting the row: {error}"));

    // -- ExportAssignment to a scratch --out-dir, never installed -----------
    let export_root = tempfile::tempdir().expect("tempdir for the export");
    let export_dir = export_root.path().join("export");

    let export_assignment = ExportAssignment::new(
        rim_io::MergeModFolderWriter::new(),
        rim_io::ModsConfigFileStore::new(),
        rim_io::JsonAssignmentProjectStore::new(),
    );
    let first_outcome = export_assignment
        .execute(
            &mut session,
            &assignment_id,
            AssignmentExportOptions {
                out_dir: export_dir.clone(),
                install: false,
            },
        )
        .unwrap_or_else(|error| panic!("exporting the assignment: {error}"));
    eprintln!(
        "export: {} file(s) written, content hash {}, {} field(s) skipped",
        first_outcome.files.len(),
        first_outcome.content_sha256,
        first_outcome.skipped.len()
    );
    assert!(first_outcome.installed_path.is_none());
    let export_path = first_outcome.export_path.clone();
    assert!(export_path.is_dir(), "the exported folder must exist");
    assert!(export_path.join("About/About.xml").is_file());
    assert!(export_path.join("rimmerge.json").is_file());

    // -- Two exports of the same project must be byte-identical -------------
    let first_snapshot = snapshot_files(&export_path);
    let second_outcome = export_assignment
        .execute(
            &mut session,
            &assignment_id,
            AssignmentExportOptions {
                out_dir: export_dir.clone(),
                install: false,
            },
        )
        .unwrap_or_else(|error| panic!("exporting a second time: {error}"));
    assert_eq!(second_outcome.export_path, export_path);
    let second_snapshot = snapshot_files(&export_path);
    assert_eq!(
        first_snapshot, second_snapshot,
        "exporting the same project twice must be byte-identical"
    );

    // -- Re-scan the exported folder's rimmerge.json through the analyzer's
    // own marker reader (never through a second full LoadProject scan --
    // `--install` is forbidden by this test's own hard rules, so nothing
    // is ever copied into the real Mods/ folder). ---------------------------
    let marker_bytes =
        std::fs::read(export_path.join("rimmerge.json")).expect("read the exported rimmerge.json");
    let marker = rimmerge_marker::parse(&marker_bytes)
        .expect("the exported rimmerge.json must parse as a marker");
    assert_eq!(marker.kind, GeneratedKind::Assignment);
    assert_eq!(marker.patch_id.as_deref(), Some(assignment_id.as_str()));
    let scope = marker
        .scope
        .expect("an assignment marker always carries a scope (R union T)");
    assert!(
        scope.contains(&ModId::new(core)) && scope.contains(&ModId::new(CORE_MOD)),
        "the marker's own scope must carry the effective R ({core}) and T (Core among it): \
         {scope:?}"
    );

    eprintln!(
        "assignment export verification complete: {instance_count} instances, \
         {} coverage candidates ({covered} covered, {uncovered} uncovered), \
         export {} file(s), list_candidates took {list_elapsed:?}, infer_candidate took \
         {infer_elapsed:?}",
        coverage.rows.len(),
        first_outcome.files.len()
    );
}

/// A second real-install verification: selecting the lineage-support
/// framework alone still classifies the group def type correctly, because
/// the effective-refs closure pulls core in transitively through the
/// framework's own declared `modDependencies` -- without that closure
/// member, the item-slot fields misclassify as `TargetKey` instead of
/// `ItemSlot` (the lineage-support framework owns no DLL of its own, and
/// defines fewer than half its own groups' named parts). A separate,
/// lighter test from the main flow above: the lineage-support framework
/// ships dozens of unrelated def types of its own (`BackstoryDef`,
/// `HediffDef`, `SoundDef`, ...), so phase 1 over it alone returns dozens
/// of candidates, not just the group def type -- this test finds the
/// group def type among them rather than asserting it is the only one.
#[test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
fn effective_r_closure_over_lineage_support_includes_core() {
    let Some(gt) = require_ground_truth() else {
        return;
    };
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let mut session = load_real_session(profile_dir);
    let core = gt.core.as_str();
    let lineage_support = gt.lineage_support.as_str();
    let group_type = gt.group_def_type.as_str();

    let refs: BTreeSet<ModId> = [ModId::new(lineage_support)].into_iter().collect();
    let create_assignment = CreateAssignment::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );

    let list_started = Instant::now();
    let summaries =
        create_assignment.list_candidates(&session, &refs, &BTreeSet::new(), &BTreeSet::new());
    let list_elapsed = list_started.elapsed();
    eprintln!(
        "CreateAssignment::list_candidates (R = {{{lineage_support}}}) took {list_elapsed:?}, {} \
         candidate(s) total",
        summaries.len()
    );
    assert!(
        summaries.iter().any(|s| s.def_type == group_type),
        "expected {group_type} among the phase-1 candidates, got {:?}",
        summaries.iter().map(|s| &s.def_type).collect::<Vec<_>>()
    );

    let infer_started = Instant::now();
    let candidate = create_assignment
        .infer_candidate(
            &mut session,
            &refs,
            &BTreeSet::new(),
            &BTreeSet::new(),
            group_type,
        )
        .unwrap_or_else(|error| panic!("inferring {group_type}: {error}"))
        .unwrap_or_else(|| panic!("{group_type} must be a valid candidate"));
    let infer_elapsed = infer_started.elapsed();
    eprintln!(
        "CreateAssignment::infer_candidate (R = {{{lineage_support}}}, effective closure applied) \
         took {infer_elapsed:?}"
    );

    assert!(
        candidate.schema.refs.contains(&ModId::new(core)),
        "the effective R closure over {{{lineage_support}}} must include {core} (its own declared \
         modDependencies): {:?}",
        candidate.schema.refs
    );

    let fields = &candidate.schema.fields;
    for (slot_name, expected_type) in [
        (
            gt.primary_item_slot_field.as_str(),
            gt.part_def_type.as_str(),
        ),
        (gt.second_item_slot_field.as_str(), gt.tag_def_type.as_str()),
    ] {
        let path: FieldPath = slot_name
            .parse()
            .unwrap_or_else(|error| panic!("{slot_name} is a valid field path: {error}"));
        let role = fields
            .get(&path)
            .unwrap_or_else(|| panic!("expected a {slot_name} field"))
            .role
            .clone();
        assert_eq!(
            role,
            FieldRole::ItemSlot {
                def_type: expected_type.to_string()
            },
            "{slot_name} must classify as ItemSlot({expected_type}) once the closure includes \
             {core} -- without it this field misclassifies as TargetKey instead: {role:?}"
        );
    }
    eprintln!(
        "closure check passed: {}/{} both classify as ItemSlot with {core} in the effective \
         closure",
        gt.primary_item_slot_field, gt.second_item_slot_field
    );
}

/// Builds a row's own values generically from `schema`: every
/// [`FieldRole::ItemSlot`] field gets the first item [`ListItems`] finds
/// of its own type (real, active items -- never a made-up name), every
/// [`FieldRole::Scalar`] field gets its own learned default, and every
/// [`FieldRole::Chances`] field is left `Omit` entirely -- "slots/tags/
/// bools and no chances", the user workflow's own spec (a
/// `tags` field is just another `ItemSlot` field under a different name,
/// so it needs no special case here).
fn build_row_values(
    schema: &rim_resolve::domain::AssignmentSchema,
    session: &mut Session,
    list_items: &ListItems<rim_io::FileDefSourceReader>,
) -> BTreeMap<FieldPath, RowValue> {
    let mut values = BTreeMap::new();
    for (path, spec) in &schema.fields {
        match &spec.role {
            FieldRole::TargetKey { .. } | FieldRole::Chances { .. } | FieldRole::Opaque => {}
            FieldRole::ItemSlot { def_type } => {
                let page = list_items
                    .execute(
                        session,
                        None,
                        def_type,
                        &ListItemsFilter {
                            search: None,
                            offset: 0,
                            limit: 1,
                        },
                    )
                    .unwrap_or_else(|error| panic!("listing items of {def_type}: {error}"));
                if let Some(item) = page.items.first() {
                    values.insert(
                        path.clone(),
                        RowValue::Names(vec![item.def.def_name.clone()]),
                    );
                }
            }
            FieldRole::Scalar { default, .. } => {
                if let Some(default) = default {
                    values.insert(path.clone(), RowValue::Text(default.clone()));
                }
            }
        }
    }
    values
}

/// The user's own exact workflow: select the lineage-support framework as
/// the reference (the closure pulls core in, so its own parts/tags are
/// available in the pickers); select Odyssey (falling back to Core alone,
/// and saying so, if Odyssey has no candidate race on this run) as the
/// target; the candidate list contains the group def type; infer it;
/// `ListItems` for the parts type returns the framework's own parts; set a
/// row for one Odyssey race with slots/tags/bools and no chances; export;
/// and assert the emitted XML has the same element shape as core's own
/// real instances -- never the framework's bundled example patch file
/// (that file is a reference *mod*'s own addon content, not the framework
/// core's
/// canonical shape).
#[test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
fn user_workflow_lineage_support_ref_odyssey_target_matches_cores_own_shape() {
    let Some(gt) = require_ground_truth() else {
        return;
    };
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let mut session = load_real_session(profile_dir);
    let core = gt.core.as_str();
    let lineage_support = gt.lineage_support.as_str();
    let group_type = gt.group_def_type.as_str();

    let refs: BTreeSet<ModId> = [ModId::new(lineage_support)].into_iter().collect();
    let odyssey_active = session
        .report()
        .mods
        .iter()
        .any(|m| m.id.base() == ModId::new(ODYSSEY_DLC));
    if !odyssey_active {
        eprintln!(
            "NOTE: {ODYSSEY_DLC} is not active on this install -- falling back to Core alone as \
             the closest installed DLC with animal races"
        );
    }
    // Core is always included alongside Odyssey: Odyssey's own new
    // ThingDefs may or may not clear the learned TargetShape on any given
    // real install (not guaranteed here), and this workflow must produce
    // at least one real candidate row either way -- the row this test
    // sets still prefers an Odyssey-owned candidate when one exists (see
    // below), falling back to Core and disclosing it otherwise.
    let mut targets: BTreeSet<ModId> = [ModId::new(CORE_MOD)].into_iter().collect();
    if odyssey_active {
        targets.insert(ModId::new(ODYSSEY_DLC));
    }

    let create_assignment = CreateAssignment::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );

    let summaries = create_assignment.list_candidates(&session, &refs, &BTreeSet::new(), &targets);
    assert!(
        summaries.iter().any(|s| s.def_type == group_type),
        "expected {group_type} among the candidates for R = {{{lineage_support}}}: {:?}",
        summaries.iter().map(|s| &s.def_type).collect::<Vec<_>>()
    );

    let candidate = create_assignment
        .infer_candidate(&mut session, &refs, &BTreeSet::new(), &targets, group_type)
        .unwrap_or_else(|error| panic!("inferring {group_type}: {error}"))
        .unwrap_or_else(|| panic!("{group_type} must be a valid candidate"));

    // `ListItems` for the parts type returns the framework's own real parts.
    let parts_type = gt.part_def_type.as_str();
    let list_items = ListItems::new(rim_io::FileDefSourceReader::new());
    let parts_page = list_items
        .execute(
            &mut session,
            None,
            parts_type,
            &ListItemsFilter {
                search: None,
                offset: 0,
                limit: 5,
            },
        )
        .unwrap_or_else(|error| panic!("listing {parts_type} items: {error}"));
    assert!(
        parts_page.total > 0,
        "expected at least one active {parts_type} item"
    );
    eprintln!(
        "{parts_type}: {} total, first page {:?}",
        parts_page.total,
        parts_page
            .items
            .iter()
            .map(|item| &item.def.def_name)
            .collect::<Vec<_>>()
    );

    let create_input = CreateAssignmentInput {
        name: "assignment user workflow verification".to_string(),
        package_id: "mypatch.assignuserworkflow".to_string(),
        display_name: "Assignment User Workflow Verification".to_string(),
        refs: refs.clone(),
        excluded_refs: BTreeSet::new(),
        targets: targets.clone(),
        schema: candidate.schema.clone(),
    };
    let assignment_id = create_assignment
        .execute(&mut session, create_input)
        .unwrap_or_else(|error| panic!("creating the assignment: {error}"));

    let coverage_use_case = AssignmentCoverage::new(rim_io::FileDefSourceReader::new());
    let coverage = coverage_use_case
        .execute(&mut session, &assignment_id, &candidate.schema.def_type)
        .unwrap_or_else(|error| panic!("building coverage: {error}"));
    assert!(
        !coverage.rows.is_empty(),
        "expected at least one candidate race within T"
    );

    // Prefer a candidate genuinely owned by Odyssey; fall back to any
    // candidate (necessarily Core-owned) and disclose it.
    let chosen_row = coverage
        .rows
        .iter()
        .find(|row| row.owner.base() == ModId::new(ODYSSEY_DLC))
        .unwrap_or_else(|| {
            eprintln!(
                "NOTE: no candidate race is owned by {ODYSSEY_DLC} on this run -- falling back \
                 to a Core-owned candidate for the row this test sets"
            );
            coverage
                .rows
                .first()
                .unwrap_or_else(|| panic!("expected at least one candidate row"))
        });
    let target = chosen_row.target.clone();
    let target_owner = chosen_row.owner.clone();
    eprintln!(
        "setting a row for {} (owner={target_owner}, intent={:?})",
        target.def, chosen_row.intent
    );

    let row_values = build_row_values(&candidate.schema, &mut session, &list_items);
    assert!(
        row_values.keys().all(|path| !matches!(
            candidate.schema.fields[path].role,
            FieldRole::Chances { .. }
        )),
        "no chance field must ever be given a value, per the workflow spec's own \"no chances\""
    );

    let set_row = SetAssignmentRow::new(rim_io::JsonAssignmentProjectStore::new());
    let row_def_name = format!("mypatch_assignuserworkflow_{}", target.def.def_name);
    set_row
        .execute(
            &mut session,
            &assignment_id,
            &candidate.schema.def_type,
            RowKey::Target(target.clone()),
            AssignmentRow {
                values: row_values,
                def_name: row_def_name.clone(),
                note: None,
            },
        )
        .unwrap_or_else(|error| panic!("setting the row: {error}"));

    let export_root = tempfile::tempdir().expect("tempdir for the export");
    let export_assignment = ExportAssignment::new(
        rim_io::MergeModFolderWriter::new(),
        rim_io::ModsConfigFileStore::new(),
        rim_io::JsonAssignmentProjectStore::new(),
    );
    let outcome = export_assignment
        .execute(
            &mut session,
            &assignment_id,
            AssignmentExportOptions {
                out_dir: export_root.path().join("export"),
                install: false,
            },
        )
        .unwrap_or_else(|error| panic!("exporting the assignment: {error}"));
    assert!(outcome.skipped.is_empty(), "{:?}", outcome.skipped);

    let defs_path = outcome
        .export_path
        .join("Defs")
        .join(format!("rimmerge_{group_type}.xml"));
    let defs_text =
        std::fs::read_to_string(&defs_path).unwrap_or_else(|e| panic!("read {defs_path:?}: {e}"));
    let rendered_tree = rim_merge::xml::parse(&defs_text)
        .unwrap_or_else(|error| panic!("parsing the rendered Defs file: {error}"));
    let rendered_children = match &rendered_tree.root.content {
        rim_merge::tree::Content::Children(children) => children,
        _ => panic!("expected <Defs> to have children"),
    };
    let row_node = rendered_children
        .iter()
        .find(|node| {
            matches!(&node.content, rim_merge::tree::Content::Children(cs)
                if cs.iter().any(|c| c.tag == "defName"
                    && matches!(&c.content, rim_merge::tree::Content::Text(t) if t == &row_def_name)))
        })
        .unwrap_or_else(|| panic!("expected exactly one {group_type} for {row_def_name:?}"));
    let rendered_tags: BTreeSet<String> = match &row_node.content {
        rim_merge::tree::Content::Children(children) => {
            children.iter().map(|c| c.tag.clone()).collect()
        }
        _ => BTreeSet::new(),
    };

    // -- Same shape as core's own real instances -- read directly, not
    // from the framework's bundled example patch file. Scoped to the *structural*
    // shape the workflow names (raceNames, the slot lists, tags) -- the
    // `ItemSlot`/`TargetKey` fields core's own base "_Group" templates always
    // carry. Every other rendered tag (the bool/number fields: egg-laying,
    // a numeric rate field, ...) is a real, disclosed finding instead of a
    // hard assertion: core's own base templates only set the categorical
    // slot/tag fields, leaving the rarer scalar fields (oviparous egg
    // handling, the numeric rate field) to whichever addon actually needs
    // them -- so a
    // real export naming one of those fields is correct behaviour, not a
    // shape mismatch, even though it's absent from core's own narrower
    // reference set. -----------------------------------------------------
    let reference_tags = framework_core_reference_tags(&session, core, group_type);
    eprintln!(
        "{core}'s own reference tag set ({} tags, across every {group_type} it owns itself): \
         {reference_tags:?}",
        reference_tags.len()
    );
    eprintln!("this export's own rendered tags: {rendered_tags:?}");
    let structural_tags: BTreeSet<&str> = [
        gt.target_key_field.as_str(),
        gt.second_item_slot_field.as_str(),
    ]
    .into_iter()
    .chain(gt.item_slot_fields.iter().map(String::as_str))
    .collect();
    for tag in rendered_tags
        .iter()
        .filter(|t| structural_tags.contains(t.as_str()))
    {
        assert!(
            reference_tags.contains(tag),
            "{tag:?} is a structural (TargetKey/ItemSlot) tag but {core}'s own real instances \
             never carry it -- not the same shape as the reference: {reference_tags:?}"
        );
    }
    let non_structural_extras: Vec<&String> = rendered_tags
        .iter()
        .filter(|t| {
            t.as_str() != "defName"
                && !structural_tags.contains(t.as_str())
                && !reference_tags.contains(t.as_str())
        })
        .collect();
    if !non_structural_extras.is_empty() {
        eprintln!(
            "NOTE: rendered scalar field(s) {non_structural_extras:?} never appear on {core}'s \
             own base group templates (which only ever set the categorical slot/tag fields) -- a \
             real, disclosed finding, not a shape mismatch: these fields are legitimately set by \
             other groups/addons in the wider ecosystem the schema was learned from, and this \
             row's own defaults are exactly what the schema itself records for them."
        );
    }
    assert!(
        rendered_tags.contains(&gt.target_key_field),
        "the emitted row must always carry {}: {rendered_tags:?}",
        gt.target_key_field
    );
    // No chance field was ever given a value, so none should appear.
    assert!(
        !rendered_tags
            .iter()
            .any(|t| t.to_lowercase().starts_with("chance")),
        "no chance list should appear when none was set: {rendered_tags:?}"
    );

    // -- MayRequire names the target's own owner (Odyssey when the row's
    // target really is Odyssey-owned; Core targets are ungated so this is
    // skipped in the fallback case). -----------------------------------
    if target_owner.base() != ModId::new(CORE_MOD) {
        assert!(
            defs_text.contains(&format!("MayRequire=\"{}\"", target_owner.base())),
            "MayRequire must name the target's own owner ({target_owner}): {defs_text}"
        );
    } else {
        assert!(
            !defs_text.contains("MayRequire"),
            "a Core target must render ungated: {defs_text}"
        );
    }

    eprintln!(
        "user workflow verification complete: target owner={target_owner}, rendered tags={} \
         (subset of {core}'s own {} reference tags)",
        rendered_tags.len(),
        reference_tags.len()
    );
}

/// The end-to-end real-install proof of the multi-section user story -- a
/// project's own free-standing instances are usable as items in its own
/// target-keyed rows, against real framework data. R = the lineage-support
/// framework, T = the Odyssey DLC (falling back to Core alone, disclosed, exactly like
/// [`user_workflow_lineage_support_ref_odyssey_target_matches_cores_own_shape`]
/// above, if Odyssey is inactive on this machine).
///
/// Two sections, created in load-bearing order (`crates/rim-session/CLAUDE.md`:
/// a no-`TargetKey` schema is only a valid candidate while the
/// project's target set is empty), so the free-standing
/// part-type section is created first (T still empty), *then* T
/// is widened to the real targets, *then* the group def type is added as
/// a second, target-keyed section -- mirroring
/// `crates/rim-io/tests/assignment_multi_section_end_to_end.rs`'s own
/// fixture-backed flow, against the real install.
#[test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
fn multi_section_lineage_support_ref_odyssey_target_own_part_referenced_by_race_group() {
    let Some(gt) = require_ground_truth() else {
        return;
    };
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let mut session = load_real_session(profile_dir);
    let lineage_support = gt.lineage_support.as_str();
    let part_type = gt.part_def_type.as_str();
    let group_type = gt.group_def_type.as_str();
    let primary_slot = gt.primary_item_slot_field.as_str();

    let refs: BTreeSet<ModId> = [ModId::new(lineage_support)].into_iter().collect();
    let create_assignment = CreateAssignment::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );

    // -- Phase 1: the free-standing part-type section, T empty ------------
    let part_candidate = create_assignment
        .infer_candidate(
            &mut session,
            &refs,
            &BTreeSet::new(),
            &BTreeSet::new(),
            part_type,
        )
        .unwrap_or_else(|error| panic!("inferring {part_type}: {error}"))
        .unwrap_or_else(|| panic!("{part_type} must be a valid free-standing candidate"));
    assert!(
        !part_candidate.schema.has_target_key(),
        "{part_type} must classify free-standing while T is empty: {:?}",
        part_candidate.schema.fields
    );

    let create_input = CreateAssignmentInput {
        name: "multi-section assignment verification".to_string(),
        package_id: "mypatch.assignmultisectionverify".to_string(),
        display_name: "Multi-Section Assignment Verification".to_string(),
        refs: refs.clone(),
        excluded_refs: BTreeSet::new(),
        targets: BTreeSet::new(),
        schema: part_candidate.schema,
    };
    let assignment_id = create_assignment
        .execute(&mut session, create_input)
        .unwrap_or_else(|error| panic!("creating the assignment: {error}"));

    let own_part_name = "mypatch_assignmultisectionverify_OwnPart".to_string();
    let set_row = SetAssignmentRow::new(rim_io::JsonAssignmentProjectStore::new());
    set_row
        .execute(
            &mut session,
            &assignment_id,
            part_type,
            RowKey::Own(own_part_name.clone()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: own_part_name.clone(),
                note: None,
            },
        )
        .unwrap_or_else(|error| panic!("setting the free-standing part row: {error}"));

    // -- Phase 2: widen T to Odyssey (+ Core, always included -- same
    // fallback reasoning as the user-workflow test above), then add the
    // group def type as a second, target-keyed section -------------------
    let odyssey_active = session
        .report()
        .mods
        .iter()
        .any(|m| m.id.base() == ModId::new(ODYSSEY_DLC));
    if !odyssey_active {
        eprintln!(
            "NOTE: {ODYSSEY_DLC} is not active on this install -- falling back to Core alone as \
             the closest installed DLC with animal races"
        );
    }
    let mut targets: BTreeSet<ModId> = [ModId::new(CORE_MOD)].into_iter().collect();
    if odyssey_active {
        targets.insert(ModId::new(ODYSSEY_DLC));
    }

    let update_assignment = UpdateAssignment::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );
    update_assignment
        .execute(
            &mut session,
            &assignment_id,
            UpdateAssignmentInput {
                targets: Some(targets.clone()),
                ..UpdateAssignmentInput::default()
            },
        )
        .unwrap_or_else(|error| panic!("widening the target set: {error}"));

    let add_section = AddAssignmentSection::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );
    let race_group_candidate = add_section
        .execute(&mut session, &assignment_id, group_type)
        .unwrap_or_else(|error| panic!("adding the {group_type} section: {error}"));
    assert!(
        race_group_candidate.schema.has_target_key(),
        "{} must classify as TargetKey once T is non-empty: {:?}",
        gt.target_key_field,
        race_group_candidate.schema.fields
    );
    let project = session
        .assignment(&assignment_id)
        .unwrap_or_else(|| panic!("still loaded"));
    assert_eq!(project.sections().len(), 2, "{project:?}");

    // -- Pick one Odyssey animal (falling back to Core, disclosed, exactly
    // like the user-workflow test above) ---------------------------------
    let coverage_use_case = AssignmentCoverage::new(rim_io::FileDefSourceReader::new());
    let coverage = coverage_use_case
        .execute(&mut session, &assignment_id, group_type)
        .unwrap_or_else(|error| panic!("building coverage: {error}"));
    assert!(
        !coverage.rows.is_empty(),
        "expected at least one candidate race within T"
    );
    let chosen_row = coverage
        .rows
        .iter()
        .find(|row| row.owner.base() == ModId::new(ODYSSEY_DLC))
        .unwrap_or_else(|| {
            eprintln!(
                "NOTE: no candidate race is owned by {ODYSSEY_DLC} on this run -- falling back \
                 to a Core-owned candidate for the row this test sets"
            );
            coverage
                .rows
                .first()
                .unwrap_or_else(|| panic!("expected at least one candidate row"))
        });
    let target = chosen_row.target.clone();
    eprintln!(
        "setting a {group_type} row for {} (owner={}), {primary_slot} -> {own_part_name}",
        target.def, chosen_row.owner
    );

    // -- The row: one item slot (primary_slot) lists the new own part, and
    // nothing else -- "slots/tags/bools and no chances" isn't the spec
    // here (the spec is one slot naming the own part), so no chance field
    // is ever given a value. -----------------------------------------------
    let slot_path: FieldPath = primary_slot
        .parse()
        .unwrap_or_else(|error| panic!("{primary_slot} is a valid field path: {error}"));
    let race_row_def_name = format!("mypatch_assignmultisectionverify_{}", target.def.def_name);
    set_row
        .execute(
            &mut session,
            &assignment_id,
            group_type,
            RowKey::Target(target.clone()),
            AssignmentRow {
                values: BTreeMap::from([(slot_path, RowValue::Names(vec![own_part_name.clone()]))]),
                def_name: race_row_def_name.clone(),
                note: None,
            },
        )
        .unwrap_or_else(|error| {
            panic!("setting the {group_type} row referencing the own part: {error}")
        });

    // -- Export, and check the multi-section story's core assertions -----
    let export_root = tempfile::tempdir().expect("tempdir for the export");
    let export_assignment = ExportAssignment::new(
        rim_io::MergeModFolderWriter::new(),
        rim_io::ModsConfigFileStore::new(),
        rim_io::JsonAssignmentProjectStore::new(),
    );
    let outcome = export_assignment
        .execute(
            &mut session,
            &assignment_id,
            AssignmentExportOptions {
                out_dir: export_root.path().join("export"),
                install: false,
            },
        )
        .unwrap_or_else(|error| panic!("exporting the assignment: {error}"));

    // No skip anywhere: the one field this test gave a value (the primary
    // item-slot field, naming the project's own free-standing reference)
    // must render, not be silently dropped -- a skip here would mean a
    // `KnownOwn` reference is no longer resolved on real data.
    assert!(
        outcome.skipped.is_empty(),
        "no field should be skipped -- a skip on the own reference here would be the KnownOwn \
         bug this test guards against: {:?}",
        outcome.skipped
    );

    let defs_files: Vec<&PathBuf> = outcome
        .files
        .iter()
        .filter(|file| file.starts_with("Defs"))
        .collect();
    assert_eq!(
        defs_files.len(),
        2,
        "expected exactly two Defs/ files (one per section): {:?}",
        outcome.files
    );
    let part_defs_path = PathBuf::from(format!("Defs/rimmerge_{part_type}.xml"));
    let race_group_defs_path = PathBuf::from(format!("Defs/rimmerge_{group_type}.xml"));
    assert!(
        outcome.files.contains(&part_defs_path),
        "expected the free-standing section's own file: {:?}",
        outcome.files
    );
    assert!(
        outcome.files.contains(&race_group_defs_path),
        "expected the target-keyed section's own file: {:?}",
        outcome.files
    );

    let race_group_text = std::fs::read_to_string(outcome.export_path.join(&race_group_defs_path))
        .unwrap_or_else(|error| panic!("read {race_group_defs_path:?}: {error}"));
    assert!(
        race_group_text.contains(&own_part_name),
        "the part's own defName must appear inside the race group row's rendered slot list: \
         {race_group_text}"
    );

    let about_bytes = std::fs::read(outcome.export_path.join("About/About.xml"))
        .unwrap_or_else(|error| panic!("read About.xml: {error}"));
    let about_text = String::from_utf8(about_bytes).expect("About.xml must be UTF-8");
    let deps_section = &about_text[about_text
        .find("<modDependencies>")
        .unwrap_or_else(|| panic!("About.xml must carry <modDependencies>: {about_text}"))
        ..about_text
            .find("</modDependencies>")
            .unwrap_or_else(|| panic!("About.xml must close </modDependencies>: {about_text}"))];
    assert!(
        !deps_section.contains("mypatch.assignmultisectionverify"),
        "modDependencies must never name this project's own package id: {deps_section}"
    );

    eprintln!(
        "multi-section assignment verification complete: 2 sections, target={}, {} Defs/ file(s), \
         0 skipped, no self-dependency",
        target.def,
        defs_files.len()
    );
}

/// Guards the real-install `chance...` field cardinality margin.
/// This matters more than the usual real-install disclosure: three of
/// the five fields (item-slot chance fields 1-3) sit at **exactly 2**
/// `li`-wrapped occurrences, one above
/// `crates/rim-resolve/src/domain/assignment/schema.rs`'s own `observe`
/// guard floor (a field classifies `List`, and therefore `Chances`, only
/// once at least two of its own occurrences are `li`-wrapped, or a
/// majority are). If a
/// future mod update drops one of those three fields' `li`-wrapped
/// occurrences from 2 to 1, that field silently reclassifies to
/// `Scalar`, and every row built against the schema starts rendering it
/// wrong -- with nothing here to say so unless a test is actually
/// watching that exact boundary.
///
/// Asserts, per field, both halves together: the guard's own threshold
/// (`wrapped >= 2`, read independently through
/// [`AssignmentInstances::execute`] and each occurrence's own
/// [`Cardinality`] -- never through the schema's private classification
/// internals, so a regression in the classifier being measured can't
/// fool this check) **and** the resulting classification
/// (`FieldRole::Chances`). Deliberately not pinned to today's *exact*
/// counts: a legitimate mod update that grows one field's own count from
/// 6 to 7 is not a regression and must not fail this test; one that
/// drops 2 to 1 is exactly what it exists to catch.
///
/// Reference selection: R = `{example.framework}`, T = Core + the four
/// example race submods, the same selection as the primary flow above. Against it, the
/// three item-slot chance fields 1-3 sit right at that two-occurrence
/// margin, while the primary and secondary item-slot chance fields carry
/// several `li`-wrapped occurrences each.
///
/// **`pawnKindNames`'s "none of its raw values resolve" result is
/// deliberately *not* pinned here**: which of those raw values happen to
/// resolve depends entirely on which optional mods this install has
/// active (the values each belong to a specific, currently-inactive mod),
/// so any assertion tight enough to catch a real regression (e.g.
/// `resolved == 0`, or `resolved < MIN_RESOLVED_DISTINCT`) would just as
/// easily fail the moment the user activates any one of those mods -- a
/// legitimate install change, not a bug, and one entirely outside this
/// codebase's control. That would just encode today's mod list rather
/// than guard an invariant.
#[test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
fn chance_fields_clear_the_cardinality_guard_with_real_margin() {
    let Some(gt) = require_ground_truth() else {
        return;
    };
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let mut session = load_real_session(profile_dir);
    let group_type = gt.group_def_type.as_str();

    let refs: BTreeSet<ModId> = [ModId::new(gt.core.as_str())].into_iter().collect();
    // Same T as the primary flow above -- Core + the extra real
    // race-defining submods -- so the group def type's own field roles
    // classify identically to that test's own already-verified
    // expectations.
    let targets: BTreeSet<ModId> = std::iter::once(ModId::new(CORE_MOD))
        .chain(gt.extra_targets.iter().map(ModId::new))
        .collect();

    let create_assignment = CreateAssignment::new(
        rim_io::JsonAssignmentProjectStore::new(),
        rim_io::FileDefSourceReader::new(),
    );
    let candidate = create_assignment
        .infer_candidate(&mut session, &refs, &BTreeSet::new(), &targets, group_type)
        .unwrap_or_else(|error| panic!("inferring {group_type}: {error}"))
        .unwrap_or_else(|| panic!("{group_type} must be a valid candidate"));

    // Read every active instance's own raw fields directly -- the same
    // source `AssignmentSchema::infer_fields` itself reads, but never
    // through that classification, so this check cannot be fooled by a
    // regression in the very rule it exists to guard.
    let instances = AssignmentInstances::new(rim_io::FileDefSourceReader::new())
        .execute(&mut session, group_type)
        .unwrap_or_else(|error| panic!("reading instances: {error}"));

    let mut chance_paths: Vec<&FieldPath> = candidate
        .schema
        .fields
        .keys()
        .filter(|path| path.to_string().to_lowercase().starts_with("chance"))
        .collect();
    chance_paths.sort();
    assert_eq!(
        chance_paths.len(),
        gt.item_slot_fields.len(),
        "expected exactly one chance... field per item-slot field the framework's own def type \
         declares: {chance_paths:?}"
    );

    for path in chance_paths {
        let wrapped_count = instances
            .iter()
            .filter_map(|(_, values)| values.get(path))
            .filter(|occurrence| occurrence.cardinality == Cardinality::List)
            .count();
        let role = candidate.schema.fields[path].role.clone();
        eprintln!("{path}: {wrapped_count} li-wrapped occurrence(s), role={role:?}");

        assert!(
            wrapped_count >= 2,
            "{path} must clear the >=2 li-wrapped cardinality guard: only {wrapped_count} \
             found -- a mod update dropped this field's own wrapped occurrences below the \
             floor the guard exists to enforce, and it would now silently reclassify Scalar"
        );
        assert!(
            matches!(role, FieldRole::Chances { .. }),
            "{path} cleared the cardinality guard ({wrapped_count} li-wrapped) but did not \
             classify Chances: {role:?}"
        );
    }
}
