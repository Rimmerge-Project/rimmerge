//! Compat-patch commands:
//! `list_patches`, `get_patch`, `create_patch`, `update_patch`,
//! `delete_patch`, `list_patch_findings`, `get_patch_finding`,
//! `decide_patch`, `revert_patch_decision`, `import_profile_decisions`,
//! `prune_patch_decisions`, `get_patch_render`, `preview_patch_file`,
//! `export_patch`. The extended `get_merge_preview`/`set_merge_choices`
//! (which gain an optional `patchId`) live in `commands::merge` — they
//! already existed there before compat patches did.
//!
//! Every command follows `apps/desktop/CLAUDE.md`'s shape: thin, mapping
//! DTOs to `rim-session` calls inside [`with_session`]; filtering, paging,
//! and the scoped ledger itself all live in `rim-session`
//! (`Session::patch_findings`/`patch_ledger`).

use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use rim_resolve::domain::{Action, Decision, FindingKey, PatchId};
use rim_session::PreviewSlot;
use rim_session::use_cases::{
    CreatePatch, DecidePatch, DeletePatch, ExportOptions, ExportPatch, ImportProfileDecisions,
    PrunePatchDecisions, RenderPatch, RevertPatchDecision, UpdatePatch,
};

use crate::commands::{emit_session_changed, mod_names};
use crate::dto::finding::{DecideResultDto, FindingPageDto, ResolutionDetailDto};
use crate::dto::merge::{CaveatDto, MergeModFileDto};
use crate::dto::patch::{
    CreatePatchRequestDto, DecidePatchRequestDto, ExportPatchReportDto, ExportPatchRequestDto,
    ImportDecisionsReportDto, ImportProfileDecisionsRequestDto, PatchDetailDto,
    PatchFindingsRequestDto, PatchRenderDto, PatchSummaryDto, PatchUpdateResultDto,
    UpdatePatchRequestDto, patch_detail_dto, patch_render_dto, patch_summary_dto,
};
use crate::dto::project::SessionChangeReasonDto;
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Parses `text` as a [`PatchId`], mapping a malformed id to
/// [`CommandError::invalid_input`]. Shared by every command in this module
/// and by `commands::merge`'s extended `get_merge_preview`/
/// `set_merge_choices`, both of which take a patch id off the wire too.
pub(crate) fn parse_patch_id(text: &str) -> Result<PatchId, CommandError> {
    text.parse()
        .map_err(|error: rim_resolve::domain::PatchIdParseError| {
            CommandError::invalid_input(error.to_string())
        })
}

/// Builds `id`'s [`PatchDetailDto`] for the selected order — the shared
/// tail every command returning one needs: the scoped ledger's stats
/// (`Session::patch_ledger`), its orphaned decision keys
/// (`Session::patch_orphaned`), and every mod's display name.
///
/// # Errors
///
/// Returns [`CommandError::patch_not_found`] when `id` isn't loaded.
fn detail_for(
    session: &mut rim_session::Session,
    id: &PatchId,
) -> Result<PatchDetailDto, CommandError> {
    let source = session.selected();
    let stats = session.patch_ledger(id, source)?.stats;
    let orphaned = session.patch_orphaned(id, source)?;
    let names = mod_names(session);
    let project = session
        .patch(id)
        .unwrap_or_else(|| unreachable!("patch_ledger above already confirmed it's loaded"));
    Ok(patch_detail_dto(project, stats, &names, &orphaned))
}

/// Every loaded patch project, summarized.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn list_patches_inner(
    state: &AppState,
) -> Result<Vec<PatchSummaryDto>, CommandError> {
    with_session(state, move |session| {
        let source = session.selected();
        let ids: Vec<PatchId> = session.patches().map(|p| p.id().clone()).collect();
        let names = mod_names(session);
        let mut summaries = Vec::with_capacity(ids.len());
        for id in ids {
            let stats = session
                .patch_ledger(&id, source)
                .unwrap_or_else(|_| unreachable!("id came from session.patches() itself"))
                .stats;
            let project = session
                .patch(&id)
                .unwrap_or_else(|| unreachable!("id came from session.patches() itself"));
            summaries.push(patch_summary_dto(project, stats, &names));
        }
        Ok(summaries)
    })
    .await
}

/// See [`list_patches_inner`].
///
/// # Errors
///
/// See [`list_patches_inner`].
#[tauri::command]
pub async fn list_patches(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<PatchSummaryDto>, CommandError> {
    list_patches_inner(&state).await
}

/// One patch project's full detail.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::patch_not_found`] when `patch_id` isn't loaded.
pub(crate) async fn get_patch_inner(
    state: &AppState,
    patch_id: String,
) -> Result<PatchDetailDto, CommandError> {
    with_session(state, move |session| {
        let id = parse_patch_id(&patch_id)?;
        detail_for(session, &id)
    })
    .await
}

/// See [`get_patch_inner`].
///
/// # Errors
///
/// See [`get_patch_inner`].
#[tauri::command]
pub async fn get_patch(
    state: tauri::State<'_, AppState>,
    patch_id: String,
) -> Result<PatchDetailDto, CommandError> {
    get_patch_inner(&state, patch_id).await
}

/// Validates and creates a new compat patch project, persisting it before
/// making it visible on the session.
///
/// # Errors
///
/// Returns [`CommandError::patch_identity_invalid`] when the proposed
/// package id/display name is invalid or already taken,
/// [`CommandError::invalid_input`] when the scope is too small, names an
/// inactive mod, or names a Rimmerge-generated mod, or [`CommandError`]
/// when persisting the new project fails.
pub(crate) async fn create_patch_inner(
    state: &AppState,
    request: CreatePatchRequestDto,
) -> Result<PatchDetailDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let use_case = CreatePatch::new(adapters.patch_store);
        let id = use_case.execute(session, request.into())?;
        detail_for(session, &id)
    })
    .await
}

/// See [`create_patch_inner`]; also emits `session://changed` on success.
///
/// # Errors
///
/// See [`create_patch_inner`].
#[tauri::command]
pub async fn create_patch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: CreatePatchRequestDto,
) -> Result<PatchDetailDto, CommandError> {
    let result = create_patch_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::PatchCreated);
    }
    result
}

/// Applies any of `request`'s given fields to one patch project.
///
/// # Errors
///
/// Returns [`CommandError::patch_not_found`] when `request.patch_id` isn't
/// loaded, the same identity/scope errors [`create_patch_inner`] can
/// return, or [`CommandError`] when persisting the update fails.
pub(crate) async fn update_patch_inner(
    state: &AppState,
    request: UpdatePatchRequestDto,
) -> Result<PatchUpdateResultDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_patch_id(&request.patch_id)?;
        let use_case = UpdatePatch::new(adapters.patch_store);
        let scope_change = use_case.execute(session, &id, request.into())?;
        Ok(PatchUpdateResultDto {
            patch: detail_for(session, &id)?,
            scope_change: scope_change.as_ref().map(Into::into),
        })
    })
    .await
}

/// See [`update_patch_inner`]; also emits `session://changed` on success.
///
/// # Errors
///
/// See [`update_patch_inner`].
#[tauri::command]
pub async fn update_patch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: UpdatePatchRequestDto,
) -> Result<PatchUpdateResultDto, CommandError> {
    let result = update_patch_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::PatchChanged);
    }
    result
}

/// Deletes a patch project. Never touches a previously exported folder.
///
/// # Errors
///
/// Returns [`CommandError::patch_not_found`] when `patch_id` isn't
/// loaded, or [`CommandError`] when removing the persisted file fails.
pub(crate) async fn delete_patch_inner(
    state: &AppState,
    patch_id: String,
) -> Result<(), CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_patch_id(&patch_id)?;
        let use_case = DeletePatch::new(adapters.patch_store);
        use_case.execute(session, &id)?;
        Ok(())
    })
    .await
}

/// See [`delete_patch_inner`]; also emits `session://changed` on success.
///
/// # Errors
///
/// See [`delete_patch_inner`].
#[tauri::command]
pub async fn delete_patch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    patch_id: String,
) -> Result<(), CommandError> {
    let result = delete_patch_inner(&state, patch_id).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::PatchDeleted);
    }
    result
}

/// Pages one patch's own scoped ledger for the selected order.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::patch_not_found`] when `request.patch_id` isn't
/// loaded.
pub(crate) async fn list_patch_findings_inner(
    state: &AppState,
    request: PatchFindingsRequestDto,
) -> Result<FindingPageDto, CommandError> {
    with_session(state, move |session| {
        let id = parse_patch_id(&request.patch_id)?;
        let source = session.selected();
        let page = session.patch_findings(&id, source, &request.filter.into())?;
        let items = page
            .items
            .iter()
            .filter_map(|key| {
                session
                    .patch_resolution(&id, source, key)
                    .unwrap_or_else(|_| {
                        unreachable!("id already validated by patch_findings above")
                    })
                    .map(Into::into)
            })
            .collect();
        Ok(FindingPageDto {
            total: page.total,
            items,
        })
    })
    .await
}

/// See [`list_patch_findings_inner`].
///
/// # Errors
///
/// See [`list_patch_findings_inner`].
#[tauri::command]
pub async fn list_patch_findings(
    state: tauri::State<'_, AppState>,
    request: PatchFindingsRequestDto,
) -> Result<FindingPageDto, CommandError> {
    list_patch_findings_inner(&state, request).await
}

/// One finding's full detail from a patch's own scoped ledger.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::patch_not_found`] when `patch_id` isn't loaded,
/// [`CommandError::invalid_input`] when `key` doesn't parse, or
/// [`CommandError::finding_not_found`] when it isn't currently live in
/// that patch's scope.
pub(crate) async fn get_patch_finding_inner(
    state: &AppState,
    patch_id: String,
    key: String,
) -> Result<ResolutionDetailDto, CommandError> {
    with_session(state, move |session| {
        let id = parse_patch_id(&patch_id)?;
        let finding_key = FindingKey::from_str(&key)?;
        let source = session.selected();
        session
            .patch_resolution(&id, source, &finding_key)?
            .map(Into::into)
            .ok_or_else(|| CommandError::finding_not_found(&key))
    })
    .await
}

/// See [`get_patch_finding_inner`].
///
/// # Errors
///
/// See [`get_patch_finding_inner`].
#[tauri::command]
pub async fn get_patch_finding(
    state: tauri::State<'_, AppState>,
    patch_id: String,
    key: String,
) -> Result<ResolutionDetailDto, CommandError> {
    get_patch_finding_inner(&state, patch_id, key).await
}

/// Records a decision on one of a patch's own findings, persisting it
/// before returning. `stats.resorted`/`stats.moved_mods` are always
/// `false`/`0` — a patch decision never touches the sort or the profile
/// ledger.
///
/// # Errors
///
/// Returns [`CommandError::patch_not_found`] when `request.patch_id`
/// isn't loaded, [`CommandError::invalid_input`] when `request.key`/
/// `.action` don't parse or the action/key isn't one this patch's scope
/// admits, or [`CommandError`] when persisting the decision fails.
pub(crate) async fn decide_patch_inner(
    state: &AppState,
    request: DecidePatchRequestDto,
) -> Result<DecideResultDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_patch_id(&request.patch_id)?;
        let key = FindingKey::from_str(&request.key)?;
        let action = Action::try_from(request.action)?;
        let decision = Decision {
            key,
            action,
            note: request.note,
            decided_at: jiff::Timestamp::now(),
        };
        let use_case = DecidePatch::new(adapters.patch_store);
        let stats = use_case.execute(session, &id, decision)?;
        Ok(DecideResultDto {
            stats: stats.into(),
            resorted: false,
            moved_mods: 0,
        })
    })
    .await
}

/// See [`decide_patch_inner`]; also emits `session://changed` on success.
///
/// # Errors
///
/// See [`decide_patch_inner`].
#[tauri::command]
pub async fn decide_patch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: DecidePatchRequestDto,
) -> Result<DecideResultDto, CommandError> {
    let result = decide_patch_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::PatchDecided);
    }
    result
}

/// Removes a decision from one of a patch's own findings, persisting the
/// change before returning.
///
/// # Errors
///
/// Returns [`CommandError::patch_not_found`] when `patch_id` isn't
/// loaded, [`CommandError::invalid_input`] when `key` doesn't parse,
/// [`CommandError::finding_not_found`] when there was no decision on
/// `key` to remove, or [`CommandError`] when persisting the removal
/// fails.
pub(crate) async fn revert_patch_decision_inner(
    state: &AppState,
    patch_id: String,
    key: String,
) -> Result<DecideResultDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_patch_id(&patch_id)?;
        let finding_key = FindingKey::from_str(&key)?;
        let use_case = RevertPatchDecision::new(adapters.patch_store);
        let removed = use_case.execute(session, &id, &finding_key)?;
        if removed.is_none() {
            return Err(CommandError::finding_not_found(&key));
        }
        let source = session.selected();
        let stats = session
            .patch_ledger(&id, source)
            .unwrap_or_else(|_| {
                unreachable!("revert_patch_decision above already confirmed it's loaded")
            })
            .stats;
        Ok(DecideResultDto {
            stats: stats.into(),
            resorted: false,
            moved_mods: 0,
        })
    })
    .await
}

/// See [`revert_patch_decision_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`revert_patch_decision_inner`].
#[tauri::command]
pub async fn revert_patch_decision(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    patch_id: String,
    key: String,
) -> Result<DecideResultDto, CommandError> {
    let result = revert_patch_decision_inner(&state, patch_id, key).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::PatchReverted);
    }
    result
}

/// Copies the profile's `Merge`/`ShipAsset` decisions on the given keys
/// (or, when `request.keys` is `null`, every key the patch's own scoped
/// ledger admits) into the patch, one save at the end.
///
/// # Errors
///
/// Returns [`CommandError::patch_not_found`] when `request.patch_id`
/// isn't loaded, [`CommandError::invalid_input`] when any of
/// `request.keys` doesn't parse, or [`CommandError`] when persisting the
/// import fails.
pub(crate) async fn import_profile_decisions_inner(
    state: &AppState,
    request: ImportProfileDecisionsRequestDto,
) -> Result<ImportDecisionsReportDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_patch_id(&request.patch_id)?;
        let keys = request
            .keys
            .map(|texts| {
                texts
                    .iter()
                    .map(|text| FindingKey::from_str(text))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        let use_case = ImportProfileDecisions::new(adapters.patch_store);
        let report = use_case.execute(session, &id, keys.as_deref())?;
        Ok((&report).into())
    })
    .await
}

/// See [`import_profile_decisions_inner`]; also emits `session://changed`
/// on success.
///
/// # Errors
///
/// See [`import_profile_decisions_inner`].
#[tauri::command]
pub async fn import_profile_decisions(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ImportProfileDecisionsRequestDto,
) -> Result<ImportDecisionsReportDto, CommandError> {
    let result = import_profile_decisions_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::PatchDecided);
    }
    result
}

/// Removes every decision `patch_id`'s own scoped ledger currently
/// considers orphaned.
///
/// # Errors
///
/// Returns [`CommandError::patch_not_found`] when `patch_id` isn't
/// loaded, or [`CommandError`] when persisting the prune fails.
pub(crate) async fn prune_patch_decisions_inner(
    state: &AppState,
    patch_id: String,
) -> Result<PatchDetailDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_patch_id(&patch_id)?;
        let use_case = PrunePatchDecisions::new(adapters.patch_store);
        use_case.execute(session, &id)?;
        detail_for(session, &id)
    })
    .await
}

/// See [`prune_patch_decisions_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`prune_patch_decisions_inner`].
#[tauri::command]
pub async fn prune_patch_decisions(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    patch_id: String,
) -> Result<PatchDetailDto, CommandError> {
    let result = prune_patch_decisions_inner(&state, patch_id).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::PatchChanged);
    }
    result
}

/// Renders `patch_id`'s own merge mod candidate in memory — what
/// `export_patch` would currently produce. Never writes.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, [`CommandError::patch_not_found`] when `patch_id` isn't
/// loaded, or a `MergeSourceFailed`/`internal` [`CommandError`] when
/// rendering fails.
pub(crate) async fn get_patch_render_inner(
    state: &AppState,
    patch_id: String,
) -> Result<PatchRenderDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_patch_id(&patch_id)?;
        let project = session
            .patch(&id)
            .cloned()
            .ok_or_else(|| CommandError::patch_not_found(&patch_id))?;
        let use_case = RenderPatch::new(adapters.def_reader, adapters.asset_locator);
        let render = use_case.execute(session, &id)?;

        // Every entry's own preview is already cached under this slot by
        // `RenderPatch`/`PlanMerge` above — collecting each one's caveats
        // here (rather than `MergeModRender` aggregating them itself) is
        // the same split `commands::merge::get_merge_preview_inner` uses:
        // caveat *text* is a display concern, built once the command has
        // the cached previews in hand.
        let slot = PreviewSlot::patch(session.selected(), id.clone());
        let caveats: Vec<CaveatDto> = render
            .entries
            .iter()
            .filter_map(|entry| session.merge_preview(&slot, &entry.key))
            .flat_map(|preview| preview.plan.caveats.iter().map(CaveatDto::from))
            .collect();

        let names = mod_names(session);
        Ok(patch_render_dto(&render, &project, &names, caveats))
    })
    .await
}

/// See [`get_patch_render_inner`].
///
/// # Errors
///
/// See [`get_patch_render_inner`].
#[tauri::command]
pub async fn get_patch_render(
    state: tauri::State<'_, AppState>,
    patch_id: String,
) -> Result<PatchRenderDto, CommandError> {
    get_patch_render_inner(&state, patch_id).await
}

/// Renders `patch_id`'s own merge mod candidate in memory and returns the
/// text content of the rendered file whose relative path exactly matches
/// `relative_path` — never writes anything to disk.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, [`CommandError::patch_not_found`] when `patch_id` isn't
/// loaded, [`CommandError::invalid_input`] when no rendered file matches
/// `relative_path` or the match is a binary (`CopyFrom`) asset copy
/// rather than text, or a `MergeSourceFailed`/`internal` [`CommandError`]
/// when rendering itself fails.
pub(crate) async fn preview_patch_file_inner(
    state: &AppState,
    patch_id: String,
    relative_path: String,
) -> Result<MergeModFileDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let id = parse_patch_id(&patch_id)?;
        let use_case = RenderPatch::new(adapters.def_reader, adapters.asset_locator);
        let render = use_case.execute(session, &id)?;
        let wanted = PathBuf::from(&relative_path);

        let rendered = render.rendered.ok_or_else(|| {
            CommandError::invalid_input(format!(
                "{relative_path}: nothing renders for the current decisions"
            ))
        })?;
        let file = rendered
            .files
            .iter()
            .find(|file| file.relative_path == wanted)
            .ok_or_else(|| {
                CommandError::invalid_input(format!(
                    "{relative_path}: not a file this render produces"
                ))
            })?;
        match &file.content {
            rim_merge::emit::FileContent::Text(content) => Ok(MergeModFileDto {
                content: content.clone(),
            }),
            rim_merge::emit::FileContent::CopyFrom(_) => Err(CommandError::invalid_input(format!(
                "{relative_path}: a binary asset copy has no text preview"
            ))),
        }
    })
    .await
}

/// See [`preview_patch_file_inner`].
///
/// # Errors
///
/// See [`preview_patch_file_inner`].
#[tauri::command]
pub async fn preview_patch_file(
    state: tauri::State<'_, AppState>,
    patch_id: String,
    relative_path: String,
) -> Result<MergeModFileDto, CommandError> {
    preview_patch_file_inner(&state, patch_id, relative_path).await
}

/// Renders, writes, and (when asked) installs `request.patch_id`'s
/// patch. Refuses to write to the game's `Mods` folder while
/// `RimWorldWin64.exe` looks like it's running (per
/// [`crate::state::AppState::game_process_probe`]) unless `request.force`
/// is set — the same gate [`crate::commands::apply::apply`] uses; this
/// use case has no process access of its own.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is
/// loaded, [`CommandError::rimworld_running`] when `request.install` is
/// set, the game process is detected, and `request.force` is `false`,
/// [`CommandError::patch_not_found`] when `request.patch_id` isn't
/// loaded, [`CommandError::invalid_input`] when there's nothing to
/// export or `request.out_dir` is (or is inside) the game's `Mods`
/// folder or an existing folder not generated by this patch, or
/// [`CommandError`] when rendering, writing, or persisting fails.
pub(crate) async fn export_patch_inner(
    state: &AppState,
    request: ExportPatchRequestDto,
) -> Result<ExportPatchReportDto, CommandError> {
    let adapters = state.adapters.clone();
    let probe = Arc::clone(&state.game_process_probe);
    with_session(state, move |session| {
        let id = parse_patch_id(&request.patch_id)?;
        if request.install && !request.force && probe.is_running() {
            return Err(CommandError::rimworld_running());
        }

        // Absolutized (`std::path::absolute`) and, when it already
        // exists, canonicalized (resolving `.`/`..`, symlinks, and
        // junctions, with the `\\?\` prefix stripped back off) —
        // `ExportPatch` itself has no filesystem access of its own and
        // can only compare `out_dir` lexically (see that use case's doc
        // comment on `is_inside`), so a relative path from a hand-typed
        // field or a junction pointing into the game's `Mods` folder
        // must be resolved here, before it ever reaches the use case.
        // Tauri's own folder picker always returns an absolute path, so
        // this only ever does real work against a hand-typed `out_dir`.
        let out_dir = rim_io::resolve_user_dir(Path::new(&request.out_dir)).map_err(|error| {
            CommandError::invalid_input(format!("resolving out_dir {}: {error}", request.out_dir))
        })?;

        let use_case = ExportPatch::new(
            adapters.merge_mod_writer,
            adapters.def_reader,
            adapters.asset_locator,
            adapters.config_store,
            adapters.patch_store,
        );
        let outcome = use_case.execute(
            session,
            &id,
            ExportOptions {
                out_dir,
                install: request.install,
            },
        )?;

        Ok(ExportPatchReportDto {
            export_path: outcome.export_path.display().to_string(),
            installed_path: outcome
                .installed_path
                .map(|path| path.display().to_string()),
            mods_config_backup_path: outcome
                .mods_config_backup
                .map(|path| path.display().to_string()),
            skipped: outcome.skipped.iter().map(ToString::to_string).collect(),
            files: outcome
                .files
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            decisions_sha256: outcome.decisions_sha256,
        })
    })
    .await
}

/// See [`export_patch_inner`]; also emits `session://changed` on success.
///
/// # Errors
///
/// See [`export_patch_inner`].
#[tauri::command]
pub async fn export_patch(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: ExportPatchRequestDto,
) -> Result<ExportPatchReportDto, CommandError> {
    let result = export_patch_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::PatchExported);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::DefKey;
    use rim_resolve::test_support::ReportBuilder;
    use rim_session::test_support::bionic_heart_fixture_flat;

    use super::*;
    use crate::dto::finding::ActionDto;
    use crate::dto::merge::{
        MergeFieldFilterDto, MergePreviewRequestDto, SetMergeChoicesRequestDto,
    };
    use crate::dto::patch::{
        CreatePatchRequestDto, PatchFindingsRequestDto, UpdatePatchRequestDto,
    };
    use crate::error::CommandErrorCode;
    use crate::test_support::session_with_temp_paths;

    /// A session with a live `HediffDef/BionicHeart` `DefOverride`
    /// finding between `ludeon.rimworld`/`example.bionicsfork` — the same
    /// worked example `commands::merge`'s own tests use — plus a real
    /// temp-dir profile so `CreatePatch`/`DecidePatch`/`ExportPatch`'s
    /// own stores actually persist somewhere disposable. Uses
    /// `bionic_heart_fixture_flat`, not `bionic_heart_fixture` itself: the
    /// real fixture's `ParentName` difference fires the structural guard,
    /// which would keep a decided merge permanently `NeedsFieldInput` and
    /// break this module's own full-lifecycle (create/decide/export) tests —
    /// none of which are about the guard.
    fn bionic_heart_state() -> (tempfile::TempDir, AppState) {
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
        state.adapters.def_reader = std::sync::Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);
        (temp_dir, state)
    }

    fn create_request() -> CreatePatchRequestDto {
        CreatePatchRequestDto {
            name: "BIONICS compat".to_string(),
            package_id: "sample.bionicscompat".to_string(),
            display_name: "BIONICS Compatibility".to_string(),
            scope: vec![
                "ludeon.rimworld".to_string(),
                "example.bionicsfork".to_string(),
            ],
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

    #[tokio::test]
    async fn get_patch_reports_patch_not_found_for_an_unknown_id() {
        let (_temp_dir, state) = bionic_heart_state();

        let result = get_patch_inner(&state, "abcdef012345".to_string()).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::PatchNotFound);
    }

    #[tokio::test]
    async fn create_patch_rejects_an_inactive_scope_member() {
        let (_temp_dir, state) = bionic_heart_state();
        let mut request = create_request();
        request.scope = vec!["ludeon.rimworld".to_string(), "not.active".to_string()];

        let result = create_patch_inner(&state, request).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn create_patch_rejects_a_package_id_already_used_by_an_active_mod() {
        let (_temp_dir, state) = bionic_heart_state();
        let mut request = create_request();
        request.package_id = "ludeon.rimworld".to_string();

        let result = create_patch_inner(&state, request).await;

        assert_eq!(
            result.unwrap_err().code,
            CommandErrorCode::PatchIdentityInvalid
        );
    }

    /// The full patch lifecycle: create -> list
    /// -> decide (scoped) -> `get_merge_preview` with `patchId` ->
    /// `set_merge_choices` with `patchId` -> export to a temp dir (files
    /// exist, `About.xml` names the package id) -> delete.
    #[tokio::test]
    async fn full_patch_lifecycle_create_list_decide_merge_export_delete() {
        let (temp_dir, state) = bionic_heart_state();

        let created = create_patch_inner(&state, create_request())
            .await
            .expect("a valid patch must be created");
        let patch_id = created.id.clone();
        assert_eq!(created.scope.len(), 2);

        let listed = list_patches_inner(&state)
            .await
            .expect("listing must succeed");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, patch_id);

        let decide_result = decide_patch_inner(
            &state,
            DecidePatchRequestDto {
                patch_id: patch_id.clone(),
                key: bionic_heart_key().to_string(),
                action: ActionDto::Ignore,
                note: None,
            },
        )
        .await
        .expect("Ignore is always a valid patch action");
        assert!(!decide_result.resorted);
        assert_eq!(decide_result.moved_mods, 0);

        // Revert the Ignore so the merge lifecycle below starts clean,
        // then decide it through the extended merge commands instead.
        revert_patch_decision_inner(&state, patch_id.clone(), bionic_heart_key().to_string())
            .await
            .expect("revert must succeed");

        let preview = crate::commands::merge::get_merge_preview_inner(
            &state,
            MergePreviewRequestDto {
                key: bionic_heart_key().to_string(),
                filter: MergeFieldFilterDto::default(),
                patch_id: Some(patch_id.clone()),
            },
        )
        .await
        .expect("a scoped preview must build for a finding this patch's scope admits");
        assert_eq!(
            preview
                .owners
                .iter()
                .map(|o| o.mod_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ludeon.rimworld", "example.bionicsfork"]
        );
        assert!(preview.out_of_scope_owners.is_empty());

        let merge_state = crate::commands::merge::set_merge_choices_inner(
            &state,
            SetMergeChoicesRequestDto {
                key: bionic_heart_key().to_string(),
                choices: BTreeMap::from([(
                    "label".to_string(),
                    crate::dto::finding::MergeChoiceDto::From {
                        mod_id: "ludeon.rimworld".to_string(),
                    },
                )]),
                patch_id: Some(patch_id.clone()),
            },
        )
        .await
        .expect("a valid choice on a scoped finding must be accepted");
        assert!(matches!(
            merge_state,
            crate::dto::merge::MergeStateDto::Complete { .. }
        ));
        // This assertion guards that `set_merge_choices` with a
        // `patchId` must record its decision on the *patch's own*
        // decision set, never the profile's — the two are independent
        // bounded contexts (`rim-session`'s own `CLAUDE.md`).
        {
            let mut guard = state.session.write().expect("lock");
            let session = guard.as_mut().expect("loaded");
            assert!(
                session.decisions().get(&bionic_heart_key()).is_none(),
                "a patch-scoped decision must never land in the profile's own decision set"
            );
        }

        let scoped_page = list_patch_findings_inner(
            &state,
            PatchFindingsRequestDto {
                patch_id: patch_id.clone(),
                filter: crate::dto::finding::FindingFilterDto {
                    limit: 200,
                    ..crate::dto::finding::FindingFilterDto::default()
                },
            },
        )
        .await
        .expect("listing the patch's own findings must succeed");
        assert_eq!(scoped_page.total, 1);
        assert!(scoped_page.items[0].scope.is_some());

        let export_dir = temp_dir.path().join("export");
        let export_report = export_patch_inner(
            &state,
            ExportPatchRequestDto {
                patch_id: patch_id.clone(),
                out_dir: export_dir.display().to_string(),
                install: false,
                force: false,
            },
        )
        .await
        .expect("export must succeed");
        let about_xml_path =
            std::path::Path::new(&export_report.export_path).join("About/About.xml");
        let about_xml = std::fs::read_to_string(&about_xml_path)
            .unwrap_or_else(|error| panic!("must read {}: {error}", about_xml_path.display()));
        assert!(
            about_xml.contains("sample.bionicscompat"),
            "About.xml must carry the patch's own package id: {about_xml}"
        );
        assert!(!export_report.files.is_empty());

        delete_patch_inner(&state, patch_id.clone())
            .await
            .expect("delete must succeed");
        let listed_after_delete = list_patches_inner(&state)
            .await
            .expect("listing must still succeed");
        assert!(listed_after_delete.is_empty());
    }

    #[tokio::test]
    async fn export_patch_refuses_when_the_game_is_running_and_install_is_not_forced() {
        let (temp_dir, mut state) = bionic_heart_state();
        state.game_process_probe = Arc::new(FixedProbe(true));
        let created = create_patch_inner(&state, create_request())
            .await
            .expect("a valid patch must be created");
        decide_patch_inner(
            &state,
            DecidePatchRequestDto {
                patch_id: created.id.clone(),
                key: bionic_heart_key().to_string(),
                action: ActionDto::Ignore,
                note: None,
            },
        )
        .await
        .expect("Ignore is always a valid patch action");

        let result = export_patch_inner(
            &state,
            ExportPatchRequestDto {
                patch_id: created.id,
                out_dir: temp_dir.path().join("out").display().to_string(),
                install: true,
                force: false,
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::RimworldRunning);
    }

    struct FixedProbe(bool);

    impl crate::state::GameProcessProbe for FixedProbe {
        fn is_running(&self) -> bool {
            self.0
        }
    }

    /// The counterpart to
    /// [`export_patch_refuses_when_the_game_is_running_and_install_is_not_forced`]:
    /// `force: true` bypasses the running-game refusal and the install
    /// actually copies the rendered folder into the (temp, scratch)
    /// game's own `Mods/`.
    #[tokio::test]
    async fn export_patch_installs_when_forced_despite_the_game_running() {
        let (temp_dir, mut state) = bionic_heart_state();
        state.game_process_probe = Arc::new(FixedProbe(true));
        let created = create_patch_inner(&state, create_request())
            .await
            .expect("a valid patch must be created");

        let merge_state = crate::commands::merge::set_merge_choices_inner(
            &state,
            SetMergeChoicesRequestDto {
                key: bionic_heart_key().to_string(),
                choices: BTreeMap::from([(
                    "label".to_string(),
                    crate::dto::finding::MergeChoiceDto::From {
                        mod_id: "ludeon.rimworld".to_string(),
                    },
                )]),
                patch_id: Some(created.id.clone()),
            },
        )
        .await
        .expect("a valid choice on a scoped finding must be accepted");
        assert!(matches!(
            merge_state,
            crate::dto::merge::MergeStateDto::Complete { .. }
        ));

        let export_report = export_patch_inner(
            &state,
            ExportPatchRequestDto {
                patch_id: created.id,
                out_dir: temp_dir.path().join("out").display().to_string(),
                install: true,
                force: true,
            },
        )
        .await
        .expect("forced export with install must succeed despite the game running");

        assert!(
            export_report.installed_path.is_some(),
            "a forced install must still produce an installed path"
        );
    }

    #[tokio::test]
    async fn update_patch_reports_patch_not_found_for_an_unknown_id() {
        let (_temp_dir, state) = bionic_heart_state();

        let result = update_patch_inner(
            &state,
            UpdatePatchRequestDto {
                patch_id: "abcdef012345".to_string(),
                name: Some("renamed".to_string()),
                package_id: None,
                display_name: None,
                author: None,
                description: None,
                scope: None,
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::PatchNotFound);
    }

    #[tokio::test]
    async fn import_profile_decisions_reports_a_skipped_out_of_scope_key() {
        let (_temp_dir, state) = bionic_heart_state();
        let created = create_patch_inner(&state, create_request())
            .await
            .expect("a valid patch must be created");
        // A `DefOverride` naming only one of this patch's two scope
        // members plus an outside mod: `owner_set_membership` needs at
        // least *two* scope members among the owners, so this key is
        // `Outside` — `Merge`/`ShipAsset` is the shape `ImportProfileDecisions`
        // actually forwards to `PatchProject::decide` (an `Ignore`
        // decision is filtered out before ever reaching it).
        let out_of_scope_key = FindingKey::DefOverride {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Unrelated".to_string(),
            },
            owners: [ModId::new("ludeon.rimworld"), ModId::new("unrelated.mod")]
                .into_iter()
                .collect(),
        };
        {
            let mut guard = state.session.write().expect("lock");
            let session = guard.as_mut().expect("loaded");
            session
                .decide(Decision {
                    key: out_of_scope_key.clone(),
                    action: Action::Merge {
                        key: DefKey {
                            def_type: "ThingDef".to_string(),
                            def_name: "Unrelated".to_string(),
                        },
                        choices: BTreeMap::new(),
                    },
                    note: None,
                    decided_at: jiff::Timestamp::UNIX_EPOCH,
                })
                .expect("every Action validates on the profile (ResolveError is uninhabited)");
        }

        let report = import_profile_decisions_inner(
            &state,
            ImportProfileDecisionsRequestDto {
                patch_id: created.id,
                keys: Some(vec![out_of_scope_key.to_string()]),
            },
        )
        .await
        .expect("import must succeed even when every key is skipped");

        assert!(report.imported.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].key, out_of_scope_key.to_string());
    }

    #[test]
    fn parse_patch_id_rejects_a_malformed_id() {
        let result = parse_patch_id("not-a-valid-id");
        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }
}
