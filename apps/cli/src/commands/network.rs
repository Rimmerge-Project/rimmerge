//! `rimmerge network`: reads and flips the app-global network policy
//! (`<base>/app-settings.json`) — a privacy switch shouldn't require
//! hand-editing JSON. `status` reads no network; `on`/`off` write the
//! master switch (`--updates`/`--auto-refresh` and the per-source flags
//! optionally set the other toggles alongside it); `set` writes only the
//! toggles it is given and never the master switch. No subcommand
//! resolves a project or touches a session — the policy is app-global,
//! same as `db status`/`db refresh`'s own [`crate::common::read_network_policy`].

use clap::{Args, Subcommand, ValueEnum};
use rim_session::app_settings::AppSettings;
use rim_session::use_cases::{ResetNetworkPolicy, UpdateAppSettings};

use crate::common::{default_profile_base, print_recovered_notice_if_any, read_app_settings_load};

#[derive(Debug, Subcommand)]
pub enum NetworkCommand {
    /// Prints the master switch and each feature toggle.
    Status,
    /// Allows internet access: the two GitHub hosts Rimmerge can contact
    /// (update checks and rule-database refreshes), never your local
    /// network. Flips the master switch only, unless
    /// `--updates`/`--auto-refresh`/the per-source flags are also given.
    On(ToggleArgs),
    /// Sets only the switches given (`--updates`, `--auto-refresh` and the
    /// per-source flags), leaving the master switch exactly as it is: the
    /// way to turn one source off without also allowing internet access.
    /// At least one switch is required.
    Set(ToggleArgs),
    /// Turns internet access off (the master switch only, unless
    /// `--updates`/`--auto-refresh`/the per-source flags are also
    /// given). Every automatic request and every manual `db refresh`/
    /// `check-update` are skipped while this is off.
    Off(ToggleArgs),
    /// Restores every internet-access switch (master, updates,
    /// auto-refresh, and all three per-source fetch toggles) to its
    /// default — every switch on, including the Steam Workshop database
    /// (about 49 MB, downloaded only when you run `db refresh`) — leaving
    /// the stale-reminder threshold untouched. The one way to recover
    /// from a corrupt `app-settings.json`, which fails closed (every
    /// switch off) rather than crashing, without hand-editing the file.
    Reset,
}

/// Mirrors a plain `bool` for `clap`'s `ValueEnum` derive, so
/// `--updates on|off` reads the same vocabulary as the subcommand names
/// themselves rather than `--updates true|false`.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OnOffArg {
    /// `true`.
    On,
    /// `false`.
    Off,
}

impl From<OnOffArg> for bool {
    fn from(value: OnOffArg) -> Self {
        matches!(value, OnOffArg::On)
    }
}

#[derive(Debug, Args)]
pub struct ToggleArgs {
    /// Also sets the automatic update-check toggle.
    #[arg(long, value_enum)]
    updates: Option<OnOffArg>,
    /// Also sets the automatic rule-database refresh toggle.
    #[arg(long = "auto-refresh", value_enum)]
    auto_refresh: Option<OnOffArg>,
    /// Also sets whether a refresh may fetch `communityRules.json` at
    /// all.
    #[arg(long = "community-rules", value_enum)]
    community_rules: Option<OnOffArg>,
    /// Also sets whether a refresh may fetch the Steam Workshop
    /// database (about 49 MB; on by default). This source is manual-only regardless — see
    /// `RuleDatabase::is_auto_refresh_eligible` — this flag only ever
    /// controls whether a refresh may fetch it at all, not whether
    /// automatic refresh covers it.
    #[arg(long = "steam-workshop", value_enum)]
    steam_workshop: Option<OnOffArg>,
    /// Also sets whether a refresh may fetch `rimmergeRules.json` at
    /// all.
    #[arg(long = "rimmerge-rules", value_enum)]
    rimmerge_rules: Option<OnOffArg>,
}

pub fn run(command: &NetworkCommand) -> anyhow::Result<()> {
    match command {
        NetworkCommand::Status => run_status(),
        NetworkCommand::On(args) => run_set(MasterSwitch::Set(true), args),
        NetworkCommand::Off(args) => run_set(MasterSwitch::Set(false), args),
        NetworkCommand::Set(args) => run_set(MasterSwitch::Unchanged, args),
        NetworkCommand::Reset => run_reset(),
    }
}

fn run_status() -> anyhow::Result<()> {
    let load = read_app_settings_load();
    let settings = load.settings();
    for line in format_status_lines(&settings) {
        println!("{line}");
    }
    print_recovered_notice_if_any(&load);
    Ok(())
}

/// What a `network` write does to the master `allow_network` switch.
#[derive(Debug, Clone, Copy)]
enum MasterSwitch {
    /// `on`/`off`: sets it.
    Set(bool),
    /// `set`: leaves it exactly as it is.
    Unchanged,
}

impl ToggleArgs {
    /// Whether no switch flag was given at all.
    fn is_empty(&self) -> bool {
        self.updates.is_none()
            && self.auto_refresh.is_none()
            && self.community_rules.is_none()
            && self.steam_workshop.is_none()
            && self.rimmerge_rules.is_none()
    }
}

fn run_set(master: MasterSwitch, args: &ToggleArgs) -> anyhow::Result<()> {
    if matches!(master, MasterSwitch::Unchanged) && args.is_empty() {
        anyhow::bail!(
            "network set: name at least one switch (--updates, --auto-refresh, --community-rules,              --steam-workshop or --rimmerge-rules)"
        );
    }
    let base = default_profile_base()?;
    let mut settings = read_app_settings_load().settings();
    if let MasterSwitch::Set(allow_network) = master {
        settings.network.allow_network = allow_network;
    }
    if let Some(updates) = args.updates {
        settings.network.check_for_updates = updates.into();
    }
    if let Some(auto_refresh) = args.auto_refresh {
        settings.network.auto_refresh_rule_databases = auto_refresh.into();
    }
    if let Some(community_rules) = args.community_rules {
        settings.network.fetch_community_rules = community_rules.into();
    }
    if let Some(steam_workshop) = args.steam_workshop {
        settings.network.fetch_steam_workshop = steam_workshop.into();
    }
    if let Some(rimmerge_rules) = args.rimmerge_rules {
        settings.network.fetch_rimmerge_rules = rimmerge_rules.into();
    }

    let use_case = UpdateAppSettings::new(rim_io::JsonAppSettingsStore::new());
    use_case.execute(&base, settings)?;

    for line in format_status_lines(&settings) {
        println!("{line}");
    }
    Ok(())
}

/// Restores every network switch to its default — the recovery path for
/// a corrupt `app-settings.json`, which fails closed (every switch off)
/// rather than crashing (see `rim_io::JsonAppSettingsStore`'s own load
/// contract) — without hand-editing JSON.
fn run_reset() -> anyhow::Result<()> {
    let base = default_profile_base()?;
    let use_case = ResetNetworkPolicy::new(rim_io::JsonAppSettingsStore::new());
    let reset = use_case.execute(&base)?;

    for line in format_status_lines(&reset) {
        println!("{line}");
    }
    Ok(())
}

/// `network status`'s own lines, shared with `network on`/`off` so a
/// write is confirmed with the identical wording a subsequent `status`
/// would print — never a bespoke "done" line that could drift from it.
fn format_status_lines(settings: &AppSettings) -> Vec<String> {
    let on_off = |value: bool| if value { "on" } else { "off" };
    let network = &settings.network;
    vec![
        format!("allow_network: {}", on_off(network.allow_network)),
        format!("check_for_updates: {}", on_off(network.check_for_updates)),
        format!(
            "auto_refresh_rule_databases: {}",
            on_off(network.auto_refresh_rule_databases)
        ),
        format!(
            "fetch_community_rules: {}",
            on_off(network.fetch_community_rules)
        ),
        format!(
            "fetch_steam_workshop: {}",
            on_off(network.fetch_steam_workshop)
        ),
        format!(
            "fetch_rimmerge_rules: {}",
            on_off(network.fetch_rimmerge_rules)
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_lines_name_every_field_with_on_or_off() {
        let lines = format_status_lines(&AppSettings::default());
        assert_eq!(
            lines,
            vec![
                "allow_network: on",
                "check_for_updates: on",
                "auto_refresh_rule_databases: on",
                "fetch_community_rules: on",
                "fetch_steam_workshop: on",
                "fetch_rimmerge_rules: on",
            ]
        );
    }

    #[test]
    fn on_off_arg_maps_to_the_matching_bool() {
        assert!(bool::from(OnOffArg::On));
        assert!(!bool::from(OnOffArg::Off));
    }
}
