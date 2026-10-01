//! Tests for merge planning.

use rim_analyzer::analysis::{IndexedPatchOp, SourceIndex};
use rim_analyzer::domain::{DefEntry, PatchOp, TemplateEntry};
use rim_merge::diff::DiffClass;
use rim_merge::plan::PlanOp;
use rim_merge::tree::PathSegment;
use rim_resolve::domain::{DefKey, OrderSource};

use super::*;
use crate::ports::{DefSourceError, ElementExpectation};
use crate::test_support::{
    arid_shrubland_wild_animals_fixture, bionic_heart_fixture, bionic_heart_fixture_flat,
    clean_override_fixture, comp_class_trigger_fixture, conflicting_wall_fixture,
    five_owner_scope_fixture, locator, root_class_trigger_fixture, session_with_sources,
    session_with_sources_and_mods, thing_class_trigger_fixture,
};
use rim_resolve::domain::{Action, MergeState};

/// `arid_shrubland_wild_animals_fixture`'s own `FindingKey` — every
/// contributor.
fn arid_shrubland_wild_animals_key() -> FindingKey {
    FindingKey::PatchCollision {
        key: DefKey {
            def_type: "BiomeDef".to_string(),
            def_name: "AridShrubland".to_string(),
        },
        selector: Selector::DefName,
        sub_path: Some("wildAnimals".to_string()),
        mods: [
            ModId::new("a.mod"),
            ModId::new("b.mod"),
            ModId::new("c.mod"),
        ]
        .into_iter()
        .collect(),
    }
}

/// A minimal, always-mutating `PatchOperationAdd`-style
/// [`IndexedPatchOp`] targeting `ThingDef/Wall`'s `statBases` — enough
/// for [`top_level_operations`] to see it, with no other field
/// populated realistically since nothing downstream of the
/// scope-restriction filter under test ever reads them.
fn wall_add_op(mod_id: ModId, op_locator: XmlLocator) -> IndexedPatchOp {
    IndexedPatchOp {
        mod_id,
        op: PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
            xpath: Some(r#"Defs/ThingDef[defName="Wall"]/statBases"#.to_string()),
            target: None,
            find_mod_context: Vec::new(),
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
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

fn bionic_heart_key() -> FindingKey {
    FindingKey::DefOverride {
        key: DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
        },
        owners: [
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork"),
        ]
        .into_iter()
        .collect(),
    }
}

/// The `BionicHeart` worked example under the structural guard: every
/// field difference is still `OneSided` toward the winner (BIONICS), so the
/// per-field fold alone would still produce a no-op plan — but BIONICS's
/// own `ParentName` (`addedPartExampleSynth`) differs from Core's
/// (`AddedBodyPartBase`), so the guard fires on this exact def
/// regardless. This is a deliberate trap: `bionic_heart_fixture`'s two
/// owners' different `ParentName`s happen to resolve to the same effective
/// fields two hops up (both climb to `ImplantHediffBase` eventually), and
/// the guard still fires anyway — there is no template-equivalence check,
/// so this scenario previews `NeedsFieldInput`, never
/// `Complete { op_count: 0 }`.
#[test]
fn the_bionic_heart_preview_is_guarded_by_its_own_parent_name_difference() {
    let fixture = bionic_heart_fixture();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let use_case = PlanMerge::new(fixture.reader);

    let preview = use_case
        .execute(&mut session, &bionic_heart_key())
        .expect("planning must succeed");

    assert_eq!(preview.base, ModId::new("ludeon.rimworld"));
    assert_eq!(preview.winner, ModId::new("example.bionicsfork"));
    assert_eq!(
        preview.owners,
        vec![
            ModId::new("ludeon.rimworld"),
            ModId::new("example.bionicsfork")
        ]
    );
    assert_eq!(
        preview.structural_change,
        Some(rim_merge::diff::StructuralChange {
            field: rim_merge::diff::StructuralField::ParentName,
            by: ModId::new("example.bionicsfork"),
        })
    );
    let total_fields = preview.diff.fields.len();
    assert_eq!(
        preview.state,
        MergeState::NeedsFieldInput {
            unresolved: total_fields,
            total: total_fields,
        },
        "the guard forces NeedsFieldInput even though every field would \
             otherwise auto-resolve to a no-op: {:#?}",
        preview.state
    );
    // `defaultLabelColor` is inherited from BIONICS's own template chain
    // and absent from Core's — the diff must still surface it,
    // unaffected by the guard (only `state` changes, never
    // `diff.fields` itself — see `state_from_plan`'s own doc comment
    // for why).
    assert!(
        preview
            .diff
            .fields
            .iter()
            .any(|field| field.path.to_string() == "defaultLabelColor")
    );
}

/// A stored choice for every genuinely differing field still can't
/// clear the guard: the guard fires on the def as a whole, not on any one
/// field, so even a "fully resolved" set of choices leaves the
/// preview `NeedsFieldInput` — proving `redecide_for_clean_merge` (only
/// ever promotes from `MergeState::Complete`) can never promote this
/// finding to `Merge`, no matter what the user picks per field.
#[test]
fn a_stored_choice_never_clears_the_guard() {
    let fixture = bionic_heart_fixture();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    session
        .decide(rim_resolve::domain::Decision {
            key: bionic_heart_key(),
            action: Action::Merge {
                key: DefKey {
                    def_type: "HediffDef".to_string(),
                    def_name: "BionicHeart".to_string(),
                },
                choices: [(
                    "label".parse().unwrap(),
                    MergeChoice::From {
                        mod_id: ModId::new("ludeon.rimworld"),
                    },
                )]
                .into_iter()
                .collect(),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("merge is always a valid action");
    let use_case = PlanMerge::new(fixture.reader);

    let preview = use_case
        .execute(&mut session, &bionic_heart_key())
        .expect("planning must succeed");

    assert!(preview.structural_change.is_some());
    assert!(
        matches!(preview.state, MergeState::NeedsFieldInput { .. }),
        "{:?}",
        preview.state
    );
    // The plan itself is unaffected by the guard — the choice really
    // did resolve to a real op; only the overall `state` is forced.
    assert_eq!(preview.plan.ops.len(), 1);

    let suggestion = rim_resolve::domain::Suggestion {
        action: Action::Accept,
        confidence: rim_resolve::domain::Confidence::new(60).unwrap(),
        rationale: rim_resolve::domain::Rationale::DefOverrideUnexplained,
        alternatives: vec![rim_resolve::domain::Alternative {
            action: Action::Merge {
                key: DefKey {
                    def_type: "HediffDef".to_string(),
                    def_name: "BionicHeart".to_string(),
                },
                choices: BTreeMap::new(),
            },
            rationale: rim_resolve::domain::Rationale::MergeUnexplainedDefOverride,
        }],
    };
    let redecided = rim_resolve::domain::redecide_for_clean_merge(
        suggestion,
        &DefKey {
            def_type: "HediffDef".to_string(),
            def_name: "BionicHeart".to_string(),
        },
        &preview.state,
        false,
        preview.structural_guard_field().as_deref(),
        rim_resolve::domain::MergeFindingKind::DefOverride,
    );
    assert!(
        !matches!(redecided.action, Action::Merge { .. }),
        "a guarded def must never be promoted, however complete its \
             stored choices are: {redecided:?}"
    );
    assert!(
        redecided
            .alternatives
            .iter()
            .all(|alt| !matches!(alt.action, Action::Merge { .. })),
        "a guarded def must not even offer Merge as an alternative — it can never \
             complete, so there is nothing to lead with: {:?}",
        redecided.alternatives
    );
}

/// A choice naming the earlier (base) owner explicitly still resolves
/// to a real op — proves a real `MergeChoice` reaches the underlying
/// `rim_merge` plan, not just the no-choice default path above.
/// `label` is a plain leaf both owners' raw nodes already carry, so
/// this is an ordinary `PlanOp::Replace` on the winner's own existing
/// node. (`plan_def_override`'s rule 3 — an `Add` for a field only present
/// through inheritance, never declared in the winner's own raw node at all
/// — does not apply, and no fixture in this test module exercises rule
/// 3's own `Add` path.) Uses [`bionic_heart_fixture_flat`], not
/// [`bionic_heart_fixture`] itself: this test is about choice mechanics,
/// not the structural guard — see that fixture's own doc comment for why
/// it exists separately.
#[test]
fn a_stored_choice_produces_a_non_empty_plan() {
    let fixture = bionic_heart_fixture_flat();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    session
        .decide(rim_resolve::domain::Decision {
            key: bionic_heart_key(),
            action: Action::Merge {
                key: DefKey {
                    def_type: "HediffDef".to_string(),
                    def_name: "BionicHeart".to_string(),
                },
                choices: [(
                    "label".parse().unwrap(),
                    MergeChoice::From {
                        mod_id: ModId::new("ludeon.rimworld"),
                    },
                )]
                .into_iter()
                .collect(),
            },
            note: None,
            decided_at: jiff::Timestamp::UNIX_EPOCH,
        })
        .expect("merge is always a valid action");
    let use_case = PlanMerge::new(fixture.reader);

    let preview = use_case
        .execute(&mut session, &bionic_heart_key())
        .expect("planning must succeed");

    assert!(preview.structural_change.is_none());
    assert!(matches!(
        preview.state,
        MergeState::Complete { op_count: 1 }
    ));
    assert_eq!(preview.plan.ops.len(), 1);
}

/// Each structural-guard trigger field: `thingClass`, root `Class`, and a
/// comp's `Class` each
/// independently force `NeedsFieldInput` on an otherwise-clean,
/// two-owner override — `ParentName`'s own trigger is
/// [`the_bionic_heart_preview_is_guarded_by_its_own_parent_name_difference`]
/// above (the real-install fixture already demonstrates it).
#[test]
fn thing_class_difference_guards_the_preview() {
    let fixture = thing_class_trigger_fixture();
    let mut session =
        session_with_sources_and_mods(fixture.sources, fixture.report, &["core.mod", "winner.mod"]);
    let use_case = PlanMerge::new(fixture.reader);
    let key = widget_key();

    let preview = use_case
        .execute(&mut session, &key)
        .expect("planning must succeed");

    assert_eq!(
        preview.structural_change,
        Some(rim_merge::diff::StructuralChange {
            field: rim_merge::diff::StructuralField::ThingClass,
            by: ModId::new("winner.mod"),
        })
    );
    // The full variant, not just `matches!` — pins `unresolved ==
    // total` too: a loose `matches!` would still pass if
    // `state_from_plan`'s guarded branch fell back to
    // `plan.unresolved.len()` instead of `total_fields`.
    assert_eq!(
        preview.state,
        MergeState::NeedsFieldInput {
            unresolved: 2,
            total: 2
        },
        "{:?}",
        preview.state
    );
}

#[test]
fn root_class_difference_guards_the_preview() {
    let fixture = root_class_trigger_fixture();
    let mut session =
        session_with_sources_and_mods(fixture.sources, fixture.report, &["core.mod", "winner.mod"]);
    let use_case = PlanMerge::new(fixture.reader);
    let key = widget_key();

    let preview = use_case
        .execute(&mut session, &key)
        .expect("planning must succeed");

    assert_eq!(
        preview.structural_change,
        Some(rim_merge::diff::StructuralChange {
            field: rim_merge::diff::StructuralField::RootClass,
            by: ModId::new("winner.mod"),
        })
    );
    assert!(
        matches!(preview.state, MergeState::NeedsFieldInput { .. }),
        "{:?}",
        preview.state
    );
}

#[test]
fn comp_class_difference_guards_the_preview() {
    let fixture = comp_class_trigger_fixture();
    let mut session =
        session_with_sources_and_mods(fixture.sources, fixture.report, &["core.mod", "winner.mod"]);
    let use_case = PlanMerge::new(fixture.reader);
    let key = widget_key();

    let preview = use_case
        .execute(&mut session, &key)
        .expect("planning must succeed");

    assert_eq!(
        preview.structural_change,
        Some(rim_merge::diff::StructuralChange {
            field: rim_merge::diff::StructuralField::CompClass,
            by: ModId::new("winner.mod"),
        })
    );
    assert!(
        matches!(preview.state, MergeState::NeedsFieldInput { .. }),
        "{:?}",
        preview.state
    );
}

/// The negative: only a leaf value differs (`conflicting_wall_fixture`'s
/// own three-owner `<label>` conflict) — the guard must not fire, and
/// the preview is exactly what it would be without the guard (a genuine,
/// unrelated `DiffClass::Conflict` on `<label>` still makes it
/// `NeedsFieldInput`, but for the ordinary reason, not the guard).
#[test]
fn only_leaf_values_differing_never_fires_the_guard() {
    let fixture = conflicting_wall_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod"],
    );
    let use_case = PlanMerge::new(fixture.reader);
    let key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("core.mod"),
            ModId::new("a.mod"),
            ModId::new("b.mod"),
        ]
        .into_iter()
        .collect(),
    };

    let preview = use_case
        .execute(&mut session, &key)
        .expect("planning must succeed");

    assert!(preview.structural_change.is_none());
    assert_eq!(
        preview.state,
        MergeState::NeedsFieldInput {
            unresolved: 1,
            // 2, not 1: `defName` (`Unchanged`, all three owners
            // agree) is itself a diffed field alongside `label` — the
            // guard's absence, not the diff's field count, is what
            // this test is about.
            total: 2
        },
        "unchanged with the structural guard absent: one genuine label conflict, no \
             structural trigger: {:?}",
        preview.state
    );
}

fn widget_key() -> FindingKey {
    FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Widget".to_string(),
        },
        owners: [ModId::new("core.mod"), ModId::new("winner.mod")]
            .into_iter()
            .collect(),
    }
}

/// A locator whose seeded content no longer matches what it's
/// expected to (the def source changed since the scan) surfaces as
/// `PlanMergeError::Source(DefSourceError::Stale)`, never a silent
/// mis-read.
#[test]
fn a_stale_locator_surfaces_as_a_source_error() {
    let mut fixture = bionic_heart_fixture();
    let core_locator = locator("core_bionic.xml", 0);
    fixture.reader = fixture.reader.with_error(
        core_locator.clone(),
        DefSourceError::Stale {
            file: core_locator.file.to_path_buf(),
            path: core_locator.element_path.clone(),
            expected: "HediffDef/BionicHeart".to_string(),
        },
    );
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let use_case = PlanMerge::new(fixture.reader);

    let result = use_case.execute(&mut session, &bionic_heart_key());

    assert!(matches!(
        result,
        Err(PlanMergeError::Source(DefSourceError::Stale { .. }))
    ));
}

/// Real-install shape (a measurable share of real `DefOverride`
/// findings): an owner the finding's own `owners` set names has no entry
/// in `SourceIndex::defs` at all — `build_owner_versions` must still
/// surface this as `PlanMergeError::MissingSource`, not silently skip the
/// owner or panic.
#[test]
fn an_owner_with_no_source_index_entry_surfaces_as_a_missing_source_error() {
    let mut fixture = bionic_heart_fixture();
    fixture.sources.defs.remove(&(
        ModId::new("example.bionicsfork"),
        ("HediffDef".to_string(), "BionicHeart".to_string()),
    ));
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let use_case = PlanMerge::new(fixture.reader);

    let result = use_case.execute(&mut session, &bionic_heart_key());

    assert!(
        matches!(result, Err(PlanMergeError::MissingSource(_))),
        "{result:?}"
    );
}

/// A finding key naming neither a def override nor a patch collision
/// can never be merged.
#[test]
fn an_unsupported_finding_kind_is_rejected() {
    let fixture = bionic_heart_fixture();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let use_case = PlanMerge::new(fixture.reader);
    let key = FindingKey::MissingMod {
        mod_id: ModId::new("ludeon.rimworld"),
    };

    let result = use_case.execute(&mut session, &key);

    assert!(matches!(result, Err(PlanMergeError::UnsupportedFinding(_))));
}

/// A collision `sub_path` ending in `text()` addresses the *text*
/// of the element before it, which as a `FieldPath` is just that
/// element (its `Value::Leaf` is exactly that text) — so the
/// trailing step contributes no segment.
#[test]
fn a_text_node_sub_path_maps_to_the_element_that_owns_the_text() {
    let path = field_path_from_sub_path(Some("graphicData/texPath/text()"))
        .expect("a text() sub_path is representable")
        .expect("a non-empty sub_path yields a path");

    assert_eq!(path.to_string(), "graphicData/texPath");
}

/// A root predicate never reaches a `sub_path` (the analyzer strips
/// it), but the parser must still accept the shape it does produce.
#[test]
fn an_unrepresentable_sub_path_is_reported_rather_than_guessed() {
    let reason = field_path_from_sub_path(Some("comps/li[contains(x,\"y\")]"))
        .expect_err("an unsupported predicate must not be guessed at");

    assert!(reason.contains("contains("), "{reason}");
}

/// Previews are cached per [`OrderSource`] slot and survive
/// [`Session::select`] — computing one under `Current` must not
/// disturb (or be visible from) `Suggested`'s own cache.
#[test]
fn previews_are_cached_per_order_source() {
    let fixture = bionic_heart_fixture();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let use_case = PlanMerge::new(fixture.reader);

    use_case
        .execute(&mut session, &bionic_heart_key())
        .expect("planning under Current must succeed");
    assert!(
        session
            .merge_preview(
                &PreviewSlot::profile(OrderSource::Current),
                &bionic_heart_key()
            )
            .is_some()
    );
    assert!(
        session
            .merge_preview(
                &PreviewSlot::profile(OrderSource::Suggested),
                &bionic_heart_key()
            )
            .is_none(),
        "a preview computed under Current must not leak into Suggested's cache"
    );
}

/// A [`DefSourceReader`] that counts every [`DefSourceReader::read_element`]
/// call it forwards — for
/// [`a_second_identical_request_reuses_the_cached_preview_without_rereading_sources`],
/// which needs to *measure* that a second, identical build genuinely
/// skips the replay rather than merely asserting the same output twice.
struct CountingReader<R> {
    inner: R,
    reads: std::rc::Rc<std::cell::Cell<usize>>,
}

impl<R: DefSourceReader> DefSourceReader for CountingReader<R> {
    fn read_element(
        &self,
        locator: &XmlLocator,
        expected: &ElementExpectation,
    ) -> Result<String, DefSourceError> {
        self.reads.set(self.reads.get() + 1);
        self.inner.read_element(locator, expected)
    }
}

/// The per-`(slot, key, choices)` cache: a second [`PlanMerge::execute`] call for the
/// same finding, with nothing changed in between (no decision, no
/// resort), must reuse the cached preview outright rather than
/// replaying the def's sources again — measured with
/// [`CountingReader`] rather than merely re-asserting the same result,
/// since a rebuild that happens to produce byte-identical output would
/// otherwise look the same as a real cache hit.
#[test]
fn a_second_identical_request_reuses_the_cached_preview_without_rereading_sources() {
    let fixture = bionic_heart_fixture();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let reads = std::rc::Rc::new(std::cell::Cell::new(0));
    let reader = CountingReader {
        inner: fixture.reader,
        reads: reads.clone(),
    };
    let use_case = PlanMerge::new(reader);

    use_case
        .execute(&mut session, &bionic_heart_key())
        .expect("the first build must succeed");
    let after_first = reads.get();
    assert!(
        after_first > 0,
        "the first build must actually read at least one source"
    );

    use_case
        .execute(&mut session, &bionic_heart_key())
        .expect("the second, identical build must succeed");
    assert_eq!(
        reads.get(),
        after_first,
        "an identical second request (same stored choices, same scope) must not read any source again"
    );
}

/// [`PlanMerge::execute_in`] against a hand-built [`MergeContext`]
/// whose choices come from nowhere the session knows about proves the
/// planning logic itself never reaches for [`Session::decisions`] —
/// only [`PlanMerge::execute`]'s own profile delegation does that, to
/// build the context in the first place.
#[test]
fn execute_in_plans_against_the_given_context_not_the_sessions_own_decisions() {
    // `bionic_heart_fixture_flat`, not `bionic_heart_fixture`: this
    // test is about context isolation (profile vs. patch slot), not
    // the structural guard — see the flat fixture's own doc comment.
    let fixture = bionic_heart_fixture_flat();
    let mut session = session_with_sources(fixture.sources, fixture.report);
    let use_case = PlanMerge::new(fixture.reader);
    let key = bionic_heart_key();
    let patch_id: rim_resolve::domain::PatchId = "abcdef012345".parse().expect("valid patch id");
    let ctx = MergeContext {
        slot: PreviewSlot::patch(OrderSource::Current, patch_id),
        choices: [(
            "label".parse().unwrap(),
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        )]
        .into_iter()
        .collect(),
        scope: None,
    };

    let preview = use_case
        .execute_in(&mut session, ctx.clone(), &key)
        .expect("planning against a hand-built context must succeed");

    assert!(matches!(
        preview.state,
        MergeState::Complete { op_count: 1 }
    ));
    assert!(
        session
            .merge_preview(&PreviewSlot::profile(OrderSource::Current), &key)
            .is_none(),
        "a patch context's preview must never land in the profile's own slot"
    );
    assert!(session.merge_preview(&ctx.slot, &key).is_some());
}

// -- Scoped participant rule --

/// The field a [`PlanOp`] addresses — whichever of `path`/`parent` its
/// variant carries — for asserting *which* field a test's plan
/// touched, exhaustive so a new `PlanOp` variant is a compile error
/// here rather than a silently untested one.
fn op_path(op: &PlanOp) -> &FieldPath {
    match op {
        PlanOp::Replace { path, .. }
        | PlanOp::Remove { path }
        | PlanOp::ReplaceInheritFalse { path, .. }
        | PlanOp::SetAttribute { path, .. } => path,
        PlanOp::Add { parent, .. } => parent,
    }
}

fn five_owner_wall_key() -> FindingKey {
    FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("ludeon.rimworld"),
            ModId::new("a.mod"),
            ModId::new("b.mod"),
            ModId::new("c.mod"),
            ModId::new("d.mod"),
        ]
        .into_iter()
        .collect(),
    }
}

/// The worked five-owner example: a scope of `{b.mod, d.mod}` restricts a
/// `DefOverride`'s
/// diff participants to `ludeon.rimworld` (Core, always implicit),
/// `b.mod`, and `d.mod` — `a.mod`'s (uninteresting) and `c.mod`'s
/// (`fillPercent`) changes never reach the diff at all. Exactly one op
/// results (`statBases/MaxHitPoints`, `depends_on = {b.mod, d.mod}`);
/// the excluded owners surface as `Caveat::OutOfScopeOwners`.
#[test]
fn scoped_participant_rule_restricts_a_def_override_to_scope_and_core() {
    let fixture = five_owner_scope_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["ludeon.rimworld", "a.mod", "b.mod", "c.mod", "d.mod"],
    );
    let use_case = PlanMerge::new(fixture.reader);
    let scope =
        PatchScope::new([ModId::new("b.mod"), ModId::new("d.mod")]).expect("two distinct members");
    let patch_id: rim_resolve::domain::PatchId = "abcdef012345".parse().expect("valid patch id");
    let ctx = MergeContext {
        slot: PreviewSlot::patch(OrderSource::Current, patch_id),
        choices: BTreeMap::new(),
        scope: Some(scope),
    };

    let preview = use_case
        .execute_in(&mut session, ctx, &five_owner_wall_key())
        .expect("planning against a scoped context must succeed");

    assert_eq!(
        preview.owners,
        vec![
            ModId::new("ludeon.rimworld"),
            ModId::new("b.mod"),
            ModId::new("d.mod"),
        ],
        "participants must be exactly owners ∩ (scope ∪ Core), in the selected order"
    );
    assert_eq!(preview.base, ModId::new("ludeon.rimworld"));
    assert_eq!(preview.winner, ModId::new("d.mod"));
    assert!(matches!(
        preview.state,
        MergeState::Complete { op_count: 1 }
    ));
    assert_eq!(preview.plan.ops.len(), 1);
    assert_eq!(
        op_path(&preview.plan.ops[0].op).to_string(),
        "statBases/MaxHitPoints",
        "the worked example's only real op targets B's own contested field"
    );
    assert_eq!(
        preview.plan.ops[0].depends_on,
        [ModId::new("b.mod"), ModId::new("d.mod")]
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "the one op must carry exactly the winner and the field's contributor"
    );
    assert!(
        !preview.plan.ops.iter().any(|planned| {
            op_path(&planned.op)
                .segments()
                .iter()
                .any(|segment| matches!(segment, PathSegment::Child(name) if name == "fillPercent"))
        }),
        "c.mod's fillPercent change is out of scope and must never reach an op: {:?}",
        preview.plan.ops
    );
    let out_of_scope = preview
        .plan
        .caveats
        .iter()
        .find_map(|caveat| match caveat {
            Caveat::OutOfScopeOwners { mods } => Some(mods.clone()),
            _ => None,
        })
        .expect("a.mod and c.mod must be reported as out-of-scope owners");
    assert_eq!(out_of_scope, vec![ModId::new("a.mod"), ModId::new("c.mod")]);
}

/// A scope admitting fewer than two of the finding's owners (e.g. one
/// of the two scope members went inactive since the patch was
/// created) can't produce a diff at all — `CannotMerge`, not an error.
#[test]
fn fewer_than_two_scoped_participants_cannot_merge() {
    let fixture = five_owner_scope_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["ludeon.rimworld", "a.mod", "b.mod", "c.mod", "d.mod"],
    );
    let use_case = PlanMerge::new(fixture.reader);
    // Neither scope member owns this finding at all, so the only
    // participant left is Core itself — one participant, below the
    // two a diff needs.
    let scope = PatchScope::new([ModId::new("nobody1.mod"), ModId::new("nobody2.mod")])
        .expect("two distinct members");
    let patch_id: rim_resolve::domain::PatchId = "abcdef012345".parse().expect("valid patch id");
    let ctx = MergeContext {
        slot: PreviewSlot::patch(OrderSource::Current, patch_id),
        choices: BTreeMap::new(),
        scope: Some(scope),
    };

    let preview = use_case
        .execute_in(&mut session, ctx, &five_owner_wall_key())
        .expect("a preview always comes back, even when it cannot merge");

    assert!(
        matches!(preview.state, MergeState::CannotMerge { .. }),
        "{:?}",
        preview.state
    );
}

/// With *zero* owners inside `scope ∪ {Core}`
/// (not even one — every real owner is a stranger to this scope), the
/// `CannotMerge` reason must name the scope itself, never fall back to
/// naming an out-of-scope owner as if it belonged to the patch.
#[test]
fn zero_scoped_participants_names_the_scope_not_an_out_of_scope_owner() {
    let fixture = clean_override_fixture("Wall", "contributor.mod", "winner.mod");
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "contributor.mod", "winner.mod"],
    );
    let use_case = PlanMerge::new(fixture.reader);
    // None of `core.mod`/`contributor.mod`/`winner.mod` is
    // `ludeon.rimworld` (so Core never joins as an implicit
    // participant here) and none is in scope either.
    let scope = PatchScope::new([ModId::new("nobody1.mod"), ModId::new("nobody2.mod")])
        .expect("two distinct members");
    let patch_id: rim_resolve::domain::PatchId = "abcdef012345".parse().expect("valid patch id");
    let key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("core.mod"),
            ModId::new("contributor.mod"),
            ModId::new("winner.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let ctx = MergeContext {
        slot: PreviewSlot::patch(OrderSource::Current, patch_id),
        choices: BTreeMap::new(),
        scope: Some(scope),
    };

    let preview = use_case
        .execute_in(&mut session, ctx, &key)
        .expect("a preview always comes back, even when it cannot merge");

    match &preview.state {
        MergeState::CannotMerge { reason } => assert_eq!(
            reason, "no member of this patch's scope owns the def in the selected order",
            "must name the scope, not fall back to naming an out-of-scope owner"
        ),
        other => panic!("expected CannotMerge, got {other:?}"),
    }
}

/// The collision path's own fewer-than-two-participants guard,
/// mirroring [`fewer_than_two_scoped_participants_cannot_merge`]'s
/// def-override case: `plan_patch_collision` refuses not just an *empty*
/// `mods_in_order` but any collision with fewer than two scope members'
/// contributions, reporting `CannotMerge` the way one participant already
/// does for a def override instead of falling through into
/// `plan::plan_patch_collision`.
#[test]
fn a_scoped_collision_with_only_one_contributing_member_cannot_merge() {
    let fixture = five_owner_scope_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["ludeon.rimworld", "a.mod", "b.mod", "c.mod", "d.mod"],
    );
    let use_case = PlanMerge::new(fixture.reader);
    let scope =
        PatchScope::new([ModId::new("b.mod"), ModId::new("d.mod")]).expect("two distinct members");
    let patch_id: rim_resolve::domain::PatchId = "abcdef012345".parse().expect("valid patch id");
    // Only `b.mod` (in scope) and `c.mod` (out of scope) ever patch
    // this def, per the finding key itself — restricting to scope
    // leaves exactly one contributing member.
    let key = FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: Selector::DefName,
        sub_path: None,
        mods: [ModId::new("b.mod"), ModId::new("c.mod")]
            .into_iter()
            .collect(),
    };
    let ctx = MergeContext {
        slot: PreviewSlot::patch(OrderSource::Current, patch_id),
        choices: BTreeMap::new(),
        scope: Some(scope),
    };

    let preview = use_case
        .execute_in(&mut session, ctx, &key)
        .expect("a preview always comes back, even when it cannot merge");

    match &preview.state {
        MergeState::CannotMerge { reason } => assert_eq!(
            reason,
            "only b.mod of this patch's scope has a contribution to this collision in the selected order"
        ),
        other => panic!("expected CannotMerge, got {other:?}"),
    }
}

/// `plan_patch_collision` filters contributions down to scope *before*
/// reading any top-level op's source text — so a stale or unreadable file
/// from an out-of-scope mod (one whose ops a scoped preview was never
/// going to replay anyway) cannot fail the whole preview. `c.mod` here
/// patches the same def as the two scope members but is excluded from
/// scope and wired to error on read; the preview must still succeed,
/// having never asked the reader for `c.mod`'s file at all.
#[test]
fn out_of_scope_ops_are_filtered_before_reading_their_source() {
    let order_ids = ["core.mod", "b.mod", "c.mod", "d.mod"];

    let mut sources = SourceIndex::default();
    let raw_locator = locator("core_wall.xml", 0);
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
            locator: raw_locator.clone(),
        }],
    );

    let b_locator = locator("b_patch.xml", 0);
    let c_locator = locator("c_patch.xml", 0);
    let d_locator = locator("d_patch.xml", 0);
    sources.patch_ops_by_def.insert(
        (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        ),
        vec![
            wall_add_op(ModId::new("b.mod"), b_locator.clone()),
            wall_add_op(ModId::new("c.mod"), c_locator.clone()),
            wall_add_op(ModId::new("d.mod"), d_locator.clone()),
        ],
    );

    let mut elements = BTreeMap::new();
    elements.insert(raw_locator,
            "<ThingDef><defName>Wall</defName><statBases><MaxHitPoints>200</MaxHitPoints></statBases></ThingDef>"
                .to_string());
    elements.insert(
        b_locator.clone(),
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases</xpath>
                 <value><Plasteel>10</Plasteel></value>
               </Operation>"#
            .to_string(),
    );
    elements.insert(
        d_locator.clone(),
        r#"<Operation Class="PatchOperationAdd">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases</xpath>
                 <value><Steel>5</Steel></value>
               </Operation>"#
            .to_string(),
    );
    let reader = crate::test_support::InMemoryDefSourceReader::new(elements).with_error(
        c_locator.clone(),
        DefSourceError::Stale {
            file: c_locator.file.to_path_buf(),
            path: c_locator.element_path.clone(),
            expected: "ThingDef/Wall patch op".to_string(),
        },
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("core.mod")
        .mod_("b.mod")
        .mod_("c.mod")
        .mod_("d.mod")
        .build();
    let mut session = session_with_sources_and_mods(sources, report, &order_ids);
    let use_case = PlanMerge::new(reader);

    let scope =
        PatchScope::new([ModId::new("b.mod"), ModId::new("d.mod")]).expect("two distinct members");
    let patch_id: rim_resolve::domain::PatchId = "abcdef012345".parse().expect("valid patch id");
    let key = FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: Selector::DefName,
        sub_path: None,
        mods: [
            ModId::new("b.mod"),
            ModId::new("c.mod"),
            ModId::new("d.mod"),
        ]
        .into_iter()
        .collect(),
    };
    let ctx = MergeContext {
        slot: PreviewSlot::patch(OrderSource::Current, patch_id),
        choices: BTreeMap::new(),
        scope: Some(scope),
    };

    let preview = use_case
        .execute_in(&mut session, ctx, &key)
        .expect("a stale out-of-scope mod's file must never fail a scoped preview");

    assert_eq!(
        preview.owners,
        vec![ModId::new("b.mod"), ModId::new("d.mod")],
        "only the in-scope contributors must have been replayed"
    );
}

/// `PlanMerge`'s own half: `plan_patch_collision`'s own returned
/// `fields` (`rim_merge::diff::collision_fields`) expand `wildAnimals`
/// into one
/// `FieldDiff` per key — 4 `Unchanged` base keys, 3 disjoint `OneSided`
/// adds, and `Cobra` alone `Conflict{a,b,c}` (a/b agree at `0.5`, c
/// differs at `0.9`) — leaving exactly one field unresolved out of 8,
/// never a single whole-subtree field.
#[test]
fn arid_shrubland_wild_animals_collision_expands_per_key_with_one_conflict() {
    let fixture = arid_shrubland_wild_animals_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod", "c.mod"],
    );
    let use_case = PlanMerge::new(fixture.reader);
    let key = arid_shrubland_wild_animals_key();

    let preview = use_case
        .execute(&mut session, &key)
        .expect("planning must succeed");

    assert_eq!(
        preview.diff.fields.len(),
        8,
        "4 unchanged base keys + 3 disjoint adds + 1 contested key: {:#?}",
        preview.diff.fields
    );
    let field = |name: &str| {
        preview
            .diff
            .fields
            .iter()
            .find(|f| f.path.to_string() == format!("wildAnimals/{name}"))
            .unwrap_or_else(|| panic!("expected a field for {name}: {:#?}", preview.diff.fields))
    };
    for name in ["Tortoise", "Warg", "Camel", "Locust"] {
        assert_eq!(field(name).class, DiffClass::Unchanged, "{name}");
    }
    assert_eq!(
        field("Allosaurus").class,
        DiffClass::OneSided {
            by: ModId::new("a.mod")
        }
    );
    assert_eq!(
        field("Mammoth").class,
        DiffClass::OneSided {
            by: ModId::new("b.mod")
        }
    );
    assert_eq!(
        field("Hyena").class,
        DiffClass::OneSided {
            by: ModId::new("c.mod")
        }
    );
    assert_eq!(
        field("Cobra").class,
        DiffClass::Conflict {
            by: [
                ModId::new("a.mod"),
                ModId::new("b.mod"),
                ModId::new("c.mod")
            ]
            .into_iter()
            .collect()
        },
        "a/b agree at 0.5, c differs at 0.9 — never auto-resolved"
    );
    assert_eq!(
        preview.state,
        MergeState::NeedsFieldInput {
            unresolved: 1,
            total: 8
        },
        "only Cobra is unresolved"
    );
}

/// The per-`(slot, key, choices)` preview cache must still invalidate
/// correctly when `choices` holds one entry per contested *key* of a keyed
/// map, not a single `sub_path`-keyed choice: deciding `Cobra` alone must
/// produce a fresh preview, never the stale `NeedsFieldInput` one cached
/// before the decision — if the cache key and the per-entry choices
/// disagreed, a stale preview would be served for a different set of entry
/// choices.
#[test]
fn arid_shrubland_wild_animals_choice_on_one_key_replans_not_from_a_stale_cache() {
    let fixture = arid_shrubland_wild_animals_fixture();
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "a.mod", "b.mod", "c.mod"],
    );
    let use_case = PlanMerge::new(fixture.reader);
    let key = arid_shrubland_wild_animals_key();
    let slot = PreviewSlot::profile(session.selected());

    // Deliberately calls `execute_in` with hand-built contexts (not
    // `Session::decide` + `execute`) — `decide` itself would clear
    // this key's cache entry outright
    // (`invalidate_ledgers_for_merge_decision`), which would make this
    // test pass even if `merge_preview_matches`'s own choices
    // comparison were broken. Two `execute_in` calls against the
    // *same* slot/key with two different `choices` maps is what
    // actually exercises the per-`(slot, key, choices)`
    // cache comparison.
    let empty_ctx = MergeContext {
        slot: slot.clone(),
        choices: BTreeMap::new(),
        scope: None,
    };
    let before = use_case
        .execute_in(&mut session, empty_ctx, &key)
        .expect("planning must succeed");
    assert!(matches!(
        before.state,
        MergeState::NeedsFieldInput { unresolved: 1, .. }
    ));

    let decided_ctx = MergeContext {
        slot,
        choices: [(
            "wildAnimals/Cobra".parse().expect("valid path"),
            MergeChoice::From {
                mod_id: ModId::new("a.mod"),
            },
        )]
        .into_iter()
        .collect(),
        scope: None,
    };
    let after = use_case
        .execute_in(&mut session, decided_ctx, &key)
        .expect("planning under a different choices map must succeed");

    assert_eq!(
        after.state,
        MergeState::Complete { op_count: 1 },
        "a choices map naming From(a.mod) for wildAnimals/Cobra \
             (differing from the empty map cached above) must never reuse \
             the stale NeedsFieldInput preview — the cache must compare \
             the *whole* per-entry choices map, not a single \
             sub_path-keyed lookup, now that a keyed-map collision's own \
             choices span several FieldPaths under one sub_path"
    );
}

/// Nearest-registrant template resolution, proven inside `PlanMerge`
/// itself: a `DefOverride`'s own per-owner candidate construction walks
/// each owner's `ParentName` chain starting from *that owner's own*
/// position, not from "the earliest registrant". Two owners of
/// `ThingDef/Wall`, both inheriting `<fuel>` purely from a
/// `ParentName="Base"` with three registrants, sit on either side of the
/// middle registrant and must resolve to *different* answers — proving
/// the resolution is genuinely per-owner, not a single global pick.
#[test]
fn a_def_overrides_own_owners_each_resolve_an_ambiguous_parent_template_from_their_own_position() {
    let r1 = ModId::new("r1.mod");
    let owner_a = ModId::new("owner_a.mod");
    let r2 = ModId::new("r2.mod");
    let owner_b = ModId::new("owner_b.mod");
    let r3 = ModId::new("r3.mod");

    let r1_locator = locator("r1_base.xml", 0);
    let r2_locator = locator("r2_base.xml", 0);
    let r3_locator = locator("r3_base.xml", 0);
    let owner_a_locator = locator("owner_a_wall.xml", 0);
    let owner_b_locator = locator("owner_b_wall.xml", 0);

    let mut sources = SourceIndex::default();
    sources.templates.insert(
        ("ThingDef".to_string(), "Base".to_string()),
        vec![
            (
                r1.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: r1_locator.clone(),
                },
            ),
            (
                r2.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: r2_locator.clone(),
                },
            ),
            (
                r3.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: r3_locator.clone(),
                },
            ),
        ],
    );
    for (owner, entry_locator) in [(&owner_a, &owner_a_locator), (&owner_b, &owner_b_locator)] {
        sources.defs.insert(
            (owner.clone(), ("ThingDef".to_string(), "Wall".to_string())),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: Some("Base".to_string()),
                locator: entry_locator.clone(),
            }],
        );
    }

    let mut elements = BTreeMap::new();
    elements.insert(
        r1_locator,
        "<ThingDef Name=\"Base\"><fuel>1</fuel></ThingDef>".to_string(),
    );
    elements.insert(
        r2_locator,
        "<ThingDef Name=\"Base\"><fuel>2</fuel></ThingDef>".to_string(),
    );
    elements.insert(
        r3_locator,
        "<ThingDef Name=\"Base\"><fuel>3</fuel></ThingDef>".to_string(),
    );
    elements.insert(
        owner_a_locator,
        "<ThingDef ParentName=\"Base\"><defName>Wall</defName></ThingDef>".to_string(),
    );
    elements.insert(
        owner_b_locator,
        "<ThingDef ParentName=\"Base\"><defName>Wall</defName></ThingDef>".to_string(),
    );

    let report = rim_resolve::test_support::ReportBuilder::new()
        .mod_("r1.mod")
        .mod_("owner_a.mod")
        .mod_("r2.mod")
        .mod_("owner_b.mod")
        .mod_("r3.mod")
        .def_override("ThingDef", "Wall", &["owner_a.mod", "owner_b.mod"])
        .build();
    let mut session = session_with_sources_and_mods(
        sources,
        report,
        &["r1.mod", "owner_a.mod", "r2.mod", "owner_b.mod", "r3.mod"],
    );
    let use_case = PlanMerge::new(crate::test_support::InMemoryDefSourceReader::new(elements));

    let key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [owner_a.clone(), owner_b.clone()].into_iter().collect(),
    };
    let preview = use_case
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let fuel = preview
        .diff
        .fields
        .iter()
        .find(|f| f.path.to_string() == "fuel")
        .expect("fuel must appear in the diff — both owners inherit it, and they differ");
    assert_eq!(
        fuel.candidates.get(&owner_a),
        Some(&rim_merge::diff::Value::Leaf("1".to_string())),
        "owner_a (position 1) must inherit r1 (position 0) — nearest at or before it"
    );
    assert_eq!(
        fuel.candidates.get(&owner_b),
        Some(&rim_merge::diff::Value::Leaf("2".to_string())),
        "owner_b (position 3) must inherit r2 (position 2), not r1 (the old A1 rule) or r3 (loads after owner_b, invisible to it)"
    );
}

/// Pins `MergePreview::final_values`. Uses
/// [`crate::test_support::clean_override_fixture`] specifically
/// because `description` (`contributor.mod`'s own field) is `OneSided`
/// by a mod that is *not* the winner — `field.candidates.get(&winner)`
/// for that path is `Value::Absent` (the winner never sets
/// `description` at all), so a regression that wired the `DefOverride`
/// branch to `candidates.get(&winner)` instead of
/// `rim_merge::plan::resolved_field_values` would still pass every
/// other test in this suite (fixtures where the two coincide) but fails
/// this one outright.
#[test]
fn final_values_reports_the_merges_own_resolved_value_not_the_winners_isolated_candidate() {
    let fixture =
        crate::test_support::clean_override_fixture("Wall", "contributor.mod", "winner.mod");
    let mut session = session_with_sources_and_mods(
        fixture.sources,
        fixture.report,
        &["core.mod", "contributor.mod", "winner.mod"],
    );
    let use_case = PlanMerge::new(fixture.reader);
    let key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("core.mod"),
            ModId::new("contributor.mod"),
            ModId::new("winner.mod"),
        ]
        .into_iter()
        .collect(),
    };

    let preview = use_case
        .execute(&mut session, &key)
        .expect("planning must succeed");

    let description_path: rim_merge::tree::FieldPath = "description".parse().unwrap();
    assert_eq!(
        preview
            .diff
            .fields
            .iter()
            .find(|f| f.path == description_path)
            .map(|f| f.candidates.get(&ModId::new("winner.mod"))),
        Some(Some(&rim_merge::diff::Value::Absent)),
        "sanity check: the winner never sets description at all"
    );
    assert_eq!(
        preview.final_values.get(&description_path),
        Some(&rim_merge::diff::Value::Leaf("a plain Wall".to_string())),
        "final must report the merge's own resolved value (contributor.mod's own text), \
             never the winner's own (absent) candidate: {:#?}",
        preview.final_values
    );
}
