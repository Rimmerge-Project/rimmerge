//! [`ReadModPreview`]: reads one mod's `About/Preview.png` or
//! `About/ModIcon.png` back, for the mod info panel's image. The
//! large-payload twin of [`super::ReadModAbout`] — kept a separate call
//! so the panel's text loads without waiting on a multi-megabyte image.

use rim_analyzer::domain::ModId;

use crate::Session;
use crate::mod_info::resolve_installed_mod;
use crate::ports::{AboutImage, AssetLocator, DefSourceError};
use crate::use_cases::read_texture::ReadTextureOutput;

/// `id` (compared by [`ModId::base`]) names nothing in the current
/// report at all — not active, not inactive, not even a declared-but-
/// missing entry.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0} is not a known mod")]
pub struct ReadModPreviewError(pub ModId);

/// Why a located preview file couldn't be read back — kept distinct from
/// [`ModPreview::Absent`]/[`ModPreview::NotOnDisk`] (both normal,
/// expected outcomes, not failures) so the panel can phrase "too
/// large"/"wrong format" differently from "no preview image" at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewUnreadable {
    /// Over [`crate::ports::MAX_TEXTURE_BYTES`].
    TooLarge,
    /// The file's content doesn't sniff as PNG or JPEG.
    UnsupportedFormat,
    /// Any other read failure (permission denied, ...).
    Io(String),
}

/// [`ReadModPreview::execute`]'s result. `Absent`/`NotOnDisk` are not
/// errors — a mod with no preview image, or a missing mod with no folder
/// to look in, are both normal, expected outcomes the panel shows
/// neutrally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModPreview {
    /// `About/Preview.png` exists and was read successfully.
    Image(ReadTextureOutput),
    /// No `About/Preview.png` under this mod's `About/` folder.
    Absent,
    /// No mod folder to look in at all (a missing mod).
    NotOnDisk,
    /// A preview file exists but couldn't be read back.
    Unreadable(PreviewUnreadable),
}

/// Reads one mod's `About/Preview.png` back, for the desktop app's
/// `read_mod_preview` command.
pub struct ReadModPreview<Locator> {
    locator: Locator,
}

impl<Locator: AssetLocator> ReadModPreview<Locator> {
    /// Builds the use case from its port.
    #[must_use]
    pub fn new(locator: Locator) -> Self {
        Self { locator }
    }

    /// Resolves `id`'s mod root from `session`'s report (the exact id
    /// first, then by [`ModId::base`], each across active mods then
    /// inactive ones) and reads
    /// `image` back — the path is derived here from session data alone;
    /// the request carries no path field at all, so "only a mod's own
    /// `About/` folder is ever read" holds by construction, not by a
    /// check this method would otherwise have to repeat.
    ///
    /// # Errors
    ///
    /// Returns [`ReadModPreviewError`] when `id` names nothing in the
    /// current report at all — not even a declared-but-missing entry.
    pub fn execute(
        &self,
        session: &Session,
        id: &ModId,
        image: AboutImage,
    ) -> Result<ModPreview, ReadModPreviewError> {
        let Some(root) = Self::resolve_root(session, id)? else {
            return Ok(ModPreview::NotOnDisk);
        };
        let Some(path) = self.locator.locate_about_image(&root, image) else {
            return Ok(ModPreview::Absent);
        };
        Ok(match self.locator.read_texture(&path) {
            Ok(texture) => ModPreview::Image(ReadTextureOutput { path, texture }),
            Err(DefSourceError::TooLarge { .. }) => {
                ModPreview::Unreadable(PreviewUnreadable::TooLarge)
            }
            Err(DefSourceError::UnsupportedFormat { .. }) => {
                ModPreview::Unreadable(PreviewUnreadable::UnsupportedFormat)
            }
            Err(other) => ModPreview::Unreadable(PreviewUnreadable::Io(other.to_string())),
        })
    }

    /// `Some(root)` for an active or inactive mod, `None` for a mod
    /// that's declared active but never found on disk (no folder to
    /// derive a path from). Errors only when `id` names nothing in the
    /// report's inventory at all.
    ///
    /// The exact id wins over a mere base match: an active mod and an
    /// inactive `_steam` copy of it share a base, and asking about the
    /// inactive copy must read *its* folder, not the active one's.
    fn resolve_root(
        session: &Session,
        id: &ModId,
    ) -> Result<Option<std::path::PathBuf>, ReadModPreviewError> {
        let report = session.report();
        if let Some(resolved) = resolve_installed_mod(report, id) {
            return Ok(Some(resolved.path().to_path_buf()));
        }
        let target = id.base();
        if report.missing_mods.iter().any(|m| m.base() == target) {
            return Ok(None);
        }
        Err(ReadModPreviewError(id.clone()))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::ports::TextureFormat;
    use crate::test_support::{FakeAssetLocator, session_fixture};

    fn png_bytes() -> crate::ports::TextureBytes {
        crate::ports::TextureBytes {
            format: TextureFormat::Png,
            bytes: vec![0x89, 0x50, 0x4E, 0x47],
        }
    }

    #[test]
    fn resolves_the_exact_about_folder_of_the_active_mod() {
        let session = session_fixture(&["a"]);
        let root = session
            .report()
            .mods
            .iter()
            .find(|m| m.id == ModId::new("a"))
            .expect("mod a exists")
            .path
            .clone();
        let locator = FakeAssetLocator::default()
            .with_about_image(
                root.clone(),
                AboutImage::Preview,
                Some(PathBuf::from("dont-care")),
            )
            .with_bytes(PathBuf::from("dont-care"), png_bytes());
        let use_case = ReadModPreview::new(locator);

        let preview = use_case
            .execute(&session, &ModId::new("a"), AboutImage::Preview)
            .expect("must resolve");

        assert!(matches!(preview, ModPreview::Image(_)));
        assert_eq!(
            use_case.locator.last_about_image_request(),
            Some((root, AboutImage::Preview))
        );
    }

    #[test]
    fn a_mod_with_no_preview_file_is_absent() {
        let session = session_fixture(&["a"]);
        let use_case = ReadModPreview::new(FakeAssetLocator::default());

        let preview = use_case
            .execute(&session, &ModId::new("a"), AboutImage::Preview)
            .expect("must resolve");

        assert_eq!(preview, ModPreview::Absent);
    }

    #[test]
    fn an_oversized_preview_is_unreadable_too_large() {
        let session = session_fixture(&["a"]);
        let root = session
            .report()
            .mods
            .iter()
            .find(|m| m.id == ModId::new("a"))
            .expect("mod a exists")
            .path
            .clone();
        let path = PathBuf::from("preview.png");
        let locator = FakeAssetLocator::default()
            .with_about_image(root, AboutImage::Preview, Some(path.clone()))
            .with_read_error(
                path.clone(),
                DefSourceError::TooLarge {
                    file: path.clone(),
                    max_bytes: 8,
                    actual_bytes: 9,
                },
            );
        let use_case = ReadModPreview::new(locator);

        let preview = use_case
            .execute(&session, &ModId::new("a"), AboutImage::Preview)
            .expect("must resolve");

        assert_eq!(preview, ModPreview::Unreadable(PreviewUnreadable::TooLarge));
    }

    #[test]
    fn an_unsupported_format_preview_is_unreadable() {
        let session = session_fixture(&["a"]);
        let root = session
            .report()
            .mods
            .iter()
            .find(|m| m.id == ModId::new("a"))
            .expect("mod a exists")
            .path
            .clone();
        let path = PathBuf::from("preview.png");
        let locator = FakeAssetLocator::default()
            .with_about_image(root, AboutImage::Preview, Some(path.clone()))
            .with_read_error(
                path.clone(),
                DefSourceError::UnsupportedFormat { file: path.clone() },
            );
        let use_case = ReadModPreview::new(locator);

        let preview = use_case
            .execute(&session, &ModId::new("a"), AboutImage::Preview)
            .expect("must resolve");

        assert_eq!(
            preview,
            ModPreview::Unreadable(PreviewUnreadable::UnsupportedFormat)
        );
    }

    #[test]
    fn an_inactive_copy_sharing_a_base_with_an_active_mod_reads_its_own_folder() {
        let mut report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("shared.mod")
            .inactive("shared.mod_steam")
            .build();
        let inactive_root = PathBuf::from("inactive/shared.mod_steam");
        report.inactive_mods[0].path = inactive_root.clone();
        let session = crate::Session::new(
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
        let use_case = ReadModPreview::new(FakeAssetLocator::default());

        let preview = use_case
            .execute(
                &session,
                &ModId::new("shared.mod_steam"),
                AboutImage::Preview,
            )
            .expect("must resolve");

        assert_eq!(preview, ModPreview::Absent);
        assert_eq!(
            use_case.locator.last_about_image_request(),
            Some((inactive_root, AboutImage::Preview)),
            "the exact id must win over the active mod sharing its base"
        );
    }

    #[test]
    fn a_missing_mod_is_not_on_disk() {
        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("a")
            .missing_mod("ghost.mod")
            .build();
        let session = crate::Session::new(
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
        let use_case = ReadModPreview::new(FakeAssetLocator::default());

        let preview = use_case
            .execute(&session, &ModId::new("ghost.mod"), AboutImage::Preview)
            .expect("must resolve");

        assert_eq!(preview, ModPreview::NotOnDisk);
    }

    #[test]
    fn an_unknown_id_is_an_error() {
        let session = session_fixture(&["a"]);
        let use_case = ReadModPreview::new(FakeAssetLocator::default());

        let result = use_case.execute(&session, &ModId::new("nobody"), AboutImage::Preview);

        assert!(result.is_err());
    }

    #[test]
    fn resolves_the_icon_independently_of_the_preview() {
        let session = session_fixture(&["a"]);
        let root = session
            .report()
            .mods
            .iter()
            .find(|m| m.id == ModId::new("a"))
            .expect("mod a exists")
            .path
            .clone();
        let icon_path = PathBuf::from("icon.png");
        let locator = FakeAssetLocator::default()
            .with_about_image(root, AboutImage::Icon, Some(icon_path.clone()))
            .with_bytes(icon_path, png_bytes());
        let use_case = ReadModPreview::new(locator);

        let preview = use_case
            .execute(&session, &ModId::new("a"), AboutImage::Icon)
            .expect("must resolve");

        assert!(matches!(preview, ModPreview::Image(_)));
    }
}
