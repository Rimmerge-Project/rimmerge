//! Tests for the replay evaluator.

use super::*;
use crate::patch_behaviours::{ClassBehaviour, ClassMatch, GateFields};
use crate::patch_behaviours::{ClassGate, ConditionalKind, CustomBehaviour, GateBehaviour};
use crate::tree::{Content, FieldNode};
use crate::xml;

fn ctx() -> (BTreeSet<ModId>, BTreeMap<String, ModId>) {
    let mut active = BTreeSet::new();
    active.insert(ModId::new("mod.a"));
    active.insert(ModId::new("mod.b"));
    let names = BTreeMap::new();
    (active, names)
}

/// Invented class names, one per modelled [`CustomBehaviour`], mapped
/// through a gate whose own element vocabulary is likewise invented.
///
/// **This crate's own tests never name a real third-party class**:
/// which real class has which behaviour is data, and lives in the
/// `rules` repo (`rules/data/patch-operations.json`, embedded into
/// `rim-io` at compile time — see `crates/rim-io/build.rs`). The
/// behaviours themselves — what the tests below actually pin — are
/// code, and invented names exercise them exactly as well. That the
/// bundled map still reproduces today's behaviour on the real
/// classes is pinned separately, by `tests/replay_parity.rs`, which
/// reads that file rather than restating it.
fn example_behaviours() -> PatchOperationBehaviours {
    PatchOperationBehaviours::new(
        BTreeMap::from([
            (
                "Example.PatchOperationSetModExtension".to_string(),
                ClassBehaviour {
                    match_kind: ClassMatch::Suffix,
                    behaviour: CustomBehaviour::SetModExtension,
                },
            ),
            (
                "Example.PatchOperationAddOrReplace".to_string(),
                ClassBehaviour {
                    match_kind: ClassMatch::Suffix,
                    behaviour: CustomBehaviour::AddOrReplace,
                },
            ),
            (
                "Example.PatchOperationReplaceResearchCoords".to_string(),
                ClassBehaviour {
                    match_kind: ClassMatch::Suffix,
                    behaviour: CustomBehaviour::ReplaceResearchCoords,
                },
            ),
        ]),
        vec![ClassGate {
            class: "Example.".to_string(),
            match_kind: ClassMatch::Prefix,
            behaviour: GateBehaviour::ModsLoadedGate,
            fields: GateFields {
                requires_all: "doesRequire".to_string(),
                conditional_type: "conditionalType".to_string(),
                conditional_param: "conditionalParam".to_string(),
            },
            conditional_types: BTreeMap::from([
                ("Cond_ModsLoaded".to_string(), ConditionalKind::AllLoaded),
                (
                    "Cond_ModsNotLoaded".to_string(),
                    ConditionalKind::NoneLoaded,
                ),
            ]),
        }],
    )
}

fn replay_one_using(
    tree_xml: &str,
    operation_xml: &str,
    def_type: &str,
    def_name: &str,
    behaviours: &PatchOperationBehaviours,
) -> ReplayOutcome {
    let tree = xml::parse(tree_xml).unwrap();
    let (active, names) = ctx();
    let mod_id = ModId::new("mod.a");
    let context = ReplayContext {
        active_mods: &active,
        mod_names_by_display: &names,
        def_type,
        def_name,
        selector: Selector::DefName,
        def_exists: def_existence_unknown(),
        this_def_present: true,
        behaviours,
    };
    replay(
        tree,
        &[PatchContribution {
            mod_id: &mod_id,
            operation_xml,
        }],
        &context,
    )
}

fn replay_one_for(
    tree_xml: &str,
    operation_xml: &str,
    def_type: &str,
    def_name: &str,
) -> ReplayOutcome {
    replay_one_using(
        tree_xml,
        operation_xml,
        def_type,
        def_name,
        PatchOperationBehaviours::none(),
    )
}

fn replay_one(tree_xml: &str, operation_xml: &str) -> ReplayOutcome {
    replay_one_for(tree_xml, operation_xml, "ThingDef", "W")
}

#[test]
fn patch_operation_add_appends_to_every_matched_node() {
    let outcome = replay_one(
        r#"<ThingDef><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none());
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    assert_eq!(children.len(), 2);
    assert_eq!(children[1].tag, "y");
}

#[test]
fn patch_operation_add_honours_prepend_order() {
    let outcome = replay_one(
        r#"<ThingDef><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                 <order>Prepend</order>
                 <value><y>2</y></value>
               </Operation>"#,
    );
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    assert_eq!(children[0].tag, "y");
    assert_eq!(children[1].tag, "x");
}

#[test]
fn patch_operation_add_matching_zero_nodes_records_a_failed_op_caveat() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="W"]/nope</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
    );
    assert!(matches!(&outcome.caveats[0], Caveat::FailedOp { .. }));
}

// -- Group B: head-position child-value predicates ------------------
// (A real install's badge-fork shape,
// `ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]/comps`.)

#[test]
fn head_content_predicate_add_applies_when_the_current_defs_content_matches() {
    let outcome = replay_one_for(
        r#"<ExampleRace.ThingDef_ExampleRace>
                 <race><intelligence>Humanlike</intelligence></race>
                 <comps><x>1</x></comps>
               </ExampleRace.ThingDef_ExampleRace>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]/comps</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
        "ExampleRace.ThingDef_ExampleRace",
        "Human",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let comps = outcome.tree.get(&"comps".parse().unwrap()).unwrap();
    let Content::Children(children) = &comps.content else {
        unreachable!()
    };
    assert_eq!(children.len(), 2);
    assert_eq!(children[1].tag, "y");
    assert!(outcome.top_level_outcomes[0].succeeded);
}

/// The current def's own content does not satisfy the predicate
/// (`intelligence` is `ToolUser`, not `Humanlike`) — the op is a
/// no-op for *this* def, "succeeded elsewhere" (RimWorld's own
/// selector simply didn't choose this def's node), never a failure
/// or an `Unsupported`.
#[test]
fn head_content_predicate_add_is_a_no_op_when_the_current_defs_content_does_not_match() {
    let outcome = replay_one_for(
        r#"<ExampleRace.ThingDef_ExampleRace>
                 <race><intelligence>ToolUser</intelligence></race>
                 <comps><x>1</x></comps>
               </ExampleRace.ThingDef_ExampleRace>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]/comps</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
        "ExampleRace.ThingDef_ExampleRace",
        "Human",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty());
    let comps = outcome.tree.get(&"comps".parse().unwrap()).unwrap();
    let Content::Children(children) = &comps.content else {
        unreachable!()
    };
    assert_eq!(children.len(), 1, "the op must not have touched this def");
    assert!(outcome.top_level_outcomes[0].succeeded);
}

/// A head naming a different def *type* entirely never applies here,
/// whatever the current def's own content happens to contain —
/// mirrors the ordinary name-identifying head's own
/// `target.def_type == context.def_type` check.
#[test]
fn head_content_predicate_add_is_a_no_op_for_a_different_def_type() {
    let outcome = replay_one_for(
        r#"<ThingDef>
                 <race><intelligence>Humanlike</intelligence></race>
                 <comps><x>1</x></comps>
               </ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]/comps</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
        "ThingDef",
        "Human",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let comps = outcome.tree.get(&"comps".parse().unwrap()).unwrap();
    let Content::Children(children) = &comps.content else {
        unreachable!()
    };
    assert_eq!(children.len(), 1, "a different def type must never match");
}

/// A `PatchOperationConditional` whose own `<xpath>` is this shape,
/// not matching the current def, falls back to the conservative
/// branch walk exactly like any other `Unsupported` test — moot
/// (`Ok(true)`, no error) when neither branch could affect this def,
/// never a silently wrong `false`. Proves `cross_def_existence`'s new
/// `targets.is_empty()` refusal reaches `Unsupported` rather than
/// asserting the condition doesn't hold, and that the branch walk still
/// recovers from it the same way it already does for other unresolvable
/// cross-def tests.
#[test]
fn head_content_predicate_conditional_not_matching_this_def_is_moot_when_branches_cannot_affect_it()
{
    let outcome = replay_one_for(
        r#"<ExampleRace.ThingDef_ExampleRace>
                 <race><intelligence>ToolUser</intelligence></race>
               </ExampleRace.ThingDef_ExampleRace>"#,
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]/comps</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/SomeOtherDef[defName="Other"]/comps</xpath>
                   <value><y>2</y></value>
                 </match>
               </Operation>"#,
        "ExampleRace.ThingDef_ExampleRace",
        "Human",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.top_level_outcomes[0].succeeded);
}

// -- Group B takes the filter-head outcome path -------------------------
// (ground-truthed against a real-install false positive: a compat
// patch's `PatchOperationConditional` on a `thingClass="Pawn"`-style
// content head decisively answered `false` on a def whose own `comps`
// happened to be empty, wrongly predicting the `nomatch` branch's
// injection would run.)

/// The content predicate matches this def, but the narrower sub-path
/// under it (`/comps`) is empty on this particular def — that is no
/// evidence the op failed (some *other* matching def may well have
/// `comps`), so it must be suppressed exactly like an empty
/// `head_filter_predicate` match: no `Caveat::FailedOp`, "succeeded
/// elsewhere", counted in `suppressed_filter_head_ops`.
#[test]
fn group_b_head_empty_on_this_def_is_not_a_failure() {
    let outcome = replay_one_for(
        r#"<ExampleRace.ThingDef_Pawn><thingClass>Pawn</thingClass></ExampleRace.ThingDef_Pawn>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ExampleRace.ThingDef_Pawn[thingClass="Pawn"]/comps</xpath>
                 <value><li>Injected</li></value>
               </Operation>"#,
        "ExampleRace.ThingDef_Pawn",
        "Example",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(
        outcome.caveats.is_empty(),
        "a global query's empty match on one def is no evidence of failure: {:?}",
        outcome.caveats
    );
    assert!(outcome.top_level_outcomes[0].succeeded);
    assert_eq!(outcome.suppressed_filter_head_ops, 1);
}

/// The mirror of [`group_b_head_empty_on_this_def_is_not_a_failure`]: a
/// non-empty match still mutates normally, same as before this fix —
/// `head_content_predicate_add_applies_when_the_current_defs_content_matches`
/// already pins the same behaviour for `PatchOperationAdd`; this pins it
/// for `PatchOperationConditional`'s own `match`/`nomatch` choice.
#[test]
fn group_b_conditional_nonempty_takes_match() {
    let outcome = replay_one_for(
        r#"<ExampleRace.ThingDef_Pawn>
                 <thingClass>Pawn</thingClass>
                 <comps><x>1</x></comps>
               </ExampleRace.ThingDef_Pawn>"#,
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/ExampleRace.ThingDef_Pawn[thingClass="Pawn"]/comps</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ExampleRace.ThingDef_Pawn[defName="Example"]/comps</xpath>
                   <value><y>2</y></value>
                 </match>
                 <nomatch Class="PatchOperationAdd">
                   <xpath>Defs/ExampleRace.ThingDef_Pawn[defName="Example"]</xpath>
                   <value><shouldnotrun>1</shouldnotrun></value>
                 </nomatch>
               </Operation>"#,
        "ExampleRace.ThingDef_Pawn",
        "Example",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let comps = outcome.tree.get(&"comps".parse().unwrap()).unwrap();
    let Content::Children(children) = &comps.content else {
        unreachable!()
    };
    assert_eq!(children.len(), 2);
    assert_eq!(children[1].tag, "y");
    assert!(
        outcome.tree.get(&"shouldnotrun".parse().unwrap()).is_none(),
        "the nomatch branch must not have run"
    );
}

/// **Regression for the real-install false positive.** The content
/// predicate matches this def (`thingClass="Pawn"`), but its own
/// `/comps` is absent — before this fix, `condition_matches` decisively
/// answered `false` and ran `nomatch`, wrongly injecting a node no real
/// game run ever creates. The `nomatch` branch's own op targets this
/// exact def by name, so `branches_can_affect_this_def` cannot call it
/// moot either: the whole operation must come back `Unsupported`
/// (from the conservative branch walk, since it *could* affect this
/// def), never a silently successful `nomatch`.
#[test]
fn group_b_conditional_empty_is_unsupported_not_nomatch() {
    let outcome = replay_one_for(
        r#"<ExampleRace.ThingDef_Pawn><thingClass>Pawn</thingClass></ExampleRace.ThingDef_Pawn>"#,
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/ExampleRace.ThingDef_Pawn[thingClass="Pawn"]/comps</xpath>
                 <nomatch Class="PatchOperationAdd">
                   <xpath>Defs/ExampleRace.ThingDef_Pawn[defName="Example"]</xpath>
                   <value><comps><li>Injected</li></comps></value>
                 </nomatch>
               </Operation>"#,
        "ExampleRace.ThingDef_Pawn",
        "Example",
    );
    assert!(
        matches!(outcome.error, Some(ReplayError::Unsupported { .. })),
        "a Group B head with no local match must never resolve the Conditional decisively \
         false and silently run its nomatch branch: {:?}",
        outcome.error
    );
    assert!(
        outcome.tree.get(&"comps".parse().unwrap()).is_none(),
        "the nomatch branch must never have run"
    );
}

/// An empty Group B match inside a `PatchOperationSequence` must not
/// abort it — it is "succeeded elsewhere", not a
/// `Caveat::FailedOp`-bearing failure, so the sequence's later member
/// still runs.
#[test]
fn group_b_failure_inside_sequence_does_not_abort_it() {
    let outcome = replay_one_for(
        r#"<ExampleRace.ThingDef_Pawn>
                 <thingClass>Pawn</thingClass>
                 <statBases><x>1</x></statBases>
               </ExampleRace.ThingDef_Pawn>"#,
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/ExampleRace.ThingDef_Pawn[thingClass="Pawn"]/comps</xpath>
                     <value><li>Injected</li></value>
                   </li>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/ExampleRace.ThingDef_Pawn[defName="Example"]/statBases</xpath>
                     <value><y>2</y></value>
                   </li>
                 </operations>
               </Operation>"#,
        "ExampleRace.ThingDef_Pawn",
        "Example",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty(), "{:?}", outcome.caveats);
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    // The second member ran: the first member's own empty Group B match
    // never counted as a sequence-aborting failure.
    assert_eq!(children.len(), 2);
}

/// Sanity check: an ordinary `defName`-`or` head is **not** Group B (it
/// names its defs, so its per-def answer is exact) and stays unaffected
/// — an empty match still records a real `Caveat::FailedOp`, and nothing
/// is suppressed.
#[test]
fn defname_or_head_is_unaffected() {
    let outcome = replay_one_for(
        r#"<ThingDef><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="Example" or defName="Other"]/nope</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
        "ThingDef",
        "Example",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(matches!(&outcome.caveats[0], Caveat::FailedOp { .. }));
    assert_eq!(outcome.suppressed_filter_head_ops, 0);
}

#[test]
fn patch_operation_insert_prepends_when_order_says_so() {
    let outcome = replay_one(
        r#"<ThingDef><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationInsert">
                 <xpath>Defs/ThingDef[defName="W"]/statBases/x</xpath>
                 <order>Prepend</order>
                 <value><before>0</before></value>
               </Operation>"#,
    );
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    assert_eq!(children[0].tag, "before");
    assert_eq!(children[1].tag, "x");
}

#[test]
fn patch_operation_remove_removes_matched_nodes() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationRemove">
                 <xpath>Defs/ThingDef[defName="W"]/statBases/x</xpath>
               </Operation>"#,
    );
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    assert_eq!(stat_bases.content, Content::Children(Vec::new()));
    assert!(outcome.caveats.is_empty());
}

#[test]
fn patch_operation_remove_matching_zero_nodes_records_a_failed_op_caveat() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationRemove">
                 <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
               </Operation>"#,
    );
    assert!(matches!(&outcome.caveats[0], Caveat::FailedOp { .. }));
}

// -- Whole-def `Remove` is modelled -----------------------------------

/// A `Remove` whose resolved selection is exactly the def root empties
/// the tree and discloses `Caveat::DefRemoved` instead of `Unsupported`
/// — the one class `reject_root_target` does not refuse.
#[test]
fn patch_operation_remove_on_the_def_root_is_modelled_not_refused() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationRemove">
                 <xpath>Defs/ThingDef[defName="W"]</xpath>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(outcome.def_removed_by, Some(ModId::new("mod.a")));
    assert!(
        matches!(&outcome.caveats[0], Caveat::DefRemoved { mod_id } if *mod_id == ModId::new("mod.a")),
        "{:?}",
        outcome.caveats
    );
    assert!(outcome.top_level_outcomes[0].succeeded);
    assert_eq!(outcome.tree.root.content, Content::Empty);
}

/// The rule's narrowness: `Replace`/`Insert`/`SetName` on the def node stay
/// `Unsupported` — only `Remove` can express "this node is gone"
/// without the enclosing `<Defs>` document.
#[test]
fn patch_operation_replace_on_the_def_root_stays_unsupported() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="W"]</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
    );
    assert!(
        matches!(&outcome.error,
            Some(ReplayError::Unsupported { reason, .. })
                if reason.contains("PatchOperationReplace")
        ),
        "{:?}",
        outcome.error
    );
}

/// `patch_eval::replay`'s own internal loop (a multi-contribution
/// call, as `plan::plan_patch_collision` makes) threads the removal
/// into later contributions too, not only `effective::compute`'s own
/// outer fold — see `effective.rs`'s
/// `a_whole_def_remove_followed_by_a_later_op_on_the_same_def_is_modelled`
/// for the cross-call shape seen on real installs.
#[test]
fn patch_operation_remove_on_the_def_root_blocks_a_later_contribution_in_the_same_replay_call() {
    let tree = xml::parse("<ThingDef><statBases><x>1</x></statBases></ThingDef>").unwrap();
    let (active, names) = ctx();
    let remover = ModId::new("mod.remover");
    let later = ModId::new("mod.later");
    let context = ReplayContext {
        active_mods: &active,
        mod_names_by_display: &names,
        def_type: "ThingDef",
        def_name: "W",
        selector: Selector::DefName,
        def_exists: def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };
    let outcome = replay(
        tree,
        &[
            PatchContribution {
                mod_id: &remover,
                operation_xml: r#"<Operation Class="PatchOperationRemove">
                         <xpath>Defs/ThingDef[defName="W"]</xpath>
                       </Operation>"#,
            },
            PatchContribution {
                mod_id: &later,
                operation_xml: r#"<Operation Class="PatchOperationAdd">
                         <xpath>Defs/ThingDef[defName="W"]</xpath>
                         <value><y>2</y></value>
                       </Operation>"#,
            },
        ],
        &context,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(outcome.def_removed_by, Some(remover));
    assert_eq!(outcome.top_level_outcomes.len(), 2);
    assert!(outcome.top_level_outcomes[0].succeeded);
    assert!(
        !outcome.top_level_outcomes[1].succeeded,
        "the later mod's own Add must fail once the def is gone"
    );
}

#[test]
fn patch_operation_replace_replaces_the_matched_node() {
    let outcome = replay_one_for(
        "<BiomeDef><plantDensity>0.65</plantDensity></BiomeDef>",
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/BiomeDef[defName="W"]/plantDensity</xpath>
                 <value><plantDensity>0.9</plantDensity></value>
               </Operation>"#,
        "BiomeDef",
        "W",
    );
    let density = outcome.tree.get(&"plantDensity".parse().unwrap()).unwrap();
    assert_eq!(density.content, Content::Text("0.9".to_string()));
}

#[test]
fn patch_operation_attribute_set_and_add_and_remove() {
    let set = replay_one(
        r#"<ThingDef><statBases MayRequire="x"><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationAttributeSet">
                 <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                 <attribute>MayRequire</attribute>
                 <value>y</value>
               </Operation>"#,
    );
    let stat_bases = set.tree.get(&"statBases".parse().unwrap()).unwrap();
    assert_eq!(
        stat_bases.attrs.get("MayRequire").map(String::as_str),
        Some("y")
    );

    let removed = replay_one(
        r#"<ThingDef><statBases MayRequire="x"><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationAttributeRemove">
                 <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                 <attribute>MayRequire</attribute>
               </Operation>"#,
    );
    let stat_bases = removed.tree.get(&"statBases".parse().unwrap()).unwrap();
    assert!(!stat_bases.attrs.contains_key("MayRequire"));

    let added_skips_existing = replay_one(
        r#"<ThingDef><statBases MayRequire="x"><y>1</y></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationAttributeAdd">
                 <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                 <attribute>MayRequire</attribute>
                 <value>z</value>
               </Operation>"#,
    );
    let stat_bases = added_skips_existing
        .tree
        .get(&"statBases".parse().unwrap())
        .unwrap();
    assert_eq!(
        stat_bases.attrs.get("MayRequire").map(String::as_str),
        Some("x")
    );
}

#[test]
fn patch_operation_set_name_renames_the_matched_node() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationSetName">
                 <xpath>Defs/ThingDef[defName="W"]/statBases/x</xpath>
                 <name>renamed</name>
               </Operation>"#,
    );
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    assert_eq!(children[0].tag, "renamed");
}

#[test]
fn patch_operation_add_mod_extension_creates_the_container_when_absent() {
    let outcome = replay_one(
        "<ThingDef></ThingDef>",
        r#"<Operation Class="PatchOperationAddModExtension">
                 <xpath>Defs/ThingDef[defName="W"]</xpath>
                 <value><li Class="Foo"/></value>
               </Operation>"#,
    );
    let mod_extensions = outcome.tree.get(&"modExtensions".parse().unwrap()).unwrap();
    let Content::Children(children) = &mod_extensions.content else {
        unreachable!()
    };
    assert_eq!(children.len(), 1);
    assert_eq!(
        children[0].attrs.get("Class").map(String::as_str),
        Some("Foo")
    );
}

#[test]
fn patch_operation_add_mod_extension_matching_zero_nodes_fails_without_injecting_root() {
    let outcome = replay_one(
        "<ThingDef></ThingDef>",
        r#"<Operation Class="PatchOperationAddModExtension">
                 <xpath>Defs/ThingDef[defName="W"]/nope</xpath>
                 <value><li Class="Foo"/></value>
               </Operation>"#,
    );
    assert!(
        outcome
            .tree
            .get(&"modExtensions".parse().unwrap())
            .is_none()
    );
    assert!(matches!(&outcome.caveats[0], Caveat::FailedOp { .. }));
}

#[test]
fn a_sequence_stops_after_its_first_failing_member() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationRemove">
                     <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
                   </li>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                     <value><y>2</y></value>
                   </li>
                 </operations>
               </Operation>"#,
    );
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    // The second (Add) operation never runs — the sequence stopped
    // after the first (Remove) one failed to match.
    assert_eq!(children.len(), 1);
    assert_eq!(outcome.caveats.len(), 1);
}

#[test]
fn a_failure_inside_a_nested_sequence_propagates_to_the_outer_one() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationSequence">
                     <operations>
                       <li Class="PatchOperationRemove">
                         <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
                       </li>
                     </operations>
                   </li>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                     <value><y>2</y></value>
                   </li>
                 </operations>
               </Operation>"#,
    );
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    // The inner sequence's own failure propagates outward, so the
    // outer sequence's second member (the Add) never runs either.
    assert_eq!(children.len(), 1);
}

#[test]
fn conditional_runs_match_when_the_xpath_currently_matches() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/ThingDef[defName="W"]/statBases/x</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><matched>true</matched></value>
                 </match>
                 <nomatch Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><matched>false</matched></value>
                 </nomatch>
               </Operation>"#,
    );
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    assert_eq!(children.last().unwrap().tag, "matched");
    assert_eq!(
        children.last().unwrap().content,
        Content::Text("true".to_string())
    );
}

#[test]
fn conditional_runs_nomatch_when_the_xpath_currently_does_not_match() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><matched>true</matched></value>
                 </match>
                 <nomatch Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><matched>false</matched></value>
                 </nomatch>
               </Operation>"#,
    );
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    assert_eq!(
        children.last().unwrap().content,
        Content::Text("false".to_string())
    );
}

#[test]
fn a_conditional_targeting_a_different_def_is_unsupported() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/ThingDef[defName="SomeOtherDef"]/statBases/x</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><matched>true</matched></value>
                 </match>
               </Operation>"#,
    );
    assert!(outcome.error.is_some());
}

#[test]
fn a_mutation_targeting_a_different_def_succeeds_elsewhere_without_being_applied() {
    // Same Sequence carries an op for a different def and one for
    // ours — the foreign one must be skipped, not applied and not
    // treated as a failure.
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationReplace">
                     <xpath>Defs/ThingDef[defName="SomeOtherDef"]/statBases/x</xpath>
                     <value><x>999</x></value>
                   </li>
                   <li Class="PatchOperationReplace">
                     <xpath>Defs/ThingDef[defName="W"]/statBases/x</xpath>
                     <value><x>2</x></value>
                   </li>
                 </operations>
               </Operation>"#,
    );
    assert!(outcome.error.is_none());
    assert!(outcome.caveats.is_empty());
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    assert_eq!(children[0].content, Content::Text("2".to_string()));
}

#[test]
fn find_mod_runs_match_when_a_named_mod_is_active() {
    let tree = xml::parse("<ThingDef><statBases><x>1</x></statBases></ThingDef>").unwrap();
    let mut active = BTreeSet::new();
    active.insert(ModId::new("mod.a"));
    let mut names = BTreeMap::new();
    names.insert("Example Combat Mod".to_string(), ModId::new("mod.a"));
    let mod_id = ModId::new("mod.a");
    let context = ReplayContext {
        active_mods: &active,
        mod_names_by_display: &names,
        def_type: "ThingDef",
        def_name: "W",
        selector: Selector::DefName,
        def_exists: def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };
    let outcome = replay(
        tree,
        &[PatchContribution {
            mod_id: &mod_id,
            operation_xml: r#"<Operation Class="PatchOperationFindMod">
                     <mods><li>Example Combat Mod</li></mods>
                     <match Class="PatchOperationAdd">
                       <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                       <value><ce>true</ce></value>
                     </match>
                   </Operation>"#,
        }],
        &context,
    );
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    assert_eq!(children.last().unwrap().tag, "ce");
}

#[test]
fn a2_custom_sequence_like_class_runs_at_its_declared_default_enabled() {
    let outcome = replay_one_for(
        "<BiomeDef><plantDensity>0.65</plantDensity></BiomeDef>",
        r#"<Operation Class="Example.PatchOperationToggableSequence">
                 <enabled>True</enabled>
                 <operations>
                   <li Class="PatchOperationReplace">
                     <xpath>Defs/BiomeDef[defName="W"]/plantDensity</xpath>
                     <value><plantDensity>0.9</plantDensity></value>
                   </li>
                 </operations>
               </Operation>"#,
        "BiomeDef",
        "W",
    );
    let density = outcome.tree.get(&"plantDensity".parse().unwrap()).unwrap();
    assert_eq!(density.content, Content::Text("0.9".to_string()));
    assert!(matches!(
        &outcome.caveats[0],
        Caveat::ModSettingDefault { .. }
    ));
}

#[test]
fn a2_custom_sequence_like_class_skips_when_declared_default_is_disabled() {
    let outcome = replay_one_for(
        "<BiomeDef><plantDensity>0.65</plantDensity></BiomeDef>",
        r#"<Operation Class="Example.PatchOperationModOption">
                 <defaultValue>False</defaultValue>
                 <operations>
                   <li Class="PatchOperationReplace">
                     <xpath>Defs/BiomeDef[defName="W"]/plantDensity</xpath>
                     <value><plantDensity>0.9</plantDensity></value>
                   </li>
                 </operations>
               </Operation>"#,
        "BiomeDef",
        "W",
    );
    let density = outcome.tree.get(&"plantDensity".parse().unwrap()).unwrap();
    assert_eq!(density.content, Content::Text("0.65".to_string()));
}

#[test]
fn an_unsupported_xpath_aborts_the_whole_replay() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="W"]/statBases/x[contains(foo,"bar")]</xpath>
                 <value><x>2</x></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_some());
}

#[test]
fn an_unknown_class_without_operations_is_unsupported() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="Example.PatchOperationResearchPrereq">
                 <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
               </Operation>"#,
    );
    assert!(outcome.error.is_some());
}

#[test]
fn a_contribution_that_is_not_well_formed_xml_is_a_malformed_operation_error() {
    let outcome = replay_one("<ThingDef></ThingDef>", "<Operation Class=");
    assert!(matches!(
        outcome.error,
        Some(ReplayError::MalformedOperation { .. })
    ));
    assert!(matches!(
        &outcome.caveats[0],
        Caveat::MalformedOperation { .. }
    ));
}

/// A contribution's own operation XML, mod-provided and untrusted, is
/// parsed directly by `roxmltree::Document::parse` — a recursive-descent
/// parser that can overflow the stack on pathologically deep nesting
/// **before** it would ever return the ordinary parse-failure branch
/// above, or before `crate::xml::MAX_DEPTH`'s later, post-parse check
/// could run. Run on a 1 MiB stack (the CLI's own main thread size): the
/// real attack shape (a ~300 KB Workshop patch, 50,000 nested elements)
/// must come back as the ordinary `MalformedOperation` outcome, not crash
/// the process.
#[test]
fn a_pathologically_deep_contribution_is_rejected_before_parsing_not_crashed() {
    let mut operation_xml = concat!(
        r#"<Operation Class="PatchOperationAdd">"#,
        r#"<xpath>Defs/ThingDef[defName="W"]/statBases</xpath><value>"#,
    )
    .to_string();
    for _ in 0..50_000 {
        operation_xml.push_str("<a>");
    }
    operation_xml.push_str("leaf");
    for _ in 0..50_000 {
        operation_xml.push_str("</a>");
    }
    operation_xml.push_str("</value></Operation>");

    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || {
            let outcome = replay_one(
                "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
                &operation_xml,
            );
            assert!(matches!(
                outcome.error,
                Some(ReplayError::MalformedOperation { .. })
            ));
            assert!(matches!(
                &outcome.caveats[0],
                Caveat::MalformedOperation { .. }
            ));
        })
        .expect("spawning the probe thread")
        .join()
        .expect(
            "a pathologically deep contribution overflowed a 1 MiB stack instead of being \
             rejected cleanly",
        );
}

#[test]
fn duplicate_identity_items_are_mutated_by_the_correct_index_not_aliased() {
    let outcome = replay_one(
        r#"<ThingDef><comps><li Class="Foo"><v>1</v></li><li Class="Foo"><v>2</v></li></comps></ThingDef>"#,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="W"]/comps/li[2]/v</xpath>
                 <value><v>changed</v></value>
               </Operation>"#,
    );
    let comps = outcome.tree.get(&"comps".parse().unwrap()).unwrap();
    let Content::Children(items) = &comps.content else {
        unreachable!()
    };
    let Content::Children(first) = &items[0].content else {
        unreachable!()
    };
    let Content::Children(second) = &items[1].content else {
        unreachable!()
    };
    assert_eq!(first[0].content, Content::Text("1".to_string()));
    assert_eq!(second[0].content, Content::Text("changed".to_string()));
}

/// A real doors-mod shape over Example Core —
/// `li[@Class="..."][things/li="Column"]/things` — must select only the `li`
/// whose own `things` list actually contains `Column`, not every same-`Class`
/// sibling.
#[test]
fn nested_child_text_predicate_selects_only_the_matching_li() {
    let outcome = replay_one_for(
        r#"<PreceptDef><comps>
                 <li Class="RoomRequirement_ExampleAnyOfCount"><things><li>Column</li></things></li>
                 <li Class="RoomRequirement_ExampleAnyOfCount"><things><li>Pillar</li></things></li>
               </comps></PreceptDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/PreceptDef[defName="W"]/comps/li[@Class="RoomRequirement_ExampleAnyOfCount"][things/li="Column"]/things</xpath>
                 <value><li>Wood</li></value>
               </Operation>"#,
        "PreceptDef",
        "W",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty(), "{:?}", outcome.caveats);

    let comps = outcome.tree.get(&"comps".parse().unwrap()).unwrap();
    let Content::Children(items) = &comps.content else {
        unreachable!()
    };
    let things_of = |item: &FieldNode| -> Vec<String> {
        let Content::Children(children) = &item.content else {
            unreachable!()
        };
        let things = children.iter().find(|c| c.tag == "things").unwrap();
        let Content::Children(list) = &things.content else {
            unreachable!()
        };
        list.iter()
            .map(|li| match &li.content {
                Content::Text(text) => text.clone(),
                other => unreachable!("{other:?}"),
            })
            .collect()
    };
    assert_eq!(
        things_of(&items[0]),
        vec!["Column".to_string(), "Wood".to_string()],
        "the Column-carrying li must gain the new item"
    );
    assert_eq!(
        things_of(&items[1]),
        vec!["Pillar".to_string()],
        "the Pillar-carrying li must be untouched"
    );
}

#[test]
fn the_plant_density_golden_two_real_ops_produce_an_identical_outcome() {
    // Uses real-shaped Flora / Example Biomes operations (trimmed
    // fixtures).
    let flora_op = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xml/flora_plant_density.xml"
    ))
    .unwrap();
    let prehistoric_op = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xml/biomes_plant_density.xml"
    ))
    .unwrap();
    let core_biome = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xml/core_temperate_forest.xml"
    ))
    .unwrap();

    let tree = xml::parse(&core_biome).unwrap();
    let (active, names) = ctx();
    let flora = ModId::new("example.flora.core");
    let prehistoric = ModId::new("examplebiomes.biomes");
    let context = ReplayContext {
        active_mods: &active,
        mod_names_by_display: &names,
        def_type: "BiomeDef",
        def_name: "TemperateForest",
        selector: Selector::DefName,
        def_exists: def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };

    let final_under_order = replay(
        tree.clone(),
        &[
            PatchContribution {
                mod_id: &flora,
                operation_xml: &flora_op,
            },
            PatchContribution {
                mod_id: &prehistoric,
                operation_xml: &prehistoric_op,
            },
        ],
        &context,
    );
    let flora_last = replay(
        tree,
        &[
            PatchContribution {
                mod_id: &prehistoric,
                operation_xml: &prehistoric_op,
            },
            PatchContribution {
                mod_id: &flora,
                operation_xml: &flora_op,
            },
        ],
        &context,
    );

    let density_a = final_under_order
        .tree
        .get(&"plantDensity".parse().unwrap())
        .unwrap();
    let density_b = flora_last
        .tree
        .get(&"plantDensity".parse().unwrap())
        .unwrap();
    assert_eq!(density_a.content, Content::Text("0.9".to_string()));
    assert_eq!(density_a.content, density_b.content);
}

#[test]
fn the_multi_biome_sequence_only_applies_the_op_that_targets_our_def() {
    // Regression: a real-shaped Flora `<operations>` list
    // carrying replaces for several *different* biomes in one
    // Sequence — replaying it against TemperateForest's tree must
    // only apply the TemperateForest op, never another biome's.
    let flora_op = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xml/flora_plant_density_multi_biome.xml"
    ))
    .unwrap();
    let core_biome = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xml/core_temperate_forest.xml"
    ))
    .unwrap();

    let outcome = replay_one_for(&core_biome, &flora_op, "BiomeDef", "TemperateForest");

    assert!(outcome.error.is_none());
    let density = outcome.tree.get(&"plantDensity".parse().unwrap()).unwrap();
    assert_eq!(density.content, Content::Text("0.9".to_string()));
}

// =================================================================
// Extended xpath grammar
// =================================================================

fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/xml/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// [`replay_one_for`] with a caller-supplied [`DefExists`].
fn replay_one_knowing(
    tree_xml: &str,
    operation_xml: &str,
    def_type: &str,
    def_name: &str,
    def_exists: DefExists<'_>,
) -> ReplayOutcome {
    let tree = xml::parse(tree_xml).unwrap();
    let (active, names) = ctx();
    let mod_id = ModId::new("mod.a");
    let context = ReplayContext {
        active_mods: &active,
        mod_names_by_display: &names,
        def_type,
        def_name,
        selector: Selector::DefName,
        def_exists,
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };
    replay(
        tree,
        &[PatchContribution {
            mod_id: &mod_id,
            operation_xml,
        }],
        &context,
    )
}

fn children_of<'a>(outcome: &'a ReplayOutcome, path: &str) -> &'a [FieldNode] {
    let node = outcome
        .tree
        .get(&path.parse().unwrap())
        .unwrap_or_else(|| panic!("expected {path} to exist"));
    match &node.content {
        Content::Children(children) => children,
        other => panic!("expected children at {path}, got {other:?}"),
    }
}

fn text_at(outcome: &ReplayOutcome, path: &str) -> Content {
    outcome
        .tree
        .get(&path.parse().unwrap())
        .unwrap_or_else(|| panic!("expected {path} to exist"))
        .content
        .clone()
}

// --- Multi-def heads -------------------------------------------------

/// The Remedies-style insert names two defs in one head; replaying
/// it against *either* of them must apply it, and against a third def
/// must not.
#[test]
fn a_multi_def_head_op_applies_to_every_def_it_names_and_no_others() {
    let operation = fixture("remedies_insert_success_always.xml");
    let tree = r#"<ThingDef><comps><li Class="CompProperties_Power"/></comps></ThingDef>"#;

    for def_name in ["ExCharcoalCrematorium", "ExApparelWashingTub"] {
        let outcome = replay_one_for(tree, &operation, "ThingDef", def_name);
        assert!(outcome.error.is_none(), "{def_name}: {:?}", outcome.error);
        let comps = children_of(&outcome, "comps");
        assert_eq!(comps.len(), 2, "{def_name}");
        assert_eq!(
            comps[1].attrs.get("Class").map(String::as_str),
            Some("CompProperties_AffectedByFacilities"),
            "{def_name}"
        );
    }

    let elsewhere = replay_one_for(tree, &operation, "ThingDef", "SomethingElse");
    assert!(elsewhere.error.is_none());
    assert_eq!(children_of(&elsewhere, "comps").len(), 1);
    assert!(elsewhere.caveats.is_empty());
}

// --- <success> -------------------------------------------------------

/// Every `<success>` mode, as RimWorld's own `PatchOperation.Apply`
/// maps it: the wrapping sequence's second member runs only when the
/// first reports success.
#[test]
fn success_modes_decide_what_an_enclosing_sequence_sees() {
    struct Case {
        success: &'static str,
        /// Whether the first member (a Remove that matches nothing)
        /// is reported as a success, so the second member runs.
        second_runs: bool,
    }
    let cases = [
        Case {
            success: "",
            second_runs: false,
        },
        Case {
            success: "<success>Normal</success>",
            second_runs: false,
        },
        Case {
            success: "<success>Always</success>",
            second_runs: true,
        },
        Case {
            success: "<success>Invert</success>",
            second_runs: true,
        },
        Case {
            success: "<success>Never</success>",
            second_runs: false,
        },
    ];

    for case in cases {
        let operation = format!(
            r#"<Operation Class="PatchOperationSequence">
                     <operations>
                       <li Class="PatchOperationRemove">
                         {}
                         <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
                       </li>
                       <li Class="PatchOperationAdd">
                         <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                         <value><y>2</y></value>
                       </li>
                     </operations>
                   </Operation>"#,
            case.success
        );
        let outcome = replay_one(
            "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
            &operation,
        );
        let expected = if case.second_runs { 2 } else { 1 };
        assert_eq!(
            children_of(&outcome, "statBases").len(),
            expected,
            "success={:?}",
            case.success
        );
    }
}

/// `Invert` on an operation that *did* match turns it into a failure
/// for the enclosing sequence — the other half of the mode.
#[test]
fn success_invert_turns_a_matching_op_into_a_failure() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationRemove">
                     <success>Invert</success>
                     <xpath>Defs/ThingDef[defName="W"]/statBases/x</xpath>
                   </li>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                     <value><y>2</y></value>
                   </li>
                 </operations>
               </Operation>"#,
    );
    // The Remove itself ran (x is gone) but reported failure, so the
    // Add never did.
    assert!(children_of(&outcome, "statBases").is_empty());
}

// --- TopLevelOutcome ------------------------------------------------

/// The whole point of the field: a `<success>Always</success>`
/// leaf that matched nothing still leaves its `Caveat::FailedOp`
/// behind in the flat, unchanged `caveats` list (merge-preview
/// behaviour, untouched), but `TopLevelOutcome::succeeded` correctly
/// reports this operation as a *success* — the case `VerifyOrder`
/// must never turn into a finding.
#[test]
fn success_always_on_a_matched_nothing_leaf_reports_succeeded_true_in_top_level_outcomes() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationRemove">
                 <success>Always</success>
                 <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
               </Operation>"#,
    );
    assert_eq!(outcome.caveats.len(), 1, "unchanged: still worth surfacing");
    assert!(matches!(outcome.caveats[0], Caveat::FailedOp { .. }));
    assert_eq!(outcome.top_level_outcomes.len(), 1);
    assert!(
        outcome.top_level_outcomes[0].succeeded,
        "Success.Always must report this operation as succeeded"
    );
    assert_eq!(outcome.top_level_outcomes[0].caveats.len(), 1);
}

/// The ordinary case (no `<success>` override): a matched-nothing
/// leaf reports `succeeded: false` — this is the shape `VerifyOrder`
/// must keep predicting.
#[test]
fn an_ordinary_matched_nothing_leaf_reports_succeeded_false_in_top_level_outcomes() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationRemove">
                 <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
               </Operation>"#,
    );
    assert_eq!(outcome.top_level_outcomes.len(), 1);
    assert!(!outcome.top_level_outcomes[0].succeeded);
}

/// One entry per contribution actually reached, in order — two
/// separate top-level operations from two different mods.
#[test]
fn top_level_outcomes_has_one_entry_per_contribution_in_order() {
    let tree = xml::parse("<ThingDef><statBases><x>1</x></statBases></ThingDef>").unwrap();
    let (active, names) = ctx();
    let mod_a = ModId::new("mod.a");
    let mod_b = ModId::new("mod.b");
    let context = ReplayContext {
        active_mods: &active,
        mod_names_by_display: &names,
        def_type: "ThingDef",
        def_name: "W",
        selector: Selector::DefName,
        def_exists: def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };
    let outcome = replay(
        tree,
        &[
            PatchContribution {
                mod_id: &mod_a,
                operation_xml: r#"<Operation Class="PatchOperationReplace">
                         <xpath>Defs/ThingDef[defName="W"]/statBases/x</xpath>
                         <value><x>2</x></value>
                       </Operation>"#,
            },
            PatchContribution {
                mod_id: &mod_b,
                operation_xml: r#"<Operation Class="PatchOperationRemove">
                         <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
                       </Operation>"#,
            },
        ],
        &context,
    );
    assert_eq!(outcome.top_level_outcomes.len(), 2);
    assert_eq!(outcome.top_level_outcomes[0].mod_id, mod_a);
    assert!(outcome.top_level_outcomes[0].succeeded);
    assert_eq!(outcome.top_level_outcomes[1].mod_id, mod_b);
    assert!(!outcome.top_level_outcomes[1].succeeded);
}

/// A leaf mutation's own identity: `"{qualified class}({its own
/// xpath text})"` — the confirmed real-log shape.
#[test]
fn operation_identity_for_a_leaf_mutation_is_class_and_xpath() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationRemove">
                 <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
               </Operation>"#,
    );
    assert_eq!(
        outcome.top_level_outcomes[0].identity,
        r#"Verse.PatchOperationRemove(Defs/ThingDef[defName="W"]/statBases/nope)"#
    );
}

/// A `PatchOperationSequence`'s own identity names the child that
/// actually failed — `"{class}(count={N}, lastFailedOperation={leaf's
/// own identity})"`, the confirmed real-log shape (a real xenotype-patches
/// mod's own log line).
#[test]
fn operation_identity_for_a_sequence_names_its_failed_child() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationRemove">
                     <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
                   </li>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                     <value><y>2</y></value>
                   </li>
                 </operations>
               </Operation>"#,
    );
    assert!(!outcome.top_level_outcomes[0].succeeded);
    assert_eq!(
        outcome.top_level_outcomes[0].identity,
        r#"Verse.PatchOperationSequence(count=2, lastFailedOperation=Verse.PatchOperationRemove(Defs/ThingDef[defName="W"]/statBases/nope))"#
    );
    assert_eq!(
        outcome.top_level_outcomes[0].failed_leaf_xpath.as_deref(),
        Some(r#"Defs/ThingDef[defName="W"]/statBases/nope"#)
    );
}

/// An earlier `<success>Always>` sibling that itself matches nothing
/// still leaves its own `Caveat::FailedOp` behind (by design — it stays
/// true and worth surfacing even though the boolean is suppressed to
/// `true`, so the sequence keeps going). A caveat-scan-based
/// `lastFailedOperation` would pick *that* xpath, not the real stopping
/// child's, since it is simply the last `FailedOp` caveat in the flat
/// list. The real failing node is tracked structurally through control
/// flow instead, so `identity` and `failed_leaf_xpath` both correctly
/// name the child that actually stopped the sequence, never the
/// earlier, already-suppressed one.
#[test]
fn a_success_always_siblings_own_failure_never_masks_the_real_stopping_childs_identity() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationRemove">
                     <success>Always</success>
                     <xpath>Defs/ThingDef[defName="W"]/statBases/suppressed</xpath>
                   </li>
                   <li Class="PatchOperationRemove">
                     <xpath>Defs/ThingDef[defName="W"]/statBases/real</xpath>
                   </li>
                 </operations>
               </Operation>"#,
    );
    assert!(!outcome.top_level_outcomes[0].succeeded);
    assert_eq!(
        outcome.top_level_outcomes[0].identity,
        r#"Verse.PatchOperationSequence(count=2, lastFailedOperation=Verse.PatchOperationRemove(Defs/ThingDef[defName="W"]/statBases/real))"#,
        "must name the real stopping child, never the earlier success=Always-suppressed one"
    );
    assert_eq!(
        outcome.top_level_outcomes[0].failed_leaf_xpath.as_deref(),
        Some(r#"Defs/ThingDef[defName="W"]/statBases/real"#),
        "leaf_xpath must agree with identity about which leaf actually failed"
    );
}

/// The other shape: a bare `PatchOperationTest`
/// that fails pushes **no** `Caveat::FailedOp` at all (it's a pure
/// condition check, not a mutation) — a caveat-scan finds nothing (or,
/// worse, an unrelated caveat from elsewhere), while the structural
/// tracker correctly attributes the failure to the Test node itself.
#[test]
fn a_bare_top_level_test_failing_reports_itself_as_the_failed_leaf() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationTest">
                 <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
               </Operation>"#,
    );
    assert!(
        !outcome.top_level_outcomes[0].succeeded,
        "a Test whose condition is false must itself report failure"
    );
    assert!(
        outcome.top_level_outcomes[0].caveats.is_empty(),
        "a Test never pushes a Caveat::FailedOp — this is exactly why a caveat scan can't find it"
    );
    assert_eq!(
        outcome.top_level_outcomes[0].identity,
        r#"Verse.PatchOperationTest(Defs/ThingDef[defName="W"]/statBases/nope)"#
    );
    assert_eq!(
        outcome.top_level_outcomes[0].failed_leaf_xpath.as_deref(),
        Some(r#"Defs/ThingDef[defName="W"]/statBases/nope"#)
    );
}

/// A custom sequence-like class (a direct `<operations>` child, no
/// `<xpath>` of its own — the exact shape a real toggle-in-mod-options
/// wrapper has on a real install) must not render as bare
/// `"{class}()"` regardless of which child actually failed, which
/// would be indistinguishable from a different failing operation of
/// the same class. Detection is structural (`has_operations_child`,
/// the same check `apply_operation_body`'s own sequence-like fallback
/// uses), so it gets the identical
/// `count=`/`lastFailedOperation=` treatment `PatchOperationSequence`
/// itself gets, with no class-name special-casing needed.
#[test]
fn operation_identity_for_a_custom_sequence_like_class_names_its_failed_child() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="Example.PatchOperationToggableSequence">
                 <label>toggle</label>
                 <operations>
                   <li Class="PatchOperationRemove">
                     <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
                   </li>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                     <value><y>2</y></value>
                   </li>
                 </operations>
               </Operation>"#,
    );
    assert!(!outcome.top_level_outcomes[0].succeeded);
    assert_eq!(
        outcome.top_level_outcomes[0].identity,
        r#"Example.PatchOperationToggableSequence(count=2, lastFailedOperation=Verse.PatchOperationRemove(Defs/ThingDef[defName="W"]/statBases/nope))"#
    );
}

/// Two different toggle-wrapper operations of the same custom class,
/// each failing on its own distinct child, must render distinct
/// identities; otherwise many distinct real failures on one
/// compatibility-patch mod would render as one indistinguishable
/// `"<that class>()"` line.
#[test]
fn operation_identity_for_two_custom_sequences_never_collide() {
    let first = replay_one(
        "<GeneDef><statOffsets><x>1</x></statOffsets></GeneDef>",
        r#"<Operation Class="Example.PatchOperationToggableSequence">
                 <operations>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/GeneDef[defName="Learning_Fast"]/statOffsets/nope</xpath>
                     <value><y>2</y></value>
                   </li>
                 </operations>
               </Operation>"#,
    );
    let second = replay_one_for(
        "<ExampleVehicles.VehicleTurretDef><ammunition><thingDefs><x>1</x></thingDefs></ammunition></ExampleVehicles.VehicleTurretDef>",
        r#"<Operation Class="Example.PatchOperationToggableSequence">
                 <operations>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/ExampleVehicles.VehicleTurretDef[defName="EV_Turret"]/ammunition/thingDefs/nope</xpath>
                     <value><y>2</y></value>
                   </li>
                 </operations>
               </Operation>"#,
        "ExampleVehicles.VehicleTurretDef",
        "EV_Turret",
    );
    assert_ne!(
        first.top_level_outcomes[0].identity,
        second.top_level_outcomes[0].identity
    );
}

/// A custom mod-setting-toggle class shaped like a Conditional
/// (`<match>`/`<nomatch>` branches, no top-level `<xpath>`,
/// no `<operations>` list — the toggle's own condition is just its
/// declared default) has genuinely nothing on the top-level node
/// itself to distinguish it by. Must say so explicitly rather than
/// render bare `()`, which would be indistinguishable from a
/// different failing operation of the same custom class.
#[test]
fn operation_identity_with_nothing_to_distinguish_says_so() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="Foo.CustomToggleCondition">
                 <match Class="PatchOperationRemove">
                   <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
                 </match>
               </Operation>"#,
    );
    assert!(!outcome.top_level_outcomes[0].succeeded);
    assert_eq!(
        outcome.top_level_outcomes[0].identity,
        "Foo.CustomToggleCondition(no distinguishing detail available)"
    );
}

/// `PatchOperationFindMod`'s own identity names the `<mods>` it
/// checks, not an xpath (it has none of its own).
#[test]
fn operation_identity_for_find_mod_names_the_checked_mods() {
    // `Example Temperature Expanded` must be *active* for the
    // `<match>` branch to run at all — mirrors the real
    // `[Example Climate Tuning] PatchOperationFindMod(Example
    // Temperature Expanded) failed` log line exactly: the mod is
    // active, the match branch ran, and its own Remove matched
    // nothing, so the FindMod's own top-level outcome fails too.
    let tree = xml::parse("<ThingDef><statBases><x>1</x></statBases></ThingDef>").unwrap();
    let climate_mod = ModId::new("example.vecontent.climate");
    let mut active = BTreeSet::new();
    active.insert(climate_mod.clone());
    let mut names = BTreeMap::new();
    names.insert("Example Temperature Expanded".to_string(), climate_mod);
    let mod_a = ModId::new("mod.a");
    let context = ReplayContext {
        active_mods: &active,
        mod_names_by_display: &names,
        def_type: "ThingDef",
        def_name: "W",
        selector: Selector::DefName,
        def_exists: def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };
    let outcome = replay(
        tree,
        &[PatchContribution {
            mod_id: &mod_a,
            operation_xml: r#"<Operation Class="PatchOperationFindMod">
                     <mods><li>Example Temperature Expanded</li></mods>
                     <match Class="PatchOperationRemove">
                       <xpath>Defs/ThingDef[defName="W"]/statBases/nope</xpath>
                     </match>
                   </Operation>"#,
        }],
        &context,
    );
    assert!(!outcome.top_level_outcomes[0].succeeded);
    assert_eq!(
        outcome.top_level_outcomes[0].identity,
        "Verse.PatchOperationFindMod(Example Temperature Expanded)"
    );
}

// --- Custom classes with <match>/<nomatch>/<operation> ----------------

/// The real Example Furniture toggle: `<nomatch>` only, no `<xpath>`,
/// no declared toggle. Default true means the `<match>` branch runs —
/// there isn't one, so nothing happens, which is what "the option is
/// on" means for this mod.
#[test]
fn a_custom_toggle_class_with_only_a_nomatch_branch_leaves_the_def_alone() {
    let operation = fixture("furniture_mod_option.xml");
    let outcome = replay_one_for(
        "<ThingDef><designationCategory>Furniture</designationCategory></ThingDef>",
        &operation,
        "ThingDef",
        "ExampleTable1x1",
    );

    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(
        text_at(&outcome, "designationCategory"),
        Content::Text("Furniture".to_string())
    );
    assert!(matches!(
        &outcome.caveats[0],
        Caveat::ModSettingDefault { .. }
    ));
}

/// The Example Weapons toggle: `<match>` only, no `<xpath>`. The
/// default runs the branch, whose Replace then rewrites the tag.
#[test]
fn a_custom_toggle_class_runs_its_match_branch_at_the_default() {
    let operation = fixture("armoury_quest_reward.xml");
    let outcome = replay_one_for(
        "<ThingDef><thingSetMakerTags><li>RewardStandardLowFreq</li></thingSetMakerTags></ThingDef>",
        &operation,
        "ThingDef",
        "Gun_Revolver_Unique",
    );

    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let tags = children_of(&outcome, "thingSetMakerTags");
    assert_eq!(tags.len(), 1);
    assert_eq!(
        tags[0].content,
        Content::Text("RewardStandardHighFreq".to_string())
    );
}

/// The same shape with the toggle declared off takes the `<nomatch>`
/// branch instead. `<default>` is the third spelling the toggle shape
/// accepts, alongside `<enabled>` and `<defaultValue>`.
#[test]
fn a_custom_toggle_class_declared_off_runs_its_nomatch_branch() {
    for toggle in [
        "<enabled>False</enabled>",
        "<defaultValue>False</defaultValue>",
        "<default>False</default>",
    ] {
        let operation = format!(
            r#"<Operation Class="SomeMod.PatchOperationModOption_Whatever">
                     {toggle}
                     <match Class="PatchOperationAdd">
                       <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                       <value><on>true</on></value>
                     </match>
                     <nomatch Class="PatchOperationAdd">
                       <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                       <value><off>true</off></value>
                     </nomatch>
                   </Operation>"#
        );
        let outcome = replay_one(
            "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
            &operation,
        );
        let stat_bases = children_of(&outcome, "statBases");
        assert_eq!(stat_bases.last().unwrap().tag, "off", "{toggle}");
    }
}

/// A custom class wrapping a single `<operation>` runs it at the same
/// declared default.
#[test]
fn a_custom_toggle_class_with_a_single_operation_child_runs_it() {
    let enabled = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="SomeMod.PatchOperationSetting">
                 <operation Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><y>2</y></value>
                 </operation>
               </Operation>"#,
    );
    assert_eq!(children_of(&enabled, "statBases").len(), 2);

    let disabled = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="SomeMod.PatchOperationSetting">
                 <defaultValue>False</defaultValue>
                 <operation Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><y>2</y></value>
                 </operation>
               </Operation>"#,
    );
    assert_eq!(children_of(&disabled, "statBases").len(), 1);
}

/// A custom class that carries an `<xpath>` of its own is a real
/// mutation, not a toggle — it must stay `Unsupported` rather than
/// being guessed at through its `<match>` branch.
#[test]
fn a_custom_class_with_its_own_xpath_stays_unsupported() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="Example.PatchOperationResearchPrereg">
                 <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><y>2</y></value>
                 </match>
               </Operation>"#,
    );
    assert!(outcome.error.is_some());
}

// --- Custom classes a data file maps onto a behaviour -------------------

const SET_MOD_EXTENSION_OP: &str = r#"<Operation Class="Example.PatchOperationSetModExtension">
                 <xpath>Defs/ThingDef[defName="W"]</xpath>
                 <value>
                   <li Class="Example.Props"><def>NewProps</def></li>
                 </value>
               </Operation>"#;

#[test]
fn set_mod_extension_replaces_the_same_class_li_else_appends() {
    let behaviours = example_behaviours();

    let appended = replay_one_using(
        r#"<ThingDef><modExtensions><li Class="Other.Props"><v>keep</v></li></modExtensions></ThingDef>"#,
        SET_MOD_EXTENSION_OP,
        "ThingDef",
        "W",
        &behaviours,
    );
    let items = children_of(&appended, "modExtensions");
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[1].attrs.get("Class").map(String::as_str),
        Some("Example.Props")
    );

    let replaced = replay_one_using(
        r#"<ThingDef><modExtensions><li Class="Example.Props"><def>OLD</def></li></modExtensions></ThingDef>"#,
        SET_MOD_EXTENSION_OP,
        "ThingDef",
        "W",
        &behaviours,
    );
    let items = children_of(&replaced, "modExtensions");
    assert_eq!(items.len(), 1);
    let Content::Children(fields) = &items[0].content else {
        unreachable!()
    };
    assert_eq!(fields[0].content, Content::Text("NewProps".to_string()));

    let created = replay_one_using(
        "<ThingDef/>",
        SET_MOD_EXTENSION_OP,
        "ThingDef",
        "W",
        &behaviours,
    );
    assert_eq!(children_of(&created, "modExtensions").len(), 1);
}

/// The same operation, with no data loaded at all, is `Unsupported` —
/// the degradation `PatchOperationBehaviours::none` deliberately has.
#[test]
fn a_custom_class_with_no_loaded_behaviour_is_unsupported() {
    let outcome = replay_one_for(
        r#"<ThingDef><modExtensions/></ThingDef>"#,
        SET_MOD_EXTENSION_OP,
        "ThingDef",
        "W",
    );
    assert!(matches!(
        outcome.error,
        Some(ReplayError::Unsupported { .. })
    ));
}

#[test]
fn add_or_replace_replaces_an_existing_field_and_adds_a_missing_one() {
    let outcome = replay_one_using(
        "<ResearchProjectDef><prerequisites><li>OldPrereq</li></prerequisites></ResearchProjectDef>",
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="Example.PatchOperationAddOrReplace">
                     <xpath>Defs/ResearchProjectDef[defName="R"]</xpath>
                     <value><prerequisites><li>NewPrereq</li></prerequisites></value>
                   </li>
                   <li Class="Example.PatchOperationAddOrReplace">
                     <xpath>Defs/ResearchProjectDef[defName="R"]</xpath>
                     <value><techLevel>Spacer</techLevel></value>
                   </li>
                 </operations>
               </Operation>"#,
        "ResearchProjectDef",
        "R",
        &example_behaviours(),
    );

    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let prerequisites = children_of(&outcome, "prerequisites");
    assert_eq!(prerequisites.len(), 1);
    assert_eq!(
        prerequisites[0].content,
        Content::Text("NewPrereq".to_string())
    );
    assert_eq!(
        text_at(&outcome, "techLevel"),
        Content::Text("Spacer".to_string())
    );
}

/// A class the loaded data doesn't map stays `Unsupported` even when
/// its name is one letter off a mapped one — the table is matched by
/// suffix, never guessed at.
#[test]
fn a_lookalike_custom_class_is_not_treated_as_a_known_one() {
    let outcome = replay_one_using(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="Example.PatchOperationAddOrReplaced">
                 <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
        "ThingDef",
        "W",
        &example_behaviours(),
    );
    assert!(outcome.error.is_some());
}

/// A data row naming a behaviour string this binary does not
/// implement never reaches replay at all: the loader drops it (with a
/// warning), so the class arrives here unmapped and degrades to
/// `Unsupported` — never to some other behaviour.
#[test]
fn an_unmapped_class_never_borrows_another_rows_behaviour() {
    let outcome = replay_one_using(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="Unrelated.PatchOperationWhatever">
                 <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
        "ThingDef",
        "W",
        &example_behaviours(),
    );
    assert!(matches!(
        outcome.error,
        Some(ReplayError::Unsupported { .. })
    ));
}

// --- Cross-def existence tests ------------------------------------------

#[test]
fn a_cross_def_conditional_is_answered_through_def_exists() {
    let operation = fixture("fishing_cross_def_conditional.xml");
    let tree = "<ExampleFishing.BiomeTempDef><biomes><li>Existing</li></biomes></ExampleFishing.BiomeTempDef>";

    let present = replay_one_knowing(
        tree,
        &operation,
        "ExampleFishing.BiomeTempDef",
        "EXF_BiomeWarm",
        &|def_type, def_name| Some(def_type == "BiomeDef" && def_name == "ExampleBlightForest"),
    );
    assert!(present.error.is_none(), "{:?}", present.error);
    assert_eq!(children_of(&present, "biomes").len(), 2);

    let absent = replay_one_knowing(
        tree,
        &operation,
        "ExampleFishing.BiomeTempDef",
        "EXF_BiomeWarm",
        &|_, _| Some(false),
    );
    assert!(absent.error.is_none(), "{:?}", absent.error);
    assert_eq!(children_of(&absent, "biomes").len(), 1);
}

/// An unknown answer keeps the op `Unsupported` — the whole point of
/// [`DefExists`] returning an `Option`.
#[test]
fn a_cross_def_conditional_with_an_unknown_answer_stays_unsupported() {
    let operation = fixture("fishing_cross_def_conditional.xml");
    let outcome = replay_one_for(
        "<ExampleFishing.BiomeTempDef><biomes><li>Existing</li></biomes></ExampleFishing.BiomeTempDef>",
        &operation,
        "ExampleFishing.BiomeTempDef",
        "EXF_BiomeWarm",
    );
    assert!(outcome.error.is_some());
}

/// Only a *bare* head is answerable this way: a cross-def xpath with
/// steps under it needs that def's own document.
#[test]
fn a_cross_def_conditional_with_steps_stays_unsupported_even_when_the_def_is_known() {
    let outcome = replay_one_knowing(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/ThingDef[defName="Other"]/comps</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><y>2</y></value>
                 </match>
               </Operation>"#,
        "ThingDef",
        "W",
        &|_, _| Some(true),
    );
    assert!(outcome.error.is_some());
}

/// A `PatchOperationTest` gets the same treatment: a bare cross-def
/// head is answerable, and a false answer stops the sequence.
#[test]
fn a_cross_def_test_stops_a_sequence_when_the_def_is_absent() {
    let operation = r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationTest">
                     <xpath>Defs/ThingDef[defName="Other"]</xpath>
                   </li>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                     <value><y>2</y></value>
                   </li>
                 </operations>
               </Operation>"#;
    let tree = "<ThingDef><statBases><x>1</x></statBases></ThingDef>";

    let present = replay_one_knowing(tree, operation, "ThingDef", "W", &|_, _| Some(true));
    assert_eq!(children_of(&present, "statBases").len(), 2);

    let absent = replay_one_knowing(tree, operation, "ThingDef", "W", &|_, _| Some(false));
    assert_eq!(children_of(&absent, "statBases").len(), 1);
}

// --- Unanswerable Conditionals and custom coordinates -------------

/// A real shape: the Conditional's own
/// test names a template by `@Name` — unanswerable, since
/// `cross_def_existence` only ever answers a bare `defName`
/// existence check, never a `[@Name=...]` one — and both branches
/// address that same unrelated template, never the def actually
/// being replayed. The whole Conditional is moot to this replay.
#[test]
fn a7_unanswerable_conditional_with_elsewhere_only_branches_succeeds() {
    let outcome = replay_one_for(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/GeneDef[@Name="WB_FunctionalWingBase"]</xpath>
                 <match Class="PatchOperationReplace">
                   <xpath>Defs/GeneDef[@Name="WB_FunctionalWingBase"]/label</xpath>
                   <value><label>winged</label></value>
                 </match>
                 <nomatch Class="PatchOperationReplace">
                   <xpath>Defs/GeneDef[@Name="WB_FunctionalWingBase"]/label</xpath>
                   <value><label>wingless</label></value>
                 </nomatch>
               </Operation>"#,
        "ThingDef",
        "XQE_Levitation",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty());
    assert_eq!(children_of(&outcome, "statBases").len(), 1);
}

/// Negative case: the test is just as unanswerable, but the `match`
/// branch's own xpath names the def actually being replayed — the
/// conservative branch walk must not skip this failure.
#[test]
fn a7_unanswerable_conditional_with_a_branch_touching_this_def_stays_unsupported() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/GeneDef[@Name="WB_FunctionalWingBase"]</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><matched>true</matched></value>
                 </match>
               </Operation>"#,
    );
    assert!(outcome.error.is_some());
}

/// Second negative case: an unrecognized custom class inside a branch
/// is conservatively "could affect this def" even though its own
/// xpath happens to name a different def — the walk only rules a branch out
/// when it fully understands what the branch does.
#[test]
fn a7_unanswerable_conditional_with_an_unmodelled_branch_class_stays_unsupported() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/GeneDef[@Name="WB_FunctionalWingBase"]</xpath>
                 <match Class="SomeMod.UnknownCustomOp">
                   <xpath>Defs/GeneDef[@Name="WB_FunctionalWingBase"]/label</xpath>
                 </match>
               </Operation>"#,
    );
    assert!(outcome.error.is_some());
}

/// The strict grammar rejects an `and`-composed head predicate,
/// but the analyzer's own loose `xpath_target::parse_all` still names
/// both defs it disjuncts over — neither of them this replay's own
/// def, so the op succeeds elsewhere rather than aborting the replay.
#[test]
fn a8_a_loose_grammar_head_naming_only_other_defs_succeeds_elsewhere() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>*/ThingDef[(defName = "X" or defName = "Y") and not(comps)]/statBases</xpath>
                 <value><statBases><z>1</z></statBases></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty());
    assert_eq!(children_of(&outcome, "statBases")[0].tag, "x");
}

/// The companion to the case above: a head the strict grammar rejects
/// whose loose `parse_all` scan names *this* def is not "elsewhere".
/// `head_filter_predicate` answers it exactly, against this def's own
/// tree — `defName` is an ordinary child element, and
/// `not(comps)` holds, so the op applies.
#[test]
fn a8_a_loose_grammar_head_naming_this_def_is_now_evaluated_against_the_tree() {
    let outcome = replay_one(
        r#"<ThingDef><defName>W</defName><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>*/ThingDef[(defName = "W" or defName = "Y") and not(comps)]/statBases</xpath>
                 <value><statBases><z>1</z></statBases></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(children_of(&outcome, "statBases")[0].tag, "z");
}

/// The other half of the same shape: the `and`-composed term is
/// **false** for this def (it does have `comps`), so the head does not
/// select it. A clean no-op — no mutation, no caveat, and no
/// prediction, counted instead in
/// [`ReplayOutcome::suppressed_filter_head_ops`]... except that it
/// never even reaches the selection stage: `is_this_def` is already
/// `false`, which is "succeeded elsewhere", not "selected nothing".
#[test]
fn a8_a_loose_grammar_head_whose_and_term_fails_leaves_the_def_untouched() {
    let outcome = replay_one(
        r#"<ThingDef><defName>W</defName><comps><li>c</li></comps><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>*/ThingDef[(defName = "W" or defName = "Y") and not(comps)]/statBases</xpath>
                 <value><statBases><z>1</z></statBases></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty(), "{:?}", outcome.caveats);
    assert_eq!(outcome.suppressed_filter_head_ops, 0);
    assert_eq!(children_of(&outcome, "statBases")[0].tag, "x");
}

// --- Filter heads: replaying each real skip shape --------------------
//
// Every shape gets the same four questions asked of it, because
// widening a grammar is exactly how a replay goes silently wrong: does
// a match
// really apply the mutation, does a non-match leave a *clean* no-op
// (no mutation, no `Caveat::FailedOp`, no prediction), does the wrong
// def type refuse regardless of content, and — for `@ParentName`, the
// shape whose attribute lives on the tree rather than in `root.attrs`
// — does a def with no `ParentName` at all answer `false` rather than
// matching everything.

/// Replays one operation against a def whose own existence is denied
/// (`this_def_present: false`) — `verify_order`'s zero-owner
/// placeholder, the case the bare-type-head gate exists for.
fn replay_one_absent(tree_xml: &str, operation_xml: &str) -> ReplayOutcome {
    let tree = xml::parse(tree_xml).unwrap();
    let (active, names) = ctx();
    let mod_id = ModId::new("mod.a");
    let context = ReplayContext {
        active_mods: &active,
        mod_names_by_display: &names,
        def_type: "ThingDef",
        def_name: "W",
        selector: Selector::DefName,
        def_exists: def_existence_unknown(),
        this_def_present: false,
        behaviours: PatchOperationBehaviours::none(),
    };
    replay(
        tree,
        &[PatchContribution {
            mod_id: &mod_id,
            operation_xml,
        }],
        &context,
    )
}

const PARENT_NAME_OP: &str = r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[@ParentName="BulletBase"]/statBases</xpath>
             <value><y>2</y></value>
           </Operation>"#;

/// The largest shape: `@ParentName` is read off the tree (where
/// `xml::parse` lifts it to), so the op applies.
#[test]
fn a_parent_name_head_matching_this_def_applies_the_mutation() {
    let outcome = replay_one(
        r#"<ThingDef ParentName="BulletBase"><statBases><x>1</x></statBases></ThingDef>"#,
        PARENT_NAME_OP,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let children = children_of(&outcome, "statBases");
    assert_eq!(children.len(), 2);
    assert_eq!(children[1].tag, "y");
}

/// The same op against a def whose `ParentName` is something else: a
/// clean no-op, no caveat, and nothing counted — a head that doesn't
/// name this def is "succeeded elsewhere", not "selected nothing".
#[test]
fn a_parent_name_head_naming_another_template_leaves_this_def_untouched() {
    let outcome = replay_one(
        r#"<ThingDef ParentName="ApparelBase"><statBases><x>1</x></statBases></ThingDef>"#,
        PARENT_NAME_OP,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty(), "{:?}", outcome.caveats);
    assert_eq!(outcome.suppressed_filter_head_ops, 0);
    assert_eq!(children_of(&outcome, "statBases").len(), 1);
}

/// **The regression this shape's whole correctness rests on.** A def
/// with *no* `ParentName` must answer `false`, not match. Evaluated
/// with `predicate_matches` instead of `root_predicate_matches` this
/// would read an empty `root.attrs` and answer `false` here too — the
/// reason the positive case above is the one that pins the choice.
#[test]
fn a_parent_name_head_against_a_def_with_no_parent_name_is_a_no_op() {
    let outcome = replay_one(
        r#"<ThingDef><statBases><x>1</x></statBases></ThingDef>"#,
        PARENT_NAME_OP,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty(), "{:?}", outcome.caveats);
    assert_eq!(children_of(&outcome, "statBases").len(), 1);
}

/// The head's def *type* is checked before its predicate, so a
/// `HediffDef` carrying the very same `ParentName` is untouched.
#[test]
fn a_parent_name_head_against_the_wrong_def_type_is_a_no_op() {
    let outcome = replay_one_for(
        r#"<HediffDef ParentName="BulletBase"><statBases><x>1</x></statBases></HediffDef>"#,
        PARENT_NAME_OP,
        "HediffDef",
        "W",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty(), "{:?}", outcome.caveats);
    assert_eq!(children_of(&outcome, "statBases").len(), 1);
}

/// A bare-type head selects across every def of the type, so
/// it matches this one — and mutates it.
#[test]
fn a_bare_type_head_applies_to_the_def_being_replayed() {
    let outcome = replay_one(
        r#"<ThingDef><race><mechEnabledWorkTypes><li>Hauling</li></mechEnabledWorkTypes></race></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>/Defs/ThingDef/race/mechEnabledWorkTypes [li [text()="Hauling"] ]</xpath>
                 <value><li>Cleaning</li></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    let children = children_of(&outcome, "race/mechEnabledWorkTypes");
    assert_eq!(children.len(), 2);
    assert_eq!(children[1].tag, "li");
}

/// The rule that keeps filter heads from becoming a new prediction
/// source: a bare-type head that selects **nothing** on
/// this def says nothing about the op's real outcome (it is a global
/// query), so it is reported as succeeded, pushes no
/// `Caveat::FailedOp`, and is counted instead.
#[test]
fn a_bare_type_head_selecting_nothing_here_is_counted_not_predicted() {
    let outcome = replay_one(
        r#"<ThingDef><race><mechEnabledWorkTypes><li>Doctor</li></mechEnabledWorkTypes></race></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>/Defs/ThingDef/race/mechEnabledWorkTypes [li [text()="Hauling"] ]</xpath>
                 <value><li>Cleaning</li></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty(), "{:?}", outcome.caveats);
    assert_eq!(outcome.suppressed_filter_head_ops, 1);
    assert!(outcome.top_level_outcomes[0].succeeded);
    assert_eq!(children_of(&outcome, "race/mechEnabledWorkTypes").len(), 1);
}

/// The bare-type-head gate: the zero-owner synthetic placeholder
/// (`this_def_present: false`) must never be matched by a head that
/// names no def at all — its whole premise is that the def is absent.
#[test]
fn a_bare_type_head_never_matches_the_zero_owner_placeholder() {
    let outcome = replay_one_absent(
        r#"<ThingDef></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>/Defs/ThingDef/race</xpath>
                 <value><li>Cleaning</li></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty(), "{:?}", outcome.caveats);
    assert_eq!(outcome.suppressed_filter_head_ops, 0);
}

/// `contains(text(), …)` inside a head-position child filter.
#[test]
fn a_contains_text_head_matching_this_def_applies_the_mutation() {
    let outcome = replay_one(
        r#"<ThingDef><thingClass>RimWorld.Building_Storage</thingClass><building><defaultStorageSettings><x>1</x></defaultStorageSettings></building></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>/Defs/ThingDef [thingClass [contains(text(), "Storage")] ]/building/defaultStorageSettings</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(
        children_of(&outcome, "building/defaultStorageSettings").len(),
        2
    );
}

/// The same op against a def whose `thingClass` does not contain the
/// literal: a clean no-op.
#[test]
fn a_contains_text_head_not_matching_this_def_is_a_clean_no_op() {
    let outcome = replay_one(
        r#"<ThingDef><thingClass>RimWorld.Building_Bed</thingClass><building><defaultStorageSettings><x>1</x></defaultStorageSettings></building></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>/Defs/ThingDef [thingClass [contains(text(), "Storage")] ]/building/defaultStorageSettings</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty(), "{:?}", outcome.caveats);
    assert_eq!(outcome.suppressed_filter_head_ops, 0);
    assert_eq!(
        children_of(&outcome, "building/defaultStorageSettings").len(),
        1
    );
}

/// The `and`-composed content head, both halves true.
#[test]
fn an_and_composed_content_head_applies_only_when_every_term_holds() {
    let operation = r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[inspectorTabs/li="ITab_Bills" and not(comps)]</xpath>
             <value><comps><li>added</li></comps></value>
           </Operation>"#;

    let matching = replay_one(
        r#"<ThingDef><inspectorTabs><li>ITab_Bills</li></inspectorTabs></ThingDef>"#,
        operation,
    );
    assert!(matching.error.is_none(), "{:?}", matching.error);
    assert_eq!(children_of(&matching, "comps").len(), 1);

    // `not(comps)` is false here, so the head does not select the def.
    let not_matching = replay_one(
        r#"<ThingDef><inspectorTabs><li>ITab_Bills</li></inspectorTabs><comps><li>existing</li></comps></ThingDef>"#,
        operation,
    );
    assert!(not_matching.error.is_none(), "{:?}", not_matching.error);
    assert!(
        not_matching.caveats.is_empty(),
        "{:?}",
        not_matching.caveats
    );
    assert_eq!(children_of(&not_matching, "comps").len(), 1);
}

/// The critical shape: a step predicate negating a
/// child filter. `comps` has no `CompProperties_AffectedByFacilities`
/// item, so the step selects it and the `Add` lands.
#[test]
fn a_step_predicate_negating_a_child_filter_selects_when_the_class_is_absent() {
    let outcome = replay_one(
        r#"<ThingDef><comps><li Class="CompProperties_Refuelable" /></comps></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="W"]/comps[not(li[@Class="CompProperties_AffectedByFacilities"])]</xpath>
                 <value><li Class="CompProperties_AffectedByFacilities" /></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(children_of(&outcome, "comps").len(), 2);
}

/// Its mirror — the class *is* present, so the step matches nothing.
/// This head is an ordinary `defName=` one, **not** a filter head, so
/// the usual `Caveat::FailedOp` prediction path is untouched: the
/// no-prediction conservatism applies to filter head shapes only.
#[test]
fn a_step_predicate_negating_a_child_filter_still_predicts_when_the_class_is_present() {
    let outcome = replay_one(
        r#"<ThingDef><comps><li Class="CompProperties_AffectedByFacilities" /></comps></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="W"]/comps[not(li[@Class="CompProperties_AffectedByFacilities"])]</xpath>
                 <value><li Class="CompProperties_Refuelable" /></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(outcome.suppressed_filter_head_ops, 0);
    assert!(matches!(
        outcome.caveats.as_slice(),
        [Caveat::FailedOp { .. }]
    ));
    assert!(!outcome.top_level_outcomes[0].succeeded);
    assert_eq!(children_of(&outcome, "comps").len(), 1);
}

/// `@ParentName` `and`-composed with a relative-path existence
/// test, both halves required.
#[test]
fn a_parent_name_head_and_composed_with_a_relative_path_test_needs_both() {
    let operation = r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[@ParentName="BodyPartProstheticBase" and costList/ComponentIndustrial]</xpath>
             <value><techLevel>Industrial</techLevel></value>
           </Operation>"#;

    let matching = replay_one(
        r#"<ThingDef ParentName="BodyPartProstheticBase"><costList><ComponentIndustrial>2</ComponentIndustrial></costList></ThingDef>"#,
        operation,
    );
    assert!(matching.error.is_none(), "{:?}", matching.error);
    assert!(matching.tree.get(&"techLevel".parse().unwrap()).is_some());

    let not_matching = replay_one(
        r#"<ThingDef ParentName="BodyPartProstheticBase"><costList><Steel>2</Steel></costList></ThingDef>"#,
        operation,
    );
    assert!(not_matching.error.is_none(), "{:?}", not_matching.error);
    assert!(
        not_matching.caveats.is_empty(),
        "{:?}",
        not_matching.caveats
    );
    assert!(
        not_matching
            .tree
            .get(&"techLevel".parse().unwrap())
            .is_none()
    );
}

/// A nested child filter, two levels deep, in head position.
#[test]
fn a_nested_child_filter_head_matches_only_the_def_that_carries_the_class() {
    let operation = r#"<Operation Class="PatchOperationAdd">
             <xpath>/Defs/ThingDef[comps and comps[li[@Class="ExampleFurniture.CompProperties_Mountable"]]]</xpath>
             <value><rotatable>true</rotatable></value>
           </Operation>"#;

    let matching = replay_one(
        r#"<ThingDef><comps><li Class="ExampleFurniture.CompProperties_Mountable" /></comps></ThingDef>"#,
        operation,
    );
    assert!(matching.error.is_none(), "{:?}", matching.error);
    assert!(matching.tree.get(&"rotatable".parse().unwrap()).is_some());

    let not_matching = replay_one(
        r#"<ThingDef><comps><li Class="CompProperties_Refuelable" /></comps></ThingDef>"#,
        operation,
    );
    assert!(not_matching.error.is_none(), "{:?}", not_matching.error);
    assert!(
        not_matching.caveats.is_empty(),
        "{:?}",
        not_matching.caveats
    );
    assert!(
        not_matching
            .tree
            .get(&"rotatable".parse().unwrap())
            .is_none()
    );
}

/// A real install's device-standby-mod shape: a `./`-prefixed relative path with a
/// mid-segment filter and a trailing value test, `and`-composed with
/// `@ParentName`. Both a match and a near-miss, because the near-miss
/// is the one that would silently mutate the wrong def.
#[test]
fn a_self_axis_relative_path_head_matches_only_the_def_that_satisfies_every_segment() {
    let operation = r#"<Operation Class="PatchOperationAdd">
             <xpath>/Defs/ThingDef[@ParentName="BenchBase" and ./comps/li[@Class="CompProperties_Power"]/compClass="CompPowerTrader"]/comps</xpath>
             <value><li Class="ExampleStandby.CompProperties_Standby" /></value>
           </Operation>"#;

    let matching = replay_one(
        r#"<ThingDef ParentName="BenchBase"><comps><li Class="CompProperties_Power"><compClass>CompPowerTrader</compClass></li></comps></ThingDef>"#,
        operation,
    );
    assert!(matching.error.is_none(), "{:?}", matching.error);
    assert_eq!(children_of(&matching, "comps").len(), 2);

    // Same `@Class`, different `compClass` — the last segment is what
    // decides, and it must.
    let near_miss = replay_one(
        r#"<ThingDef ParentName="BenchBase"><comps><li Class="CompProperties_Power"><compClass>CompPowerPlant</compClass></li></comps></ThingDef>"#,
        operation,
    );
    assert!(near_miss.error.is_none(), "{:?}", near_miss.error);
    assert!(near_miss.caveats.is_empty(), "{:?}", near_miss.caveats);
    assert_eq!(children_of(&near_miss, "comps").len(), 1);
}

/// **Probed through `replay` rather than reasoned about.**
/// `not(@ParentName="X")` applies to the *def node*, whose `ParentName`
/// `xml::parse` lifts onto the tree. Delegated to `predicate_matches`
/// it would read an empty `root.attrs`, conclude the attribute is
/// absent, and so match **every** def — the exact inversion of the
/// shape it excludes. `not(@ParentName` is common across active mods
/// on a real install.
#[test]
fn a_negated_parent_name_head_excludes_the_def_that_carries_it() {
    let operation = r#"<Operation Class="PatchOperationAdd">
             <xpath>Defs/ThingDef[not(@ParentName="BenchBase")]/statBases</xpath>
             <value><y>2</y></value>
           </Operation>"#;

    let carries_it = replay_one(
        r#"<ThingDef ParentName="BenchBase"><statBases><x>1</x></statBases></ThingDef>"#,
        operation,
    );
    assert!(carries_it.error.is_none(), "{:?}", carries_it.error);
    assert_eq!(
        children_of(&carries_it, "statBases").len(),
        1,
        "the def this head negates must be left alone"
    );

    let does_not = replay_one(
        r#"<ThingDef ParentName="ApparelBase"><statBases><x>1</x></statBases></ThingDef>"#,
        operation,
    );
    assert!(does_not.error.is_none(), "{:?}", does_not.error);
    assert_eq!(children_of(&does_not, "statBases").len(), 2);

    let has_none = replay_one(
        r#"<ThingDef><statBases><x>1</x></statBases></ThingDef>"#,
        operation,
    );
    assert!(has_none.error.is_none(), "{:?}", has_none.error);
    assert_eq!(children_of(&has_none, "statBases").len(), 2);
}

/// The `@Name` half of the case above — same lifting, same inversion.
#[test]
fn a_negated_name_attribute_head_excludes_the_template_that_carries_it() {
    let outcome = replay_one_for(
        r#"<ThingDef Name="BenchBase"><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[not(@Name="BenchBase")]/statBases</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
        "ThingDef",
        "BenchBase",
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(children_of(&outcome, "statBases").len(), 1);
}

/// The negative case: an attribute `xml::parse` discards outright
/// cannot be answered *inside* a `not(...)` either. Wrapping it must
/// keep the `Unsupported` refusal, never quietly become `true`.
#[test]
fn a_negated_discarded_attribute_head_stays_unsupported() {
    let outcome = replay_one(
        r#"<ThingDef><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[not(@Abstract="true")]/statBases</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
    );
    assert!(matches!(
        outcome.error,
        Some(ReplayError::Unsupported { .. })
    ));
}

/// A condition is a claim about
/// the whole document, so a global filter head selecting nothing
/// *here* is unanswerable, not `false`. Answering `false` took the
/// `<nomatch>` branch — silently wrong merge output.
#[test]
fn a_conditional_on_a_filter_head_selecting_nothing_here_is_unsupported_not_nomatch() {
    let outcome = replay_one(
        r#"<ThingDef ParentName="ApparelBase"><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/ThingDef[@ParentName="ApparelBase"]/comps</xpath>
                 <nomatch Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[@ParentName="ApparelBase"]/statBases</xpath>
                   <value><y>2</y></value>
                 </nomatch>
               </Operation>"#,
    );
    assert!(
        matches!(outcome.error, Some(ReplayError::Unsupported { .. })),
        "{:?}",
        outcome.error
    );
    assert_eq!(
        children_of(&outcome, "statBases").len(),
        1,
        "the <nomatch> branch must not have run"
    );
}

/// The other half: a `PatchOperationTest` on the same head would
/// otherwise report `succeeded: false` — a *prediction*, which is
/// exactly what `resolve_selection` refuses to emit for this head.
#[test]
fn a_test_on_a_filter_head_selecting_nothing_here_never_reports_a_failure() {
    let outcome = replay_one(
        r#"<ThingDef ParentName="ApparelBase"><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationTest">
                 <xpath>Defs/ThingDef[@ParentName="ApparelBase"]/comps</xpath>
               </Operation>"#,
    );
    assert!(
        matches!(outcome.error, Some(ReplayError::Unsupported { .. })),
        "{:?}",
        outcome.error
    );
    assert!(
        outcome.top_level_outcomes.is_empty(),
        "an Unsupported operation produces no outcome at all, let alone a failing one"
    );
}

/// A filter-head condition that **does** select something here is
/// decisive and stays so — the node exists, however many other defs
/// the head also names.
#[test]
fn a_conditional_on_a_filter_head_that_selects_something_here_still_matches() {
    let outcome = replay_one(
        r#"<ThingDef ParentName="ApparelBase"><comps><li>c</li></comps><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/ThingDef[@ParentName="ApparelBase"]/comps</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[@ParentName="ApparelBase"]/statBases</xpath>
                   <value><y>2</y></value>
                 </match>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(children_of(&outcome, "statBases").len(), 2);
}

/// The `XPathResolution::Elsewhere` branch needs its own coverage,
/// since `head_filter_predicate` answers the `and`-composed fixtures
/// first. This shape reaches it: the `//`
/// tail makes `parse_after_head` refuse, so *no* strict entry point
/// takes it, while `xpath_target::parse_all`'s loose scan still reads
/// a head naming only another def.
#[test]
fn a8_elsewhere_is_still_reached_when_no_head_query_can_take_the_xpath() {
    let outcome = replay_one(
        r#"<ThingDef><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Other"]//comps</xpath>
                 <value><comps><z>1</z></comps></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty(), "{:?}", outcome.caveats);
    assert_eq!(outcome.suppressed_filter_head_ops, 0);
    assert!(outcome.top_level_outcomes[0].succeeded);
}

/// The malformed author predicate keeps stopping the replay,
/// which is the only honest answer to input this grammar cannot read.
#[test]
fn the_malformed_not_equality_predicate_still_stops_the_replay() {
    let outcome = replay_one(
        r#"<ThingDef><race><petness>0.5</petness></race></ThingDef>"#,
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="W"]/race[not(petness)="petness"]</xpath>
                 <value><race><petness>0.9</petness></race></value>
               </Operation>"#,
    );
    assert!(matches!(
        outcome.error,
        Some(ReplayError::Unsupported { .. })
    ));
}

/// The `Err`-stays-a-refusal rule: a head predicate on an attribute
/// `xml::parse` discards outright cannot be answered from one def's
/// tree, and must stop the replay rather than read as "doesn't match".
#[test]
fn a_filter_head_on_a_discarded_attribute_stays_unsupported() {
    let outcome = replay_one(
        r#"<ThingDef><statBases><x>1</x></statBases></ThingDef>"#,
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[@Abstract="true"]/statBases</xpath>
                 <value><y>2</y></value>
               </Operation>"#,
    );
    assert!(matches!(
        outcome.error,
        Some(ReplayError::Unsupported { .. })
    ));
}

/// `Example.PatchOperationReplaceResearchCoords` replaces both
/// coordinate children on the matched node and leaves every other
/// field untouched.
#[test]
fn replace_research_coords_replaces_existing_coordinates_and_preserves_other_fields() {
    let outcome = replay_one_using(
        r#"<ResearchProjectDef><researchViewX>1.0</researchViewX><researchViewY>2.0</researchViewY><label>Old</label></ResearchProjectDef>"#,
        r#"<Operation Class="Example.PatchOperationReplaceResearchCoords">
                 <xpath>Defs/ResearchProjectDef[defName="SteppingStone1"]</xpath>
                 <researchViewX>5.5</researchViewX>
                 <researchViewY>6.5</researchViewY>
               </Operation>"#,
        "ResearchProjectDef",
        "SteppingStone1",
        &example_behaviours(),
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(
        text_at(&outcome, "researchViewX"),
        Content::Text("5.5".to_string())
    );
    assert_eq!(
        text_at(&outcome, "researchViewY"),
        Content::Text("6.5".to_string())
    );
    assert_eq!(text_at(&outcome, "label"), Content::Text("Old".to_string()));
}

/// Both coordinate children are appended, in order, when the
/// matched node has neither yet.
#[test]
fn replace_research_coords_appends_when_the_node_has_neither_coordinate() {
    let outcome = replay_one_using(
        r#"<ResearchProjectDef><label>Old</label></ResearchProjectDef>"#,
        r#"<Operation Class="Example.PatchOperationReplaceResearchCoords">
                 <xpath>Defs/ResearchProjectDef[defName="SteppingStone1"]</xpath>
                 <researchViewX>5.5</researchViewX>
                 <researchViewY>6.5</researchViewY>
               </Operation>"#,
        "ResearchProjectDef",
        "SteppingStone1",
        &example_behaviours(),
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(
        text_at(&outcome, "researchViewX"),
        Content::Text("5.5".to_string())
    );
    assert_eq!(
        text_at(&outcome, "researchViewY"),
        Content::Text("6.5".to_string())
    );
}

/// The `Skip()` gate: `<doesRequire>` naming a mod that isn't active
/// skips the op outright (a no-op, not a failure).
#[test]
fn replace_research_coords_does_require_gate_skips_when_not_all_active() {
    let outcome = replay_one_using(
        r#"<ResearchProjectDef><researchViewX>1.0</researchViewX></ResearchProjectDef>"#,
        r#"<Operation Class="Example.PatchOperationReplaceResearchCoords">
                 <xpath>Defs/ResearchProjectDef[defName="SteppingStone1"]</xpath>
                 <doesRequire>mod.c</doesRequire>
                 <researchViewX>5.5</researchViewX>
                 <researchViewY>6.5</researchViewY>
               </Operation>"#,
        "ResearchProjectDef",
        "SteppingStone1",
        &example_behaviours(),
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(
        text_at(&outcome, "researchViewX"),
        Content::Text("1.0".to_string())
    );
}

/// `Cond_ModsLoaded` skips when the named mod is inactive, and runs
/// normally when it is active.
#[test]
fn replace_research_coords_cond_mods_loaded_gate() {
    let inactive_named = replay_one_using(
        r#"<ResearchProjectDef><researchViewX>1.0</researchViewX></ResearchProjectDef>"#,
        r#"<Operation Class="Example.PatchOperationReplaceResearchCoords">
                 <xpath>Defs/ResearchProjectDef[defName="SteppingStone1"]</xpath>
                 <conditionalType>Cond_ModsLoaded</conditionalType>
                 <conditionalParam>mod.c</conditionalParam>
                 <researchViewX>5.5</researchViewX>
                 <researchViewY>6.5</researchViewY>
               </Operation>"#,
        "ResearchProjectDef",
        "SteppingStone1",
        &example_behaviours(),
    );
    assert!(inactive_named.error.is_none(), "{:?}", inactive_named.error);
    assert_eq!(
        text_at(&inactive_named, "researchViewX"),
        Content::Text("1.0".to_string())
    );

    let active_named = replay_one_using(
        r#"<ResearchProjectDef><researchViewX>1.0</researchViewX></ResearchProjectDef>"#,
        r#"<Operation Class="Example.PatchOperationReplaceResearchCoords">
                 <xpath>Defs/ResearchProjectDef[defName="SteppingStone1"]</xpath>
                 <conditionalType>Cond_ModsLoaded</conditionalType>
                 <conditionalParam>mod.a</conditionalParam>
                 <researchViewX>5.5</researchViewX>
                 <researchViewY>6.5</researchViewY>
               </Operation>"#,
        "ResearchProjectDef",
        "SteppingStone1",
        &example_behaviours(),
    );
    assert!(active_named.error.is_none(), "{:?}", active_named.error);
    assert_eq!(
        text_at(&active_named, "researchViewX"),
        Content::Text("5.5".to_string())
    );
}

/// `Cond_ModsNotLoaded` is `Cond_ModsLoaded`'s mirror: it skips when
/// the named mod *is* active.
#[test]
fn replace_research_coords_cond_mods_not_loaded_skips_when_the_named_mod_is_active() {
    let outcome = replay_one_using(
        r#"<ResearchProjectDef><researchViewX>1.0</researchViewX></ResearchProjectDef>"#,
        r#"<Operation Class="Example.PatchOperationReplaceResearchCoords">
                 <xpath>Defs/ResearchProjectDef[defName="SteppingStone1"]</xpath>
                 <conditionalType>Cond_ModsNotLoaded</conditionalType>
                 <conditionalParam>mod.a</conditionalParam>
                 <researchViewX>5.5</researchViewX>
                 <researchViewY>6.5</researchViewY>
               </Operation>"#,
        "ResearchProjectDef",
        "SteppingStone1",
        &example_behaviours(),
    );
    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(
        text_at(&outcome, "researchViewX"),
        Content::Text("1.0".to_string())
    );
}

/// `Cond_Surgery`/`Cond_Power` read the user's own mod settings, which
/// this replay has no access to — any `conditionalType` besides the
/// two IL-verified ones stays `Unsupported` rather than guessing.
#[test]
fn replace_research_coords_an_unmodelled_conditional_type_is_unsupported() {
    let outcome = replay_one_using(
        r#"<ResearchProjectDef><researchViewX>1.0</researchViewX></ResearchProjectDef>"#,
        r#"<Operation Class="Example.PatchOperationReplaceResearchCoords">
                 <xpath>Defs/ResearchProjectDef[defName="SteppingStone1"]</xpath>
                 <conditionalType>Cond_Surgery</conditionalType>
                 <researchViewX>5.5</researchViewX>
                 <researchViewY>6.5</researchViewY>
               </Operation>"#,
        "ResearchProjectDef",
        "SteppingStone1",
        &example_behaviours(),
    );
    assert!(outcome.error.is_some());
}

/// A missing `<researchViewY>` on the operation itself is
/// `Unsupported`, never silently applied with only one coordinate.
#[test]
fn replace_research_coords_missing_a_coordinate_child_is_unsupported() {
    let outcome = replay_one_using(
        r#"<ResearchProjectDef><researchViewX>1.0</researchViewX></ResearchProjectDef>"#,
        r#"<Operation Class="Example.PatchOperationReplaceResearchCoords">
                 <xpath>Defs/ResearchProjectDef[defName="SteppingStone1"]</xpath>
                 <researchViewX>5.5</researchViewX>
               </Operation>"#,
        "ResearchProjectDef",
        "SteppingStone1",
        &example_behaviours(),
    );
    assert!(outcome.error.is_some());
}

// --- Items 3 + 8: text() ---------------------------------------------

#[test]
fn a_text_node_replace_sets_the_elements_text() {
    let operation = fixture("sandbags_tex_path_text_replace.xml");
    let outcome = replay_one_for(
        "<ThingDef><graphicData><texPath>Things/Building/Security/Sandbags_Atlas</texPath></graphicData></ThingDef>",
        &operation,
        "ThingDef",
        "Sandbags",
    );

    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert_eq!(
        text_at(&outcome, "graphicData/texPath"),
        Content::Text("ExampleThings/Security/Linked/SandbagsUpscaled_Atlas".to_string())
    );
}

#[test]
fn a_text_node_remove_clears_the_elements_text_without_removing_it() {
    let outcome = replay_one(
        "<ThingDef><label>old</label></ThingDef>",
        r#"<Operation Class="PatchOperationRemove">
                 <xpath>Defs/ThingDef[defName="W"]/label/text()</xpath>
               </Operation>"#,
    );
    assert_eq!(text_at(&outcome, "label"), Content::Empty);
}

/// `text()` selects an empty node set on an element with no text, so
/// the op matches nothing — a failure, not a silent write.
#[test]
fn a_text_node_op_on_an_element_without_text_matches_nothing() {
    let outcome = replay_one(
        "<ThingDef><comps><li>x</li></comps></ThingDef>",
        r#"<Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="W"]/comps/text()</xpath>
                 <value>nope</value>
               </Operation>"#,
    );
    assert!(outcome.error.is_none());
    assert!(matches!(&outcome.caveats[0], Caveat::FailedOp { .. }));
}

/// Only Replace and Remove model a text node; anything else is
/// `Unsupported` rather than applied to the element instead.
#[test]
fn other_ops_on_a_text_node_are_unsupported() {
    for class in [
        "PatchOperationAdd",
        "PatchOperationInsert",
        "PatchOperationSetName",
        "PatchOperationAttributeSet",
    ] {
        let operation = format!(
            r#"<Operation Class="{class}">
                     <xpath>Defs/ThingDef[defName="W"]/label/text()</xpath>
                     <name>x</name>
                     <attribute>x</attribute>
                     <value><y>2</y></value>
                   </Operation>"#
        );
        let outcome = replay_one("<ThingDef><label>old</label></ThingDef>", &operation);
        assert!(outcome.error.is_some(), "{class}");
    }
}

// --- Items 2 + 9: root predicates ------------------------------------

/// `[not(comps)]` on the def node decides whether the op matches at
/// all: RimWorld's own guard for "only patch defs that don't already
/// have this".
#[test]
fn a_root_predicate_decides_whether_the_op_matches() {
    let operation = r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="W"][not(comps)]</xpath>
                 <value><comps/></value>
               </Operation>"#;

    let holds = replay_one("<ThingDef><label>x</label></ThingDef>", operation);
    assert!(holds.error.is_none());
    assert!(holds.tree.get(&"comps".parse().unwrap()).is_some());

    let fails = replay_one(
        r#"<ThingDef><comps><li Class="A"/></comps></ThingDef>"#,
        operation,
    );
    assert!(fails.error.is_none());
    assert_eq!(children_of(&fails, "comps").len(), 1);
    assert!(matches!(&fails.caveats[0], Caveat::FailedOp { .. }));
}

/// A bare `[comps]` is XPath's child-existence test — the other half
/// of `not(...)`.
#[test]
fn a_child_existence_root_predicate_gates_the_op_too() {
    let operation = r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="W"][comps]/comps</xpath>
                 <value><li Class="B"/></value>
               </Operation>"#;

    let holds = replay_one(
        r#"<ThingDef><comps><li Class="A"/></comps></ThingDef>"#,
        operation,
    );
    assert_eq!(children_of(&holds, "comps").len(), 2);

    let fails = replay_one("<ThingDef><label>x</label></ThingDef>", operation);
    assert!(matches!(&fails.caveats[0], Caveat::FailedOp { .. }));
}

/// A false root predicate sends a Conditional down its `nomatch`
/// branch, exactly like a step that matched nothing would.
#[test]
fn a_false_root_predicate_takes_a_conditionals_nomatch_branch() {
    let outcome = replay_one(
        r#"<ThingDef><comps><li Class="A"/></comps></ThingDef>"#,
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>Defs/ThingDef[defName="W"][not(comps)]</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/comps</xpath>
                   <value><li Class="matched"/></value>
                 </match>
                 <nomatch Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/comps</xpath>
                   <value><li Class="notmatched"/></value>
                 </nomatch>
               </Operation>"#,
    );
    let comps = children_of(&outcome, "comps");
    assert_eq!(
        comps.last().unwrap().attrs.get("Class").map(String::as_str),
        Some("notmatched")
    );
}

/// A root predicate this crate can't answer from one def's tree is
/// `Unsupported`, never guessed.
#[test]
fn a_root_predicate_on_a_discarded_attribute_is_unsupported() {
    let outcome = replay_one(
        "<ThingDef><label>x</label></ThingDef>",
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="W"][@Abstract="True"]</xpath>
                 <value><comps/></value>
               </Operation>"#,
    );
    assert!(outcome.error.is_some());
}

// --- Root-targeting ops that were silently no-ops --------------------

/// Replacing or inserting-beside the def node itself needs the
/// enclosing `<Defs>` document. Unrefused, these would walk off the end
/// of the path-addressed mutators and report `Ok` having done nothing —
/// a wrong outcome presented as a real one. **`PatchOperationRemove` is
/// deliberately not in this list** — it's
/// modelled instead (`patch_operation_remove_on_the_def_root_is_modelled_not_refused`,
/// above) — see that test and `reject_root_target`'s own doc comment
/// for why `Remove` alone can express "this node is gone" without the
/// whole document.
#[test]
fn replacing_or_inserting_beside_the_def_node_itself_is_unsupported_not_a_silent_no_op() {
    for class in ["PatchOperationReplace", "PatchOperationInsert"] {
        let operation = format!(
            r#"<Operation Class="{class}">
                     <xpath>Defs/ThingDef[defName="W"]</xpath>
                     <value><ThingDef><label>new</label></ThingDef></value>
                   </Operation>"#
        );
        let outcome = replay_one("<ThingDef><label>old</label></ThingDef>", &operation);
        assert!(outcome.error.is_some(), "{class}");
    }
}

// --- Document root (`/Defs`) scoping ----------------------------------
//
// Real case: a mod-settings framework's own option-wrapper class
// (structurally a sequence — see `has_operations_child`) bundles a
// `PatchOperationAdd` on `<xpath>/Defs</xpath>` (adding a whole new
// top-level def) alongside ops on other, unrelated defs. Replaying that
// root add for one of those other defs must not fail outright (a bare
// `/Defs` is a recognized head), or it would stop the whole replay
// before it ever reached the op that actually mattered
// (`BiomeDef/AridShrubland` in `General-Patches.xml`).

/// An addition aimed at the bare document root can never affect an
/// existing def's own node — it is simply not this replay's to run,
/// exactly like an op whose xpath names a different def outright: no
/// error, no caveat, and the def's tree is untouched.
#[test]
fn a_root_targeted_addition_succeeds_elsewhere_without_touching_the_def() {
    for (class, extra) in [
        ("PatchOperationAdd", ""),
        ("PatchOperationInsert", ""),
        ("PatchOperationAddModExtension", ""),
    ] {
        for xpath in ["/Defs", "Defs", "/Defs/"] {
            let operation = format!(
                r#"<Operation Class="{class}">
                         <xpath>{xpath}</xpath>
                         {extra}
                         <value><WorldGenStepDef><defName>New</defName></WorldGenStepDef></value>
                       </Operation>"#
            );
            let outcome = replay_one(
                "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
                &operation,
            );
            assert!(outcome.error.is_none(), "{class} {xpath}");
            assert!(outcome.caveats.is_empty(), "{class} {xpath}");
            let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
            assert_eq!(
                stat_bases.content,
                Content::Children(vec![FieldNode {
                    tag: "x".to_string(),
                    attrs: BTreeMap::new(),
                    content: Content::Text("1".to_string()),
                }]),
                "{class} {xpath}"
            );
        }
    }
}

/// A Replace/Remove/SetName/attribute op aimed at the document root
/// needs the whole `<Defs>` document to express — stays `Unsupported`,
/// naming the class, never a silent no-op.
#[test]
fn a_root_targeted_rewrite_stays_unsupported() {
    for class in [
        "PatchOperationReplace",
        "PatchOperationRemove",
        "PatchOperationSetName",
        "PatchOperationAttributeSet",
    ] {
        let operation = format!(
            r#"<Operation Class="{class}">
                     <xpath>/Defs</xpath>
                     <value><ThingDef/></value>
                   </Operation>"#
        );
        let outcome = replay_one("<ThingDef><label>old</label></ThingDef>", &operation);
        let Some(ReplayError::Unsupported { reason, .. }) = &outcome.error else {
            panic!("{class}: expected Unsupported, got {:?}", outcome.error);
        };
        assert!(reason.contains(class), "{class}: {reason}");
    }
}

/// A `PatchOperationTest`/`PatchOperationConditional` on the document
/// root always matches — `/Defs` always exists — so its `<match>`
/// branch runs.
#[test]
fn a_conditional_on_the_document_root_always_matches() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationConditional">
                 <xpath>/Defs</xpath>
                 <match Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><y>2</y></value>
                 </match>
                 <nomatch Class="PatchOperationAdd">
                   <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                   <value><z>3</z></value>
                 </nomatch>
               </Operation>"#,
    );
    assert!(outcome.error.is_none());
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    assert_eq!(children.len(), 2);
    assert_eq!(children[1].tag, "y");
}

/// A `PatchOperationTest` whose xpath is the document root never fails
/// a Sequence — it always matches, so the rest of the Sequence keeps
/// running.
#[test]
fn a_test_on_the_document_root_never_aborts_a_sequence() {
    let outcome = replay_one(
        "<ThingDef><statBases><x>1</x></statBases></ThingDef>",
        r#"<Operation Class="PatchOperationSequence">
                 <operations>
                   <li Class="PatchOperationTest">
                     <xpath>/Defs</xpath>
                   </li>
                   <li Class="PatchOperationAdd">
                     <xpath>Defs/ThingDef[defName="W"]/statBases</xpath>
                     <value><y>2</y></value>
                   </li>
                 </operations>
               </Operation>"#,
    );
    assert!(outcome.error.is_none());
    let stat_bases = outcome.tree.get(&"statBases".parse().unwrap()).unwrap();
    let Content::Children(children) = &stat_bases.content else {
        unreachable!()
    };
    assert_eq!(children.len(), 2);
}

/// The real-world shape this pass fixes: a root `Add` (a mod bundling
/// an unrelated whole-new-def addition into the same patch file) before
/// *and* after a `Replace` that actually targets the def being
/// replayed. Before this pass, the first root `Add` alone stopped the
/// whole replay (`ReplayError::Unsupported`) before the `Replace` ever
/// ran; now both root adds are no-ops here and the `Replace` goes
/// through cleanly.
#[test]
fn a_root_add_before_and_after_a_real_replace_still_replays_the_replace() {
    let tree = xml::parse("<BiomeDef><plantDensity>0.65</plantDensity></BiomeDef>").unwrap();
    let (active, names) = ctx();
    let mod_id = ModId::new("mod.a");
    let context = ReplayContext {
        active_mods: &active,
        mod_names_by_display: &names,
        def_type: "BiomeDef",
        def_name: "AridShrubland",
        selector: Selector::DefName,
        def_exists: def_existence_unknown(),
        this_def_present: true,
        behaviours: PatchOperationBehaviours::none(),
    };
    let root_add = r#"<Operation Class="PatchOperationAdd">
                             <xpath>/Defs</xpath>
                             <value><WorldGenStepDef><defName>New</defName></WorldGenStepDef></value>
                           </Operation>"#;
    let replace = r#"<Operation Class="PatchOperationReplace">
                            <xpath>Defs/BiomeDef[defName="AridShrubland"]/plantDensity</xpath>
                            <value><plantDensity>0.9</plantDensity></value>
                          </Operation>"#;

    let outcome = replay(
        tree,
        &[
            PatchContribution {
                mod_id: &mod_id,
                operation_xml: root_add,
            },
            PatchContribution {
                mod_id: &mod_id,
                operation_xml: replace,
            },
            PatchContribution {
                mod_id: &mod_id,
                operation_xml: root_add,
            },
        ],
        &context,
    );

    assert!(outcome.error.is_none(), "{:?}", outcome.error);
    assert!(outcome.caveats.is_empty());
    let density = outcome.tree.get(&"plantDensity".parse().unwrap()).unwrap();
    assert_eq!(density.content, Content::Text("0.9".to_string()));
}
