//! DTOs for `open_mod_link`/`open_app_link`/`get_app_version`. Every
//! link-opening command takes only a closed enum naming *which* link, and
//! the backend re-derives the actual URL — the frontend never sends a
//! URL, so a compromised or buggy webview can't hand the opener an
//! arbitrary one.

use rim_session::app_link::AppLinkTarget;
use rim_session::use_cases::ModLinkKind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which of a mod's own external links to open. Mirrors [`ModLinkKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ModLinkKindDto {
    /// See [`ModLinkKind::Workshop`].
    Workshop,
    /// See [`ModLinkKind::Homepage`].
    Homepage,
}

impl From<ModLinkKindDto> for ModLinkKind {
    fn from(value: ModLinkKindDto) -> Self {
        match value {
            ModLinkKindDto::Workshop => Self::Workshop,
            ModLinkKindDto::Homepage => Self::Homepage,
        }
    }
}

/// One of the app's own fixed external links. Mirrors [`AppLinkTarget`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum AppLinkTargetDto {
    /// See [`AppLinkTarget::GithubRepo`].
    GithubRepo,
    /// See [`AppLinkTarget::GithubIssues`].
    GithubIssues,
    /// See [`AppLinkTarget::Support`].
    Support,
}

impl From<AppLinkTargetDto> for AppLinkTarget {
    fn from(value: AppLinkTargetDto) -> Self {
        match value {
            AppLinkTargetDto::GithubRepo => Self::GithubRepo,
            AppLinkTargetDto::GithubIssues => Self::GithubIssues,
            AppLinkTargetDto::Support => Self::Support,
        }
    }
}
