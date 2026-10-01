//! `rimmerge check-update`: a manual, explicit check for a newer
//! Rimmerge release — never automatic, and it never runs unless invoked.
//! Honours `allow_network` and an active rate limit
//! ([`UpdateCheckRequest::Manual`] skips only the first-run/cadence
//! gates `RunLaunchNetworkChecks`' own automatic path uses, since a
//! click is its own consent). Records its result in
//! `<base>/notifications.json`'s `update_check` state, the same file
//! the desktop's own "Check now"/bell read — so a check run from here
//! is reflected there too.

use clap::Args;
use rim_session::notifications::LatestRelease;
use rim_session::use_cases::{
    CheckForUpdate, CheckForUpdateOutcome, UpdateCheckRequest, UpdateCheckRunOutcome,
    UpdateCheckSkipReason,
};

use crate::common::{TerminalSafe, default_profile_base, read_network_policy};

/// The `<owner>/<repo>/releases/tag/` prefix a stable, parsed version is
/// appended to — never anything from the response body itself, per
/// `docs/privacy-and-network.md`. Kept as a local `const` rather than
/// shared with `apps/desktop`'s own copy
/// (`useNotificationActions.ts`'s `RELEASE_URL_PREFIX`): the two are
/// separate languages/crates with no shared string-constants module, and
/// this one line is cheaper to keep in sync by inspection than to add a
/// cross-language dependency for.
const RELEASE_URL_PREFIX: &str = "https://github.com/Rimmerge-Project/rimmerge/releases/tag/v";

/// `rimmerge check-update` takes no flags — it's a manual, explicit
/// action with nothing to configure.
#[derive(Debug, Args)]
pub struct CheckUpdateArgs {}

pub fn run(_args: &CheckUpdateArgs) -> anyhow::Result<()> {
    let base = default_profile_base()?;
    let policy = read_network_policy();
    let use_case = CheckForUpdate::new(
        rim_io::GithubReleaseFeed::new(),
        rim_io::JsonNotificationStateStore::new(),
    );
    let outcome = use_case.execute(
        UpdateCheckRequest::Manual,
        &policy,
        &base,
        jiff::Timestamp::now(),
    );
    println!("{}", TerminalSafe::line(format_outcome(&outcome)));
    Ok(())
}

/// The release page URL for one published version — built entirely on
/// this side from a `const` prefix and the already-parsed version.
fn release_url(version: &rim_session::notifications::AppVersion) -> String {
    format!("{RELEASE_URL_PREFIX}{version}")
}

/// One line describing `outcome` — a pure function, unit-tested without
/// a socket or a clock in sight.
fn format_outcome(outcome: &CheckForUpdateOutcome) -> String {
    match outcome {
        CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Updated {
            latest: LatestRelease { version, .. },
        }) => format!(
            "A newer version is available: {version} — {}",
            release_url(version)
        ),
        CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Unchanged) => "Up to date.".to_string(),
        CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Failed { failure }) => {
            format!("Check failed: {}", failure.detail)
        }
        CheckForUpdateOutcome::Skipped(reason) => {
            format!("Skipped: {}", format_skip_reason(reason))
        }
    }
}

/// The reason text for every [`UpdateCheckSkipReason`] variant. Only
/// [`UpdateCheckSkipReason::NetworkDisabled`] and
/// [`UpdateCheckSkipReason::RateLimitedUntil`] are reachable from this
/// command's own [`UpdateCheckRequest::Manual`] call (the other three
/// gate only the automatic path — see
/// [`rim_session::use_cases::CheckForUpdate::execute`]'s own doc
/// comment) — every variant still gets real text rather than a
/// catch-all, since this function has no way to assume which caller it
/// runs under, and an exhaustive `match` costs nothing here.
fn format_skip_reason(reason: &UpdateCheckSkipReason) -> String {
    match reason {
        UpdateCheckSkipReason::AwaitingFirstRun => {
            "the first-run notice hasn't been answered yet".to_string()
        }
        UpdateCheckSkipReason::AlreadyRanThisLaunch => "already checked this launch".to_string(),
        UpdateCheckSkipReason::NetworkDisabled => "internet access is off".to_string(),
        UpdateCheckSkipReason::CheckDisabled => "automatic checks are off".to_string(),
        UpdateCheckSkipReason::NotDue => "checked recently — try again later".to_string(),
        UpdateCheckSkipReason::RateLimitedUntil(until) => {
            format!("GitHub's rate limit was reached; try again after {until}")
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_session::notifications::AppVersion;
    use rim_session::ports::FetchFailure;

    use super::*;

    fn version(raw: &str) -> AppVersion {
        AppVersion::running(raw).expect("valid semver")
    }

    #[test]
    fn updated_names_the_version_and_the_release_url() {
        let outcome = CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Updated {
            latest: LatestRelease {
                version: version("0.2.0"),
                published_at: jiff::Timestamp::UNIX_EPOCH,
            },
        });
        let line = format_outcome(&outcome);
        assert!(line.contains("0.2.0"), "{line}");
        assert!(
            line.contains("https://github.com/Rimmerge-Project/rimmerge/releases/tag/v0.2.0"),
            "{line}"
        );
    }

    #[test]
    fn unchanged_says_up_to_date() {
        let outcome = CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Unchanged);
        assert_eq!(format_outcome(&outcome), "Up to date.");
    }

    #[test]
    fn failed_names_the_reason() {
        let outcome = CheckForUpdateOutcome::Ran(UpdateCheckRunOutcome::Failed {
            failure: FetchFailure::unclassified("connection timed out"),
        });
        assert_eq!(
            format_outcome(&outcome),
            "Check failed: connection timed out"
        );
    }

    #[test]
    fn network_disabled_is_reported_as_a_skip() {
        let outcome = CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::NetworkDisabled);
        assert_eq!(format_outcome(&outcome), "Skipped: internet access is off");
    }

    #[test]
    fn rate_limited_names_the_retry_time() {
        let until = jiff::Timestamp::UNIX_EPOCH;
        let outcome =
            CheckForUpdateOutcome::Skipped(UpdateCheckSkipReason::RateLimitedUntil(until));
        let line = format_outcome(&outcome);
        assert!(line.contains("GitHub's rate limit"), "{line}");
        assert!(line.contains(&until.to_string()), "{line}");
    }
}
