//! [`ReadModAbout`]: resolves a mod's [`ModInfo`] plus its lazily-read
//! `About.xml` details (description, mod version, icon path) in one
//! call — the mod info panel's text half. The image (`ReadModPreview`)
//! stays a separate call: it's the large payload, this is a few KB.

use std::str::FromStr;

use rim_analyzer::domain::{GameVersion, ModId};
use rim_analyzer::extract::rich_text;

use crate::Session;
use crate::mod_info::{
    ExternalUrl, HomepageLink, ModInfo, UnknownMod, classify_homepage, workshop_url,
};
use crate::ports::{AboutReadError, ModAboutReader};

/// A description's sanitized runs, plus whether the source text was
/// truncated before parsing (see [`rim_analyzer::extract::rich_text`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptionText {
    /// The sanitized runs — never HTML, safe to render directly.
    pub runs: Vec<rich_text::RichRun>,
    /// Whether the source text was truncated before parsing.
    pub truncated: bool,
}

/// The `About.xml` read's own outcome. Never folded into an error: a
/// missing, changed, or unreadable `About.xml` still shows the rest of
/// the panel, just without a description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AboutOutcome {
    /// Read and parsed successfully, and the re-read `packageId` still
    /// matches the mod this info is about.
    Read {
        /// `<modVersion>`, verbatim.
        mod_version: Option<String>,
        /// `<modIconPath>`, verbatim.
        mod_icon_path: Option<String>,
        /// The sanitized description, when `<description>` is present.
        description: Option<DescriptionText>,
        /// `<url>`, classified for the "open in browser" affordance. An
        /// active mod's own [`crate::mod_info::ActiveModInfo::homepage`]
        /// (scan-time, immediately available) is the preferred source for
        /// it; this re-read copy exists for an inactive mod, which has no
        /// `url` field of its own at scan time.
        homepage: Option<HomepageLink>,
    },
    /// The re-read `packageId`'s base no longer matches this mod's own
    /// id — the folder changed since the scan. The text of a mod that
    /// merely got a new version under the same id is simply newer than
    /// the scan, which reads as [`Self::Read`]; only an id mismatch is
    /// flagged.
    Changed,
    /// The file couldn't be read or parsed.
    Unreadable(AboutUnreadable),
    /// No `About.xml` to read at all (a missing mod).
    NotOnDisk,
}

/// Why an `About.xml` that exists couldn't be shown: a closed cause the
/// interface translates, plus the adapter's English text as a technical
/// detail.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AboutUnreadable {
    /// The file exists but reading it failed.
    #[error("cannot read About.xml: {detail}")]
    Io {
        /// The underlying error's text.
        detail: String,
    },
    /// The file was read but its XML is invalid.
    #[error("invalid About.xml: {detail}")]
    Xml {
        /// The parser's error text.
        detail: String,
    },
}

/// [`ReadModAbout::execute`]'s result: the resolved [`ModInfo`] plus the
/// `About.xml` read outcome for the same mod. No `PartialEq`: `ModInfo`
/// doesn't implement it (see its own doc comment).
#[derive(Debug, Clone)]
pub struct ModInfoWithAbout {
    /// The resolved mod info.
    pub info: ModInfo,
    /// The `About.xml` read outcome for the same mod.
    pub about: AboutOutcome,
}

/// Which of a mod's own external links [`ModInfoWithAbout::link_url`]
/// should resolve — the `open_mod_link` command's own request shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModLinkKind {
    /// The Steam Workshop item page.
    Workshop,
    /// The `About.xml`-declared homepage.
    Homepage,
}

impl ModInfoWithAbout {
    /// Resolves `kind`'s URL for this mod — `None` when it has no such
    /// link at all (no workshop id, an absent homepage, or one present
    /// but not `http`/`https` with a host).
    ///
    /// An active mod's homepage comes from its own scan-time
    /// [`crate::mod_info::ActiveModInfo::homepage`] field (immediately
    /// available, no re-read needed); an inactive mod's
    /// comes from this same call's own lazily re-read `about.homepage`
    /// (an [`rim_analyzer::domain::InactiveMod`] carries no `url` of its
    /// own at scan time — see that type's own doc comment).
    #[must_use]
    pub fn link_url(&self, kind: ModLinkKind) -> Option<ExternalUrl> {
        match kind {
            ModLinkKind::Workshop => self.workshop_id().map(workshop_url),
            ModLinkKind::Homepage => self.homepage_url(),
        }
    }

    fn workshop_id(&self) -> Option<u64> {
        match &self.info {
            ModInfo::Active(active) => active.workshop_id,
            ModInfo::Inactive(inactive) => inactive.workshop_id,
            ModInfo::Missing(_) => None,
        }
    }

    fn homepage_url(&self) -> Option<ExternalUrl> {
        let homepage = match &self.info {
            ModInfo::Active(active) => active.homepage.as_ref(),
            ModInfo::Inactive(_) => match &self.about {
                AboutOutcome::Read { homepage, .. } => homepage.as_ref(),
                AboutOutcome::Changed | AboutOutcome::Unreadable(_) | AboutOutcome::NotOnDisk => {
                    None
                }
            },
            ModInfo::Missing(_) => None,
        }?;
        match homepage {
            HomepageLink::Openable(url) => Some(url.clone()),
            HomepageLink::Text(_) => None,
        }
    }
}

/// Composes [`Session::mod_info`] with a lazy, per-selection
/// `About.xml` read through a [`ModAboutReader`] port.
pub struct ReadModAbout<Reader> {
    reader: Reader,
}

impl<Reader: ModAboutReader> ReadModAbout<Reader> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(reader: Reader) -> Self {
        Self { reader }
    }

    /// Resolves `id`'s [`ModInfo`], then reads its `About.xml` details
    /// (skipped outright for a missing mod, which has no `About.xml` to
    /// read).
    ///
    /// # Errors
    ///
    /// Returns [`UnknownMod`] when `id` names nothing in the current
    /// report.
    pub fn execute(
        &self,
        session: &mut Session,
        id: &ModId,
        source: rim_resolve::domain::OrderSource,
    ) -> Result<ModInfoWithAbout, UnknownMod> {
        let info = session.mod_info(id, source)?;
        // Falls back to an unmatchable version on a parse failure (should
        // never happen — the metadata string is written from a real
        // `GameVersion` at scan time) rather than erroring the whole
        // read: the effect is only that a `descriptionsByVersion`
        // override never applies, and the base `<description>` is used.
        let game_version = GameVersion::from_str(&session.report().metadata.game_version)
            .unwrap_or(GameVersion::new(0, 0));
        let game_dir = session.paths().game_dir.clone();
        let about = self.read_about(&info, game_version, &game_dir);
        Ok(ModInfoWithAbout { info, about })
    }

    fn read_about(
        &self,
        info: &ModInfo,
        game_version: GameVersion,
        game_dir: &std::path::Path,
    ) -> AboutOutcome {
        let (root, source, expected_base) = match info {
            ModInfo::Active(active) => (&active.root, active.source, active.mod_id.base()),
            ModInfo::Inactive(inactive) => {
                (&inactive.root, inactive.source, inactive.mod_id.base())
            }
            ModInfo::Missing(_) => return AboutOutcome::NotOnDisk,
        };
        match self
            .reader
            .read_details(root, source, game_version, game_dir)
        {
            Ok(details) if details.package_id.base() == expected_base => AboutOutcome::Read {
                mod_version: details.mod_version,
                mod_icon_path: details.mod_icon_path,
                homepage: classify_homepage(details.url.as_deref()),
                description: details.description.map(|text| {
                    let parsed = rich_text::parse(&text);
                    DescriptionText {
                        runs: parsed.runs,
                        truncated: parsed.truncated,
                    }
                }),
            },
            Ok(_) => AboutOutcome::Changed,
            Err(AboutReadError::NotFound) => AboutOutcome::NotOnDisk,
            Err(AboutReadError::Io(detail)) => {
                AboutOutcome::Unreadable(AboutUnreadable::Io { detail })
            }
            Err(AboutReadError::Xml(detail)) => {
                AboutOutcome::Unreadable(AboutUnreadable::Xml { detail })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use rim_analyzer::domain::Source;
    use rim_analyzer::extract::about_xml::AboutDetails;
    use rim_resolve::domain::OrderSource;

    use super::*;
    use crate::test_support::session_fixture;

    /// A [`ModAboutReader`] that returns whatever was seeded for the
    /// exact `(mod_root, source)` pair it's asked about.
    #[derive(Default)]
    struct FakeAboutReader {
        by_root: std::collections::BTreeMap<PathBuf, Result<AboutDetails, AboutReadError>>,
    }

    impl FakeAboutReader {
        fn with(mut self, root: PathBuf, result: Result<AboutDetails, AboutReadError>) -> Self {
            self.by_root.insert(root, result);
            self
        }
    }

    impl ModAboutReader for FakeAboutReader {
        fn read_details(
            &self,
            mod_root: &Path,
            _source: Source,
            _game_version: GameVersion,
            _game_dir: &Path,
        ) -> Result<AboutDetails, AboutReadError> {
            self.by_root
                .get(mod_root)
                .cloned()
                .unwrap_or(Err(AboutReadError::NotFound))
        }
    }

    fn details(package_id: &str, description: Option<&str>) -> AboutDetails {
        AboutDetails {
            package_id: ModId::new(package_id),
            description: description.map(str::to_string),
            mod_version: Some("1.0".to_string()),
            mod_icon_path: None,
            url: None,
        }
    }

    #[test]
    fn reads_details_for_an_active_mod() {
        let mut session = session_fixture(&["a"]);
        let root = session
            .report()
            .mods
            .iter()
            .find(|m| m.id == ModId::new("a"))
            .expect("mod a exists")
            .path
            .clone();
        let reader =
            FakeAboutReader::default().with(root, Ok(details("a", Some("hello <b>world</b>"))));
        let use_case = ReadModAbout::new(reader);

        let result = use_case
            .execute(&mut session, &ModId::new("a"), OrderSource::Current)
            .expect("must resolve");

        let AboutOutcome::Read { description, .. } = result.about else {
            panic!("expected Read");
        };
        let description = description.expect("description was seeded");
        assert!(!description.runs.is_empty());
    }

    #[test]
    fn a_package_id_mismatch_is_changed() {
        let mut session = session_fixture(&["a"]);
        let root = session
            .report()
            .mods
            .iter()
            .find(|m| m.id == ModId::new("a"))
            .expect("mod a exists")
            .path
            .clone();
        let reader = FakeAboutReader::default().with(root, Ok(details("different.id", None)));
        let use_case = ReadModAbout::new(reader);

        let result = use_case
            .execute(&mut session, &ModId::new("a"), OrderSource::Current)
            .expect("must resolve");

        assert_eq!(result.about, AboutOutcome::Changed);
    }

    #[test]
    fn a_reader_io_error_is_unreadable_with_its_cause_and_detail() {
        let mut session = session_fixture(&["a"]);
        let root = session
            .report()
            .mods
            .iter()
            .find(|m| m.id == ModId::new("a"))
            .expect("mod a exists")
            .path
            .clone();
        let reader =
            FakeAboutReader::default().with(root, Err(AboutReadError::Io("boom".to_string())));
        let use_case = ReadModAbout::new(reader);

        let result = use_case
            .execute(&mut session, &ModId::new("a"), OrderSource::Current)
            .expect("must resolve");

        assert_eq!(
            result.about,
            AboutOutcome::Unreadable(AboutUnreadable::Io {
                detail: "boom".to_string()
            })
        );
    }

    #[test]
    fn a_reader_xml_error_is_unreadable_as_xml() {
        let mut session = session_fixture(&["a"]);
        let root = session
            .report()
            .mods
            .iter()
            .find(|m| m.id == ModId::new("a"))
            .expect("mod a exists")
            .path
            .clone();
        let reader = FakeAboutReader::default()
            .with(root, Err(AboutReadError::Xml("unclosed tag".to_string())));
        let use_case = ReadModAbout::new(reader);

        let result = use_case
            .execute(&mut session, &ModId::new("a"), OrderSource::Current)
            .expect("must resolve");

        assert_eq!(
            result.about,
            AboutOutcome::Unreadable(AboutUnreadable::Xml {
                detail: "unclosed tag".to_string()
            })
        );
    }

    #[test]
    fn an_inactive_steam_copy_sharing_a_base_with_an_active_mod_reads_its_own_about() {
        let mut report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("shared.mod")
            .inactive("shared.mod_steam")
            .build();
        let active_root = report.mods[0].path.clone();
        let inactive_root = PathBuf::from("inactive/shared.mod_steam");
        report.inactive_mods[0].path = inactive_root.clone();
        let mut session = crate::Session::new(
            crate::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("shared.mod")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let reader = FakeAboutReader::default()
            .with(active_root, Ok(details("shared.mod", Some("active text"))))
            .with(
                inactive_root,
                Ok(details("shared.mod_steam", Some("inactive text"))),
            );
        let use_case = ReadModAbout::new(reader);

        let result = use_case
            .execute(
                &mut session,
                &ModId::new("shared.mod_steam"),
                OrderSource::Current,
            )
            .expect("must resolve");

        assert!(
            matches!(result.info, ModInfo::Inactive(_)),
            "the exact id must win over the active mod sharing its base"
        );
        let AboutOutcome::Read { description, .. } = result.about else {
            panic!("expected Read");
        };
        assert_eq!(
            description.expect("description was seeded").runs,
            rich_text::parse("inactive text").runs
        );
    }

    #[test]
    fn a_missing_mod_reads_nothing() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("a")
            .missing_mod("ghost.mod")
            .build();
        let mut session = crate::Session::new(
            crate::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a"), ModId::new("ghost.mod")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let reader = FakeAboutReader::default();
        let use_case = ReadModAbout::new(reader);

        let result = use_case
            .execute(&mut session, &ModId::new("ghost.mod"), OrderSource::Current)
            .expect("must resolve");

        assert_eq!(result.about, AboutOutcome::NotOnDisk);
    }

    #[test]
    fn an_unknown_mod_is_an_error() {
        let mut session = session_fixture(&["a"]);
        let use_case = ReadModAbout::new(FakeAboutReader::default());

        let result = use_case.execute(&mut session, &ModId::new("nobody"), OrderSource::Current);

        assert!(result.is_err());
    }

    fn active_details(package_id: &str, url: Option<&str>) -> AboutDetails {
        AboutDetails {
            package_id: ModId::new(package_id),
            description: None,
            mod_version: None,
            mod_icon_path: None,
            url: url.map(str::to_string),
        }
    }

    #[test]
    fn link_url_workshop_is_built_from_the_active_mods_own_workshop_id() {
        let mut builder = rim_resolve::test_support::ReportBuilder::new();
        builder = builder.mod_with("a", |m| m.workshop_id = Some(123));
        let mut session = crate::Session::new(
            crate::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            builder.build(),
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let use_case = ReadModAbout::new(FakeAboutReader::default());

        let with_about = use_case
            .execute(&mut session, &ModId::new("a"), OrderSource::Current)
            .expect("must resolve");

        assert_eq!(
            with_about
                .link_url(crate::use_cases::ModLinkKind::Workshop)
                .as_ref()
                .map(ExternalUrl::as_str),
            Some("https://steamcommunity.com/sharedfiles/filedetails/?id=123")
        );
    }

    #[test]
    fn link_url_homepage_for_an_active_mod_comes_from_its_scan_time_url() {
        let mut builder = rim_resolve::test_support::ReportBuilder::new();
        builder = builder.mod_with("a", |m| m.url = Some("https://example.com".to_string()));
        let mut session = crate::Session::new(
            crate::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            builder.build(),
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let use_case = ReadModAbout::new(FakeAboutReader::default());

        let with_about = use_case
            .execute(&mut session, &ModId::new("a"), OrderSource::Current)
            .expect("must resolve");

        assert_eq!(
            with_about
                .link_url(ModLinkKind::Homepage)
                .as_ref()
                .map(ExternalUrl::as_str),
            Some("https://example.com")
        );
    }

    #[test]
    fn link_url_homepage_for_an_inactive_mod_comes_from_the_lazy_about_read() {
        let report = crate::test_support::report_fixture_with_inactive(&["a"], &["inactive.mod"]);
        let mut session = crate::Session::new(
            crate::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let root = {
            let ModInfo::Inactive(inactive) = session
                .mod_info(&ModId::new("inactive.mod"), OrderSource::Current)
                .expect("resolves")
            else {
                panic!("expected Inactive");
            };
            inactive.root
        };
        let reader = FakeAboutReader::default().with(
            root,
            Ok(active_details(
                "inactive.mod",
                Some("https://example.com/inactive"),
            )),
        );
        let use_case = ReadModAbout::new(reader);

        let with_about = use_case
            .execute(
                &mut session,
                &ModId::new("inactive.mod"),
                OrderSource::Current,
            )
            .expect("must resolve");

        assert_eq!(
            with_about
                .link_url(ModLinkKind::Homepage)
                .as_ref()
                .map(ExternalUrl::as_str),
            Some("https://example.com/inactive")
        );
    }

    #[test]
    fn link_url_is_none_when_the_mod_has_no_such_link() {
        let mut session = session_fixture(&["a"]);
        let use_case = ReadModAbout::new(FakeAboutReader::default());

        let with_about = use_case
            .execute(&mut session, &ModId::new("a"), OrderSource::Current)
            .expect("must resolve");

        assert_eq!(with_about.link_url(ModLinkKind::Workshop), None);
        assert_eq!(with_about.link_url(ModLinkKind::Homepage), None);
    }

    #[test]
    fn link_url_is_none_for_a_missing_mod() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("a")
            .missing_mod("ghost.mod")
            .build();
        let mut session = crate::Session::new(
            crate::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            crate::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            crate::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a"), ModId::new("ghost.mod")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        );
        let use_case = ReadModAbout::new(FakeAboutReader::default());

        let with_about = use_case
            .execute(&mut session, &ModId::new("ghost.mod"), OrderSource::Current)
            .expect("must resolve");

        assert_eq!(with_about.link_url(ModLinkKind::Workshop), None);
        assert_eq!(with_about.link_url(ModLinkKind::Homepage), None);
    }
}
