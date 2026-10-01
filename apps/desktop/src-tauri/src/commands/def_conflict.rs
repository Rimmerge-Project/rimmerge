//! `get_def_conflict_view`: the def-shaped finding conflict panel's own
//! command. Read-only: never persists
//! anything and never emits `session://changed`.

use std::str::FromStr;

use rim_resolve::domain::FindingKey;
use rim_session::PreviewSlot;
use rim_session::def_conflict_view::{self, FieldRowFilter};
use rim_session::use_cases::{InspectDef, PlanMerge};

use crate::commands::patch::parse_patch_id;
use crate::dto::def_conflict::{
    DefConflictViewDto, DefConflictViewRequestDto, build_def_conflict_view_dto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Builds one def-shaped finding's field-by-field conflict view — against
/// the profile's own findings when `request.patch_id` is `null`, or
/// against that compat patch's own scoped preview and decisions
/// when it names one, mirroring
/// `get_merge_preview` exactly.
/// `Session::def_conflict_view`/`def_conflict_view_for_patch` are a pure
/// join over an already-cached [`rim_session::use_cases::DefInspection`]
/// and (for `defOverride`/`patchCollision`) an already-cached merge
/// preview under that same slot — this command always builds both,
/// before calling either, so their own cache-miss errors (`NotInspected`/
/// `NotPlanned`) are structurally unreachable through this command.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] when `request.key`/`request.patch_id`
/// doesn't parse or `request.key` doesn't name a def-shaped finding,
/// [`CommandError::patch_not_found`] when `request.patch_id` names a
/// patch this session doesn't have loaded,
/// [`CommandError::finding_not_found`] when `request.patch_id` is set but
/// `key` isn't admitted by that patch's own scoped ledger (the unscoped
/// path has no such liveness precondition),
/// [`CommandError::def_not_found`] when it names a def/template this scan
/// didn't index, or a `MergeSourceFailed`/`internal` [`CommandError`] when
/// inspecting or planning fails.
pub(crate) async fn get_def_conflict_view_inner(
    state: &AppState,
    request: DefConflictViewRequestDto,
) -> Result<DefConflictViewDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let key = FindingKey::from_str(&request.key)?;
        let def_ref = key.def_ref().ok_or_else(|| {
            CommandError::invalid_input(format!("{} is not a def-shaped finding", request.key))
        })?;

        let patch_id = request
            .patch_id
            .as_deref()
            .map(parse_patch_id)
            .transpose()?;
        let source = session.selected();
        // Only the *scoped* path gets a liveness check — mirroring
        // `get_merge_preview`'s own `patch_resolution` guard, since
        // "is this finding actually admitted by this patch's own scope"
        // is a precondition only patch scoping has. The unscoped path has
        // no `session.resolution` check: `Session::def_conflict_view`'s own
        // precondition is "already inspected/planned", never "already live
        // in the profile's own ledger" — a `DefRef`'s def-shaped key can be
        // genuinely worth inspecting even when the ledger hasn't (yet)
        // recorded a resolution for it.
        let slot = match &patch_id {
            Some(id) => {
                if session.patch_resolution(id, source, &key)?.is_none() {
                    return Err(CommandError::finding_not_found(&request.key));
                }
                PreviewSlot::patch(source, id.clone())
            }
            None => PreviewSlot::profile(source),
        };

        InspectDef::new(adapters.def_reader.clone()).execute(session, &def_ref)?;
        let mut plan_failure = None;
        match &key {
            FindingKey::PatchCollision { .. } => {
                let ctx = session.merge_context(slot.clone(), &key)?;
                PlanMerge::new(adapters.def_reader.clone()).execute_in(session, ctx, &key)?;
            }
            FindingKey::DefOverride { .. } => {
                // A
                // `DefOverride` participant's own `ParentName` chain that
                // can't be resolved at all makes `PlanMerge`'s own
                // `plan_def_override` hard-error (`MissingSource`/
                // `Inherit`) rather than cache any preview —
                // `Session::def_conflict_view`/`def_conflict_view_for_patch`
                // already degrade gracefully for exactly that (falls back
                // to `EffectiveDef::provenance` alone), so this swallows
                // the same two variants here rather than surfacing them
                // as a command failure and never rendering anything at
                // all (an empty panel is never acceptable). `InspectDef`'s own
                // `Problem`s are derived
                // only from the *winning* owner's own chain, so a
                // *losing* owner's own broken chain (this hard error can
                // still fire even when the winner itself is perfectly
                // clean — `plan_def_override` resolves every
                // participant's chain, not just the winner's) must never
                // just vanish: the swallowed error is captured and handed
                // to `def_conflict_view_with_plan_failure` below, which
                // appends it as a `Problem` instead of silently dropping
                // it. Any other failure (a stale scan, bad XML) is still
                // a real error worth surfacing.
                use rim_session::use_cases::PlanMergeError;
                let ctx = session.merge_context(slot.clone(), &key)?;
                if let Err(error) =
                    PlanMerge::new(adapters.def_reader.clone()).execute_in(session, ctx, &key)
                {
                    if matches!(
                        error,
                        PlanMergeError::MissingSource(_) | PlanMergeError::Inherit(_)
                    ) {
                        plan_failure = Some(error);
                    } else {
                        return Err(error.into());
                    }
                }
            }
            _ => {}
        }

        let view = match &patch_id {
            Some(id) => session.def_conflict_view_for_patch_with_plan_failure(
                id,
                &key,
                plan_failure.as_ref(),
            )?,
            None => session.def_conflict_view_with_plan_failure(&key, plan_failure.as_ref())?,
        };
        let filter: FieldRowFilter = (&request.filter).into();
        let page = def_conflict_view::page(&view, &filter);

        Ok(build_def_conflict_view_dto(&view, &page))
    })
    .await
}

/// See [`get_def_conflict_view_inner`].
///
/// # Errors
///
/// See [`get_def_conflict_view_inner`].
#[tauri::command]
pub async fn get_def_conflict_view(
    state: tauri::State<'_, AppState>,
    request: DefConflictViewRequestDto,
) -> Result<DefConflictViewDto, CommandError> {
    get_def_conflict_view_inner(&state, request).await
}

#[cfg(test)]
mod tests {
    use rim_session::test_support::{
        bionic_heart_fixture, bionic_heart_fixture_with_conflict,
        def_override_missing_template_fixture,
        def_override_missing_template_on_a_losing_owner_fixture, head_normal_fixture,
        plant_density_fixture, session_fixture, two_mod_list_fixture,
        unsupported_op_patch_collision_fixture,
    };

    use super::*;
    use crate::commands::patch::create_patch_inner;
    use crate::dto::def_conflict::FieldRowFilterDto;
    use crate::dto::patch::CreatePatchRequestDto;
    use crate::error::CommandErrorCode;
    use crate::test_support::session_with_temp_paths;

    fn state_with(session: rim_session::Session) -> AppState {
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        state
    }

    fn default_filter() -> FieldRowFilterDto {
        FieldRowFilterDto {
            only_changed: false,
            offset: 0,
            limit: 200,
        }
    }

    #[tokio::test]
    async fn rejects_an_unparseable_key() {
        let state = state_with(session_fixture(&["a"]));

        let result = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: "not a valid key".to_string(),
                filter: default_filter(),
                patch_id: None,
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn rejects_a_key_that_is_not_def_shaped() {
        let state = state_with(session_fixture(&["a"]));

        let result = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: "missing_mod:nobody".to_string(),
                filter: default_filter(),
                patch_id: None,
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn reports_def_not_found_for_an_unknown_def_override_key() {
        let state = state_with(session_fixture(&["a"]));

        let result = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: "def_override:ThingDef/DoesNotExist:[a]".to_string(),
                filter: default_filter(),
                patch_id: None,
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::DefNotFound);
    }

    /// A fresh session — nothing inspected or planned yet — still builds a
    /// full view on the first call: the command runs `InspectDef`/
    /// `PlanMerge` itself, so `Session::def_conflict_view`'s own
    /// cache-miss errors never surface through this command.
    #[tokio::test]
    async fn a_fresh_session_with_nothing_cached_returns_a_full_view_on_first_call() {
        let fixture = bionic_heart_fixture();
        let (_temp_dir, session) = session_with_temp_paths(
            fixture.report,
            fixture.sources,
            &["ludeon.rimworld", "example.bionicsfork"],
        );
        let mut state = AppState::default();
        state.adapters.def_reader = std::sync::Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);

        let key = "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]";
        let dto = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: key.to_string(),
                filter: default_filter(),
                patch_id: None,
            },
        )
        .await
        .expect("a live DefOverride finding must build a view on the very first call");

        assert_eq!(dto.def_ref, "HediffDef/BionicHeart");
        assert!(dto.problems.is_empty());
        assert!(
            dto.fields.iter().any(|f| f.path == "label"),
            "the contested label field must appear: {:?}",
            dto.fields
        );
    }

    /// HeadNormal's two genuine conflicts round-trip with a `LoadOrder`
    /// preference naming the current winner.
    #[tokio::test]
    async fn head_normal_conflict_rows_carry_a_load_order_preference() {
        let fixture = head_normal_fixture();
        let (_temp_dir, session) = session_with_temp_paths(
            fixture.report,
            fixture.sources,
            &["core.mod", "a.mod", "b.mod"],
        );
        let mut state = AppState::default();
        state.adapters.def_reader = std::sync::Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);

        let key = "def_override:HeadTypeDef/HeadNormal:[a.mod,b.mod,core.mod]";
        let dto = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: key.to_string(),
                filter: default_filter(),
                patch_id: None,
            },
        )
        .await
        .expect("HeadNormal must build a view");

        let conflicts: Vec<_> = dto
            .fields
            .iter()
            .filter(|f| f.kind == crate::dto::def_conflict::FieldRowKindDto::Conflict)
            .collect();
        assert_eq!(conflicts.len(), 2);
        for row in conflicts {
            assert_eq!(
                row.preference,
                crate::dto::def_conflict::PreferenceDto::LoadOrder {
                    winner: "b.mod".to_string()
                },
                "{:?}",
                row.path
            );
        }
    }

    /// `plantDensity`'s collision: one `Conflict` row plus the other field
    /// `x.mod`'s ops touch, round-tripped through the command.
    #[tokio::test]
    async fn plant_density_round_trips_the_conflict_and_context_rows() {
        let fixture = plant_density_fixture();
        let (_temp_dir, session) = session_with_temp_paths(
            fixture.report,
            fixture.sources,
            &["core.mod", "x.mod", "y.mod"],
        );
        let mut state = AppState::default();
        state.adapters.def_reader = std::sync::Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);

        let key = "patch_collision:BiomeDef/TemperateForest:def_name:plantDensity:[x.mod,y.mod]";
        let dto = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: key.to_string(),
                filter: default_filter(),
                patch_id: None,
            },
        )
        .await
        .expect("plantDensity must build a view");

        assert!(matches!(
            dto.kind,
            crate::dto::def_conflict::DefConflictKindDto::PatchCollision { sub_path: Some(_) }
        ));
        assert!(dto.fields.iter().any(|f| f.path == "wildPlantRegrowDays"));
    }

    /// Two mods adding distinct list entries: each row names its own mod,
    /// and the paged, `onlyChanged` field list hides `Unchanged` rows.
    #[tokio::test]
    async fn two_mod_list_rows_are_paged_and_only_changed_hides_unchanged_rows() {
        let fixture = two_mod_list_fixture();
        let (_temp_dir, session) = session_with_temp_paths(
            fixture.report,
            fixture.sources,
            &["core.mod", "a.mod", "b.mod"],
        );
        let mut state = AppState::default();
        state.adapters.def_reader = std::sync::Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);

        let key = "patch_collision:ThingDef/Widget:def_name:label:[a.mod,b.mod]";
        let dto = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: key.to_string(),
                filter: FieldRowFilterDto {
                    only_changed: true,
                    offset: 0,
                    limit: 200,
                },
                patch_id: None,
            },
        )
        .await
        .expect("the two-mod list collision must build a view");

        assert!(
            dto.fields
                .iter()
                .all(|f| f.kind != crate::dto::def_conflict::FieldRowKindDto::Unchanged)
        );
        let list_rows: Vec<_> = dto
            .fields
            .iter()
            .filter(|f| f.kind == crate::dto::def_conflict::FieldRowKindDto::ListEntry)
            .collect();
        assert_eq!(list_rows.len(), 2);

        let paged = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: key.to_string(),
                filter: FieldRowFilterDto {
                    only_changed: true,
                    offset: 0,
                    limit: 1,
                },
                patch_id: None,
            },
        )
        .await
        .expect("a smaller page must still succeed");
        assert_eq!(paged.fields.len(), 1);
        assert_eq!(paged.fields_total, dto.fields.len());
    }

    /// The unsupported-op fixture: rows before the stopper survive, the
    /// contested field past it is still shown with `inGame: null`, and one
    /// problem names the stopper.
    #[tokio::test]
    async fn unsupported_op_keeps_earlier_rows_and_reports_the_stopper() {
        let fixture = unsupported_op_patch_collision_fixture();
        let (_temp_dir, session) = session_with_temp_paths(
            fixture.report,
            fixture.sources,
            &["core.mod", "c.mod", "x.mod", "d.mod"],
        );
        let mut state = AppState::default();
        state.adapters.def_reader = std::sync::Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);

        let key = "patch_collision:ThingDef/Wall:def_name:label:[c.mod,d.mod]";
        let dto = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: key.to_string(),
                filter: default_filter(),
                patch_id: None,
            },
        )
        .await
        .expect("the unsupported-op collision must build a view even past a stopper");

        assert_eq!(dto.problems.len(), 1);
        match &dto.problems[0] {
            crate::dto::def_conflict::ProblemDto::UnsupportedOp { mod_id, class, .. } => {
                assert_eq!(mod_id, "x.mod");
                assert_eq!(class, "SomeThirdParty.WeirdOperation");
            }
            other => panic!("expected UnsupportedOp, got {other:?}"),
        }

        let description_row = dto
            .fields
            .iter()
            .find(|f| f.path == "description")
            .expect("c.mod's own field, before the stopper, must survive");
        assert!(description_row.in_game.is_some());

        let label_row = dto
            .fields
            .iter()
            .find(|f| f.path == "label")
            .expect("the collision's own contested field is always shown");
        assert!(label_row.in_game.is_none());
    }

    /// A `DefOverride` whose winning owner's
    /// `ParentName` chain can't be resolved makes `PlanMerge` hard-error
    /// (never cached) — the command must swallow that and still return a
    /// full view, naming the gap as a `MissingTemplate` problem, rather
    /// than propagating a `MergeSourceFailed` command error and rendering
    /// nothing at all.
    #[tokio::test]
    async fn def_override_missing_template_still_returns_a_view() {
        let fixture = def_override_missing_template_fixture();
        let (_temp_dir, session) =
            session_with_temp_paths(fixture.report, fixture.sources, &["core.mod", "broken.mod"]);
        let mut state = AppState::default();
        state.adapters.def_reader = std::sync::Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);

        let key = "def_override:ThingDef/Gizmo:[broken.mod,core.mod]";
        let dto = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: key.to_string(),
                filter: default_filter(),
                patch_id: None,
            },
        )
        .await
        .expect("a missing template must never surface as a command failure");

        assert!(
            dto.problems.iter().any(|p| matches!(p,
                crate::dto::def_conflict::ProblemDto::MissingTemplate { name, .. }
                    if name == "ReallyMissing"
            )),
            "{:?}",
            dto.problems
        );
        assert!(
            dto.fields.iter().any(|f| f.path == "description"),
            "broken.mod's own other field must still show up: {:?}",
            dto.fields
        );
    }

    /// When the
    /// *losing* owner (`broken.mod`, loading first) has the broken
    /// `ParentName` chain and the *winner* (`clean.mod`) is perfectly
    /// clean, `InspectDef`'s own `effective.completeness` is `Complete`
    /// (derived only from the winner's own chain) — so without capturing
    /// the swallowed `PlanMergeError` explicitly (as opposed to the
    /// winner-is-broken case above, which already surfaces via the
    /// existing completeness-derived path), this view would silently come
    /// back with `problems: []`, a `Complete` pill, and `broken.mod`'s own
    /// contribution simply missing from `fields` — worse than an honest
    /// `MergeSourceFailed` command error.
    #[tokio::test]
    async fn def_override_missing_template_on_a_losing_owner_still_reports_it() {
        let fixture = def_override_missing_template_on_a_losing_owner_fixture();
        let (_temp_dir, session) = session_with_temp_paths(
            fixture.report,
            fixture.sources,
            &["broken.mod", "clean.mod"],
        );
        let mut state = AppState::default();
        state.adapters.def_reader = std::sync::Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);

        let key = "def_override:ThingDef/Gizmo:[broken.mod,clean.mod]";
        let dto = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: key.to_string(),
                filter: default_filter(),
                patch_id: None,
            },
        )
        .await
        .expect("a losing owner's own missing template must never surface as a command failure");

        assert!(
            !dto.problems.is_empty(),
            "the losing owner's own broken chain must never vanish silently: {dto:?}"
        );
        assert!(
            dto.problems.iter().any(|p| match p {
                crate::dto::def_conflict::ProblemDto::MissingTemplate { name, .. } =>
                    name == "ReallyMissing",
                crate::dto::def_conflict::ProblemDto::PlanFailed { reason } =>
                    reason.contains("ReallyMissing"),
                _ => false,
            }),
            "{:?}",
            dto.problems
        );
    }

    /// A `commands/patch.rs`-style integration test for
    /// the patch-scoped path — a real `CreatePatch`, then
    /// `get_def_conflict_view_inner` with `patch_id: Some(...)` against
    /// that real patch's own scope, mirroring
    /// `commands::patch::tests::full_patch_lifecycle_...`'s own
    /// `get_merge_preview` step but for this command instead.
    #[tokio::test]
    async fn a_patch_scoped_request_builds_a_view_from_that_patchs_own_scope() {
        let fixture = bionic_heart_fixture_with_conflict();
        let (_temp_dir, session) = session_with_temp_paths(
            fixture.report,
            fixture.sources,
            &["ludeon.rimworld", "example.bionicsfork"],
        );
        let mut state = AppState::default();
        state.adapters.def_reader = std::sync::Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);

        let created = create_patch_inner(
            &state,
            CreatePatchRequestDto {
                name: "BIONICS compat".to_string(),
                package_id: "sample.bionicscompat".to_string(),
                display_name: "BIONICS Compatibility".to_string(),
                scope: vec![
                    "ludeon.rimworld".to_string(),
                    "example.bionicsfork".to_string(),
                ],
            },
        )
        .await
        .expect("a valid patch must be created");

        let key = "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]";
        let dto = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: key.to_string(),
                filter: default_filter(),
                patch_id: Some(created.id.clone()),
            },
        )
        .await
        .expect("a finding this patch's own scope admits must build a scoped view");

        assert_eq!(dto.def_ref, "HediffDef/BionicHeart");
        assert!(
            dto.fields.iter().any(|f| f.path == "label"),
            "the scoped preview's own field diff must still populate `fields`: {:?}",
            dto.fields
        );

        // An unknown patch id is rejected before any scoped work happens.
        let missing_patch_result = get_def_conflict_view_inner(
            &state,
            DefConflictViewRequestDto {
                key: key.to_string(),
                filter: default_filter(),
                patch_id: Some("000000000000".to_string()),
            },
        )
        .await;
        assert_eq!(
            missing_patch_result.unwrap_err().code,
            CommandErrorCode::PatchNotFound
        );
    }
}
