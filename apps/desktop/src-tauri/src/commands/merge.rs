//! `get_merge_preview`/`set_merge_choices`/`get_merge_mod`/
//! `preview_merge_mod_file`: the merge editor's commands.
//!
//! `get_merge_preview` and `preview_merge_mod_file` never persist
//! anything — the former only refreshes `rim-session`'s own internal
//! preview cache (never written to disk), the latter renders entirely in
//! memory. Only `set_merge_choices` mutates persisted state and emits
//! `session://changed`.

use std::collections::BTreeMap;
use std::str::FromStr;

use rim_resolve::domain::{FieldPath, FindingKey, MergeChoice};
use rim_session::PreviewSlot;
use rim_session::ports::MergeModWriter;
use rim_session::use_cases::{DecideMerge, DecidePatchMerge, PlanMerge, RenderMergeMod};

use crate::commands::{emit_session_changed, mod_names, patch::parse_patch_id};
use crate::dto::merge::{
    MergeModDto, MergeModFileDto, MergePreviewDto, MergePreviewRequestDto, MergeStateDto,
    PreviewMergeModFileRequestDto, SetMergeChoicesRequestDto, build_merge_mod_dto,
    build_preview_dto,
};
use crate::dto::project::SessionChangeReasonDto;
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Refreshes and returns one finding's merge preview, filtered and paged
/// — against the profile's own findings when `request.patch_id` is
/// `null`, or against that compat patch's own scoped preview
/// when it names one.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] when `request.key`/`request.patch_id`
/// doesn't parse, [`CommandError::patch_not_found`] when `request.patch_id`
/// names a patch this session doesn't have loaded,
/// [`CommandError::finding_not_found`] when `key` isn't currently live
/// (in the profile ledger, or that patch's own scoped ledger), or a
/// `MergeSourceFailed` [`CommandError`] when the def/patch source changed
/// since the scan.
pub(crate) async fn get_merge_preview_inner(
    state: &AppState,
    request: MergePreviewRequestDto,
) -> Result<MergePreviewDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let key = FindingKey::from_str(&request.key)?;
        let source = session.selected();
        let slot = match &request.patch_id {
            Some(patch_id_text) => {
                let patch_id = parse_patch_id(patch_id_text)?;
                if session.patch_resolution(&patch_id, source, &key)?.is_none() {
                    return Err(CommandError::finding_not_found(&request.key));
                }
                PreviewSlot::patch(source, patch_id)
            }
            None => {
                if session.resolution(source, &key).is_none() {
                    return Err(CommandError::finding_not_found(&request.key));
                }
                PreviewSlot::profile(source)
            }
        };

        let ctx = session.merge_context(slot.clone(), &key)?;
        let choices = ctx.choices.clone();
        let use_case = PlanMerge::new(adapters.def_reader);
        // Ignore the returned reference: it would otherwise keep `session`
        // borrowed for its own lifetime, blocking the `session.report()`
        // read below. Re-fetching via `Session::merge_preview` (a fresh,
        // independent immutable borrow) is the same idiom `rim-session`'s
        // own `Apply`/`RenderMergeMod` use for exactly this reason.
        use_case.execute_in(session, ctx, &key)?;
        let preview = session
            .merge_preview(&slot, &key)
            .unwrap_or_else(|| unreachable!("just cached above"));
        let page = session
            .merge_fields(&slot, &key, &(&request.filter).into())
            .unwrap_or_else(|| unreachable!("just cached above"));

        let names = mod_names(session);
        Ok(build_preview_dto(preview, &page, &key, &choices, &names))
    })
    .await
}

/// See [`get_merge_preview_inner`].
///
/// # Errors
///
/// See [`get_merge_preview_inner`].
#[tauri::command]
pub async fn get_merge_preview(
    state: tauri::State<'_, AppState>,
    request: MergePreviewRequestDto,
) -> Result<MergePreviewDto, CommandError> {
    get_merge_preview_inner(&state, request).await
}

/// Validates and stores `request.choices` as `key`'s `Merge` decision,
/// persisting it and returning the refreshed merge state.
///
/// # Errors
///
/// Returns [`CommandError::invalid_input`] when `request.key` or any
/// choice path fails to parse, when a path isn't one of this def's
/// fields, or when a `from` choice names a mod that isn't an owner;
/// [`CommandError::finding_not_found`] when `key` isn't currently live; or
/// [`CommandError`] when persisting the decision fails.
pub(crate) async fn set_merge_choices_inner(
    state: &AppState,
    request: SetMergeChoicesRequestDto,
) -> Result<MergeStateDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let key = FindingKey::from_str(&request.key)?;
        let mut choices: BTreeMap<FieldPath, MergeChoice> = BTreeMap::new();
        for (path_text, choice_dto) in request.choices {
            let path: FieldPath =
                path_text
                    .parse()
                    .map_err(|error: rim_resolve::domain::FieldPathParseError| {
                        CommandError::invalid_input(error.to_string())
                    })?;
            choices.insert(path, choice_dto.into());
        }

        match &request.patch_id {
            Some(patch_id_text) => {
                let patch_id = parse_patch_id(patch_id_text)?;
                let source = session.selected();
                if session.patch_resolution(&patch_id, source, &key)?.is_none() {
                    return Err(CommandError::finding_not_found(&request.key));
                }
                let use_case = DecidePatchMerge::new(adapters.patch_store, adapters.def_reader);
                let state = use_case.execute(session, &patch_id, &key, choices)?;
                Ok((&state).into())
            }
            None => {
                if session.resolution(session.selected(), &key).is_none() {
                    return Err(CommandError::finding_not_found(&request.key));
                }
                let use_case = DecideMerge::new(adapters.decision_store, adapters.def_reader);
                let state = use_case.execute(session, &key, choices)?;
                Ok((&state).into())
            }
        }
    })
    .await
}

/// See [`set_merge_choices_inner`]; also emits `session://changed` on
/// success — `patchDecided` when `request.patch_id` names a compat patch,
/// `mergeChanged` for the profile.
///
/// # Errors
///
/// See [`set_merge_choices_inner`].
#[tauri::command]
pub async fn set_merge_choices(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: SetMergeChoicesRequestDto,
) -> Result<MergeStateDto, CommandError> {
    let is_patch = request.patch_id.is_some();
    let result = set_merge_choices_inner(&state, request).await;
    if result.is_ok() {
        let reason = if is_patch {
            SessionChangeReasonDto::PatchDecided
        } else {
            SessionChangeReasonDto::MergeChanged
        };
        emit_session_changed(&app, reason);
    }
    result
}

/// Describes the generated merge mod: identity, whether it currently
/// exists on disk, one entry per `Merge`/`ShipAsset` decision, the files
/// a render would produce, and every mod its content depends on. Renders
/// in memory only — never writes or removes anything.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, or a `MergeSourceFailed`/`internal` [`CommandError`] when
/// building a missing preview or rendering fails.
pub(crate) async fn get_merge_mod_inner(state: &AppState) -> Result<MergeModDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let use_case = RenderMergeMod::new(adapters.def_reader, adapters.asset_locator);
        let render = use_case.execute(session)?;

        let mods_dir = session.paths().game_dir.join("Mods");
        let exists = adapters
            .merge_mod_writer
            .exists(&mods_dir, &render.identity.folder_name);
        let names = mod_names(session);

        Ok(build_merge_mod_dto(exists, &render, &names))
    })
    .await
}

/// See [`get_merge_mod_inner`].
///
/// # Errors
///
/// See [`get_merge_mod_inner`].
#[tauri::command]
pub async fn get_merge_mod(state: tauri::State<'_, AppState>) -> Result<MergeModDto, CommandError> {
    get_merge_mod_inner(&state).await
}

/// Renders the merge mod in memory and returns the text content of the
/// rendered file whose relative path exactly matches
/// `request.relative_path` — never writes anything to disk. When that
/// file is `rimmerge.json`, its `generatedAt` field is *this* render's own
/// timestamp: a preview taken now, not the one `apply` will actually
/// persist the next time it runs.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, [`CommandError::invalid_input`] when no rendered file matches
/// `request.relative_path` or the match is a binary (`CopyFrom`) asset
/// copy rather than text, or a `MergeSourceFailed`/`internal`
/// [`CommandError`] when rendering itself fails.
pub(crate) async fn preview_merge_mod_file_inner(
    state: &AppState,
    request: PreviewMergeModFileRequestDto,
) -> Result<MergeModFileDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let use_case = RenderMergeMod::new(adapters.def_reader, adapters.asset_locator);
        let render = use_case.execute(session)?;
        let wanted = std::path::PathBuf::from(&request.relative_path);

        let rendered = render.rendered.ok_or_else(|| {
            CommandError::invalid_input(format!(
                "{}: nothing renders for the current decisions",
                request.relative_path
            ))
        })?;
        let file = rendered
            .files
            .iter()
            .find(|file| file.relative_path == wanted)
            .ok_or_else(|| {
                CommandError::invalid_input(format!(
                    "{}: not a file this render produces",
                    request.relative_path
                ))
            })?;
        match &file.content {
            rim_merge::emit::FileContent::Text(content) => Ok(MergeModFileDto {
                content: content.clone(),
            }),
            rim_merge::emit::FileContent::CopyFrom(_) => Err(CommandError::invalid_input(format!(
                "{}: a binary asset copy has no text preview",
                request.relative_path
            ))),
        }
    })
    .await
}

/// See [`preview_merge_mod_file_inner`].
///
/// # Errors
///
/// See [`preview_merge_mod_file_inner`].
#[tauri::command]
pub async fn preview_merge_mod_file(
    state: tauri::State<'_, AppState>,
    request: PreviewMergeModFileRequestDto,
) -> Result<MergeModFileDto, CommandError> {
    preview_merge_mod_file_inner(&state, request).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Action, DefKey};
    use rim_resolve::test_support::ReportBuilder;
    use rim_session::test_support::{bionic_heart_fixture_flat, session_fixture};

    use super::*;
    use crate::dto::finding::MergeChoiceDto;
    use crate::dto::merge::MergeFieldFilterDto;
    use crate::error::CommandErrorCode;
    use crate::test_support::session_with_temp_paths;

    fn state_with(session: rim_session::Session) -> AppState {
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        state
    }

    /// A session with a *live* `HediffDef/BionicHeart` `DefOverride`
    /// finding — `session_fixture`/`report_fixture` carry no conflicts at
    /// all, so every merge command test above this point exits at the
    /// `finding_not_found` gate before touching `PlanMerge`/`DecideMerge`.
    /// Built from [`bionic_heart_fixture_flat`]'s real source index (so
    /// the def/template XML actually resolves) plus a hand-built report
    /// naming the same def as contested, and an in-memory `def_reader` —
    /// exercising the `Arc<dyn DefSourceReader>` seam `Adapters` now
    /// offers instead of the real `FileDefSourceReader`. Uses the flat
    /// fixture, not `bionic_heart_fixture` itself: the real fixture's
    /// `ParentName` difference fires the structural guard, which would keep
    /// every one of this module's tests permanently `NeedsFieldInput`
    /// even after a full choice — none of them are about the guard.
    ///
    /// `profile_dir` is a real, per-test [`tempfile::TempDir`] (via
    /// [`session_with_temp_paths`]) rather than
    /// [`session_fixture`]'s hardcoded relative `"profile"`:
    /// `set_merge_choices` persists through `Adapters`'s real
    /// `JsonDecisionStore` (only `def_reader` is swapped for a fake here),
    /// so every test built from this helper must get its own directory —
    /// sharing one relative path across tests running concurrently in the
    /// same process would race on the same `decisions.json`. The returned
    /// `TempDir` must stay alive for the caller's whole test (it deletes
    /// its directory on drop).
    fn bionic_heart_conflict_state() -> (tempfile::TempDir, AppState, FindingKey) {
        let fixture = bionic_heart_fixture_flat();
        let report = ReportBuilder::new()
            .mod_("ludeon.rimworld")
            .mod_("example.bionicsfork")
            .def_override(
                "HediffDef",
                "BionicHeart",
                &["ludeon.rimworld", "example.bionicsfork"],
            )
            .build();
        let (temp_dir, session) = session_with_temp_paths(
            report,
            fixture.sources,
            &["ludeon.rimworld", "example.bionicsfork"],
        );

        let mut state = AppState::default();
        state.adapters.def_reader = Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);

        let key = FindingKey::DefOverride {
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
        };
        (temp_dir, state, key)
    }

    #[tokio::test]
    async fn get_merge_preview_reports_finding_not_found_for_an_unknown_key() {
        let state = state_with(session_fixture(&["a"]));

        let result = get_merge_preview_inner(
            &state,
            MergePreviewRequestDto {
                key: "missing_mod:nobody".to_string(),
                filter: MergeFieldFilterDto::default(),
                patch_id: None,
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::FindingNotFound);
    }

    #[tokio::test]
    async fn set_merge_choices_rejects_a_malformed_field_path_before_touching_any_source() {
        let state = state_with(session_fixture(&["a"]));

        let result = set_merge_choices_inner(
            &state,
            SetMergeChoicesRequestDto {
                key: "missing_mod:nobody".to_string(),
                choices: BTreeMap::from([(
                    "li[unterminated".to_string(),
                    crate::dto::finding::MergeChoiceDto::Drop,
                )]),
                patch_id: None,
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn set_merge_choices_rejects_an_unparseable_key() {
        let state = state_with(session_fixture(&["a"]));

        let result = set_merge_choices_inner(
            &state,
            SetMergeChoicesRequestDto {
                key: "not a valid key".to_string(),
                choices: BTreeMap::new(),
                patch_id: None,
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn set_merge_choices_reports_finding_not_found_for_an_unknown_key() {
        let state = state_with(session_fixture(&["a"]));

        let result = set_merge_choices_inner(
            &state,
            SetMergeChoicesRequestDto {
                key: "missing_mod:nobody".to_string(),
                choices: BTreeMap::new(),
                patch_id: None,
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::FindingNotFound);
    }

    #[tokio::test]
    async fn get_merge_mod_with_no_decisions_reports_nothing_rendered_and_absent() {
        let state = state_with(session_fixture(&["a"]));

        let dto = get_merge_mod_inner(&state)
            .await
            .expect("rendering with no decisions must still succeed");

        assert!(dto.entries.is_empty());
        assert!(dto.files.is_empty());
        assert!(dto.source_mods.is_empty());
        assert!(
            !dto.exists,
            "session_fixture's game_dir never has a real Mods folder"
        );
    }

    #[tokio::test]
    async fn preview_merge_mod_file_with_nothing_rendered_is_invalid_input() {
        let state = state_with(session_fixture(&["a"]));

        let result = preview_merge_mod_file_inner(
            &state,
            PreviewMergeModFileRequestDto {
                relative_path: "Patches/rimmerge_ThingDef.xml".to_string(),
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn get_merge_preview_round_trips_owners_totals_and_a_field_row() {
        let (_temp_dir, state, key) = bionic_heart_conflict_state();

        let dto = get_merge_preview_inner(
            &state,
            MergePreviewRequestDto {
                key: key.to_string(),
                filter: MergeFieldFilterDto {
                    only_conflicts: false,
                    search: None,
                    offset: 0,
                    limit: 200,
                },
                patch_id: None,
            },
        )
        .await
        .expect("a live DefOverride finding must build a preview");

        assert_eq!(
            dto.owners
                .iter()
                .map(|owner| owner.mod_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ludeon.rimworld", "example.bionicsfork"]
        );
        assert_eq!(dto.total, dto.totals.fields, "no filter applied here");
        assert!(
            dto.fields.iter().any(|field| field.path == "label"),
            "the contested label field must appear in the returned page"
        );
    }

    #[tokio::test]
    async fn set_merge_choices_round_trips_the_stored_choice() {
        let (temp_dir, state, key) = bionic_heart_conflict_state();

        let dto = set_merge_choices_inner(
            &state,
            SetMergeChoicesRequestDto {
                key: key.to_string(),
                choices: BTreeMap::from([(
                    "label".to_string(),
                    MergeChoiceDto::From {
                        mod_id: "ludeon.rimworld".to_string(),
                    },
                )]),
                patch_id: None,
            },
        )
        .await
        .expect("a valid choice on a live finding must be accepted");

        assert!(
            matches!(dto, MergeStateDto::Complete { .. }),
            "a single choice fully resolves this fixture's diff: {dto:?}"
        );

        let session_guard = state.session.read().expect("lock");
        let session = session_guard.as_ref().expect("session is still loaded");
        let stored = session
            .decisions()
            .get(&key)
            .map(|decision| &decision.action);
        assert_eq!(
            stored,
            Some(&Action::Merge {
                key: DefKey {
                    def_type: "HediffDef".to_string(),
                    def_name: "BionicHeart".to_string(),
                },
                choices: BTreeMap::from([(
                    "label".parse().expect("valid field path"),
                    MergeChoice::From {
                        mod_id: ModId::new("ludeon.rimworld"),
                    }
                )]),
            }),
            "the stored decision must carry the choice just made"
        );
        assert!(
            temp_dir
                .path()
                .join("profile")
                .join("decisions.json")
                .exists(),
            "DecideMerge must have persisted decisions.json under this test's own temp dir"
        );
    }

    #[tokio::test]
    async fn preview_merge_mod_file_returns_about_xml_after_a_complete_merge_decision() {
        let (_temp_dir, state, key) = bionic_heart_conflict_state();
        set_merge_choices_inner(
            &state,
            SetMergeChoicesRequestDto {
                key: key.to_string(),
                choices: BTreeMap::from([(
                    "label".to_string(),
                    MergeChoiceDto::From {
                        mod_id: "ludeon.rimworld".to_string(),
                    },
                )]),
                patch_id: None,
            },
        )
        .await
        .expect("a valid choice on a live finding must be accepted");

        let file = preview_merge_mod_file_inner(
            &state,
            PreviewMergeModFileRequestDto {
                relative_path: "About/About.xml".to_string(),
            },
        )
        .await
        .expect("a complete merge decision must render About.xml");

        assert!(
            file.content.contains("<ModMetaData>"),
            "rendered content: {}",
            file.content
        );
    }
}
