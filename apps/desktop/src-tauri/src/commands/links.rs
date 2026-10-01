//! `open_app_link`/`get_app_version`: the app's own fixed external links
//! (About section, sidebar support link, the language picker's issues
//! link) and its version string.

use rim_session::app_link::AppLinkTarget;

use crate::dto::links::AppLinkTargetDto;
use crate::error::CommandError;
use crate::state::AppState;

/// Opens one of the app's own fixed external links in the system
/// browser. The frontend names only `target`; the URL is a constant this
/// command re-derives from it, exactly as `open_mod_link` re-derives a
/// mod's URL from `mod_id`.
///
/// # Errors
///
/// Returns [`CommandError::internal`] when the OS couldn't open it.
pub(crate) async fn open_app_link_inner(
    state: &AppState,
    target: AppLinkTargetDto,
) -> Result<(), CommandError> {
    let target: AppLinkTarget = target.into();
    state
        .link_opener
        .open(&target.url())
        .map_err(|error| CommandError::internal(error.to_string()))
}

/// See `open_app_link_inner`.
///
/// # Errors
///
/// See `open_app_link_inner`.
#[tauri::command]
pub async fn open_app_link(
    state: tauri::State<'_, AppState>,
    target: AppLinkTargetDto,
) -> Result<(), CommandError> {
    open_app_link_inner(&state, target).await
}

/// This build's own version string (`CARGO_PKG_VERSION`), for the
/// Settings page's About section.
#[must_use]
fn app_version_inner() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// See `app_version_inner`.
#[tauri::command]
pub fn get_app_version() -> String {
    app_version_inner()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{LinkOpenError, LinkOpener};
    use crate::test_support::RecordingLinkOpener;

    fn state_with_recording_opener() -> (AppState, std::sync::Arc<RecordingLinkOpener>) {
        let opener = std::sync::Arc::new(RecordingLinkOpener::default());
        let state = AppState {
            link_opener: opener.clone(),
            ..AppState::default()
        };
        (state, opener)
    }

    /// A [`LinkOpener`] whose OS call always fails.
    #[derive(Debug)]
    struct FailingLinkOpener;

    impl LinkOpener for FailingLinkOpener {
        fn open(&self, _url: &rim_session::mod_info::ExternalUrl) -> Result<(), LinkOpenError> {
            Err(LinkOpenError("no default browser".to_string()))
        }
    }

    #[tokio::test]
    async fn an_opener_failure_surfaces_as_an_internal_error_naming_the_cause() {
        let state = AppState {
            link_opener: std::sync::Arc::new(FailingLinkOpener),
            ..AppState::default()
        };

        let error = open_app_link_inner(&state, AppLinkTargetDto::GithubRepo)
            .await
            .expect_err("the opener's failure must not be swallowed");

        assert_eq!(error.code, crate::error::CommandErrorCode::Internal);
        assert!(
            error.message.contains("no default browser"),
            "the cause must reach the message: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn opens_the_github_repo_link() {
        let (state, opener) = state_with_recording_opener();

        open_app_link_inner(&state, AppLinkTargetDto::GithubRepo)
            .await
            .expect("must succeed");

        assert_eq!(
            opener.calls(),
            vec!["https://github.com/Rimmerge-Project/rimmerge".to_string()]
        );
    }

    #[tokio::test]
    async fn opens_the_github_issues_link() {
        let (state, opener) = state_with_recording_opener();

        open_app_link_inner(&state, AppLinkTargetDto::GithubIssues)
            .await
            .expect("must succeed");

        assert_eq!(
            opener.calls(),
            vec!["https://github.com/Rimmerge-Project/rimmerge/issues".to_string()]
        );
    }

    #[tokio::test]
    async fn opens_the_support_link() {
        let (state, opener) = state_with_recording_opener();

        open_app_link_inner(&state, AppLinkTargetDto::Support)
            .await
            .expect("must succeed");

        assert_eq!(
            opener.calls(),
            vec!["https://buymeacoffee.com/nephilim".to_string()]
        );
    }

    #[test]
    fn app_version_is_a_non_empty_three_part_numeric_version() {
        // Comparing against `env!("CARGO_PKG_VERSION")` directly would be
        // tautological (the function's own body reads that exact macro) —
        // this instead checks the actual shape the frontend/UI relies on,
        // which a malformed `Cargo.toml` version field could break.
        let version = app_version_inner();
        let parts: Vec<&str> = version.split('.').collect();
        assert_eq!(
            parts.len(),
            3,
            "expected major.minor.patch, got {version:?}"
        );
        for part in parts {
            assert!(
                !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()),
                "expected a numeric version component, got {part:?} in {version:?}"
            );
        }
    }
}
