//! [`ModInfo`]: one mod's full information for the mod info panel —
//! active, inactive, or missing, whichever [`crate::Session::mod_info`]
//! resolves `id` to. Pure data, built from facts [`crate::Session`]
//! already has cached (no I/O of its own — the lazily-read `About.xml`
//! description/version/icon are a separate, IO-backed use case,
//! [`crate::use_cases::ReadModAbout`], composed on top of this).

use std::collections::BTreeSet;
use std::path::PathBuf;

use rim_analyzer::domain::{
    DeclaredOrder, GeneratedMarker, InactiveMod, Mod, ModCost, ModId, Report, Source,
};
use rim_resolve::domain::Tag;
use rim_resolve::sort::Tier;

/// [`crate::Session::mod_info`]'s own failure: `id` (compared by
/// [`ModId::base`]) names nothing in the current report at all — not
/// active, not inactive, not even a missing-but-declared-active entry.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} is not a known mod")]
pub struct UnknownMod(pub ModId);

/// The report entry a mod id resolves to, before any panel data is built.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ResolvedMod<'report> {
    /// An entry of `report.mods`.
    Active(&'report Mod),
    /// An entry of `report.inactive_mods`.
    Inactive(&'report InactiveMod),
}

impl ResolvedMod<'_> {
    /// The mod's folder on disk.
    pub(crate) fn path(&self) -> &std::path::Path {
        match self {
            Self::Active(entry) => &entry.path,
            Self::Inactive(entry) => &entry.path,
        }
    }
}

/// Resolves `id` against the report's installed mods — the one lookup every
/// panel call shares, so the text half ([`crate::Session::mod_info`]) and the
/// image half (`ReadModPreview`) can never answer for different mods.
///
/// The exact id wins over a mere [`ModId::base`] match: an active mod and an
/// inactive `_steam` copy of it share a base, and asking about the inactive
/// copy must answer for *its* folder, not the active one's. Within each of the
/// two passes (exact, then base) an active mod is tried before an inactive
/// one. `None` for a mod that is not installed at all (see
/// `report.missing_mods`).
pub(crate) fn resolve_installed_mod<'report>(
    report: &'report Report,
    id: &ModId,
) -> Option<ResolvedMod<'report>> {
    let find = |matches: &dyn Fn(&ModId) -> bool| {
        report
            .mods
            .iter()
            .find(|entry| matches(&entry.id))
            .map(ResolvedMod::Active)
            .or_else(|| {
                report
                    .inactive_mods
                    .iter()
                    .find(|entry| matches(&entry.id))
                    .map(ResolvedMod::Inactive)
            })
    };
    let target = id.base();
    find(&|candidate| candidate == id).or_else(|| find(&|candidate| candidate.base() == target))
}

/// One mod's full information, resolved by [`crate::Session::mod_info`].
/// `Active`/`Inactive` are boxed: both carry enough fields that the
/// smallest variant ([`MissingModInfo`]) would otherwise pay for the
/// largest one's stack size on every `ModInfo` value, active or not.
///
/// No `PartialEq`/`Eq`: [`rim_analyzer::domain::DeclaredOrder`] (nested
/// in `ActiveModInfo`/`InactiveModInfo`) doesn't implement either, and
/// nothing in this workspace needs to compare a whole `ModInfo` for
/// equality — tests destructure the variant and assert on individual
/// fields instead.
#[derive(Debug, Clone)]
pub enum ModInfo {
    /// Found on disk and currently active.
    Active(Box<ActiveModInfo>),
    /// Found on disk but not currently active.
    Inactive(Box<InactiveModInfo>),
    /// Active per `ModsConfig.xml` but never found on disk.
    Missing(MissingModInfo),
}

/// Whether a mod has an uncommitted activate/deactivate edit pending —
/// [`crate::Session::pending_changes`]'s `unscanned` diff, narrowed to one
/// mod. `None` means the working set agrees with the last scan for this
/// mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingChange {
    /// Activated in the working set, not yet picked up by a rescan.
    ActivationPending,
    /// Deactivated in the working set, not yet picked up by a rescan.
    DeactivationPending,
}

/// An `About.xml` `url`, or the Steam Workshop page for a mod's own
/// `workshop_id`, classified for the "open in browser" affordance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HomepageLink {
    /// A parsed `http`/`https` URL with a host — safe to hand to the
    /// interface layer's own opener port (`open_mod_link`, the desktop's
    /// Tauri command) to open in the system browser.
    Openable(ExternalUrl),
    /// Present, but not `http`/`https` with a host (scheme-less,
    /// `steam:`, `javascript:`, ...) — shown, never clickable.
    Text(String),
}

/// A parsed `http`/`https` URL with a non-empty host. The private field
/// means a value can only be constructed by [`ExternalUrl::parse`] (or
/// [`workshop_url`]'s own fixed-format construction) — so "only an
/// http(s) URL with a host ever reaches the opener" holds by
/// construction, not by a check the opener itself would have to repeat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalUrl(String);

impl ExternalUrl {
    /// Parses `raw` as an `http://`/`https://` URL with a non-empty host.
    /// No path/query/fragment validation beyond that — this is a
    /// allow-only-this-scheme gate for "safe to hand to the OS opener",
    /// not a general URL parser. `raw` is mod-provided (`About.xml`'s own
    /// `url` field, or a Steam Workshop page derived from a mod's own
    /// `workshop_id`) — untrusted input, so whitespace (beyond the outer
    /// `trim`), `"`/`<`/`>`/backtick, any control character, and any
    /// non-ASCII byte are all rejected outright, rather than passed
    /// through to whatever eventually renders or shells out this string
    /// (the OS opener, `mods show`'s own terminal output).
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.chars().any(|c| {
            !c.is_ascii() || c.is_ascii_control() || matches!(c, '"' | '<' | '>' | '`' | ' ')
        }) {
            return None;
        }
        let rest = raw
            .strip_prefix("https://")
            .or_else(|| raw.strip_prefix("http://"))?;
        let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let host = &rest[..host_end];
        if host.is_empty() {
            return None;
        }
        Some(Self(raw.to_string()))
    }

    /// The URL text, exactly as parsed.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Wraps `raw` verbatim, skipping [`Self::parse`]'s own validation —
    /// for this crate's own fixed, compile-time-known-valid `http`/
    /// `https` URLs only (see [`crate::app_link::AppLinkTarget`]).
    /// `pub(crate)`: never exposed outside this crate, so an external
    /// caller can never bypass [`Self::parse`]'s validation with
    /// untrusted input.
    pub(crate) fn from_static(raw: &'static str) -> Self {
        Self(raw.to_string())
    }
}

/// The fixed Steam Workshop item page for `workshop_id` — a constant
/// format, never read from a file.
#[must_use]
pub fn workshop_url(workshop_id: u64) -> ExternalUrl {
    ExternalUrl(format!(
        "https://steamcommunity.com/sharedfiles/filedetails/?id={workshop_id}"
    ))
}

/// Classifies an `About.xml` `url` value: empty/absent is `None`; an
/// `http`/`https` URL with a host is [`HomepageLink::Openable`]; anything
/// else present (scheme-less, `steam:`, ...) is [`HomepageLink::Text`],
/// shown but never clickable.
#[must_use]
pub fn classify_homepage(raw: Option<&str>) -> Option<HomepageLink> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(match ExternalUrl::parse(raw) {
        Some(url) => HomepageLink::Openable(url),
        None => HomepageLink::Text(raw.to_string()),
    })
}

/// An active mod's full information.
#[derive(Debug, Clone)]
pub struct ActiveModInfo {
    /// The mod's id.
    pub mod_id: ModId,
    /// Its display name.
    pub name: String,
    /// Its listed authors.
    pub authors: Vec<String>,
    /// Its `About.xml` `url`, classified for the "open in browser"
    /// affordance.
    pub homepage: Option<HomepageLink>,
    /// Where its files come from.
    pub source: Source,
    /// Its `supportedVersions` list.
    pub supported_versions: Vec<String>,
    /// Whether `supported_versions` lists the report's own scanned game
    /// version.
    pub supports_game_version: bool,
    /// Its declared load-order hints.
    pub declared: DeclaredOrder,
    /// Where this mod's files live on disk.
    pub root: PathBuf,
    /// Folders this mod actually loads from, in load-priority order.
    pub loaded_folders: Vec<PathBuf>,
    /// This mod's own scan-cost row, when the report carries one.
    pub cost: Option<ModCost>,
    /// Every tag it currently carries.
    pub tags: BTreeSet<Tag>,
    /// Distinct active mods with a Hard-strength edge pointing at it.
    pub hard_dependents: usize,
    /// Distinct active mods with a Soft-strength edge pointing at it.
    pub soft_dependents: usize,
    /// Distinct active mods with an Awareness-strength edge pointing at
    /// it.
    pub awareness_dependents: usize,
    /// Whether it looks like a shared framework other mods build on.
    pub is_framework_candidate: bool,
    /// Its `rimmerge.json` marker, when it carries one.
    pub generated: Option<GeneratedMarker>,
    /// Its Steam Workshop published-file id.
    pub workshop_id: Option<u64>,
    /// The Steam Workshop item page for `workshop_id`, when it has one.
    pub workshop_url: Option<ExternalUrl>,
    /// Zero-based position in the *selected* order (`source` passed to
    /// [`crate::Session::mod_info`]).
    pub position: usize,
    /// Position in the current order, when `source` is the suggested
    /// order — `None` when `source` already *is* the current order (there
    /// is no separate "previous" to show).
    pub previous_position: Option<usize>,
    /// The tier the sorter assigned it — always from the suggested-order
    /// sort outcome, the same as `list_order`'s own `tier` column,
    /// regardless of which order is selected.
    pub tier: Tier,
    /// Live findings naming this mod, in the selected order.
    pub findings_total: usize,
    /// Of those, how many need user input.
    pub needs_input_count: usize,
    /// Whether an uncommitted activate/deactivate edit is pending on it.
    pub pending: Option<PendingChange>,
}

/// An inactive mod's information — everything discovery reads regardless
/// of activation, per [`rim_analyzer::domain::InactiveMod`]'s own doc
/// comment on why it carries no scan-derived facts.
#[derive(Debug, Clone)]
pub struct InactiveModInfo {
    /// The mod's id.
    pub mod_id: ModId,
    /// Its display name.
    pub name: String,
    /// Its listed authors.
    pub authors: Vec<String>,
    /// Where its files come from.
    pub source: Source,
    /// Its `supportedVersions` list.
    pub supported_versions: Vec<String>,
    /// Its declared load-order hints.
    pub declared: DeclaredOrder,
    /// Where this mod's files live on disk.
    pub root: PathBuf,
    /// Its Steam Workshop published-file id.
    pub workshop_id: Option<u64>,
    /// The Steam Workshop item page for `workshop_id`, when it has one.
    pub workshop_url: Option<ExternalUrl>,
    /// Its `rimmerge.json` marker, when it carries one.
    pub generated: Option<GeneratedMarker>,
    /// Whether an uncommitted activate/deactivate edit is pending on it.
    pub pending: Option<PendingChange>,
}

/// A missing mod's information: active per `ModsConfig.xml`, never found
/// on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingModInfo {
    /// The mod's id.
    pub mod_id: ModId,
    /// Every active mod that declares a dependency on this id (by
    /// [`ModId::base`]).
    pub required_by: BTreeSet<ModId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https_url_with_a_host() {
        let url = ExternalUrl::parse("https://example.com/mod").unwrap();
        assert_eq!(url.as_str(), "https://example.com/mod");
    }

    #[test]
    fn parses_http_url_with_a_host() {
        assert!(ExternalUrl::parse("http://example.com").is_some());
    }

    #[test]
    fn rejects_a_scheme_less_value() {
        assert_eq!(ExternalUrl::parse("example.com"), None);
    }

    #[test]
    fn rejects_an_http_url_with_an_empty_host() {
        assert_eq!(ExternalUrl::parse("https:///path"), None);
    }

    #[test]
    fn rejects_a_non_http_scheme() {
        assert_eq!(ExternalUrl::parse("steam://run/294100"), None);
        assert_eq!(ExternalUrl::parse("javascript:alert(1)"), None);
    }

    #[test]
    fn rejects_embedded_whitespace_quotes_angle_brackets_and_backticks() {
        // A mod's own `About.xml` `url` field is untrusted input — none
        // of these must ever reach the OS opener or a terminal.
        assert_eq!(
            ExternalUrl::parse("https://example.com/a b"),
            None,
            "embedded whitespace"
        );
        assert_eq!(
            ExternalUrl::parse("https://example.com/\"onclick=x"),
            None,
            "double quote"
        );
        assert_eq!(
            ExternalUrl::parse("https://example.com/<script>"),
            None,
            "angle brackets"
        );
        assert_eq!(
            ExternalUrl::parse("https://example.com/`whoami`"),
            None,
            "backtick"
        );
    }

    #[test]
    fn rejects_control_characters() {
        assert_eq!(ExternalUrl::parse("https://example.com/\u{0007}bell"), None);
        assert_eq!(
            ExternalUrl::parse("https://example.com/\u{009B}csi"),
            None,
            "a C1 control character, not just C0"
        );
    }

    #[test]
    fn rejects_non_ascii_bytes() {
        assert_eq!(ExternalUrl::parse("https://example.com/café"), None);
    }

    #[test]
    fn classify_homepage_of_none_or_empty_is_none() {
        assert_eq!(classify_homepage(None), None);
        assert_eq!(classify_homepage(Some("")), None);
        assert_eq!(classify_homepage(Some("   ")), None);
    }

    #[test]
    fn classify_homepage_of_an_http_url_is_openable() {
        assert!(matches!(
            classify_homepage(Some("https://example.com")),
            Some(HomepageLink::Openable(_))
        ));
    }

    #[test]
    fn classify_homepage_of_a_steam_scheme_is_text() {
        assert_eq!(
            classify_homepage(Some("steam://run/294100")),
            Some(HomepageLink::Text("steam://run/294100".to_string()))
        );
    }

    #[test]
    fn classify_homepage_of_a_scheme_less_value_is_text() {
        assert_eq!(
            classify_homepage(Some("example.com")),
            Some(HomepageLink::Text("example.com".to_string()))
        );
    }

    #[test]
    fn workshop_url_is_the_fixed_format() {
        assert_eq!(
            workshop_url(123).as_str(),
            "https://steamcommunity.com/sharedfiles/filedetails/?id=123"
        );
    }
}
