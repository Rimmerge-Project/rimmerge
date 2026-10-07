//! DTOs for `get_game_launch_status`/`launch_game`, mirrored from
//! [`rim_session::GameLaunchStatus`] and its closed companion types.
//!
//! The request carries one enum and nothing else: no path, no URL, no
//! argument. The backend re-derives the install folder from the session
//! and the route from the install itself, so a compromised webview can't
//! name what gets started. [`GameLaunchStatusDto`] is `kind`-tagged with a
//! named payload struct per variant (the `HardProblemDto` pattern).

use rim_session::use_cases::IfNotApplied;
use rim_session::{GameLaunchStatus, LaunchRouteKind, LaunchUnavailable, UnappliedReason};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// How the game would be started. Mirrors [`LaunchRouteKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum GameLaunchRouteDto {
    /// See [`LaunchRouteKind::Steam`].
    Steam,
    /// See [`LaunchRouteKind::Executable`].
    Executable,
}

impl From<LaunchRouteKind> for GameLaunchRouteDto {
    fn from(value: LaunchRouteKind) -> Self {
        match value {
            LaunchRouteKind::Steam => Self::Steam,
            LaunchRouteKind::Executable => Self::Executable,
        }
    }
}

/// Why `ModsConfig.xml` doesn't hold what the user is looking at. Mirrors
/// [`UnappliedReason`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum UnappliedReasonDto {
    /// See [`UnappliedReason::ActivationChangesNotScanned`].
    ActivationChangesNotScanned,
    /// See [`UnappliedReason::OrderDiffers`].
    OrderDiffers,
}

impl From<UnappliedReason> for UnappliedReasonDto {
    fn from(value: UnappliedReason) -> Self {
        match value {
            UnappliedReason::ActivationChangesNotScanned => Self::ActivationChangesNotScanned,
            UnappliedReason::OrderDiffers => Self::OrderDiffers,
        }
    }
}

/// Why an install can't be started from here. Mirrors [`LaunchUnavailable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum LaunchUnavailableDto {
    /// See [`LaunchUnavailable::ExecutableMissing`].
    ExecutableMissing,
}

impl From<LaunchUnavailable> for LaunchUnavailableDto {
    fn from(value: LaunchUnavailable) -> Self {
        match value {
            LaunchUnavailable::ExecutableMissing => Self::ExecutableMissing,
        }
    }
}

/// [`GameLaunchStatusDto::Ready`]'s payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GameLaunchReadyDto {
    /// How a click would start the game.
    pub route: GameLaunchRouteDto,
}

/// [`GameLaunchStatusDto::NeedsApply`]'s payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GameLaunchNeedsApplyDto {
    /// How a click would start the game.
    pub route: GameLaunchRouteDto,
    /// Why the file is behind.
    pub reason: UnappliedReasonDto,
}

/// [`GameLaunchStatusDto::Unavailable`]'s payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GameLaunchUnavailableDto {
    /// Why this install can't be started from here.
    pub reason: LaunchUnavailableDto,
}

/// What the Launch RimWorld button shows. Mirrors [`GameLaunchStatus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum GameLaunchStatusDto {
    /// See [`GameLaunchStatus::Ready`].
    Ready(GameLaunchReadyDto),
    /// See [`GameLaunchStatus::NeedsApply`].
    NeedsApply(GameLaunchNeedsApplyDto),
    /// See [`GameLaunchStatus::GameRunning`].
    GameRunning,
    /// See [`GameLaunchStatus::Unavailable`].
    Unavailable(GameLaunchUnavailableDto),
}

impl From<GameLaunchStatus> for GameLaunchStatusDto {
    fn from(value: GameLaunchStatus) -> Self {
        match value {
            GameLaunchStatus::Ready { route } => Self::Ready(GameLaunchReadyDto {
                route: route.into(),
            }),
            GameLaunchStatus::NeedsApply { route, reason } => {
                Self::NeedsApply(GameLaunchNeedsApplyDto {
                    route: route.into(),
                    reason: reason.into(),
                })
            }
            GameLaunchStatus::GameRunning => Self::GameRunning,
            GameLaunchStatus::Unavailable(reason) => Self::Unavailable(GameLaunchUnavailableDto {
                reason: reason.into(),
            }),
        }
    }
}

/// What `launch_game` does when `ModsConfig.xml` doesn't hold the selected
/// order. Mirrors [`IfNotApplied`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum IfNotAppliedDto {
    /// See [`IfNotApplied::Refuse`].
    Refuse,
    /// See [`IfNotApplied::LaunchAnyway`].
    LaunchAnyway,
}

impl From<IfNotAppliedDto> for IfNotApplied {
    fn from(value: IfNotAppliedDto) -> Self {
        match value {
            IfNotAppliedDto::Refuse => Self::Refuse,
            IfNotAppliedDto::LaunchAnyway => Self::LaunchAnyway,
        }
    }
}

/// `launch_game`'s request: the one decision the user makes. Carries no
/// path and no URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct LaunchGameRequestDto {
    /// Whether to refuse or to launch anyway when the file is behind.
    pub if_not_applied: IfNotAppliedDto,
}

/// `launch_game`'s answer: the OS accepted the request (the game may still
/// be starting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct GameLaunchedDto {
    /// How the game was started.
    pub route: GameLaunchRouteDto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_rejects_a_field_it_does_not_define() {
        let parsed = serde_json::from_str::<LaunchGameRequestDto>(
            r#"{"ifNotApplied":"refuse","path":"elsewhere"}"#,
        );

        assert!(parsed.is_err());
    }

    #[test]
    fn the_request_reads_both_choices_in_camel_case() {
        let refuse: LaunchGameRequestDto =
            serde_json::from_str(r#"{"ifNotApplied":"refuse"}"#).expect("refuse parses");
        let anyway: LaunchGameRequestDto =
            serde_json::from_str(r#"{"ifNotApplied":"launchAnyway"}"#).expect("anyway parses");

        assert_eq!(refuse.if_not_applied, IfNotAppliedDto::Refuse);
        assert_eq!(anyway.if_not_applied, IfNotAppliedDto::LaunchAnyway);
    }

    #[test]
    fn each_status_serializes_with_its_kind_and_camel_case_payload() {
        let cases = [
            (
                GameLaunchStatus::Ready {
                    route: LaunchRouteKind::Steam,
                },
                r#"{"kind":"ready","route":"steam"}"#,
            ),
            (
                GameLaunchStatus::NeedsApply {
                    route: LaunchRouteKind::Executable,
                    reason: UnappliedReason::ActivationChangesNotScanned,
                },
                r#"{"kind":"needsApply","route":"executable","reason":"activationChangesNotScanned"}"#,
            ),
            (GameLaunchStatus::GameRunning, r#"{"kind":"gameRunning"}"#),
            (
                GameLaunchStatus::Unavailable(LaunchUnavailable::ExecutableMissing),
                r#"{"kind":"unavailable","reason":"executableMissing"}"#,
            ),
        ];

        for (status, expected) in cases {
            let json = serde_json::to_string(&GameLaunchStatusDto::from(status))
                .expect("a status serializes");

            assert_eq!(json, expected);
        }
    }
}
