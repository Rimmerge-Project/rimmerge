//! Tests for the verify pass.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use rim_analyzer::analysis::{IndexedPatchOp, SourceIndex};
use rim_analyzer::domain::{
    Conflict, DefEntry, EdgeKind, FindModGate, PatchCollision, PatchCollisionEntry,
    PatchCollisionSeverity, PatchOp, XmlLocator,
};
use rim_resolve::domain::{Finding, FindingKey, OrderSource, PatchFailureCause, ReorderKind};
use std::collections::BTreeSet;

use super::counterfactual::MAX_COUNTERFACTUAL_SUBJECTS_PER_DEF;
use super::report::classify_cause;
use super::*;
use crate::test_support::{
    CallCountingReader, InMemoryDefSourceReader, locator, session_with_sources_and_mods,
};
use rim_analyzer::domain::{LoadOrder, Selector};

fn make_op(mod_id: &ModId, class: &str, xpath: &str, op_locator: XmlLocator) -> IndexedPatchOp {
    IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: class.to_string(),
            xpath: Some(xpath.to_string()),
            target: Some(rim_analyzer::domain::DefTarget {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
                selector: Selector::DefName,
                sub_path: xpath
                    .split_once(r#"defName="Wall"]/"#)
                    .map(|(_, rest)| rest.to_string()),
            }),
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: BTreeSet::new(),
            injected_paths: BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: op_locator,
        },
    }
}

fn base_sources() -> SourceIndex {
    let mut sources = SourceIndex::default();
    let core_locator = locator("core_wall.xml", 0);
    sources.defs.insert(
        (
            ModId::new("core.mod"),
            ("ThingDef".to_string(), "Wall".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: core_locator,
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Wall".to_string()),
        vec![ModId::new("core.mod")],
    );
    sources
}

fn elements() -> BTreeMap<XmlLocator, String> {
    let mut elements = BTreeMap::new();
    elements.insert(
        locator("core_wall.xml", 0),
        "<ThingDef><defName>Wall</defName>\
             <statBases><MaxHitPoints>100</MaxHitPoints></statBases><comps></comps></ThingDef>"
            .to_string(),
    );
    elements
}

/// OR-head aggregation test fixtures: one
/// `IndexedPatchOp` entry per named def, sharing one physical
/// top-level `<Operation>` locator — exactly how `SourceIndex::build`
/// indexes a real OR-ed defName head, one entry per def it names, all
/// sharing the same `(mod, file, first ordinal)` identity.
///
/// **`def_names` must literally list every def the fixture replays
/// this op against** — the replay engine parses the real `<xpath>`
/// text itself (never reads `IndexedPatchOp::target::def_name` back
/// during replay, which exists only for `SourceIndex`-style
/// bookkeeping); a def not literally named in the head is "succeeded
/// elsewhere" (`crates/rim-merge/CLAUDE.md`'s replay-scoping rule), never
/// actually evaluated against that def's own tree at all — silently
/// defeating the point of a fixture that means to exercise a real
/// per-def match/no-match. [`or_head_op_xml`] must be called with the
/// identical slice.
fn or_head_xpath(def_names: &[&str]) -> String {
    let predicate = def_names
        .iter()
        .map(|name| format!(r#"defName="{name}""#))
        .collect::<Vec<_>>()
        .join(" or ");
    format!("Defs/ThingDef[{predicate}]/neverExisted")
}

fn make_or_head_op(
    mod_id: &ModId,
    def_name: &str,
    head_xpath: &str,
    op_locator: XmlLocator,
) -> IndexedPatchOp {
    IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationReplace".to_string(),
            xpath: Some(head_xpath.to_string()),
            target: Some(rim_analyzer::domain::DefTarget {
                def_type: "ThingDef".to_string(),
                def_name: def_name.to_string(),
                selector: Selector::DefName,
                sub_path: Some("neverExisted".to_string()),
            }),
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: BTreeSet::new(),
            injected_paths: BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: op_locator,
        },
    }
}

/// The OR-headed op's own real XML text — one physical `<Operation>`
/// node, read back identically regardless of which def key's own
/// iteration of [`VerifyOrder::execute_with_progress`] asks for it
/// (same locator, same [`InMemoryDefSourceReader`] entry). `def_names`
/// must match [`make_or_head_op`]'s own calls exactly — see that
/// function's own doc comment for why.
fn or_head_op_xml(def_names: &[&str]) -> String {
    format!(
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>{}</xpath>
                 <value><neverExisted>2</neverExisted></value>
               </Operation>"#,
        or_head_xpath(def_names)
    )
}

/// A minimal `ThingDef` raw source, with or without the leaf field
/// the OR-headed op above targets — present makes the op succeed
/// against this def, absent makes it fail (`Caveat::FailedOp`, an
/// empty selection).
fn wall_xml(def_name: &str, has_field: bool) -> String {
    if has_field {
        format!("<ThingDef><defName>{def_name}</defName><neverExisted>1</neverExisted></ThingDef>")
    } else {
        format!("<ThingDef><defName>{def_name}</defName></ThingDef>")
    }
}

/// Registers `def_name` as a concrete def owned by `owner`, with
/// `has_field` controlling whether the OR-headed op's own target leaf
/// is present on its raw XML — the shared setup every OR-head fixture
/// below builds on, one def at a time.
fn register_owned_def(
    sources: &mut SourceIndex,
    elements: &mut BTreeMap<XmlLocator, String>,
    owner: &ModId,
    def_name: &str,
    has_field: bool,
) {
    let def_locator = locator(&format!("{def_name}_owner.xml"), 0);
    sources.defs.insert(
        (
            owner.clone(),
            ("ThingDef".to_string(), def_name.to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: def_name.to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: def_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), def_name.to_string()),
        vec![owner.clone()],
    );
    elements.insert(def_locator, wall_xml(def_name, has_field));
}

/// Counterfactual-phase fixtures. One owned `ThingDef/Wall` plus a handful of
/// foreign patchers whose failures are caused purely by where they
/// sit in the load order — the smallest shape that exercises the
/// post-reconciliation phase end to end through the real
/// [`VerifyOrder::execute`] entry point.
fn counterfactual_op(
    mod_id: &ModId,
    class: &str,
    sub_path: &str,
    op_locator: XmlLocator,
) -> IndexedPatchOp {
    IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: class.to_string(),
            xpath: Some(format!(r#"Defs/ThingDef[defName="Wall"]/{sub_path}"#)),
            target: Some(rim_analyzer::domain::DefTarget {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
                selector: Selector::DefName,
                sub_path: Some(sub_path.to_string()),
            }),
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: BTreeSet::new(),
            injected_paths: BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: op_locator,
        },
    }
}

/// Registers `ThingDef/Wall` with `raw_xml` as its only owner's raw
/// source, and every `(mod, class, sub_path, operation_xml)` entry as
/// one top-level patch op of its own, each in its own file so the
/// aggregation key never groups two of them together.
fn counterfactual_fixture(
    raw_xml: &str,
    patchers: &[(&str, &str, &str, String)],
) -> (SourceIndex, BTreeMap<XmlLocator, String>) {
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();

    let owner_locator = locator("core_wall.xml", 0);
    sources.defs.insert(
        (
            ModId::new("core.mod"),
            ("ThingDef".to_string(), "Wall".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: owner_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Wall".to_string()),
        vec![ModId::new("core.mod")],
    );
    elements.insert(owner_locator, raw_xml.to_string());

    let ops = patchers
        .iter()
        .enumerate()
        .map(|(index, (mod_id, class, sub_path, operation_xml))| {
            let op_locator = locator(&format!("{mod_id}_{index}.xml"), 0);
            elements.insert(op_locator.clone(), operation_xml.clone());
            counterfactual_op(&ModId::new(mod_id), class, sub_path, op_locator)
        })
        .collect();
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        ops,
    );
    (sources, elements)
}

fn counterfactual_session(sources: SourceIndex, mods: &[&str]) -> Session {
    let mut builder = rim_resolve::test_support::ReportBuilder::new();
    for id in mods {
        builder = builder.mod_(id);
    }
    session_with_sources_and_mods(sources, builder.build(), mods)
}

#[test]
fn an_unknown_row_is_upgraded_to_a_reorder_bearing_cause_by_the_counterfactual_phase() {
    // `b.mod`'s `Add` targets `container/injected`, a node `c.mod`'s
    // own `Add` creates — and `b.mod` loads first. No removed-node or injected-node edge
    // exists in this report at all, so `classify_cause` can only say
    // `Unknown`; the replay can say exactly which mod and which way.
    let (sources, elements) = counterfactual_fixture(
        "<ThingDef><defName>Wall</defName><container></container></ThingDef>",
        &[
            (
                "b.mod",
                "PatchOperationAdd",
                "container/injected",
                r#"<Operation Class="PatchOperationAdd">
                         <xpath>Defs/ThingDef[defName="Wall"]/container/injected</xpath>
                         <value><deep>2</deep></value>
                       </Operation>"#
                    .to_string(),
            ),
            (
                "c.mod",
                "PatchOperationAdd",
                "container",
                r#"<Operation Class="PatchOperationAdd">
                         <xpath>Defs/ThingDef[defName="Wall"]/container</xpath>
                         <value><injected><inner>1</inner></injected></value>
                       </Operation>"#
                    .to_string(),
            ),
        ],
    );
    let session = counterfactual_session(sources, &["core.mod", "b.mod", "c.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let without = verify.execute_with_options(
        &session,
        OrderSource::Current,
        VerifyOptions {
            counterfactual: false,
        },
    );
    assert_eq!(without.findings.len(), 1, "{:?}", without.findings);
    let Finding::PatchWillFail { cause, .. } = &without.findings[0] else {
        panic!("expected PatchWillFail, got {:?}", without.findings[0]);
    };
    assert_eq!(
        *cause,
        PatchFailureCause::Unknown,
        "the static classifier's own answer, unchanged"
    );
    assert_eq!(without.counterfactual, CounterfactualStats::default());

    let with = verify.execute(&session, OrderSource::Current);

    assert_eq!(with.findings.len(), 1, "{:?}", with.findings);
    let Finding::PatchWillFail {
        mod_id,
        cause,
        reorder_kind,
        ..
    } = &with.findings[0]
    else {
        panic!("expected PatchWillFail, got {:?}", with.findings[0]);
    };
    assert_eq!(*mod_id, ModId::new("b.mod"));
    assert_eq!(
        *cause,
        PatchFailureCause::NotYetInjected(ModId::new("c.mod")),
        "the counterfactual fix maps onto the existing Reorder-bearing cause"
    );
    assert_eq!(
        *reorder_kind,
        Some(ReorderKind::Content),
        "b.mod's own `<deep>2</deep>` only ever lands under `injected` when c.mod \
         creates it first — the final resolved def genuinely differs, the Content case"
    );
    assert_eq!(with.counterfactual.jobs, 1);
    assert_eq!(with.counterfactual.resolved, 1);
    assert_eq!(with.counterfactual.demoted_dead_targets, 0);
    assert_eq!(with.counterfactual.attempts, 1, "K - 1 for K = 2");
    assert_eq!(with.counterfactual.mods_per_def, BTreeMap::from([(2, 1)]));
}

#[test]
fn a_subject_that_also_co_owns_the_def_is_refused_rather_than_answered() {
    // The counterfactual's scope limit: `b.mod` is a
    // losing *owner* of `ThingDef/Wall` as well as a patcher of it,
    // so the reorder a counterfactual would recommend could change
    // which owner the "last owner wins" rule picks — the one thing
    // the experiment holds fixed. The fixture is otherwise identical
    // to the upgrade test above, which is what makes the difference
    // attributable: without the guard, this row would be resolved
    // exactly the same way.
    let (mut sources, mut elements) = counterfactual_fixture(
        "<ThingDef><defName>Wall</defName><container></container></ThingDef>",
        &[
            (
                "b.mod",
                "PatchOperationAdd",
                "container/injected",
                r#"<Operation Class="PatchOperationAdd">
                         <xpath>Defs/ThingDef[defName="Wall"]/container/injected</xpath>
                         <value><deep>2</deep></value>
                       </Operation>"#
                    .to_string(),
            ),
            (
                "c.mod",
                "PatchOperationAdd",
                "container",
                r#"<Operation Class="PatchOperationAdd">
                         <xpath>Defs/ThingDef[defName="Wall"]/container</xpath>
                         <value><injected><inner>1</inner></injected></value>
                       </Operation>"#
                    .to_string(),
            ),
        ],
    );

    // `b.mod` ships its own losing copy of the def, ahead of
    // `core.mod`'s winning one in load order.
    let b_owner_locator = locator("b_wall.xml", 0);
    sources.defs.insert(
        (
            ModId::new("b.mod"),
            ("ThingDef".to_string(), "Wall".to_string()),
        ),
        vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: b_owner_locator.clone(),
        }],
    );
    sources.owners_by_def.insert(
        ("ThingDef".to_string(), "Wall".to_string()),
        vec![ModId::new("b.mod"), ModId::new("core.mod")],
    );
    elements.insert(
        b_owner_locator,
        "<ThingDef><defName>Wall</defName><container></container></ThingDef>".to_string(),
    );

    // Load order puts `core.mod` last, so it still wins — the point
    // is that a recommended reorder of `b.mod` could stop being true.
    let session = counterfactual_session(sources, &["b.mod", "core.mod", "c.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    let Finding::PatchWillFail { mod_id, cause, .. } = &result.findings[0] else {
        panic!("expected PatchWillFail, got {:?}", result.findings[0]);
    };
    assert_eq!(*mod_id, ModId::new("b.mod"));
    assert_eq!(
        *cause,
        PatchFailureCause::Unknown,
        "left exactly as the static classifier set it"
    );
    assert_eq!(result.counterfactual.skipped_co_owner, 1);
    assert_eq!(
        result.counterfactual.jobs, 0,
        "the job never entered the queue"
    );
    assert_eq!(result.counterfactual.attempts, 0);
}

#[test]
fn a_dead_target_row_a_reorder_would_fix_is_demoted() {
    // `b.mod` removes `container/doomed`, which `c.mod`'s own
    // `Replace` of the whole container destroyed first — so the node
    // really is absent from the final resolved tree, which is all
    // `classify_cause`'s `DeadTarget` check ever looks at. The
    // replay shows the claim ("no reorder fixes this") is wrong.
    let (sources, elements) = counterfactual_fixture(
        "<ThingDef><defName>Wall</defName><container><doomed>1</doomed></container></ThingDef>",
        &[
            (
                "c.mod",
                "PatchOperationReplace",
                "container",
                r#"<Operation Class="PatchOperationReplace">
                         <xpath>Defs/ThingDef[defName="Wall"]/container</xpath>
                         <value><container><other>2</other></container></value>
                       </Operation>"#
                    .to_string(),
            ),
            (
                "b.mod",
                "PatchOperationRemove",
                "container/doomed",
                r#"<Operation Class="PatchOperationRemove">
                         <xpath>Defs/ThingDef[defName="Wall"]/container/doomed</xpath>
                       </Operation>"#
                    .to_string(),
            ),
        ],
    );
    let session = counterfactual_session(sources, &["core.mod", "c.mod", "b.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let without = verify.execute_with_options(
        &session,
        OrderSource::Current,
        VerifyOptions {
            counterfactual: false,
        },
    );
    assert_eq!(without.findings.len(), 1, "{:?}", without.findings);
    let Finding::PatchWillFail { cause, .. } = &without.findings[0] else {
        panic!("expected PatchWillFail, got {:?}", without.findings[0]);
    };
    assert_eq!(*cause, PatchFailureCause::DeadTarget);

    let with = verify.execute(&session, OrderSource::Current);

    let Finding::PatchWillFail {
        mod_id,
        cause,
        reorder_kind,
        ..
    } = &with.findings[0]
    else {
        panic!("expected PatchWillFail, got {:?}", with.findings[0]);
    };
    assert_eq!(*mod_id, ModId::new("b.mod"));
    assert_eq!(
        *cause,
        PatchFailureCause::RemovedBy(ModId::new("c.mod")),
        "the destroyer, named by replay rather than by a static edge"
    );
    assert_eq!(
        *reorder_kind,
        Some(ReorderKind::Cosmetic {
            existing_merge: None
        }),
        "c.mod's own Replace overwrites the *whole* container regardless of \
         whether b.mod's Remove ran first — a Replace's own <value> node discards \
         the whole matched subtree, whatever an earlier op did to it — `container` ends up \
         `<other>2</other>` either way, the Cosmetic case. This fixture's \
         report carries no PatchCollision finding, so there's no merge to point at."
    );
    assert_eq!(with.counterfactual.demoted_dead_targets, 1);
    assert_eq!(
        with.counterfactual.resolved, 0,
        "a demotion is counted separately from an Unknown resolution"
    );
}

/// Same cosmetic fixture as
/// [`a_dead_target_row_a_reorder_would_fix_is_demoted`], plus a
/// `PatchCollision` finding at the identical `(def, sub_path)` the
/// failing op targets — the fallback offered: a cosmetic row offers no
/// `set-pair` fix, but names the existing merge that keeps the losing
/// mod's intent instead.
#[test]
fn a_cosmetic_fixs_reorder_kind_names_an_existing_patch_collision() {
    let (sources, elements) = counterfactual_fixture(
        "<ThingDef><defName>Wall</defName><container><doomed>1</doomed></container></ThingDef>",
        &[
            (
                "c.mod",
                "PatchOperationReplace",
                "container",
                r#"<Operation Class="PatchOperationReplace">
                         <xpath>Defs/ThingDef[defName="Wall"]/container</xpath>
                         <value><container><other>2</other></container></value>
                       </Operation>"#
                    .to_string(),
            ),
            (
                "b.mod",
                "PatchOperationRemove",
                "container/doomed",
                r#"<Operation Class="PatchOperationRemove">
                         <xpath>Defs/ThingDef[defName="Wall"]/container/doomed</xpath>
                       </Operation>"#
                    .to_string(),
            ),
        ],
    );
    let mut report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("c.mod")
        .mod_("b.mod")
        .build();
    // Hand-built rather than `ReportBuilder::patch_collision` (which
    // always sets `sub_path: None`): this pair's own real collision sits
    // at `container/doomed`, the exact sub_path the failing op's own
    // `IndexedPatchOp::target` carries — `PatchCollisionIndex::get`
    // matches on that literal text, not a field-path re-derivation.
    report
        .conflicts
        .push(Conflict::PatchCollision(PatchCollision {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            selector: Selector::DefName,
            sub_path: Some("container/doomed".to_string()),
            mods: vec![
                PatchCollisionEntry {
                    mod_id: ModId::new("c.mod"),
                    op_class: "PatchOperationReplace".to_string(),
                },
                PatchCollisionEntry {
                    mod_id: ModId::new("b.mod"),
                    op_class: "PatchOperationRemove".to_string(),
                },
            ],
            severity: PatchCollisionSeverity::Contested,
            removed_by: Vec::new(),
        }));
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "c.mod", "b.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let with = verify.execute(&session, OrderSource::Current);

    let Finding::PatchWillFail { reorder_kind, .. } = &with.findings[0] else {
        panic!("expected PatchWillFail, got {:?}", with.findings[0]);
    };
    let Some(ReorderKind::Cosmetic { existing_merge }) = reorder_kind else {
        panic!("expected a Cosmetic reorder_kind, got {reorder_kind:?}");
    };
    let expected_key = FindingKey::PatchCollision {
        key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("container/doomed".to_string()),
        mods: BTreeSet::from([ModId::new("b.mod"), ModId::new("c.mod")]),
    };
    assert_eq!(
        *existing_merge,
        Some(expected_key),
        "names the existing PatchCollision at the identical def and sub_path"
    );
}

#[test]
fn the_counterfactual_caps_are_respected_and_counted() {
    // Thirteen patcher mods, each with its own always-failing
    // operation: the per-def subject cap keeps only four jobs (the
    // other nine are counted, not silently dropped), and the K cap
    // then rejects the def outright at K = 13 — with the K itself
    // recorded, so the distribution of K on real installs stays
    // measurable.
    let patchers: Vec<(String, String, String, String)> = (0..13)
        .map(|index| {
            let sub_path = format!("neverExisted{index}");
            (
                format!("m{index:02}.mod"),
                "PatchOperationReplace".to_string(),
                sub_path.clone(),
                format!(
                    r#"<Operation Class="PatchOperationReplace">
                             <xpath>Defs/ThingDef[defName="Wall"]/{sub_path}</xpath>
                             <value><{sub_path}>2</{sub_path}></value>
                           </Operation>"#
                ),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str, &str, String)> = patchers
        .iter()
        .map(|(id, class, sub_path, xml)| {
            (id.as_str(), class.as_str(), sub_path.as_str(), xml.clone())
        })
        .collect();
    let (sources, elements) =
        counterfactual_fixture("<ThingDef><defName>Wall</defName></ThingDef>", &borrowed);
    let mut mods = vec!["core.mod".to_string()];
    mods.extend(patchers.iter().map(|(id, ..)| id.clone()));
    let mod_refs: Vec<&str> = mods.iter().map(String::as_str).collect();
    let session = counterfactual_session(sources, &mod_refs);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(result.findings.len(), 13, "{:?}", result.findings);
    assert_eq!(
        result.counterfactual.jobs, MAX_COUNTERFACTUAL_SUBJECTS_PER_DEF,
        "the per-def subject cap is what bounds the queue here"
    );
    assert_eq!(
        result.counterfactual.skipped_too_many_subjects,
        13 - MAX_COUNTERFACTUAL_SUBJECTS_PER_DEF
    );
    assert_eq!(
        result.counterfactual.mods_per_def,
        BTreeMap::from([(13, 1)]),
        "K is measured even for a def the cap then rejects"
    );
    assert_eq!(
        result.counterfactual.skipped_too_many_mods, MAX_COUNTERFACTUAL_SUBJECTS_PER_DEF,
        "every surviving job on the over-large def is counted, never silently applied"
    );
    assert_eq!(result.counterfactual.attempts, 0, "nothing was replayed");
    assert!(
        result.findings.iter().all(|finding| matches!(
            finding,
            Finding::PatchWillFail {
                cause: PatchFailureCause::DeadTarget,
                ..
            }
        )),
        "every cause is left exactly as the static classifier set it"
    );
}

#[test]
fn the_counterfactual_phase_extends_the_progress_total_once_mid_stream() {
    // Phase 1 reports against the candidate-key count; phase 2
    // raises `total` by the job count `J`, once, and the last tick
    // is `total`/`total`.
    let (sources, elements) = counterfactual_fixture(
        "<ThingDef><defName>Wall</defName><container></container></ThingDef>",
        &[
            (
                "b.mod",
                "PatchOperationAdd",
                "container/injected",
                r#"<Operation Class="PatchOperationAdd">
                         <xpath>Defs/ThingDef[defName="Wall"]/container/injected</xpath>
                         <value><deep>2</deep></value>
                       </Operation>"#
                    .to_string(),
            ),
            (
                "c.mod",
                "PatchOperationAdd",
                "container",
                r#"<Operation Class="PatchOperationAdd">
                         <xpath>Defs/ThingDef[defName="Wall"]/container</xpath>
                         <value><injected><inner>1</inner></injected></value>
                       </Operation>"#
                    .to_string(),
            ),
        ],
    );
    let session = counterfactual_session(sources, &["core.mod", "b.mod", "c.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let mut ticks: Vec<(usize, usize)> = Vec::new();
    let report =
        verify.execute_with_progress(&session, OrderSource::Current, &mut |done, total| {
            ticks.push((done, total));
        });

    assert_eq!(report.counterfactual.jobs, 1);
    assert_eq!(
        ticks,
        vec![(0, 1), (1, 2), (2, 2)],
        "one candidate key, then one counterfactual job, then the final tick"
    );
    assert!(
        ticks.windows(2).all(|pair| pair[0].0 <= pair[1].0),
        "the reported fraction stays monotonic across the phase boundary"
    );
}

/// The zero-owner fast path runs its own `patch_eval::replay`, so the
/// suppressed-prediction count has to be summed there as well as off
/// `EffectiveDef` on the replayed path — reading only one of the two
/// undercounts the very number that decides whether the conservatism is
/// lifted.
///
/// `not(comps)` holds on the synthetic `<ThingDef></ThingDef>`
/// placeholder, so this filter head does select this def; its
/// `statBases` step then matches nothing, which is a suppression
/// rather than a prediction (the head is a global query).
#[test]
fn a_filter_head_suppression_on_the_zero_owner_path_is_counted() {
    let mut sources = SourceIndex::default();
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Ghost".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("x.mod"),
            "PatchOperationAdd",
            r#"Defs/ThingDef[not(comps)]/statBases"#,
            x_locator.clone(),
        )],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[not(comps)]/statBases</xpath>
                 <value><li>GeneA</li></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(
        result.suppressed_filter_head_ops, 1,
        "the zero-owner path's own suppression must reach the report"
    );
    assert!(
        result.findings.is_empty(),
        "a filter head never predicts: {:?}",
        result.findings
    );
}

/// The zero-owner fast path replays against a synthetic, empty
/// placeholder, never a winner's own raw XML (there is none), so the
/// assertions cover `operation`/`leaf_xpath` as well as `cause`.
#[test]
fn a_patch_target_naming_a_def_with_zero_owners_is_dead_target() {
    let mut sources = SourceIndex::default();
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "Example_Pariah".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("x.mod"),
            "PatchOperationAdd",
            r#"Defs/XenotypeDef[defName="Example_Pariah"]/genes"#,
            x_locator.clone(),
        )],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/XenotypeDef[defName="Example_Pariah"]/genes</xpath>
                 <value><li>GeneA</li></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    match &result.findings[0] {
        Finding::PatchWillFail {
            mod_id,
            cause,
            operation,
            leaf_xpath,
            ..
        } => {
            assert_eq!(*mod_id, ModId::new("x.mod"));
            assert_eq!(*cause, PatchFailureCause::DeadTarget);
            assert_eq!(
                operation,
                r#"Verse.PatchOperationAdd(Defs/XenotypeDef[defName="Example_Pariah"]/genes)"#
            );
            assert_eq!(
                leaf_xpath.as_deref(),
                Some(r#"Defs/XenotypeDef[defName="Example_Pariah"]/genes"#)
            );
        }
        other => panic!("expected PatchWillFail, got {other:?}"),
    }
    assert_eq!(result.defs_checked, 1);
    assert!(result.skipped.is_empty());
}

/// Row 1 of the zero-owner probe table, run through the real
/// `VerifyOrder::execute` entry point (not just `rim-merge`'s own unit
/// level): the zero-owner fast path's synthetic
/// `<{def_type}></{def_type}>` placeholder must not make the replayed def
/// look like it *exists* (an always-present root node, even though the
/// whole point of this fast path is that the def doesn't). Otherwise a
/// `Conditional` whose bare-def-head condition should read "false" reads
/// "true", taking the `<match>` branch and predicting the `Add` inside it
/// as a `DeadTarget` failure. Real RimWorld: the def doesn't exist, the
/// condition is false, it takes `<nomatch>` (absent here), finds nothing
/// to do, and logs nothing at all. Must predict zero findings.
#[test]
fn a_conditional_on_a_zero_owner_defs_own_bare_head_predicts_nothing() {
    let mut sources = SourceIndex::default();
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "Example_Pariah".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("x.mod"),
            "PatchOperationConditional",
            r#"Defs/XenotypeDef[defName="Example_Pariah"]"#,
            x_locator.clone(),
        )],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/XenotypeDef[defName="Example_Pariah"]</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/XenotypeDef[defName="Example_Pariah"]/genes</xpath>
                   <value><li>GeneA</li></value>
                 </match>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(
        result.findings.is_empty(),
        "a Conditional whose condition correctly reads false on a genuinely absent def must predict nothing (RimWorld logs nothing): {:?}",
        result.findings
    );
    assert_eq!(result.defs_checked, 1, "the def was still a real candidate");
}

/// Row 2 of the probe table: a bare top-level `Test` on a zero-owner
/// def's own head must not read `succeeded=true` from the placeholder's
/// fabricated existence, which would predict nothing — a false
/// *negative*: real RimWorld's own `PatchOperationTest` genuinely fails
/// (the def doesn't exist), and RimWorld logs "Patch operation ...
/// failed" for it. Must predict exactly one finding, naming the Test
/// itself.
#[test]
fn a_bare_test_on_a_zero_owner_defs_own_bare_head_predicts_a_failure() {
    let mut sources = SourceIndex::default();
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "Example_Pariah".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("x.mod"),
            "PatchOperationTest",
            r#"Defs/XenotypeDef[defName="Example_Pariah"]"#,
            x_locator.clone(),
        )],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationTest">
                 <xpath>Defs/XenotypeDef[defName="Example_Pariah"]</xpath>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    match &result.findings[0] {
        Finding::PatchWillFail {
            mod_id,
            operation,
            leaf_xpath,
            ..
        } => {
            assert_eq!(*mod_id, ModId::new("x.mod"));
            assert_eq!(
                operation,
                r#"Verse.PatchOperationTest(Defs/XenotypeDef[defName="Example_Pariah"])"#
            );
            assert_eq!(
                leaf_xpath.as_deref(),
                Some(r#"Defs/XenotypeDef[defName="Example_Pariah"]"#)
            );
        }
        other => panic!("expected PatchWillFail, got {other:?}"),
    }
}

/// The zero-owner fast path's own half: `zero_owner_outcomes` must not
/// discard `ReplayOutcome::error`, or a malformed or unsupported op on a
/// zero-owner def's own patch list silently truncates
/// `top_level_outcomes` with nothing recorded — an earlier unsupported op
/// means a later op on the same target is genuinely unverified (never
/// reached), not confirmed clean.
#[test]
fn a_zero_owner_defs_own_unsupported_op_is_recorded_as_skipped() {
    let mut sources = SourceIndex::default();
    let u_locator = locator("u_patch.xml", 0);
    let b_locator = locator("b_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "Example_Pariah".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &ModId::new("u.mod"),
                "Some.Totally.Unknown.Class",
                r#"Defs/XenotypeDef[defName="Example_Pariah"]/genes"#,
                u_locator.clone(),
            ),
            make_op(
                &ModId::new("b.mod"),
                "PatchOperationAdd",
                r#"Defs/XenotypeDef[defName="Example_Pariah"]/genes"#,
                b_locator.clone(),
            ),
        ],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        u_locator,
        r#"<Operation Class="Some.Totally.Unknown.Class"></Operation>"#.to_string(),
    );
    elements.insert(
        b_locator,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/XenotypeDef[defName="Example_Pariah"]/genes</xpath>
                 <value><li>GeneA</li></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("u.mod")
        .mod_("b.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["u.mod", "b.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(
        result.findings.is_empty(),
        "b.mod's own Add is never reached — must not be guessed at: {:?}",
        result.findings
    );
    assert_eq!(result.skipped.len(), 1, "{:?}", result.skipped);
    // `ReplayError::Unsupported`'s own `Display` names the xpath and
    // reason but not which mod shipped it (unlike `Stopper::Replay`,
    // the replayed path's own equivalent, which does) — a real,
    // disclosed asymmetry between the two paths' skip messages, left
    // as is because closing it needs a `ReplayError` shape change
    // reaching every existing caller. What this test checks is that the
    // truncation is recorded at all.
    let (skipped_def, skipped_selector, reason) = &result.skipped[0];
    assert_eq!(skipped_def.def_type, "XenotypeDef");
    assert_eq!(skipped_def.def_name, "Example_Pariah");
    assert_eq!(*skipped_selector, Selector::DefName);
    assert!(reason.contains("Some.Totally.Unknown.Class"), "{reason}");
}

/// Row 3 of the probe table: a `Sequence` wrapping
/// `[Test(bare def head), Add(.../genes)]` on a zero-owner def must not
/// name the `Add` as `lastFailedOperation=` — real RimWorld's own
/// `Sequence` stops at the *first* op that fails, and since the def
/// genuinely doesn't exist, that's the `Test`, so the `Add` is never
/// even reached. Naming the `Add` would break the log-grep contract
/// `operation` exists for. Must name the `Test`, never the `Add`.
#[test]
fn a_sequence_wrapping_a_bare_test_then_an_add_on_a_zero_owner_def_names_the_test_not_the_add() {
    let mut sources = SourceIndex::default();
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "Example_Pariah".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("x.mod"),
            "PatchOperationSequence",
            r#"Defs/XenotypeDef[defName="Example_Pariah"]"#,
            x_locator.clone(),
        )],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationTest">
                     <xpath>Defs/XenotypeDef[defName="Example_Pariah"]</xpath>
                   </li>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/XenotypeDef[defName="Example_Pariah"]/genes</xpath>
                     <value><li>GeneA</li></value>
                   </li>
                 </operations>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    match &result.findings[0] {
        Finding::PatchWillFail {
            operation,
            leaf_xpath,
            ..
        } => {
            assert_eq!(
                operation,
                r#"Verse.PatchOperationSequence(count=2, lastFailedOperation=Verse.PatchOperationTest(Defs/XenotypeDef[defName="Example_Pariah"]))"#,
                "must name the Test, which is what really stopped the sequence, never the Add it never reached"
            );
            assert_eq!(
                leaf_xpath.as_deref(),
                Some(r#"Defs/XenotypeDef[defName="Example_Pariah"]"#),
                "leaf_xpath must agree with the identity about which op actually failed"
            );
        }
        other => panic!("expected PatchWillFail, got {other:?}"),
    }
}

/// The desktop apply dialog's own progress bar depends on `on_progress`
/// being called at least once per candidate key, with a strictly
/// non-decreasing `checked`, ending exactly at `(total, total)`.
///
/// **`total` is stable here only because this fixture produces no
/// counterfactual job at all** (its one finding is a zero-owner
/// `DeadTarget`, which has no raw XML to re-permute): `total` grows
/// once, mid-stream, whenever the counterfactual phase has work —
/// `the_counterfactual_phase_extends_the_progress_total_once_mid_stream`
/// is that contract's own test.
#[test]
fn execute_with_progress_reports_a_monotonic_checked_out_of_a_stable_total() {
    let mut sources = SourceIndex::default();
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "Example_Pariah".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("x.mod"),
            "PatchOperationAdd",
            r#"Defs/XenotypeDef[defName="Example_Pariah"]/genes"#,
            x_locator.clone(),
        )],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/XenotypeDef[defName="Example_Pariah"]/genes</xpath>
                 <value><li>GeneA</li></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let mut calls: Vec<(usize, usize)> = Vec::new();
    let result = verify.execute_with_progress(&session, OrderSource::Current, &mut |c, t| {
        calls.push((c, t));
    });

    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    assert!(!calls.is_empty());
    assert_eq!(
        result.counterfactual.jobs, 0,
        "the premise of this test's own stable-total assertion below"
    );
    let total = calls[0].1;
    assert!(calls.iter().all(|(_, t)| *t == total), "{calls:?}");
    assert!(
        calls.windows(2).all(|w| w[0].0 <= w[1].0),
        "checked must never decrease: {calls:?}"
    );
    assert_eq!(*calls.last().expect("at least one call"), (total, total));
}

/// A `<success>Always</success>` compat patch targeting a def that
/// doesn't exist anywhere — a real, observed shape (a real content-pack mod's own
/// style guards its own patches this way, since the
/// target content pack may not be installed) — must not be predicted
/// as a failure, exactly the same success-suppression rule the
/// replayed path already gets.
#[test]
fn a_success_always_op_targeting_a_zero_owner_def_predicts_nothing() {
    let mut sources = SourceIndex::default();
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "Example_Pariah".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("x.mod"),
            "PatchOperationAdd",
            r#"Defs/XenotypeDef[defName="Example_Pariah"]/genes"#,
            x_locator.clone(),
        )],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationAdd">
                 <success>Always</success>
                 <xpath>Defs/XenotypeDef[defName="Example_Pariah"]/genes</xpath>
                 <value><li>GeneA</li></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(
        result.findings.is_empty(),
        "a Success.Always op must never be predicted as failing, even against a zero-owner target: {:?}",
        result.findings
    );
    assert_eq!(result.defs_checked, 1, "the def was still a real candidate");
}

#[test]
fn a_gated_off_op_targeting_a_zero_owner_def_predicts_nothing() {
    // A compat patch wraps its op in `PatchOperationFindMod(ModB)`, and
    // ModB isn't installed — the op never runs at all, so it can never
    // fail, regardless of whether its target also happens to have zero
    // owners (the "mod A patches mod B's defs, B not installed" case: a
    // patch target with zero owners *and* an unsatisfied gate at once).
    let mut sources = SourceIndex::default();
    let x_locator = locator("x_patch.xml", 0);
    let mut gated = make_op(
        &ModId::new("x.mod"),
        "PatchOperationAdd",
        r#"Defs/XenotypeDef[defName="Ghost"]/genes"#,
        x_locator.clone(),
    );
    gated.op.find_mod_context = vec![FindModGate::AnyActive(vec!["Mod B".to_string()])];
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "Ghost".to_string(),
            Selector::DefName,
        ),
        vec![gated],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationFindMod">
                 <mods><li>Mod B</li></mods>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/XenotypeDef[defName="Ghost"]/genes</xpath>
                   <value><li>GeneA</li></value>
                 </match>
               </Operation>"#
            .to_string(),
    );
    // `Mod B` is never active — the report names only `x.mod`.
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(
        result.findings.is_empty(),
        "a gated-off op must never be predicted as failing: {:?}",
        result.findings
    );
    assert_eq!(
        result.defs_checked, 0,
        "nothing active existed to check for this def"
    );
}

/// The owned counterpart of
/// [`a_gated_off_op_targeting_a_zero_owner_def_predicts_nothing`]: the def
/// has a real owner and raw XML, and its only patch op sits under a
/// `FindMod` gate that is closed. No operation can run, so the def is
/// neither replayed nor counted as checked, and nothing is listed as
/// skipped (a def that was never a candidate is not a def that could not
/// be checked).
#[test]
fn a_def_whose_only_op_is_gated_off_is_not_checked_and_not_skipped() {
    let mut sources = base_sources();
    let x_locator = locator("x_patch.xml", 0);
    let mut gated = make_op(
        &ModId::new("x.mod"),
        "PatchOperationAdd",
        r#"Defs/ThingDef[defName="Wall"]/comps"#,
        x_locator.clone(),
    );
    gated.op.find_mod_context = vec![FindModGate::AnyActive(vec!["Mod B".to_string()])];
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![gated],
    );
    let mut elements = elements();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationFindMod">
                 <mods><li>Mod B</li></mods>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="Wall"]/comps</xpath>
                   <value><li>CompA</li></value>
                 </match>
               </Operation>"#
            .to_string(),
    );
    // `Mod B` is never active: the report names only `core.mod` and `x.mod`.
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(result.findings.is_empty(), "{:?}", result.findings);
    assert_eq!(result.defs_checked, 0, "no operation ran against this def");
    assert!(result.skipped.is_empty(), "{:?}", result.skipped);
}

/// `gate_open` looks up every `<match>`/`<nomatch>` name lowercased, so
/// the map handed to it must be built the same way
/// (`build_gate_name_map`'s own doc comment has the full write-up). This
/// is the *positive* direction of
/// [`a_gated_off_op_targeting_a_zero_owner_def_predicts_nothing`]
/// above, which alone can pass identically whether the map is
/// lowercased or built with a bare verbatim `.collect()` (an
/// inactive mod's gate reads closed either way). Only an *active*,
/// satisfied gate can tell the two apart — and the mixed-case
/// display name is load-bearing: an all-lowercase fixture would also
/// pass with a verbatim map (it still happens to match an
/// already-lowercase name), proving nothing about the lowercasing.
#[test]
fn an_op_gated_on_an_active_mod_is_still_verified() {
    let mut sources = SourceIndex::default();
    let x_locator = locator("x_patch.xml", 0);
    let mut gated = make_op(
        &ModId::new("x.mod"),
        "PatchOperationAdd",
        r#"Defs/XenotypeDef[defName="Ghost"]/genes"#,
        x_locator.clone(),
    );
    gated.op.find_mod_context = vec![FindModGate::AnyActive(vec!["Mod B".to_string()])];
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "Ghost".to_string(),
            Selector::DefName,
        ),
        vec![gated],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationFindMod">
                 <mods><li>Mod B</li></mods>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/XenotypeDef[defName="Ghost"]/genes</xpath>
                   <value><li>GeneA</li></value>
                 </match>
               </Operation>"#
            .to_string(),
    );
    // `mod.b`'s own display name is mixed case on purpose — see this
    // test's own doc comment.
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .mod_with("mod.b", |m| m.name = "Mod B".to_string())
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod", "mod.b"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    match &result.findings[0] {
        Finding::PatchWillFail {
            mod_id, operation, ..
        } => {
            assert_eq!(*mod_id, ModId::new("x.mod"));
            assert_eq!(operation, "Verse.PatchOperationFindMod(Mod B)");
        }
        other => panic!("expected PatchWillFail, got {other:?}"),
    }
    assert_eq!(
        result.defs_checked, 1,
        "the gate was open, so this def was a real candidate"
    );
}

/// Gating checks every indexed child sharing a top-level node's own
/// identity, not only the shallowest one (`representative_op`, meant for
/// *display*) — otherwise a `Sequence` whose *first* child carries an
/// unsatisfied `MayRequire` while a *later* sibling has none would be
/// dropped wholesale, the later sibling never verified even though it
/// genuinely runs. Both indexed entries here share one top-level
/// identity (same mod, same file, same first `element_path` ordinal —
/// exactly how the analyzer indexes two leaf mutations inside one
/// `<operations>` list), the first gated on an inactive mod, the second
/// not.
#[test]
fn a_sequences_second_child_being_active_is_enough_even_when_the_first_is_gated_off() {
    let file: Arc<Path> = Arc::from(Path::new("s_patch.xml"));
    let mod_id = ModId::new("x.mod");
    let gated_child = IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
            xpath: Some(r#"Defs/XenotypeDef[defName="Ghost"]/comps"#.to_string()),
            target: Some(rim_analyzer::domain::DefTarget {
                def_type: "XenotypeDef".to_string(),
                def_name: "Ghost".to_string(),
                selector: Selector::DefName,
                sub_path: Some("comps".to_string()),
            }),
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: vec!["ghost.framework".to_string()],
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: BTreeSet::new(),
            injected_paths: BTreeSet::new(),
            conditional_xpath: None,
            // A `PatchOperationSequence`'s `<operations>` child is the one
            // shape the game actually reads MayRequire on.
            is_list_item: true,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: XmlLocator::new(file.clone(), vec![0, 0]),
        },
    };
    let active_child = IndexedPatchOp {
        mod_id: mod_id.clone(),
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
            xpath: Some(r#"Defs/XenotypeDef[defName="Ghost"]/genes"#.to_string()),
            target: Some(rim_analyzer::domain::DefTarget {
                def_type: "XenotypeDef".to_string(),
                def_name: "Ghost".to_string(),
                selector: Selector::DefName,
                sub_path: Some("genes".to_string()),
            }),
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: BTreeSet::new(),
            injected_paths: BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: true,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: XmlLocator::new(file.clone(), vec![0, 1]),
        },
    };
    let mut sources = SourceIndex::default();
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "Ghost".to_string(),
            Selector::DefName,
        ),
        vec![gated_child, active_child],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        // `top_level_operations` builds the top-level locator from
        // just `[first_ordinal]` (`0` here) — matching that, not the
        // deeper per-child paths above, since replay reads the whole
        // `<Operation>` node's own text starting there.
        XmlLocator::new(file, vec![0]),
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationAdd" MayRequire="ghost.framework">
                     <xpath>Defs/XenotypeDef[defName="Ghost"]/comps</xpath>
                     <value><li>CompGhost</li></value>
                   </li>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/XenotypeDef[defName="Ghost"]/genes</xpath>
                     <value><li>GeneA</li></value>
                   </li>
                 </operations>
               </Operation>"#
            .to_string(),
    );
    // `ghost.framework` (the first child's own `MayRequire`) is never
    // active — only `x.mod` is.
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(
        result.defs_checked, 1,
        "the second, active child must still make this def a real candidate"
    );
    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    match &result.findings[0] {
        Finding::PatchWillFail {
            operation,
            leaf_xpath,
            ..
        } => {
            assert_eq!(
                operation,
                r#"Verse.PatchOperationSequence(count=2, lastFailedOperation=Verse.PatchOperationAdd(Defs/XenotypeDef[defName="Ghost"]/genes))"#,
                "the gated-off first child never runs at all (`gates_open` skips it, treated as succeeded), so the Sequence's own real failure is the second, active child"
            );
            assert_eq!(
                leaf_xpath.as_deref(),
                Some(r#"Defs/XenotypeDef[defName="Ghost"]/genes"#)
            );
        }
        other => panic!("expected PatchWillFail, got {other:?}"),
    }
}

/// A def that exists only
/// via a whole-def `<xpath>Defs</xpath>` patch injection — no literal
/// `Defs/**/*.xml` source, so it appears in
/// `SourceIndex::injected_def_owners` but never `owners_by_def` — is
/// real, so a foreign patcher targeting it must never be predicted
/// `DeadTarget` (`has_any_owner` consults both maps, not
/// `owners_by_def` alone). It's also not replayable (no raw source to
/// read), so the honest outcome is neither a finding nor silence —
/// it must show up in `skipped`, naming why.
#[test]
fn a_patch_target_naming_an_injected_only_def_is_neither_dead_target_nor_silently_dropped() {
    let mut sources = SourceIndex::default();
    sources.injected_def_owners.insert(
        ("XenotypeDef".to_string(), "InjectedGhost".to_string()),
        vec![ModId::new("injector.mod")],
    );
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "XenotypeDef".to_string(),
            "InjectedGhost".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("x.mod"),
            "PatchOperationAdd",
            r#"Defs/XenotypeDef[defName="InjectedGhost"]/genes"#,
            x_locator.clone(),
        )],
    );
    let mut elements = BTreeMap::new();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/XenotypeDef[defName="InjectedGhost"]/genes</xpath>
                 <value><li>GeneA</li></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("injector.mod")
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["injector.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(
        result.findings.is_empty(),
        "an injected-only def must never be predicted DeadTarget: {:?}",
        result.findings
    );
    assert_eq!(
        result.skipped.len(),
        1,
        "an injected-only def with an active foreign patcher must be recorded as skipped, not silently dropped: {:?}",
        result.skipped
    );
    assert_eq!(result.skipped[0].0.def_name, "InjectedGhost");
}

#[test]
fn a_field_never_present_on_the_winner_is_dead_target() {
    let mut sources = base_sources();
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("x.mod"),
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/neverExisted"#,
            x_locator.clone(),
        )],
    );
    let mut elements = elements();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Wall"]/neverExisted</xpath>
                 <value><neverExisted>1</neverExisted></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    match &result.findings[0] {
        Finding::PatchWillFail { cause, .. } => {
            assert_eq!(*cause, PatchFailureCause::DeadTarget);
        }
        other => panic!("expected PatchWillFail, got {other:?}"),
    }
}

/// The same shape as
/// `a_field_never_present_on_the_winner_is_dead_target`, but the
/// op is wrapped in `<success>Always</success>`, on the ordinary
/// (non-zero-owner) replayed path: this idiom is very common on real
/// installs, and a leaf matching nothing under it must never surface
/// as a finding.
#[test]
fn a_success_always_op_against_a_real_owner_predicts_nothing() {
    let mut sources = base_sources();
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("x.mod"),
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/neverExisted"#,
            x_locator.clone(),
        )],
    );
    let mut elements = elements();
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <success>Always</success>
                 <xpath>Defs/ThingDef[defName="Wall"]/neverExisted</xpath>
                 <value><neverExisted>1</neverExisted></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(
        result.findings.is_empty(),
        "a Success.Always op must never be predicted as failing: {:?}",
        result.findings
    );
}

#[test]
fn an_earlier_remover_with_a_removed_node_edge_is_removed_by() {
    let mut sources = base_sources();
    let r_locator = locator("r_patch.xml", 0);
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &ModId::new("r.mod"),
                "PatchOperationRemove",
                r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#,
                r_locator.clone(),
            ),
            make_op(
                &ModId::new("x.mod"),
                "PatchOperationReplace",
                r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#,
                x_locator.clone(),
            ),
        ],
    );
    let mut elements = elements();
    elements.insert(
        r_locator,
        r#"<Operation Class="PatchOperationRemove">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
               </Operation>"#
            .to_string(),
    );
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
                 <value><MaxHitPoints>200</MaxHitPoints></value>
               </Operation>"#
            .to_string(),
    );
    // `r.mod` loads *before* `x.mod` here — the real cause of the
    // failure — while the removed-node edge (found elsewhere, at scan time)
    // says the opposite must hold for `x.mod`'s own op to succeed.
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("r.mod")
        .mod_("x.mod")
        .edge("r.mod", "x.mod", EdgeKind::PatchRemovedNode, true)
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "r.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    match &result.findings[0] {
        Finding::PatchWillFail { mod_id, cause, .. } => {
            assert_eq!(*mod_id, ModId::new("x.mod"));
            assert_eq!(*cause, PatchFailureCause::RemovedBy(ModId::new("r.mod")));
        }
        other => panic!("expected PatchWillFail, got {other:?}"),
    }
}

/// An earlier patcher's own `Unsupported` op stops the whole fold
/// (`effective::compute` never skips a stopper and continues), and that
/// truncation must be visible here: the later patcher's own `Add`
/// (which, unreached, is neither confirmed nor ruled out — matching
/// nothing on this synthetic tree tells us nothing about whether it
/// would have run at all) must not count as verified, the def must not
/// count as fully checked, and `skipped` must hear about it, per
/// `VerifyOrderReport::skipped`'s own contract. Must predict nothing for
/// the unreached op and record the stopper.
#[test]
fn a_stopper_from_an_earlier_unsupported_op_is_recorded_as_skipped_not_silently_fully_checked() {
    let mut sources = base_sources();
    let u_locator = locator("u_patch.xml", 0);
    let b_locator = locator("b_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &ModId::new("u.mod"),
                "Some.Totally.Unknown.Class",
                r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#,
                u_locator.clone(),
            ),
            make_op(
                &ModId::new("b.mod"),
                "PatchOperationAdd",
                r#"Defs/ThingDef[defName="Wall"]/comps"#,
                b_locator.clone(),
            ),
        ],
    );
    let mut elements = elements();
    // No `<xpath>`, no `<operations>`/`<match>`/`<operation>`
    // structure, and not one of `patch_eval`'s known custom classes —
    // falls through every recognized shape to `Unsupported`, exactly
    // `rim-merge`'s own `an_unsupported_contribution_stops_the_fold...`
    // fixture shape.
    elements.insert(
        u_locator,
        r#"<Operation Class="Some.Totally.Unknown.Class"></Operation>"#.to_string(),
    );
    elements.insert(
        b_locator,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="Wall"]/comps</xpath>
                 <value><li>CompFixture</li></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("u.mod")
        .mod_("b.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "u.mod", "b.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(
        result.findings.is_empty(),
        "b.mod's own Add is never reached at all — must not be guessed at either way: {:?}",
        result.findings
    );
    assert_eq!(result.skipped.len(), 1, "{:?}", result.skipped);
    let (skipped_def, skipped_selector, reason) = &result.skipped[0];
    assert_eq!(skipped_def.def_type, "ThingDef");
    assert_eq!(skipped_def.def_name, "Wall");
    assert_eq!(*skipped_selector, Selector::DefName);
    assert!(reason.contains("u.mod"), "{reason}");
    assert_eq!(
        result.defs_checked, 1,
        "still a genuine, if partial, attempt"
    );
}

#[test]
fn a_later_injector_with_an_injected_node_edge_is_not_yet_injected() {
    let mut sources = base_sources();
    let s_locator = locator("s_patch.xml", 0);
    let a_locator = locator("a_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &ModId::new("s.mod"),
                "PatchOperationReplace",
                r#"Defs/ThingDef[defName="Wall"]/comps/li[@Class="X"]"#,
                s_locator.clone(),
            ),
            make_op(
                &ModId::new("a.mod"),
                "PatchOperationAdd",
                r#"Defs/ThingDef[defName="Wall"]/comps"#,
                a_locator.clone(),
            ),
        ],
    );
    let mut elements = elements();
    elements.insert(
        s_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Wall"]/comps/li[@Class="X"]</xpath>
                 <value><li Class="X"><amount>2</amount></li></value>
               </Operation>"#
            .to_string(),
    );
    elements.insert(
        a_locator,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="Wall"]/comps</xpath>
                 <value><li Class="X"><amount>1</amount></li></value>
               </Operation>"#
            .to_string(),
    );
    // `s.mod` loads *before* `a.mod` — the node `s.mod` selects isn't
    // injected yet — while the injected-node edge says the selector must load
    // after the injector.
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("s.mod")
        .mod_("a.mod")
        .edge("s.mod", "a.mod", EdgeKind::PatchInjectedNode, true)
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "s.mod", "a.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(result.findings.len(), 1, "{:?}", result.findings);
    match &result.findings[0] {
        Finding::PatchWillFail { mod_id, cause, .. } => {
            assert_eq!(*mod_id, ModId::new("s.mod"));
            assert_eq!(
                *cause,
                PatchFailureCause::NotYetInjected(ModId::new("a.mod"))
            );
        }
        other => panic!("expected PatchWillFail, got {other:?}"),
    }
}

#[test]
fn a_reordered_current_source_no_longer_predicts_the_same_failure() {
    // The exact `an_earlier_remover_with_a_removed_node_edge_is_removed_by`
    // fixture, but with `x.mod` loaded *before* `r.mod` — the real
    // order the removed-node edge itself asks for — so `x.mod`'s own replace
    // runs first and the fold never reaches a failure at all.
    let mut sources = base_sources();
    let r_locator = locator("r_patch.xml", 0);
    let x_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![
            make_op(
                &ModId::new("r.mod"),
                "PatchOperationRemove",
                r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#,
                r_locator.clone(),
            ),
            make_op(
                &ModId::new("x.mod"),
                "PatchOperationReplace",
                r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#,
                x_locator.clone(),
            ),
        ],
    );
    let mut elements = elements();
    elements.insert(
        r_locator,
        r#"<Operation Class="PatchOperationRemove">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
               </Operation>"#
            .to_string(),
    );
    elements.insert(
        x_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
                 <value><MaxHitPoints>200</MaxHitPoints></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .mod_("r.mod")
        .edge("r.mod", "x.mod", EdgeKind::PatchRemovedNode, true)
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "x.mod", "r.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(
        result.findings.is_empty(),
        "expected no predicted failure once x.mod loads before r.mod: {:?}",
        result.findings
    );
    assert_eq!(result.defs_checked, 1, "the def is still a real candidate");
}

/// Testing `classify_cause` directly, not through the full
/// `VerifyOrder::execute` pipeline: constructing a genuine `Unknown`
/// case end to end needs a `Caveat::FailedOp` whose own target is
/// *not* absent from the resolved tree (so `DeadTarget`'s own check
/// correctly doesn't fire) and has no removed-node/injected-node edge (so neither
/// order-fixable cause fires either) — real `patch_eval::replay`
/// itself never actually produces `FailedOp` for a target that both
/// exists *and* has no explaining edge in this crate's own test
/// fixtures (every hand-built "op finds nothing" shape this module
/// tried either resolves to a genuinely absent node — `DeadTarget`,
/// correctly — or falls outside the strict xpath grammar entirely,
/// which stops the fold before any `Caveat` at all, not just before
/// `FieldPath` conversion). Calling the classifier directly with a
/// resolved tree that genuinely already has the target proves the
/// `Unknown` fallback itself, without needing to reverse-engineer a
/// real replay sequence that reaches it.
#[test]
fn a_present_target_with_no_explaining_edge_is_unknown() {
    let resolved = rim_merge::xml::parse(
        "<ThingDef><statBases><MaxHitPoints>100</MaxHitPoints></statBases></ThingDef>",
    )
    .expect("valid xml");
    let xpath = r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#;
    let indexed = vec![make_op(
        &ModId::new("x.mod"),
        "PatchOperationReplace",
        xpath,
        locator("x_patch.xml", 0),
    )];
    let order = LoadOrder::new(vec![ModId::new("core.mod"), ModId::new("x.mod")]);
    let edges = EdgeEvidence {
        removed_node: BTreeSet::new(),
        injected_node: BTreeSet::new(),
    };
    let other_patchers: BTreeSet<ModId> = BTreeSet::new();

    let cause = classify_cause(
        &ModId::new("x.mod"),
        xpath,
        &indexed,
        &other_patchers,
        &order,
        &edges,
        &resolved,
    );

    assert_eq!(cause, PatchFailureCause::Unknown);
}

/// A def only its own mod patches is replayed too (an OR-ed head can name
/// it); an op that succeeds there predicts nothing.
#[test]
fn a_def_only_its_own_mod_patches_is_replayed_and_a_succeeding_op_predicts_nothing() {
    let mut sources = base_sources();
    let core_patch_locator = locator("core_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![make_op(
            &ModId::new("core.mod"),
            "PatchOperationReplace",
            r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#,
            core_patch_locator.clone(),
        )],
    );
    let mut elements = elements();
    elements.insert(
        core_patch_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
                 <value><MaxHitPoints>150</MaxHitPoints></value>
               </Operation>"#
            .to_string(),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(result.findings.is_empty());
    assert_eq!(result.defs_checked, 1);
}

/// OR-head aggregation, first half: one physical OR-headed op, indexed
/// under two def keys (`WallA`, `WallB`) exactly as `SourceIndex::build`
/// indexes a real OR-ed defName head. It succeeds against `WallA`'s own
/// raw content and fails against `WallB`'s — RimWorld itself would log
/// nothing (the query matched `WallA`), so this must predict **nothing**,
/// not one finding for `WallB` as a per-def-key replay would.
#[test]
fn an_or_headed_op_succeeding_on_one_examined_def_key_predicts_nothing() {
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("core.mod"),
        "WallA",
        true,
    );
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("core.mod"),
        "WallB",
        false,
    );
    let head = or_head_xpath(&["WallA", "WallB"]);
    let op_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "WallA".to_string(),
            Selector::DefName,
        ),
        vec![make_or_head_op(
            &ModId::new("x.mod"),
            "WallA",
            &head,
            op_locator.clone(),
        )],
    );
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "WallB".to_string(),
            Selector::DefName,
        ),
        vec![make_or_head_op(
            &ModId::new("x.mod"),
            "WallB",
            &head,
            op_locator.clone(),
        )],
    );
    elements.insert(op_locator, or_head_op_xml(&["WallA", "WallB"]));
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(
        result.findings.is_empty(),
        "an OR-headed op succeeding on one examined def key must suppress the whole \
             group, including its own failure against another: {:?}",
        result.findings
    );
}

/// OR-head aggregation, second half: the same shape, but the op fails
/// against *every* def key it's examined under (neither `WallA` nor
/// `WallB` carries the targeted field) — RimWorld itself would log
/// this failure, so both rows must still be predicted, one per def
/// key (aggregation removes whole groups, it never merges rows).
#[test]
fn an_or_headed_op_failing_on_every_examined_def_key_still_predicts_every_one() {
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("core.mod"),
        "WallA",
        false,
    );
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("core.mod"),
        "WallB",
        false,
    );
    let head = or_head_xpath(&["WallA", "WallB"]);
    let op_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "WallA".to_string(),
            Selector::DefName,
        ),
        vec![make_or_head_op(
            &ModId::new("x.mod"),
            "WallA",
            &head,
            op_locator.clone(),
        )],
    );
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "WallB".to_string(),
            Selector::DefName,
        ),
        vec![make_or_head_op(
            &ModId::new("x.mod"),
            "WallB",
            &head,
            op_locator.clone(),
        )],
    );
    elements.insert(op_locator, or_head_op_xml(&["WallA", "WallB"]));
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(result.findings.len(), 2, "{:?}", result.findings);
    let def_names: BTreeSet<String> = result
        .findings
        .iter()
        .map(|finding| match finding {
            Finding::PatchWillFail { def_key, .. } => def_key.def_name.clone(),
            other => panic!("expected PatchWillFail, got {other:?}"),
        })
        .collect();
    assert_eq!(
        def_names,
        BTreeSet::from(["WallA".to_string(), "WallB".to_string()])
    );
}

/// The buffer aggregates across **both** replay paths — one op
/// straddling both whenever some of its OR-ed defs exist and some
/// don't. `WallA` has a real owner (the replayed path, via
/// `effective::compute`) and succeeds; `WallGhost` has zero owners
/// anywhere (the zero-owner fast path, via `zero_owner_outcomes`
/// against a synthetic placeholder) and fails, since the placeholder
/// never carries the targeted field. The success on one path must
/// still suppress the failure recorded on the other.
#[test]
fn an_or_headed_op_straddling_the_zero_owner_and_replayed_paths_is_aggregated_together() {
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("core.mod"),
        "WallA",
        true,
    );
    // `WallGhost` deliberately gets no `sources.defs`/`owners_by_def`
    // entry at all — the zero-owner fast path's own precondition.
    let head = or_head_xpath(&["WallA", "WallGhost"]);
    let op_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "WallA".to_string(),
            Selector::DefName,
        ),
        vec![make_or_head_op(
            &ModId::new("x.mod"),
            "WallA",
            &head,
            op_locator.clone(),
        )],
    );
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "WallGhost".to_string(),
            Selector::DefName,
        ),
        vec![make_or_head_op(
            &ModId::new("x.mod"),
            "WallGhost",
            &head,
            op_locator.clone(),
        )],
    );
    elements.insert(op_locator, or_head_op_xml(&["WallA", "WallGhost"]));
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert!(
        result.findings.is_empty(),
        "a success on the replayed path must suppress a failure recorded for the same \
             op on the zero-owner path: {:?}",
        result.findings
    );
}

/// "The one that must not be got wrong": a def key that failed, or was
/// never examined, must never read as a success. `WallSelf` is owned by
/// `x.mod` and its only patcher is `x.mod` itself; it lacks the field
/// too, so the group stays failed and `WallA`'s finding must not vanish.
#[test]
fn an_or_headed_op_failing_on_a_self_patched_def_key_too_is_still_predicted() {
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("core.mod"),
        "WallA",
        false,
    );
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("x.mod"),
        "WallSelf",
        false,
    );
    let head = or_head_xpath(&["WallA", "WallSelf"]);
    let op_locator = locator("x_patch.xml", 0);
    for name in ["WallA", "WallSelf"] {
        sources.patch_ops_by_def.insert(
            ("ThingDef".to_string(), name.to_string(), Selector::DefName),
            vec![make_or_head_op(
                &ModId::new("x.mod"),
                name,
                &head,
                op_locator.clone(),
            )],
        );
    }
    elements.insert(op_locator, or_head_op_xml(&["WallA", "WallSelf"]));
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(
        failing_def_names(&result),
        BTreeSet::from(["WallA".to_string(), "WallSelf".to_string()]),
        "{:?}",
        result.findings
    );
}

/// The locator recovery (`top_level.iter().zip(...)`) must be
/// length-checked, not assumed. `WallA`'s own replay is truncated by
/// an earlier, unrelated `Unsupported` stopper before it ever reaches
/// the OR-headed op — so that contribution is never reached, never
/// paired with an outcome, and must never be misread as either a
/// success (which would wrongly suppress `WallB`'s own real failure,
/// exactly the trap a mis-aligned or unchecked zip could fall into)
/// or a failure (which would double-count it against `WallB`'s own
/// single genuine row).
#[test]
fn a_truncated_replay_never_credits_its_own_unreached_or_headed_op_as_examined() {
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("core.mod"),
        "WallA",
        false,
    );
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("core.mod"),
        "WallB",
        false,
    );
    let head = or_head_xpath(&["WallA", "WallB"]);
    let stopper_locator = locator("stopper_patch.xml", 0);
    let op_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "WallA".to_string(),
            Selector::DefName,
        ),
        vec![
            // A `Replace` on the def root itself stays `Unsupported` —
            // this stops `WallA`'s whole fold before `x.mod`'s own
            // OR-headed op (ordered after it) is ever reached.
            make_op(
                &ModId::new("stopper.mod"),
                "PatchOperationReplace",
                r#"Defs/ThingDef[defName="WallA"]"#,
                stopper_locator.clone(),
            ),
            make_or_head_op(&ModId::new("x.mod"), "WallA", &head, op_locator.clone()),
        ],
    );
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "WallB".to_string(),
            Selector::DefName,
        ),
        vec![make_or_head_op(
            &ModId::new("x.mod"),
            "WallB",
            &head,
            op_locator.clone(),
        )],
    );
    elements.insert(
        stopper_locator,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="WallA"]</xpath>
                 <value><y>2</y></value>
               </Operation>"#
            .to_string(),
    );
    elements.insert(op_locator, or_head_op_xml(&["WallA", "WallB"]));
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("stopper.mod")
        .mod_("x.mod")
        .build();
    let session =
        session_with_sources_and_mods(sources, report, &["core.mod", "stopper.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(
        result.findings.len(),
        1,
        "the truncated, never-reached contribution under WallA must contribute no \
             evidence at all — WallB's own genuine failure is the only row: {:?}",
        result.findings
    );
    match &result.findings[0] {
        Finding::PatchWillFail { def_key, .. } => {
            assert_eq!(def_key.def_name, "WallB");
        }
        other => panic!("expected PatchWillFail, got {other:?}"),
    }
    assert!(
        result
            .skipped
            .iter()
            .any(|(def_key, _, _)| def_key.def_name == "WallA"),
        "WallA's own stopper must still be recorded as skipped: {:?}",
        result.skipped
    );
}

// -- completion of failed OR-headed groups ----------------------------

/// One OR-headed op by `x.mod` over `SelfDef` (owned and patched only by
/// `x.mod`, so the candidate filter never replays it) and `Missing` (owned
/// by nobody, so it fails on the zero-owner path). The shape of a mod that
/// patches `[defName="Other" or defName="Own"]` with `Other` belonging to
/// a mod that is not installed.
fn self_patched_or_head_findings(self_def: Option<bool>) -> VerifyOrderReport {
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    let mut named = vec!["Missing"];
    if let Some(has_field) = self_def {
        register_owned_def(
            &mut sources,
            &mut elements,
            &ModId::new("x.mod"),
            "SelfDef",
            has_field,
        );
        named.insert(0, "SelfDef");
    }
    let head = or_head_xpath(&named);
    let op_locator = locator("x_patch.xml", 0);
    for name in &named {
        sources.patch_ops_by_def.insert(
            (
                "ThingDef".to_string(),
                (*name).to_string(),
                Selector::DefName,
            ),
            vec![make_or_head_op(
                &ModId::new("x.mod"),
                name,
                &head,
                op_locator.clone(),
            )],
        );
    }
    elements.insert(op_locator, or_head_op_xml(&named));
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    VerifyOrder::new(InMemoryDefSourceReader::new(elements)).execute(&session, OrderSource::Current)
}

fn failing_def_names(report: &VerifyOrderReport) -> BTreeSet<String> {
    report
        .findings
        .iter()
        .map(|finding| match finding {
            Finding::PatchWillFail { def_key, .. } => def_key.def_name.clone(),
            other => panic!("expected PatchWillFail, got {other:?}"),
        })
        .collect()
}

/// The alternative that exists (`SelfDef` has the targeted field) makes
/// RimWorld's single evaluation of the OR-ed head succeed, so the op never
/// fails, even though `SelfDef` is a def only its own mod patches and
/// `Missing` fails on the zero-owner path.
#[test]
fn an_or_head_alternative_alive_on_a_self_patched_def_suppresses_the_failure() {
    let result = self_patched_or_head_findings(Some(true));

    assert!(result.findings.is_empty(), "{:?}", result.findings);
}

/// Negative control: the self-patched alternative lacks the field too, so
/// the op really fails and both def targets stay predicted.
#[test]
fn an_or_head_whose_self_patched_alternative_also_lacks_the_node_is_still_predicted() {
    let result = self_patched_or_head_findings(Some(false));

    assert_eq!(
        failing_def_names(&result),
        BTreeSet::from(["Missing".to_string(), "SelfDef".to_string()])
    );
}

/// Negative control: every alternative absent.
#[test]
fn an_or_head_whose_every_alternative_is_absent_is_still_predicted() {
    let result = self_patched_or_head_findings(None);

    assert_eq!(
        failing_def_names(&result),
        BTreeSet::from(["Missing".to_string()])
    );
}

/// A def only its own mod patches, whose op fails, is an author bug the
/// game logs too, so it is predicted like any other failing op.
#[test]
fn a_self_patched_def_whose_own_op_fails_is_predicted() {
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("x.mod"),
        "SelfDef",
        false,
    );
    let head = or_head_xpath(&["SelfDef"]);
    let op_locator = locator("x_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "SelfDef".to_string(),
            Selector::DefName,
        ),
        vec![make_or_head_op(
            &ModId::new("x.mod"),
            "SelfDef",
            &head,
            op_locator.clone(),
        )],
    );
    elements.insert(op_locator, or_head_op_xml(&["SelfDef"]));
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(
        failing_def_names(&result),
        BTreeSet::from(["SelfDef".to_string()])
    );
}

/// The Sequence shape: its first child is the OR-headed op whose alive
/// alternative is a self-patched def, its second a single-def leaf that
/// really fails. The first leaf's group is cleared and the second's is not.
#[test]
fn a_sequences_later_failing_leaf_stays_predicted_when_its_or_leaf_is_cleared() {
    let file: Arc<Path> = Arc::from(Path::new("x_patch.xml"));
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("x.mod"),
        "SelfDef",
        true,
    );
    register_owned_def(
        &mut sources,
        &mut elements,
        &ModId::new("core.mod"),
        "Lonely",
        false,
    );
    let or_head = or_head_xpath(&["SelfDef", "Missing"]);
    let single_head = "Defs/ThingDef[defName=\"Lonely\"]/neverExisted".to_string();
    let or_locator = XmlLocator::new(file.clone(), vec![0, 0]);
    let single_locator = XmlLocator::new(file.clone(), vec![0, 1]);
    let mod_id = ModId::new("x.mod");
    for name in ["SelfDef", "Missing"] {
        sources.patch_ops_by_def.insert(
            ("ThingDef".to_string(), name.to_string(), Selector::DefName),
            vec![make_or_head_op(&mod_id, name, &or_head, or_locator.clone())],
        );
    }
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Lonely".to_string(),
            Selector::DefName,
        ),
        vec![make_or_head_op(
            &mod_id,
            "Lonely",
            &single_head,
            single_locator,
        )],
    );
    elements.insert(
        XmlLocator::new(file, vec![0]),
        format!(
            r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationReplace">
                     <xpath>{or_head}</xpath>
                     <value><neverExisted>2</neverExisted></value>
                   </li>
                   <li Class="PatchOperationReplace">
                     <xpath>{single_head}</xpath>
                     <value><neverExisted>2</neverExisted></value>
                   </li>
                 </operations>
               </Operation>"#
        ),
    );
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    assert_eq!(
        failing_def_names(&result),
        BTreeSet::from(["Lonely".to_string()]),
        "{:?}",
        result.findings
    );
}

// -- reorder_conflicts ------------------------------------------------

/// A bare [`rim_resolve::sort::SortOutcome`] carrying only `order`
/// and `dropped` — the two fields [`reorder_conflicts`] reads. Every
/// other field is the type's own empty/default value; nothing here
/// exercises the sorter itself.
fn sort_outcome(
    order: LoadOrder,
    dropped: Vec<rim_resolve::sort::DroppedEdge>,
) -> rim_resolve::sort::SortOutcome {
    rim_resolve::sort::SortOutcome {
        order,
        placements: std::collections::BTreeMap::new(),
        dropped,
        any_of_choices: Vec::new(),
        warnings: Vec::new(),
        stats: rim_resolve::sort::DisturbanceStats::default(),
    }
}

/// A [`rim_resolve::sort::DroppedEdge`] naming just `after`/`before`/
/// `kind` — `witness_cycle`/`winner` are never read by
/// [`reorder_conflicts`].
fn dropped_edge(after: &str, before: &str, kind: EdgeKind) -> rim_resolve::sort::DroppedEdge {
    rim_resolve::sort::DroppedEdge {
        edge: rim_resolve::sort::OrderingEdge {
            after: ModId::new(after),
            before: ModId::new(before),
            layer: rim_resolve::sort::Layer::Inferred,
            provenance: rim_resolve::sort::EdgeProvenance::Engine {
                kind,
                detail: format!("{kind:?}"),
            },
        },
        witness_cycle: vec![ModId::new(after), ModId::new(before)],
        winner: None,
    }
}

#[test]
fn a_reorder_with_no_matching_edge_has_no_conflicts() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a.mod")
        .mod_("b.mod")
        .build();
    let outcome = sort_outcome(
        LoadOrder::new(vec![ModId::new("a.mod"), ModId::new("b.mod")]),
        Vec::new(),
    );

    let conflicts = reorder_conflicts(
        &report,
        &outcome,
        &ModId::new("b.mod"),
        &ModId::new("a.mod"),
    );

    assert!(conflicts.is_empty());
}

/// An edge in the *opposite* direction from the proposed reorder is a
/// conflict regardless of its own status — here it's satisfied under
/// `order`, mirroring the real `example.architect` case (a
/// declared `loadAfter` the reorder would reverse).
#[test]
fn an_opposite_direction_satisfied_edge_is_a_conflict() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a.mod")
        .mod_("b.mod")
        .declared_edge("a.mod", "b.mod")
        .build();
    let outcome = sort_outcome(
        LoadOrder::new(vec![ModId::new("b.mod"), ModId::new("a.mod")]),
        Vec::new(),
    );

    // Proposed: b.mod after a.mod — the opposite of the stored
    // "a.mod after b.mod" declared edge, which `order` satisfies.
    let conflicts = reorder_conflicts(
        &report,
        &outcome,
        &ModId::new("b.mod"),
        &ModId::new("a.mod"),
    );

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, EdgeKind::LoadAfter);
    assert_eq!(
        conflicts[0].status,
        rim_analyzer::domain::EdgeStatus::Satisfied
    );
    assert_eq!(conflicts[0].direction, ReorderConflictDirection::Reverses);
}

/// A same-direction edge the sorter's own `outcome.dropped` actually
/// dropped is a conflict too: enforcing the proposed reorder would
/// just re-assert a direction the sorter already tried and had to
/// drop — real `example.progression.arsenal` case's `patch_removed_node`
/// half.
#[test]
fn a_same_direction_dropped_edge_is_a_conflict() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a.mod")
        .mod_("b.mod")
        .edge("a.mod", "b.mod", EdgeKind::PatchRemovedNode, true)
        .build();
    // `order` has a.mod loading before b.mod, which violates an edge
    // requiring a.mod after b.mod — and the sorter's own outcome
    // records that it genuinely dropped that edge.
    let outcome = sort_outcome(
        LoadOrder::new(vec![ModId::new("a.mod"), ModId::new("b.mod")]),
        vec![dropped_edge("a.mod", "b.mod", EdgeKind::PatchRemovedNode)],
    );

    let conflicts = reorder_conflicts(
        &report,
        &outcome,
        &ModId::new("a.mod"),
        &ModId::new("b.mod"),
    );

    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].kind, EdgeKind::PatchRemovedNode);
    assert_eq!(
        conflicts[0].status,
        rim_analyzer::domain::EdgeStatus::Violated
    );
    assert_eq!(conflicts[0].direction, ReorderConflictDirection::ReAsserts);
}

/// A same-direction edge that `order` merely violates, without the
/// sorter ever having dropped it, is **not** a conflict — an
/// `Awareness`-strength edge like `PatchSelectsInjectedNode` is advisory
/// and violated by the hundreds under any real order, with no cycle
/// involved at all. Flagging one here would falsely tell the user a
/// free pair rule was contested.
#[test]
fn a_same_direction_violated_edge_the_sorter_never_dropped_is_not_a_conflict() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a.mod")
        .mod_("b.mod")
        .edge("a.mod", "b.mod", EdgeKind::PatchSelectsInjectedNode, true)
        .build();
    // Violates the edge (a.mod loads before b.mod), but `dropped` is
    // empty — this edge was never in the graph to begin with, or was
    // simply never enforced, not "dropped to break a cycle".
    let outcome = sort_outcome(
        LoadOrder::new(vec![ModId::new("a.mod"), ModId::new("b.mod")]),
        Vec::new(),
    );

    let conflicts = reorder_conflicts(
        &report,
        &outcome,
        &ModId::new("a.mod"),
        &ModId::new("b.mod"),
    );

    assert!(conflicts.is_empty());
}

/// An edge in the opposite direction that `order` currently
/// *violates* is still a conflict, and its own (violated) status is
/// reported through — the "opposite" rule has no status qualifier at
/// all, unlike the same-direction rule.
#[test]
fn an_opposite_direction_violated_edge_is_a_conflict_with_its_own_status() {
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("a.mod")
        .mod_("b.mod")
        .declared_edge("a.mod", "b.mod")
        .build();
    // `order` has a.mod before b.mod, which violates the stored
    // "a.mod after b.mod" edge (unlike the satisfied-edge test above,
    // which orders them the other way).
    let outcome = sort_outcome(
        LoadOrder::new(vec![ModId::new("a.mod"), ModId::new("b.mod")]),
        Vec::new(),
    );

    // Proposed: b.mod after a.mod — still the opposite of the stored
    // "a.mod after b.mod" edge, so the "opposite" rule (no status
    // qualifier) is what catches it, not the same-direction rule.
    let conflicts = reorder_conflicts(
        &report,
        &outcome,
        &ModId::new("b.mod"),
        &ModId::new("a.mod"),
    );

    assert_eq!(conflicts.len(), 1);
    assert_eq!(
        conflicts[0].status,
        rim_analyzer::domain::EdgeStatus::Violated
    );
    assert_eq!(conflicts[0].direction, ReorderConflictDirection::Reverses);
}

fn predicted_failure(operation: &str) -> Finding {
    Finding::PatchWillFail {
        mod_id: ModId::new("x.mod"),
        def_key: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: Selector::DefName,
        operation: operation.to_string(),
        leaf_xpath: None,
        cause: PatchFailureCause::DeadTarget,
        reorder_kind: None,
    }
}

fn op_key(file: &str) -> super::counterfactual::TopLevelOpKey {
    (
        ModId::new("x.mod"),
        Arc::from(Path::new(file)),
        0,
        String::new(),
    )
}

fn skip_row(def_name: &str) -> (rim_resolve::domain::DefKey, Selector, String) {
    (
        rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: def_name.to_string(),
        },
        Selector::DefName,
        format!("{def_name}: skipped"),
    )
}

/// The tally of two def keys, examined one after the other.
fn tallies_of_two_keys() -> (PassTally, PassTally) {
    let mut first = PassTally::default();
    first
        .pending
        .insert(op_key("Shared.xml"), vec![Some(predicted_failure("first"))]);
    first.pending.insert(op_key("OnlyFirst.xml"), vec![None]);
    first.skipped.push(skip_row("First"));
    first.defs_checked = 1;
    first.suppressed_filter_head_ops = 2;

    let mut second = PassTally::default();
    second.pending.insert(
        op_key("Shared.xml"),
        vec![None, Some(predicted_failure("second"))],
    );
    second.skipped.push(skip_row("Second"));
    second.defs_checked = 1;
    second.suppressed_filter_head_ops = 3;
    (first, second)
}

#[test]
fn absorbing_per_key_tallies_in_key_order_keeps_every_outcome_in_examination_order() {
    let (first, second) = tallies_of_two_keys();
    let mut combined = PassTally::default();

    combined.absorb(first);
    combined.absorb(second);

    assert_eq!(
        combined.pending,
        BTreeMap::from([
            (op_key("OnlyFirst.xml"), vec![None]),
            (
                op_key("Shared.xml"),
                vec![
                    Some(predicted_failure("first")),
                    None,
                    Some(predicted_failure("second")),
                ],
            ),
        ])
    );
    assert_eq!(
        combined.skipped,
        vec![skip_row("First"), skip_row("Second")]
    );
    assert_eq!(combined.defs_checked, 2);
    assert_eq!(combined.suppressed_filter_head_ops, 5);
}

#[test]
fn absorbing_into_a_non_empty_tally_appends_after_what_it_already_holds() {
    let (first, second) = tallies_of_two_keys();
    let mut combined = first;

    combined.absorb(second);

    assert_eq!(
        combined.pending[&op_key("Shared.xml")],
        vec![
            Some(predicted_failure("first")),
            None,
            Some(predicted_failure("second")),
        ]
    );
    assert_eq!(
        combined.skipped,
        vec![skip_row("First"), skip_row("Second")]
    );
}

/// The def keys are examined in parallel, so this pins that their outcomes
/// still come out in def-key order — the order one sequential pass produces.
#[test]
fn an_op_failing_on_several_def_keys_reports_them_in_def_key_order() {
    let def_names = ["WallA", "WallB", "WallC", "WallD", "WallE"];
    let mut sources = SourceIndex::default();
    let mut elements = BTreeMap::new();
    let head = or_head_xpath(&def_names);
    let op_locator = locator("x_patch.xml", 0);
    for def_name in def_names {
        register_owned_def(
            &mut sources,
            &mut elements,
            &ModId::new("core.mod"),
            def_name,
            false,
        );
        sources.patch_ops_by_def.insert(
            (
                "ThingDef".to_string(),
                def_name.to_string(),
                Selector::DefName,
            ),
            vec![make_or_head_op(
                &ModId::new("x.mod"),
                def_name,
                &head,
                op_locator.clone(),
            )],
        );
    }
    elements.insert(op_locator, or_head_op_xml(&def_names));
    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("x.mod")
        .build();
    let session = session_with_sources_and_mods(sources, report, &["core.mod", "x.mod"]);
    let verify = VerifyOrder::new(InMemoryDefSourceReader::new(elements));

    let result = verify.execute(&session, OrderSource::Current);

    let reported: Vec<String> = result
        .findings
        .iter()
        .map(|finding| match finding {
            Finding::PatchWillFail { def_key, .. } => def_key.def_name.clone(),
            other => panic!("expected PatchWillFail, got {other:?}"),
        })
        .collect();
    assert_eq!(reported, def_names.map(str::to_string));
}

/// Each verify opens one call view of its reader and reads only through
/// it, the replay and the counterfactual phase alike, so a file reader
/// checks each file on disk once per verify (`DefSourceReader::call_view`).
#[test]
fn each_verify_reads_through_one_call_view_of_its_reader() {
    let (sources, elements) = counterfactual_fixture(
        "<ThingDef><defName>Wall</defName><container></container></ThingDef>",
        &[
            (
                "b.mod",
                "PatchOperationAdd",
                "container/injected",
                r#"<Operation Class="PatchOperationAdd">
                         <xpath>Defs/ThingDef[defName="Wall"]/container/injected</xpath>
                         <value><deep>2</deep></value>
                       </Operation>"#
                    .to_string(),
            ),
            (
                "c.mod",
                "PatchOperationAdd",
                "container",
                r#"<Operation Class="PatchOperationAdd">
                         <xpath>Defs/ThingDef[defName="Wall"]/container</xpath>
                         <value><injected><inner>1</inner></injected></value>
                       </Operation>"#
                    .to_string(),
            ),
        ],
    );
    let session = counterfactual_session(sources, &["core.mod", "b.mod", "c.mod"]);
    let reader = CallCountingReader::new(InMemoryDefSourceReader::new(elements));
    let verify = VerifyOrder::new(&reader);

    let first = verify.execute(&session, OrderSource::Current);
    let reads_in_first = reader.reads_through_views();
    let second = verify.execute(&session, OrderSource::Current);

    assert_eq!(first.counterfactual.jobs, 1, "both phases ran");
    assert_eq!(first.findings, second.findings);
    assert!(reads_in_first > 0);
    assert_eq!(reader.views_opened(), 2, "one view per verify");
    assert_eq!(reader.reads_through_views(), 2 * reads_in_first);
    assert_eq!(reader.direct_reads(), 0);
}
