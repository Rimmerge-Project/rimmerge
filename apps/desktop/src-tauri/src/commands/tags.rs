//! `list_tags`/`set_manual_tag`/`set_manual_tags`: the rules page's tag commands.

use rim_analyzer::domain::ModId;
use rim_resolve::domain::Tag;

use crate::commands::emit_session_changed;
use crate::dto::project::SessionChangeReasonDto;
use crate::dto::tag::{
    ManualTagsOutcomeDto, SetManualTagRequestDto, SetManualTagsRequestDto, TaggingDto,
};
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

/// Adds or removes one manual tag on many mods with a single save and no re-sort.
///
/// # Errors
///
/// Returns [`CommandError::invalid_input`] when `request.tag` isn't a valid tag slug or the
/// request names too many mods, or [`CommandError`] when saving fails (every override is then
/// rolled back).
pub(crate) async fn set_manual_tags_inner(
    state: &AppState,
    request: SetManualTagsRequestDto,
) -> Result<ManualTagsOutcomeDto, CommandError> {
    let mods = request.parse_mods()?;
    let tag = Tag::new(request.tag)?;
    let mode = request.mode.into();
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let use_case = rim_session::use_cases::SetManualTags::new(adapters.rule_store);
        Ok(use_case.execute(session, &mods, &tag, mode)?.into())
    })
    .await
}

/// See `set_manual_tags_inner`; also emits `session://changed` when something changed.
///
/// # Errors
///
/// See `set_manual_tags_inner`.
#[tauri::command]
pub async fn set_manual_tags(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    request: SetManualTagsRequestDto,
) -> Result<ManualTagsOutcomeDto, CommandError> {
    let result = set_manual_tags_inner(&state, request).await;
    if result.as_ref().is_ok_and(|outcome| outcome.changed > 0) {
        emit_session_changed(&app, SessionChangeReasonDto::TagSet);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::common::TagModeDto;
    use crate::test_support::session_fixture_with_temp_paths;

    fn request(ids: &[&str], tag: &str, mode: TagModeDto) -> SetManualTagsRequestDto {
        SetManualTagsRequestDto {
            mod_ids: ids.iter().map(ToString::to_string).collect(),
            tag: tag.to_string(),
            mode,
        }
    }

    #[tokio::test]
    async fn set_manual_tags_tags_every_mod_and_writes_rules_once() {
        let (temp_dir, session) = session_fixture_with_temp_paths(&["a", "b", "c"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let outcome = set_manual_tags_inner(
            &state,
            request(&["a", "b", "a"], "framework", TagModeDto::Add),
        )
        .await
        .expect("bulk tag");

        assert_eq!((outcome.changed, outcome.unchanged), (2, 0));
        assert!(temp_dir.path().join("profile").join("rules.json").exists());
        let tagging = list_tags_inner(&state).await.expect("tags");
        assert_eq!(tagging.assignments.len(), 2);
    }

    #[tokio::test]
    async fn set_manual_tags_reports_a_tag_a_decision_keeps_and_records_nothing() {
        let (temp_dir, mut session) = session_fixture_with_temp_paths(&["a", "b"]);
        let tag = rim_resolve::domain::Tag::new("framework").expect("valid tag");
        session
            .decide(rim_resolve::domain::Decision {
                key: rim_resolve::domain::FindingKey::TagInferred {
                    mod_id: rim_analyzer::domain::ModId::new("a"),
                    tag: tag.clone(),
                },
                action: rim_resolve::domain::Action::AddTag {
                    mod_id: rim_analyzer::domain::ModId::new("a"),
                    tag,
                },
                note: None,
                decided_at: jiff::Timestamp::UNIX_EPOCH,
            })
            .expect("decide");
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let outcome = set_manual_tags_inner(
            &state,
            request(&["a", "b"], "framework", TagModeDto::Remove),
        )
        .await
        .expect("bulk remove");

        assert_eq!(
            (outcome.changed, outcome.kept_by_decision, outcome.unchanged),
            (0, 1, 1)
        );
        assert!(!temp_dir.path().join("profile").join("rules.json").exists());
    }

    #[tokio::test]
    async fn set_manual_tags_refuses_a_bad_tag_before_touching_anything() {
        let (temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);

        let refusal =
            set_manual_tags_inner(&state, request(&["a"], "Not A Slug", TagModeDto::Add)).await;

        assert!(refusal.is_err());
        assert!(!temp_dir.path().join("profile").join("rules.json").exists());
    }

    #[tokio::test]
    async fn set_manual_tags_refuses_an_unbounded_request() {
        let (_temp_dir, session) = session_fixture_with_temp_paths(&["a"]);
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        let ids: Vec<String> = (0..=50_000).map(|n| format!("mod.{n}")).collect();

        let refusal = set_manual_tags_inner(
            &state,
            SetManualTagsRequestDto {
                mod_ids: ids,
                tag: "framework".to_string(),
                mode: TagModeDto::Add,
            },
        )
        .await;

        assert!(refusal.is_err());
    }
}
