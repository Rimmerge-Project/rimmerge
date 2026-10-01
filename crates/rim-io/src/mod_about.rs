//! [`FileModAboutReader`]: reads one mod's `About.xml` details back for
//! the mod info panel — `rim-analyzer`'s own lazy
//! `infra::read_about_details`, plus (for a vanilla source) folding in
//! `ExpansionDefs.xml`'s own description, since Core/DLC's own
//! `About.xml` never carries one (the engine reads `ExpansionDef
//! .description` instead — see `rim_analyzer::extract::expansion_defs`'s
//! own doc comment).

use std::path::Path;

use rim_analyzer::domain::{GameVersion, ModId, Source};
use rim_analyzer::extract::about_xml::AboutDetails;
use rim_analyzer::extract::expansion_defs;
use rim_analyzer::infra::{self, paths as analyzer_paths};
use rim_session::ports::{AboutReadError, ModAboutReader};

/// Defensive bound on `ExpansionDefs.xml`'s own read, well over the
/// measured real file's size (a few KB) — a hostile or corrupt file is
/// refused rather than read in full.
const MAX_EXPANSION_DEFS_BYTES: u64 = 8 * 1024 * 1024;

/// Reads `<mod_root>/About/About.xml` for every source; for `Core`/`Dlc`,
/// also reads `<game_dir>/Data/Core/Defs/Misc/ExpansionDefs/ExpansionDefs.xml`
/// and overrides the description with that def's own `<description>` when
/// it has one. Stateless — every call re-reads from disk, the same
/// contract [`crate::FileAssetLocator`] carries.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileModAboutReader;

impl FileModAboutReader {
    /// Builds the reader.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// The `ExpansionDef` description linked to `id` (by
    /// [`ModId::base`]), if `<game_dir>/Data/Core/Defs/Misc/ExpansionDefs/ExpansionDefs.xml`
    /// can be read, parsed, and names one. Every failure degrades to
    /// `None` — a vanilla mod simply falls back to its own (typically
    /// absent) `About.xml` description rather than failing the whole
    /// read over Core's own file.
    fn vanilla_description(game_dir: &Path, id: &ModId) -> Option<String> {
        let path = analyzer_paths::expansion_defs_path(game_dir);
        let bytes = infra::read_bounded_with_limit(&path, MAX_EXPANSION_DEFS_BYTES).ok()?;
        let map = expansion_defs::parse(&bytes).ok()?;
        map.get(&id.base())
            .and_then(|entry| entry.description.clone())
    }
}

impl ModAboutReader for FileModAboutReader {
    fn read_details(
        &self,
        mod_root: &Path,
        source: Source,
        game_version: GameVersion,
        game_dir: &Path,
    ) -> Result<AboutDetails, AboutReadError> {
        let mut details =
            infra::read_about_details(mod_root, game_version).map_err(map_read_error)?;
        if source.is_vanilla()
            && let Some(description) = Self::vanilla_description(game_dir, &details.package_id)
        {
            details.description = Some(description);
        }
        Ok(details)
    }
}

fn map_read_error(error: infra::ReadAboutDetailsError) -> AboutReadError {
    match error {
        infra::ReadAboutDetailsError::NotFound(_) => AboutReadError::NotFound,
        infra::ReadAboutDetailsError::Io(_, message) => AboutReadError::Io(message),
        infra::ReadAboutDetailsError::Xml(xml) => AboutReadError::Xml(xml.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn v16() -> GameVersion {
        GameVersion::new(1, 6)
    }

    #[test]
    fn reads_a_mods_about_xml() {
        let dir = tempdir().expect("tempdir");
        let mod_root = dir.path().join("SampleMod");
        std::fs::create_dir_all(mod_root.join("About")).expect("create About/");
        std::fs::write(
            mod_root.join("About").join("About.xml"),
            br#"<ModMetaData>
                  <packageId>sample.mod</packageId>
                  <description>hello</description>
                </ModMetaData>"#,
        )
        .expect("seed About.xml");
        let reader = FileModAboutReader::new();

        let details = reader
            .read_details(&mod_root, Source::Local, v16(), dir.path())
            .expect("must succeed");

        assert_eq!(details.package_id, ModId::new("sample.mod"));
        assert_eq!(details.description.as_deref(), Some("hello"));
    }

    #[test]
    fn a_file_over_the_read_limit_is_refused_and_one_at_the_limit_is_read() {
        let dir = tempdir().expect("tempdir");
        let at_limit = dir.path().join("at-limit.xml");
        let over_limit = dir.path().join("over-limit.xml");
        let limit = usize::try_from(MAX_EXPANSION_DEFS_BYTES).expect("limit fits usize");
        std::fs::write(&at_limit, vec![b' '; limit]).expect("seed at-limit file");
        std::fs::write(&over_limit, vec![b' '; limit + 1]).expect("seed over-limit file");

        assert_eq!(
            infra::read_bounded_with_limit(&at_limit, MAX_EXPANSION_DEFS_BYTES)
                .expect("at the limit")
                .len(),
            limit
        );
        let error = infra::read_bounded_with_limit(&over_limit, MAX_EXPANSION_DEFS_BYTES)
            .expect_err("one byte over must be refused");

        assert!(error.to_string().contains("read limit"), "{error}");
    }

    #[test]
    fn a_missing_about_xml_is_not_found() {
        let dir = tempdir().expect("tempdir");
        let mod_root = dir.path().join("Missing");
        let reader = FileModAboutReader::new();

        let error = reader
            .read_details(&mod_root, Source::Local, v16(), dir.path())
            .unwrap_err();

        assert_eq!(error, AboutReadError::NotFound);
    }

    #[test]
    fn a_vanilla_mods_description_comes_from_expansion_defs() {
        let dir = tempdir().expect("tempdir");
        let core_root = dir.path().join("Data").join("Core");
        std::fs::create_dir_all(core_root.join("About")).expect("create About/");
        std::fs::write(
            core_root.join("About").join("About.xml"),
            br#"<ModMetaData><packageId>ludeon.rimworld</packageId></ModMetaData>"#,
        )
        .expect("seed About.xml");
        let expansion_defs_dir = core_root.join("Defs").join("Misc").join("ExpansionDefs");
        std::fs::create_dir_all(&expansion_defs_dir).expect("create ExpansionDefs/");
        std::fs::write(
            expansion_defs_dir.join("ExpansionDefs.xml"),
            br#"<Defs>
                  <ExpansionDef>
                    <label>Core</label>
                    <description>The base game.</description>
                    <linkedMod>Ludeon.RimWorld</linkedMod>
                  </ExpansionDef>
                </Defs>"#,
        )
        .expect("seed ExpansionDefs.xml");
        let reader = FileModAboutReader::new();

        let details = reader
            .read_details(&core_root, Source::Core, v16(), dir.path())
            .expect("must succeed");

        assert_eq!(details.description.as_deref(), Some("The base game."));
    }

    #[test]
    fn a_vanilla_mod_with_no_expansion_def_entry_falls_back_to_its_own_about_xml() {
        let dir = tempdir().expect("tempdir");
        let dlc_root = dir.path().join("Data").join("SomeDlc");
        std::fs::create_dir_all(dlc_root.join("About")).expect("create About/");
        std::fs::write(
            dlc_root.join("About").join("About.xml"),
            br#"<ModMetaData>
                  <packageId>ludeon.rimworld.somedlc</packageId>
                  <description>fallback</description>
                </ModMetaData>"#,
        )
        .expect("seed About.xml");
        // No ExpansionDefs.xml under `Data/Core` at all.
        let reader = FileModAboutReader::new();

        let details = reader
            .read_details(&dlc_root, Source::Dlc, v16(), dir.path())
            .expect("must succeed");

        assert_eq!(details.description.as_deref(), Some("fallback"));
    }
}
