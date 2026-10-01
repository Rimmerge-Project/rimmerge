//! `list_tags`/`set_manual_tag`: the rules page's tag commands.

use rim_analyzer::domain::ModId;
use rim_resolve::domain::Tag;

use crate::commands::emit_session_changed;
use crate::dto::project::SessionChangeReasonDto;
use crate::dto::tag::{SetManualTagRequestDto, TaggingDto};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// The current tag assignments (inference plus manual overrides).
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn list_tags_inner(state: &AppState) -> Result<TaggingDto, CommandError> {
    with_session(state, |session| Ok(session.tagging().into())).await
}

/// See [`list_tags_inner`].
///
/// # Errors
///
/// See [`list_tags_inner`].
#[tauri::command]
pub async fn list_tags(state: tauri::State<'_, AppState>) -> Result<TaggingDto, CommandError> {
    list_tags_inner(&state).await
}

/// Sets (or replaces) a manual tag override, persisting the change.
///
/// # Errors
///
/// Returns [`CommandError::invalid_input`] when `request.tag` isn't a
/// valid tag slug, or [`CommandError`] when saving fails.
pub(crate) async fn set_manual_tag_inner(
    state: &AppState,
    request: SetManualTagRequestDto,
) -> Result<TaggingDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let tag = Tag::new(request.tag)?;
        let use_case = rim_session::use_cases::SetManualTag::new(adapters.rule_store);
        use_case.execute(
            session,
            ModId::new(request.mod_id),
            tag,
            request.mode.into(),
        )?;
        Ok(session.tagging().into())
    })
    .await
}

/// See [`set_manual_tag_inner`]; also emits `session://changed` on
/// success.
///
/// # Errors
///
/// See [`set_manual_tag_inner`].
#[tauri::command]
pub async fn set_manual_tag(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: SetManualTagRequestDto,
) -> Result<TaggingDto, CommandError> {
    let result = set_manual_tag_inner(&state, request).await;
    if result.is_ok() {
        emit_session_changed(&app, SessionChangeReasonDto::TagSet);
    }
    result
}
