//! Tests for the assignment commands.

use std::collections::BTreeMap;
use std::sync::Arc;

use rim_analyzer::analysis::SourceIndex;
use rim_analyzer::domain::{DefEntry, XmlLocator};
use rim_resolve::domain::{Cardinality, FieldRole, FieldSpec};
use rim_resolve::test_support::ReportBuilder;

use super::coverage::{get_assignment_coverage_inner, list_assignment_items_inner};
use super::export::export_assignment_inner;
use super::projects::{
    create_assignment_inner, delete_assignment_inner, get_assignment_inner,
    infer_assignment_candidate_inner, list_assignment_candidates_inner, list_assignments_inner,
    parse_assignment_id, update_assignment_inner,
};
use super::rows::{
    clear_assignment_row_inner, copy_assignment_row_from_inner, set_assignment_row_inner,
};
use super::sections::{add_assignment_section_inner, remove_assignment_section_inner};
use super::*;
use crate::dto::assignment::{
    AssignmentRowDto, AssignmentSchemaDto, CreateAssignmentRequestDto, FieldSpecMapDto,
    InferAssignmentCandidateRequestDto, ListAssignmentCandidatesRequestDto, ListItemsFilterDto,
    TargetRefDto,
};
use crate::dto::common::DefKeyDto;
use crate::error::{CommandErrorCode, CommandErrorDetail};
use crate::test_support::session_with_temp_paths;
use rim_analyzer::domain::ModId;
use std::path::Path;

/// Creates a free-standing ("new def") project directly through
/// [`create_assignment_inner`] — a schema with no fields at all has no
/// `TargetKey`, so `targets: []` is accepted
/// ([`rim_session::use_cases::CreateAssignment::execute`]'s own gate).
async fn create_free_standing_project(
    state: &AppState,
    def_type: &str,
    package_id: &str,
) -> AssignmentDetailDto {
    create_assignment_inner(
        state,
        CreateAssignmentRequestDto {
            name: "Free-standing".to_string(),
            package_id: package_id.to_string(),
            display_name: "Free Standing".to_string(),
            refs: vec!["example.framework".to_string()],
            excluded_refs: Vec::new(),
            targets: Vec::new(),
            schema: AssignmentSchemaDto {
                def_type: def_type.to_string(),
                refs: vec!["example.framework".to_string()],
                fields: FieldSpecMapDto::default(),
                target_shapes: crate::dto::assignment::TargetShapeMapDto::default(),
            },
        },
    )
    .await
    .expect("a valid free-standing project must be created")
}

/// A fake [`rim_session::ports::DefSourceReader`] backed by a fixed
/// ordinal -> XML text map, mirroring `rim-session`'s own
/// `create_assignment.rs` test fixture.
struct FakeReader {
    by_ordinal: BTreeMap<u32, String>,
}

impl rim_session::ports::DefSourceReader for FakeReader {
    fn read_element(
        &self,
        locator: &XmlLocator,
        _expected: &rim_session::ports::ElementExpectation,
    ) -> Result<String, rim_session::ports::DefSourceError> {
        self.by_ordinal
            .get(&locator.element_path[0])
            .cloned()
            .ok_or_else(|| rim_session::ports::DefSourceError::Io {
                file: locator.file.to_path_buf(),
                message: "not seeded".to_string(),
            })
    }
}

fn locator(ordinal: u32) -> XmlLocator {
    XmlLocator::new(Arc::from(Path::new("Defs/fixture.xml")), vec![ordinal])
}

/// `example.framework` owns 5 `example.PartAssignmentDef` instances, 5
/// `example.OtherGroupDef` instances (a second, independent target-keyed
/// candidate type — the multi-section tests add
/// this second section to a project already carrying the first), and
/// 5 `example.PartDef` instances (a free-standing item type, referenced
/// by each `example.PartAssignmentDef` instance's own `parts` `ItemSlot` field —
/// the "inside R by ownership share" signal,
/// mirroring `apps/cli`'s own `assign_game` fixture) — each naming one
/// of 5 races owned by `target.races`, enough of everything to clear
/// `MIN_RESOLVED_DISTINCT`/`TYPE_COVERAGE_MIN`.
fn fixture() -> (SourceIndex, FakeReader) {
    let mut index = SourceIndex::default();
    let mut ordinal = 0u32;
    let mut by_ordinal = BTreeMap::new();

    for i in 0..5 {
        let group_name = format!("Group_R{i}");
        let other_name = format!("Other_R{i}");
        let race_name = format!("Race{i}");
        let part_name = format!("Part{i}");

        index.defs.insert(
            (
                ModId::new("example.framework"),
                ("example.PartDef".to_string(), part_name.clone()),
            ),
            vec![DefEntry {
                def_type: "example.PartDef".to_string(),
                def_name: part_name.clone(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(ordinal),
            }],
        );
        index.owners_by_def.insert(
            ("example.PartDef".to_string(), part_name.clone()),
            vec![ModId::new("example.framework")],
        );
        index
            .defs_by_name
            .entry(part_name.clone())
            .or_default()
            .push((
                "example.PartDef".to_string(),
                ModId::new("example.framework"),
            ));
        by_ordinal.insert(
            ordinal,
            format!("<example.PartDef><defName>{part_name}</defName></example.PartDef>"),
        );
        ordinal += 1;

        index.defs.insert(
            (
                ModId::new("example.framework"),
                ("example.PartAssignmentDef".to_string(), group_name.clone()),
            ),
            vec![DefEntry {
                def_type: "example.PartAssignmentDef".to_string(),
                def_name: group_name.clone(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(ordinal),
            }],
        );
        index.owners_by_def.insert(
            ("example.PartAssignmentDef".to_string(), group_name.clone()),
            vec![ModId::new("example.framework")],
        );
        by_ordinal.insert(ordinal,
                format!("<example.PartAssignmentDef><defName>{group_name}</defName><speciesNames><li>{race_name}</li></speciesNames><parts><li>{part_name}</li></parts></example.PartAssignmentDef>"
                ));
        ordinal += 1;

        index.defs.insert(
            (
                ModId::new("example.framework"),
                ("example.OtherGroupDef".to_string(), other_name.clone()),
            ),
            vec![DefEntry {
                def_type: "example.OtherGroupDef".to_string(),
                def_name: other_name.clone(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(ordinal),
            }],
        );
        index.owners_by_def.insert(
            ("example.OtherGroupDef".to_string(), other_name.clone()),
            vec![ModId::new("example.framework")],
        );
        by_ordinal.insert(ordinal,
                format!("<example.OtherGroupDef><defName>{other_name}</defName><otherNames><li>{race_name}</li></otherNames></example.OtherGroupDef>"
                ));
        ordinal += 1;

        index.defs.insert(
            (
                ModId::new("target.races"),
                ("ThingDef".to_string(), race_name.clone()),
            ),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: race_name.clone(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: locator(ordinal),
            }],
        );
        index.owners_by_def.insert(
            ("ThingDef".to_string(), race_name.clone()),
            vec![ModId::new("target.races")],
        );
        index
            .defs_by_name
            .entry(race_name.clone())
            .or_default()
            .push(("ThingDef".to_string(), ModId::new("target.races")));
        by_ordinal.insert(
            ordinal,
            format!("<ThingDef><defName>{race_name}</defName><race/></ThingDef>"),
        );
        ordinal += 1;
    }

    (index, FakeReader { by_ordinal })
}

fn report() -> rim_analyzer::domain::Report {
    ReportBuilder::new()
        .mod_("example.framework")
        .mod_("target.races")
        .build()
}

fn state_with_fixture() -> (tempfile::TempDir, AppState) {
    let (sources, reader) = fixture();
    let (temp_dir, session) =
        session_with_temp_paths(report(), sources, &["example.framework", "target.races"]);
    let mut state = AppState::default();
    state.adapters.def_reader = Arc::new(reader);
    *state.session.write().expect("lock") = Some(session);
    (temp_dir, state)
}

#[test]
fn parse_assignment_id_rejects_a_malformed_id() {
    let result = parse_assignment_id("not-a-valid-id");
    assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
}

#[tokio::test]
async fn get_assignment_reports_assignment_not_found_for_an_unknown_id() {
    let (_temp_dir, state) = state_with_fixture();

    let result = get_assignment_inner(&state, "abcdef012345".to_string()).await;

    assert_eq!(
        result.unwrap_err().code,
        CommandErrorCode::AssignmentNotFound
    );
}

#[tokio::test]
async fn list_assignment_candidates_names_the_fixture_type() {
    let (_temp_dir, state) = state_with_fixture();

    let response = list_assignment_candidates_inner(
        &state,
        ListAssignmentCandidatesRequestDto {
            refs: vec!["example.framework".to_string()],
            excluded_refs: Vec::new(),
            targets: vec!["target.races".to_string()],
        },
    )
    .await
    .expect("list_candidates must succeed");

    let summary = response
        .candidates
        .iter()
        .find(|c| c.def_type == "example.PartAssignmentDef")
        .expect("example.PartAssignmentDef must be listed");
    assert_eq!(summary.instance_count, 5);
    assert_eq!(
        response.effective_refs,
        vec![crate::dto::patch::ModRefDto {
            mod_id: "example.framework".to_string(),
            name: "example.framework".to_string(),
        }]
    );
}

#[tokio::test]
async fn infer_assignment_candidate_matches_the_fixture() {
    let (_temp_dir, state) = state_with_fixture();

    let candidate = infer_assignment_candidate_inner(
        &state,
        InferAssignmentCandidateRequestDto {
            refs: vec!["example.framework".to_string()],
            excluded_refs: Vec::new(),
            targets: vec!["target.races".to_string()],
            def_type: "example.PartAssignmentDef".to_string(),
        },
    )
    .await
    .expect("infer_candidate must succeed");

    assert_eq!(candidate.def_type, "example.PartAssignmentDef");
}

#[tokio::test]
async fn infer_assignment_candidate_reports_invalid_input_for_an_unknown_def_type() {
    let (_temp_dir, state) = state_with_fixture();

    let result = infer_assignment_candidate_inner(
        &state,
        InferAssignmentCandidateRequestDto {
            refs: vec!["example.framework".to_string()],
            excluded_refs: Vec::new(),
            targets: vec!["target.races".to_string()],
            def_type: "NotARealDefType".to_string(),
        },
    )
    .await;

    assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
}

fn schema_dto() -> AssignmentSchemaDto {
    let mut fields = BTreeMap::new();
    fields.insert(
        "speciesNames".to_string(),
        FieldSpec {
            role: FieldRole::TargetKey {
                def_type: "ThingDef".to_string(),
            },
            cardinality: Cardinality::List,
            observed: (5, 5),
            inferred_role: None,
        }
        .into(),
    );
    AssignmentSchemaDto {
        def_type: "example.PartAssignmentDef".to_string(),
        refs: vec!["example.framework".to_string()],
        fields: FieldSpecMapDto(fields),
        target_shapes: crate::dto::assignment::TargetShapeMapDto::default(),
    }
}

fn create_request() -> CreateAssignmentRequestDto {
    CreateAssignmentRequestDto {
        name: "Example race patch".to_string(),
        package_id: "mypatch.parts".to_string(),
        display_name: "Sample Part Patch".to_string(),
        refs: vec!["example.framework".to_string()],
        excluded_refs: Vec::new(),
        targets: vec!["target.races".to_string()],
        schema: schema_dto(),
    }
}

/// The full lifecycle: create -> list -> get -> set-row -> coverage
/// -> copy-from -> clear-row -> delete.
#[tokio::test]
async fn full_assignment_lifecycle() {
    let (_temp_dir, state) = state_with_fixture();

    let created = create_assignment_inner(&state, create_request())
        .await
        .expect("a valid project must be created");
    let assignment_id = created.id.clone();
    assert_eq!(created.package_id, "mypatch.parts");

    let listed = list_assignments_inner(&state)
        .await
        .expect("listing must succeed");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, assignment_id);
    assert_eq!(listed[0].row_count, 0);
    // Every candidate race is already referenced by `example.framework`'s
    // own reference instances this fixture's schema was inferred
    // from (`Group_R0`..`Group_R4`) — coverage's own `existing` scans
    // every active instance of the assignment type, R included, so
    // each candidate starts out `Override`, never `Cover`.
    assert_eq!(
        listed[0].uncovered_count,
        Some(0),
        "every candidate is already covered by the reference framework's own instances"
    );
    assert!(!listed[0].is_standalone);

    let target = TargetRefDto {
        key_field: "speciesNames".to_string(),
        def: DefKeyDto {
            def_type: "ThingDef".to_string(),
            def_name: "Race0".to_string(),
        },
    };
    let set_result = set_assignment_row_inner(
        &state,
        SetAssignmentRowRequestDto {
            assignment_id: assignment_id.clone(),
            section: None,
            target: Some(target.clone()),
            row: AssignmentRowDto {
                values: crate::dto::assignment::RowValueMapDto::default(),
                def_name: "mypatch_parts_Race0".to_string(),
                note: None,
            },
        },
    )
    .await
    .expect("a valid row must be accepted");
    assert!(set_result.replaced.is_none());

    let coverage = get_assignment_coverage_inner(&state, assignment_id.clone(), None)
        .await
        .expect("coverage must succeed");
    let race0 = coverage
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Race0")
        .expect("Race0 must be a candidate");
    assert!(race0.has_row);

    let copied = copy_assignment_row_from_inner(
        &state,
        CopyAssignmentRowFromRequestDto {
            assignment_id: assignment_id.clone(),
            section: None,
            target: TargetRefDto {
                key_field: "speciesNames".to_string(),
                def: DefKeyDto {
                    def_type: "ThingDef".to_string(),
                    def_name: "Race1".to_string(),
                },
            },
            source_def_name: "Group_R0".to_string(),
        },
    )
    .await
    .expect("copy-from must succeed");
    assert_eq!(copied.row.def_name, "mypatch_parts_Race1");
    assert!(copied.dropped.is_empty());

    let cleared = clear_assignment_row_inner(
        &state,
        ClearAssignmentRowRequestDto {
            assignment_id: assignment_id.clone(),
            section: None,
            target: Some(target),
            def_name: None,
        },
    )
    .await
    .expect("clearing must succeed");
    assert!(cleared.replaced.is_some());

    delete_assignment_inner(&state, assignment_id.clone())
        .await
        .expect("delete must succeed");
    let listed_after_delete = list_assignments_inner(&state)
        .await
        .expect("listing must still succeed");
    assert!(listed_after_delete.is_empty());
}

#[tokio::test]
async fn create_assignment_rejects_an_empty_refs_set() {
    let (_temp_dir, state) = state_with_fixture();
    let mut request = create_request();
    request.refs = Vec::new();

    let result = create_assignment_inner(&state, request).await;

    assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
}

#[tokio::test]
async fn create_assignment_rejects_a_package_id_already_used_by_an_active_mod() {
    let (_temp_dir, state) = state_with_fixture();
    let mut request = create_request();
    request.package_id = "example.framework".to_string();

    let result = create_assignment_inner(&state, request).await;

    assert_eq!(
        result.unwrap_err().code,
        CommandErrorCode::PatchIdentityInvalid
    );
}

#[tokio::test]
async fn set_assignment_row_reports_invalid_input_for_a_malformed_target_ref() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_assignment_inner(&state, create_request())
        .await
        .expect("a valid project must be created");

    let result = set_assignment_row_inner(
        &state,
        SetAssignmentRowRequestDto {
            assignment_id: created.id,
            section: None,
            target: Some(TargetRefDto {
                key_field: "li[#not-a-number]".to_string(),
                def: DefKeyDto {
                    def_type: "ThingDef".to_string(),
                    def_name: "Race0".to_string(),
                },
            }),
            row: AssignmentRowDto {
                values: crate::dto::assignment::RowValueMapDto::default(),
                def_name: "x".to_string(),
                note: None,
            },
        },
    )
    .await;

    assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
}

#[tokio::test]
async fn export_assignment_refuses_when_the_game_is_running_and_install_is_not_forced() {
    let (temp_dir, mut state) = state_with_fixture();
    struct FixedProbe;
    impl crate::state::GameProcessProbe for FixedProbe {
        fn is_running(&self) -> bool {
            true
        }
    }
    state.game_process_probe = Arc::new(FixedProbe);
    let created = create_assignment_inner(&state, create_request())
        .await
        .expect("a valid project must be created");
    set_assignment_row_inner(
        &state,
        SetAssignmentRowRequestDto {
            assignment_id: created.id.clone(),
            section: None,
            target: Some(TargetRefDto {
                key_field: "speciesNames".to_string(),
                def: DefKeyDto {
                    def_type: "ThingDef".to_string(),
                    def_name: "Race0".to_string(),
                },
            }),
            row: AssignmentRowDto {
                values: crate::dto::assignment::RowValueMapDto::default(),
                def_name: "mypatch_parts_Race0".to_string(),
                note: None,
            },
        },
    )
    .await
    .expect("a valid row must be accepted");

    let result = export_assignment_inner(
        &state,
        ExportAssignmentRequestDto {
            assignment_id: created.id,
            out_dir: temp_dir.path().join("out").display().to_string(),
            install: true,
            force: false,
        },
    )
    .await;

    assert_eq!(result.unwrap_err().code, CommandErrorCode::RimworldRunning);
}

#[tokio::test]
async fn export_assignment_writes_the_expected_files() {
    let (temp_dir, state) = state_with_fixture();
    let created = create_assignment_inner(&state, create_request())
        .await
        .expect("a valid project must be created");
    set_assignment_row_inner(
        &state,
        SetAssignmentRowRequestDto {
            assignment_id: created.id.clone(),
            section: None,
            target: Some(TargetRefDto {
                key_field: "speciesNames".to_string(),
                def: DefKeyDto {
                    def_type: "ThingDef".to_string(),
                    def_name: "Race0".to_string(),
                },
            }),
            row: AssignmentRowDto {
                values: crate::dto::assignment::RowValueMapDto::default(),
                def_name: "mypatch_parts_Race0".to_string(),
                note: None,
            },
        },
    )
    .await
    .expect("a valid row must be accepted");

    let report = export_assignment_inner(
        &state,
        ExportAssignmentRequestDto {
            assignment_id: created.id,
            out_dir: temp_dir.path().join("export").display().to_string(),
            install: false,
            force: false,
        },
    )
    .await
    .expect("export must succeed");

    assert!(
        report
            .files
            .contains(&"Defs/rimmerge_example.PartAssignmentDef.xml".to_string())
    );
    assert!(report.files.contains(&"About/About.xml".to_string()));
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
}

// -- add_assignment_section / remove_assignment_section -----------------

#[tokio::test]
async fn add_assignment_section_adds_a_type_the_refs_own() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_assignment_inner(&state, create_request())
        .await
        .expect("a valid project must be created");

    let detail = add_assignment_section_inner(
        &state,
        AddAssignmentSectionRequestDto {
            assignment_id: created.id,
            def_type: "example.OtherGroupDef".to_string(),
        },
    )
    .await
    .expect("a type the refs own must add cleanly");

    assert_eq!(detail.sections.len(), 2);
    assert!(
        detail
            .sections
            .iter()
            .any(|section| section.def_type == "example.OtherGroupDef")
    );
}

#[tokio::test]
async fn add_assignment_section_refuses_a_type_the_refs_do_not_own() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_assignment_inner(&state, create_request())
        .await
        .expect("a valid project must be created");

    let result = add_assignment_section_inner(
        &state,
        AddAssignmentSectionRequestDto {
            assignment_id: created.id,
            def_type: "nobody.owns.This".to_string(),
        },
    )
    .await;

    assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
}

#[tokio::test]
async fn add_assignment_section_refuses_a_duplicate_section() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_assignment_inner(&state, create_request())
        .await
        .expect("a valid project must be created");

    let result = add_assignment_section_inner(
        &state,
        AddAssignmentSectionRequestDto {
            assignment_id: created.id,
            def_type: "example.PartAssignmentDef".to_string(),
        },
    )
    .await;

    assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
}

#[tokio::test]
async fn remove_assignment_section_removes_an_unreferenced_section() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_assignment_inner(&state, create_request())
        .await
        .expect("a valid project must be created");
    add_assignment_section_inner(
        &state,
        AddAssignmentSectionRequestDto {
            assignment_id: created.id.clone(),
            def_type: "example.OtherGroupDef".to_string(),
        },
    )
    .await
    .expect("a type the refs own must add cleanly");

    let result = remove_assignment_section_inner(
        &state,
        RemoveAssignmentSectionRequestDto {
            assignment_id: created.id,
            def_type: "example.OtherGroupDef".to_string(),
            force: false,
        },
    )
    .await
    .expect("removing an unreferenced section must succeed");

    assert!(result.removed);
    assert_eq!(result.assignment.sections.len(), 1);
}

#[tokio::test]
async fn remove_assignment_section_is_idempotent_for_an_unknown_def_type() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_assignment_inner(&state, create_request())
        .await
        .expect("a valid project must be created");

    let result = remove_assignment_section_inner(
        &state,
        RemoveAssignmentSectionRequestDto {
            assignment_id: created.id,
            def_type: "no.such.type".to_string(),
            force: false,
        },
    )
    .await
    .expect("removing an absent section must still succeed");

    assert!(!result.removed);
}

/// Without `force`, a section whose own free-standing row is still
/// referenced by another section's `ItemSlot` value is refused with a
/// structured, renderable [`CommandErrorDetail::AssignmentSectionInUse`]
/// naming the referencing row — not a flattened string. Built entirely
/// through the public command surface (mirroring the CLI/`rim-io`
/// end-to-end flow: free-standing section first while T is empty, then
/// `update` widens T, then `add-section` the target-keyed type that
/// references it — a required ordering), never by reaching into
/// `rim-session`'s own `pub(crate)` mutators.
#[tokio::test]
async fn remove_assignment_section_refuses_while_referenced_then_succeeds_with_force() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_free_standing_project(&state, "example.PartDef", "mypatch.parts2").await;
    set_assignment_row_inner(
        &state,
        SetAssignmentRowRequestDto {
            assignment_id: created.id.clone(),
            section: None,
            target: None,
            row: AssignmentRowDto {
                values: crate::dto::assignment::RowValueMapDto::default(),
                def_name: "MyPart".to_string(),
                note: None,
            },
        },
    )
    .await
    .expect("a valid free-standing row must be accepted");

    update_assignment_inner(
        &state,
        UpdateAssignmentRequestDto {
            assignment_id: created.id.clone(),
            name: None,
            author: None,
            description: None,
            refs: None,
            excluded_refs: None,
            targets: Some(vec!["target.races".to_string()]),
        },
    )
    .await
    .expect("widening targets must succeed");

    add_assignment_section_inner(
        &state,
        AddAssignmentSectionRequestDto {
            assignment_id: created.id.clone(),
            def_type: "example.PartAssignmentDef".to_string(),
        },
    )
    .await
    .expect("a type the refs own, referencing the free-standing part, must add cleanly");

    set_assignment_row_inner(
        &state,
        SetAssignmentRowRequestDto {
            assignment_id: created.id.clone(),
            section: Some("example.PartAssignmentDef".to_string()),
            target: Some(TargetRefDto {
                key_field: "speciesNames".to_string(),
                def: DefKeyDto {
                    def_type: "ThingDef".to_string(),
                    def_name: "Race0".to_string(),
                },
            }),
            row: AssignmentRowDto {
                values: crate::dto::assignment::RowValueMapDto(BTreeMap::from([(
                    "parts".to_string(),
                    crate::dto::assignment::RowValueDto::Names {
                        names: vec!["MyPart".to_string()],
                    },
                )])),
                def_name: "mypatch_parts2_Race0".to_string(),
                note: None,
            },
        },
    )
    .await
    .expect("a row naming the free-standing part must be accepted");

    let result = remove_assignment_section_inner(
        &state,
        RemoveAssignmentSectionRequestDto {
            assignment_id: created.id.clone(),
            def_type: "example.PartDef".to_string(),
            force: false,
        },
    )
    .await;

    let error = result.expect_err("must be refused while referenced");
    assert_eq!(error.code, CommandErrorCode::AssignmentSectionInUse);
    let referenced_by = match error.detail.expect("structured detail must be present") {
        CommandErrorDetail::AssignmentSectionInUse { referenced_by } => referenced_by,
    };
    assert_eq!(referenced_by.len(), 1);
    assert_eq!(referenced_by[0].def_type, "example.PartAssignmentDef");
    assert_eq!(referenced_by[0].row, "ThingDef/Race0");
    assert_eq!(referenced_by[0].path, "parts");

    let forced = remove_assignment_section_inner(
        &state,
        RemoveAssignmentSectionRequestDto {
            assignment_id: created.id,
            def_type: "example.PartDef".to_string(),
            force: true,
        },
    )
    .await
    .expect("force must remove regardless");
    assert!(forced.removed);
    assert_eq!(forced.assignment.sections.len(), 1);
}

// -- set_assignment_row / clear_assignment_row: section resolution ------

#[tokio::test]
async fn set_assignment_row_without_section_refuses_once_the_project_has_two_sections() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_assignment_inner(&state, create_request())
        .await
        .expect("a valid project must be created");
    add_assignment_section_inner(
        &state,
        AddAssignmentSectionRequestDto {
            assignment_id: created.id.clone(),
            def_type: "example.OtherGroupDef".to_string(),
        },
    )
    .await
    .expect("a type the refs own must add cleanly");

    let other_target = TargetRefDto {
        key_field: "otherNames".to_string(),
        def: DefKeyDto {
            def_type: "ThingDef".to_string(),
            def_name: "Race0".to_string(),
        },
    };
    let ambiguous = set_assignment_row_inner(
        &state,
        SetAssignmentRowRequestDto {
            assignment_id: created.id.clone(),
            section: None,
            target: Some(other_target.clone()),
            row: AssignmentRowDto {
                values: crate::dto::assignment::RowValueMapDto::default(),
                def_name: "mypatch_parts_Other0".to_string(),
                note: None,
            },
        },
    )
    .await;
    assert_eq!(ambiguous.unwrap_err().code, CommandErrorCode::InvalidInput);

    let resolved = set_assignment_row_inner(
        &state,
        SetAssignmentRowRequestDto {
            assignment_id: created.id,
            section: Some("example.OtherGroupDef".to_string()),
            target: Some(other_target),
            row: AssignmentRowDto {
                values: crate::dto::assignment::RowValueMapDto::default(),
                def_name: "mypatch_parts_Other0".to_string(),
                note: None,
            },
        },
    )
    .await
    .expect("providing an explicit section must resolve the ambiguity");
    assert!(resolved.replaced.is_none());
}

// -- set_assignment_row / clear_assignment_row: free-standing rows ------

#[tokio::test]
async fn set_and_clear_assignment_row_for_a_free_standing_section() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_free_standing_project(&state, "example.PartDef", "sample.newpart").await;

    let set_result = set_assignment_row_inner(
        &state,
        SetAssignmentRowRequestDto {
            assignment_id: created.id.clone(),
            section: None,
            target: None,
            row: AssignmentRowDto {
                values: crate::dto::assignment::RowValueMapDto::default(),
                def_name: "MyPart".to_string(),
                note: None,
            },
        },
    )
    .await
    .expect("a valid free-standing row must be accepted");
    assert!(set_result.replaced.is_none());

    let detail = get_assignment_inner(&state, created.id.clone())
        .await
        .expect("must load");
    let section = &detail.sections[0];
    assert!(section.is_standalone);
    assert_eq!(section.standalone_rows.len(), 1);
    assert_eq!(section.standalone_rows[0].row.def_name, "MyPart");

    let cleared = clear_assignment_row_inner(
        &state,
        ClearAssignmentRowRequestDto {
            assignment_id: created.id,
            section: None,
            target: None,
            def_name: Some("MyPart".to_string()),
        },
    )
    .await
    .expect("clearing must succeed");
    assert!(cleared.replaced.is_some());
}

// -- get_assignment_coverage: free-standing section ----------------------

#[tokio::test]
async fn get_assignment_coverage_is_not_applicable_for_a_free_standing_section() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_free_standing_project(&state, "example.PartDef", "sample.newpart2").await;

    let coverage = get_assignment_coverage_inner(&state, created.id, None)
        .await
        .expect("coverage must succeed even though it isn't applicable");

    assert!(!coverage.applicable);
    assert!(coverage.rows.is_empty());
}

// -- list_assignment_items: own rows ------------------------------------

#[tokio::test]
async fn list_assignment_items_lists_the_projects_own_row_first_with_own_true() {
    let (_temp_dir, state) = state_with_fixture();
    let created = create_free_standing_project(&state, "example.PartDef", "sample.newpart3").await;
    set_assignment_row_inner(
        &state,
        SetAssignmentRowRequestDto {
            assignment_id: created.id.clone(),
            section: None,
            target: None,
            row: AssignmentRowDto {
                values: crate::dto::assignment::RowValueMapDto::default(),
                def_name: "MyPart".to_string(),
                note: None,
            },
        },
    )
    .await
    .expect("a valid free-standing row must be accepted");

    let page = list_assignment_items_inner(
        &state,
        ListAssignmentItemsRequestDto {
            def_type: "example.PartDef".to_string(),
            filter: ListItemsFilterDto {
                search: None,
                offset: 0,
                limit: 10,
            },
            assignment_id: Some(created.id),
        },
    )
    .await
    .expect("listing must succeed");

    // 1 own free-standing row + the fixture's own 5 active
    // `example.PartDef` instances (`Part0..Part4`, owned by
    // `example.framework` — added so `example.PartAssignmentDef`'s own `parts`
    // field infers as `ItemSlot`, see `fixture`'s own doc comment).
    assert_eq!(page.total, 6);
    assert_eq!(page.items.len(), 6);
    assert!(page.items[0].own, "the own row must sort first");
    assert_eq!(page.items[0].def.def_name, "MyPart");
    assert!(
        page.items[1..].iter().all(|item| !item.own),
        "every active def must be own: false"
    );
}
