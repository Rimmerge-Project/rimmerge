//! Mapping the "Get the recommended rules" phase-1 refusals.

use rim_session::Unavailable;
use rim_session::use_cases::GetRecommendedRulesError;

use super::{CommandError, CommandErrorCode, CommandErrorDetail};
use crate::dto::recommended_rules::UnavailableReasonDto;

impl From<GetRecommendedRulesError> for CommandError {
    /// `SettingsDamaged` gets its own `AppSettingsDamaged` (the user repairs
    /// the file in Settings); a failed save keeps `ProfileIoFailed`, the
    /// code `enable_recommended_rule_databases` returns for it.
    fn from(error: GetRecommendedRulesError) -> Self {
        let message = error.to_string();
        match error {
            GetRecommendedRulesError::Unavailable(reason) => Self::with_detail(
                CommandErrorCode::RecommendedRulesUnavailable,
                match reason {
                    Unavailable::NetworkOff => "internet access is turned off",
                    Unavailable::AwaitingFirstRun => "the first-run notice has not been answered",
                },
                CommandErrorDetail::RecommendedRulesUnavailable {
                    reason: UnavailableReasonDto::from(reason),
                },
            ),
            GetRecommendedRulesError::SettingsDamaged => {
                Self::new(CommandErrorCode::AppSettingsDamaged, message)
            }
            GetRecommendedRulesError::Settings(store_error) => store_error.into(),
        }
    }
}
