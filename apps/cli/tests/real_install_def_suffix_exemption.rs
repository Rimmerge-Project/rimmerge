//! Real-install measurement of the Def-suffix exemption's blast radius
//! (the exemption itself is
//! `rim_resolve::domain::assignment::schema::has_def_suffixed_leaf_tag`,
//! wired into `classify_reference`). This file's own test measures how
//! many fields corpus-wide the exemption rescues, and how many of those
//! may get a wrong `def_type` from a one-value vote, against the shipped
//! code, and pins the *class* of the finding (never today's exact counts,
//! which drift with the mod list) so it stays a live regression guard
//! rather than a one-off measurement.
//!
//! Uses `rim_resolve::domain::AssignmentSchema::reference_field_diagnostics`
//! (behind that crate's `test-support` feature, enabled for this crate's
//! dev-dependencies in `Cargo.toml`) rather than re-implementing any part
//! of the reference-field gate here — that function is a thin
//! diagnostic wrapper around the *exact* `vote_reference_type`/
//! `classify_by_ownership_share` helpers `classify_reference` itself
//! calls, so it can't drift from what `AssignmentSchema::infer_fields`
//! actually decides (`crates/rim-resolve/src/domain/assignment/schema.rs`'s
//! own doc comment on both).
//!
//! Scans *every* def type with at least one active instance
//! (`SourceIndex::owners_by_def`), not one hand-picked reference
//! selection — the exemption's own blast radius is a property of the
//! whole corpus, not of any one assignment candidate. `refs` is the
//! empty set for every def type: `is_def_suffixed`/`resolved_count`/
//! whether a field is rescued *only* by the exemption never depend on
//! `refs` at all (the inside/outside vote, the only place `refs`
//! matters, is skipped entirely for a rescued field — see
//! `classify_reference`'s own doc comment) — so there is no single
//! correct "R" to pick corpus-wide, and none is needed for this
//! measurement.
//!
//! `#[ignore]`d: needs the real game/workshop install (read-only) and a
//! *scratch copy* of a real profile directory in
//! `RIMMERGE_PERF_PROFILE_DIR` (never the real one). Reads every active
//! instance of every def type in the corpus, so this is one of the
//! slower real-install tests in this crate — budget several minutes in
//! `--release`.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-cli --all-features --release --run-ignored ignored-only -E 'binary(real_install_def_suffix_exemption)'`
//!
//! **Also measures the shipped Def-suffix tie discriminator, a positive
//! rule rather than a fall-through, so this file measures *live
//! production behaviour*, not a hypothetical.** A "fall through to
//! `Scalar` on a tie" discriminator is the wrong rule: on a real install
//! it flips a large share of rescued fields, many of them collaterally,
//! and demotes `List` fields to a bare-text `Scalar` — structurally
//! invalid XML, since `render_leaf`, `crates/rim-merge/src/assign.rs`,
//! emits a `Scalar` role as a bare text node regardless of cardinality.
//! The rule `AssignmentSchema::classify_reference` applies instead: on an
//! exact vote tie, prefer the type the leaf tag's own stripped root names
//! (`rim_resolve::domain::reconstruct_candidate_type`, the single
//! production copy of that string surgery, which this file reuses), when
//! that type actually exists in this scan's own corpus — keeping
//! `FieldRole::ItemSlot` and the field's `Cardinality` either way, never a
//! fall-through to `Scalar`.
//! `ReferenceFieldDiagnostics.voted_type` (the vote's own winning type,
//! *before* the discriminator can override it) is compared against each
//! rescued field's own `role` to report, per field: whether the shipped
//! rule actually changed its classification; whether each of the four
//! named clustered misclassifications a real-install inspection turns up
//! inspection (the `requiredOffering/filter/thingDefs` psychic-ritual
//! cluster, `WeatherDef/letterDef`, `mentalBreakDef`, `needDef`) classifies correctly;
//! and a narrower "likely-wrong" heuristic (a rescued field's stripped
//! leaf tag reconstructs a def type that actually exists in this corpus,
//! and the vote chose a different one) that avoids the bare "stripped tag
//! != chosen type" comparison's own false-positive flood (role names like
//! `filthDef`/`slagDef` correctly "disagree" with their own tag) — this
//! is the same reconstruction the shipped rule itself uses, so a field
//! this heuristic flags as likely-wrong but the shipped rule leaves
//! unchanged (no exact tie, so the discriminator never runs at all) is a
//! real, disclosed gap rather than a bug.

mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use rim_resolve::domain::{
    AssignmentSchema, Cardinality, FieldPath, FieldRole, MIN_RESOLVED_DISTINCT, PathSegment,
    ReferenceFieldDiagnostics, reconstruct_candidate_type,
};
use rim_session::Session;
use rim_session::use_cases::AssignmentInstances;

/// This file's own "Run with" invocation (this module's own doc comment,
/// above) — named in [`common::require_profile_dir`]'s panic message so
/// it points at the exact command for this tier, not a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p \
     rimmerge-cli --all-features --release --run-ignored ignored-only -E \
     'binary(real_install_def_suffix_exemption)'";

/// Mirrors `real_install_assign.rs`'s own `load_real_session` — this
/// crate's own established small duplication for a real-install
/// `Session`, rather than reusing `apps/cli/src/common.rs::build_session`,
/// which is private to the binary crate and unreachable from an
/// integration test (that file's own doc comment has the full
/// rationale).
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

/// One field rescued *only* by [`has_def_suffixed_leaf_tag`]'s exemption
/// — `is_def_suffixed && resolved_count < MIN_RESOLVED_DISTINCT &&
/// role.is_some()` — kept with enough context to print without holding
/// onto the whole corpus scan.
struct Rescued {
    def_type: String,
    path: FieldPath,
    diagnostics: ReferenceFieldDiagnostics,
}

/// The `def_type` a rescued field classified `ItemSlot` under — `None`
/// for any other role, which should never actually occur for a member of
/// `rescued` (the exemption's own contract, already asserted at the
/// bottom of the test below).
fn winning_type_of(diagnostics: &ReferenceFieldDiagnostics) -> Option<&str> {
    match &diagnostics.role {
        Some(FieldRole::ItemSlot { def_type }) => Some(def_type.as_str()),
        _ => None,
    }
}

/// Whether the shipped tag-reconstruction tie discriminator actually
/// changed this field's classification: `role`'s own `def_type` differs
/// from the field-wide vote's own `voted_type` (the type before the
/// discriminator can override it). Returns `(voted, chosen)` when it
/// fired, `None` when it didn't (no tie, or the tag gave no signal).
fn tag_reconstruction_changed(diagnostics: &ReferenceFieldDiagnostics) -> Option<(&str, &str)> {
    let voted = diagnostics.voted_type.as_deref()?;
    let chosen = winning_type_of(diagnostics)?;
    (voted != chosen).then_some((voted, chosen))
}

/// A rescued field's own leaf tag — every rescued field's leaf is a named
/// `Child` by construction (`has_def_suffixed_leaf_tag`'s own contract:
/// it only ever matches a `Child` segment), so this returns `None` only
/// for a path shape that could never actually be rescued.
fn leaf_tag(path: &FieldPath) -> Option<&str> {
    match path.segments().last() {
        Some(PathSegment::Child(tag)) => Some(tag.as_str()),
        _ => None,
    }
}

/// Whether `winning_type` looks like the *wrong* type for a rescued
/// field's own `leaf_tag`, under [`reconstruct_candidate_type`]'s narrow
/// heuristic (the exact reconstruction the shipped discriminator itself
/// uses — lifted into `rim-resolve` from this file's own original copy,
/// not a second, independently-maintained one): the reconstructed
/// candidate must actually exist in this corpus
/// (`existing_def_types_lower`, compared case-insensitively) and differ
/// from the type the vote actually chose.
fn is_likely_wrong(
    leaf_tag: &str,
    winning_type: &str,
    existing_def_types_lower: &BTreeSet<String>,
) -> bool {
    let Some(candidate) = reconstruct_candidate_type(leaf_tag) else {
        return false;
    };
    existing_def_types_lower.contains(&candidate.to_ascii_lowercase())
        && !winning_type.eq_ignore_ascii_case(&candidate)
}

/// One of the four specific, disclosed clustered misclassifications
/// own "Re-measure the Def-suffix exemption's
/// blast radius" entry names by inspection — matched structurally (leaf
/// tag, and for the psychic-ritual cluster its own path suffix too, since
/// `thingDefs` alone is too common a tag to name reliably) rather than by
/// mod id, since the same field shape recurs across several mods' defs.
/// `None` for every field outside these four named clusters.
fn known_bad_cluster(path: &FieldPath) -> Option<&'static str> {
    let tag = leaf_tag(path)?;
    match tag {
        "thingDefs"
            if path
                .to_string()
                .ends_with("requiredOffering/filter/thingDefs") =>
        {
            Some("psychic-ritual requiredOffering/filter/thingDefs")
        }
        "letterDef" => Some("letterDef"),
        "mentalBreakDef" => Some("mentalBreakDef"),
        "needDef" => Some("needDef"),
        _ => None,
    }
}

#[test]
#[ignore = "needs the real game/workshop install (read-only) and a scratch RIMMERGE_PERF_PROFILE_DIR copy"]
fn def_suffix_exemption_rescues_only_a_def_suffixed_band_corpus_wide() {
    let Some(profile_dir) = common::require_profile_dir(RERUN_COMMAND) else {
        return;
    };
    let mut session = load_real_session(profile_dir);

    // Every def type with at least one active instance — the corpus this
    // exemption could possibly touch. Cloned out of `owners_by_def` up
    // front so the loop below is free to borrow `session` mutably
    // (`AssignmentInstances::execute` needs `&mut Session` for its own
    // per-order instance cache) without fighting this borrow.
    let def_types: BTreeSet<String> = session
        .sources()
        .owners_by_def
        .keys()
        .map(|(def_type, _def_name)| def_type.clone())
        .collect();
    assert!(
        !def_types.is_empty(),
        "the real install must own at least one def type — an empty scan would make every \
         assertion below vacuous"
    );
    // The corpus-wide existence oracle the shipped tie discriminator
    // itself needs (`AssignmentSchema::infer_fields`'s own
    // `existing_def_type` parameter) — lower-cased once, matching
    // `is_likely_wrong`'s own comparison, since the discriminator's own
    // production callers compare case-insensitively too.
    let existing_def_types_lower: BTreeSet<String> =
        def_types.iter().map(|t| t.to_ascii_lowercase()).collect();
    let existing_def_type =
        |type_name: &str| existing_def_types_lower.contains(&type_name.to_ascii_lowercase());

    let mut rescued: Vec<Rescued> = Vec::new();
    let mut unreadable_def_types = 0usize;

    for def_type in &def_types {
        // A per-def-type read failure (a stale/malformed file somewhere
        // in a ~1000-mod corpus is plausible) is logged and skipped
        // rather than aborting the whole scan — unlike
        // `real_install_assign.rs`'s own reads, which target one
        // already-verified def type, this loop is a best-effort
        // corpus-wide *measurement*, not a correctness assertion about
        // any single def's own readability.
        let instances = match AssignmentInstances::new(rim_io::FileDefSourceReader::new())
            .execute(&mut session, def_type)
        {
            Ok(instances) => instances,
            Err(error) => {
                eprintln!("skipping {def_type}: {error}");
                unreadable_def_types += 1;
                continue;
            }
        };
        if instances.is_empty() {
            continue;
        }

        // `refs`/`dll_owner` don't affect whether a field is rescued only
        // by the exemption (this file's own module doc comment) — `refs`
        // is the empty set (no single R applies corpus-wide) and
        // `dll_owner` is still the real one, since it costs nothing to
        // wire up correctly.
        let refs: BTreeSet<rim_analyzer::domain::ModId> = BTreeSet::new();
        let sources = session.sources();
        let resolve = |value: &str| sources.defs_by_name.get(value).cloned().unwrap_or_default();
        let dll_owner = |type_name: &str| sources.dll_owner_of(type_name).cloned();

        let diagnostics = AssignmentSchema::reference_field_diagnostics(
            &instances,
            &refs,
            &resolve,
            &dll_owner,
            &existing_def_type,
        );
        for (path, field_diagnostics) in diagnostics {
            let rescued_only_by_exemption = field_diagnostics.is_def_suffixed
                && field_diagnostics.resolved_count < MIN_RESOLVED_DISTINCT
                && field_diagnostics.role.is_some();
            if rescued_only_by_exemption {
                rescued.push(Rescued {
                    def_type: def_type.clone(),
                    path,
                    diagnostics: field_diagnostics,
                });
            }
        }
    }

    let one_value: Vec<&Rescued> = rescued
        .iter()
        .filter(|r| r.diagnostics.resolved_count == 1)
        .collect();
    let list_count = rescued
        .iter()
        .filter(|r| r.diagnostics.cardinality == Cardinality::List)
        .count();
    let scalar_count = rescued
        .iter()
        .filter(|r| r.diagnostics.cardinality == Cardinality::Scalar)
        .count();

    eprintln!(
        "def-suffix exemption, real install: {} def types scanned ({} unreadable, skipped), {} \
         fields rescued only by the exemption ({} List, {} Scalar), {} of which resolved exactly \
         one value",
        def_types.len(),
        unreadable_def_types,
        rescued.len(),
        list_count,
        scalar_count,
        one_value.len()
    );
    eprintln!("one-value rescues (field path -> chosen def_type):");
    for r in &one_value {
        let def_type = match &r.diagnostics.role {
            Some(FieldRole::ItemSlot { def_type }) => def_type.as_str(),
            other => panic!(
                "{}/{} rescued by the exemption but its role is {other:?}, not ItemSlot -- the \
                 exemption's own contract (classify_reference's doc comment) says a rescued field \
                 always classifies ItemSlot unconditionally",
                r.def_type, r.path
            ),
        };
        eprintln!("  {}/{} -> {def_type}", r.def_type, r.path);
    }

    // --- The shipped tag-reconstruction tie discriminator, live ---
    // Applied to every rescued field, not only the one-value subset: a
    // tie can in principle occur at any resolved_count below
    // MIN_RESOLVED_DISTINCT, not only at exactly 1 (a 2-vote field with a
    // 2-vote runner-up ties just the same). This reports whether the
    // *actual* production classification (`role`) already differs from
    // the vote's own raw pick (`voted_type`) — the rule already ran
    // inside `reference_field_diagnostics` above, this is only reading
    // the before/after off its own output.
    let overridden: Vec<&Rescued> = rescued
        .iter()
        .filter(|r| tag_reconstruction_changed(&r.diagnostics).is_some())
        .collect();
    let overridden_list = overridden
        .iter()
        .filter(|r| r.diagnostics.cardinality == Cardinality::List)
        .count();
    let overridden_scalar = overridden
        .iter()
        .filter(|r| r.diagnostics.cardinality == Cardinality::Scalar)
        .count();

    eprintln!(
        "shipped tie discriminator: {}/{} rescued fields had their def_type overridden by the \
         leaf tag's own reconstruction ({} List, {} Scalar)",
        overridden.len(),
        rescued.len(),
        overridden_list,
        overridden_scalar
    );
    eprintln!("fields the discriminator changed (cardinality, field path, voted -> chosen):");
    for r in &overridden {
        let cardinality_tag = match r.diagnostics.cardinality {
            Cardinality::List => "LIST",
            Cardinality::Scalar => "scalar",
        };
        let (voted, chosen) = tag_reconstruction_changed(&r.diagnostics)
            .expect("already filtered to fields the discriminator changed");
        eprintln!(
            "  [{cardinality_tag}] {}/{}: {voted} -> {chosen}",
            r.def_type, r.path
        );
    }

    // --- The decisive question: does the rule fix the clustered errors? ---
    eprintln!("named clustered misclassifications, found and fix status:");
    let mut known_bad_found = 0usize;
    let mut known_bad_fixed = 0usize;
    for r in &rescued {
        let Some(cluster) = known_bad_cluster(&r.path) else {
            continue;
        };
        known_bad_found += 1;
        let fixed = tag_reconstruction_changed(&r.diagnostics).is_some();
        if fixed {
            known_bad_fixed += 1;
        }
        let def_type = winning_type_of(&r.diagnostics).unwrap_or("?");
        let voted = r.diagnostics.voted_type.as_deref().unwrap_or("?");
        eprintln!(
            "  [{cluster}] {}/{}: voted {voted}, classified {def_type} -- {}",
            r.def_type,
            r.path,
            if fixed {
                "FIXED by the shipped discriminator"
            } else {
                "NOT fixed by the shipped discriminator"
            }
        );
    }
    eprintln!(
        "named clusters: {known_bad_found} found on this install, {known_bad_fixed} fixed by the \
         shipped discriminator"
    );

    // --- The narrower, defensible "likely-wrong" heuristic: a disclosure
    // of remaining gaps (a field that looks wrong by tag-vs-type mismatch
    // but the shipped rule never touches, because no exact tie occurred),
    // not a second measurement of the rule's own firing rate. ---
    let likely_wrong: Vec<&Rescued> = rescued
        .iter()
        .filter(|r| {
            let Some(tag) = leaf_tag(&r.path) else {
                return false;
            };
            let Some(winning_type) = winning_type_of(&r.diagnostics) else {
                return false;
            };
            is_likely_wrong(tag, winning_type, &existing_def_types_lower)
        })
        .collect();
    let likely_wrong_still_wrong = likely_wrong
        .iter()
        .filter(|r| tag_reconstruction_changed(&r.diagnostics).is_none())
        .count();

    eprintln!(
        "likely-wrong heuristic (stripped leaf tag reconstructs a real corpus type the *final* \
         classification didn't pick): {}/{} rescued fields flagged, {} of those NOT touched by \
         the shipped discriminator (no exact tie, or the tag gave no signal) -- this is the \
         population still worth a second look, not a defect in the shipped rule, which only ever \
         acts on a genuine tie",
        likely_wrong.len(),
        rescued.len(),
        likely_wrong_still_wrong
    );
    eprintln!("likely-wrong fields still unchanged (field path -> classified def_type):");
    for r in &likely_wrong {
        if tag_reconstruction_changed(&r.diagnostics).is_some() {
            continue;
        }
        let def_type = winning_type_of(&r.diagnostics).unwrap_or("?");
        eprintln!("  {}/{} -> {def_type}", r.def_type, r.path);
    }

    // The class, not today's exact counts: a real install has *some*
    // rescued band (eggFertilizedDef is the motivating case),
    // and every rescued field really is Def/Defs-suffixed and really
    // classifies ItemSlot (never TargetKey — the panic above already
    // enforces the latter for the one-value subset; this covers every
    // rescued field, one-value or not).
    assert!(
        !rescued.is_empty(),
        "expected at least one field to be rescued only by the Def-suffix exemption on the real \
 install -- if this is \
         genuinely zero now, the exemption may have become dead code and is worth a second look"
    );
    for r in &rescued {
        assert!(
            r.diagnostics.is_def_suffixed,
            "{}/{} was counted as rescued but has_def_suffixed_leaf_tag is false for it",
            r.def_type, r.path
        );
        assert!(
            matches!(r.diagnostics.role, Some(FieldRole::ItemSlot { .. })),
            "{}/{} rescued by the exemption must classify ItemSlot, got {:?}",
            r.def_type,
            r.path,
            r.diagnostics.role
        );
        // A structural invariant of the vote itself, not a real-install
        // count: argmax never picks a non-maximal winner, so the
        // runner-up (when one exists at all) can never outscore it.
        if let Some(runner_up) = r.diagnostics.runner_up_vote_count {
            assert!(
                runner_up <= r.diagnostics.resolved_count,
                "{}/{} runner-up vote count {runner_up} exceeds the winning count {} -- the \
                 field-wide vote should never pick a non-maximal winner",
                r.def_type,
                r.path,
                r.diagnostics.resolved_count
            );
        }
    }
}
