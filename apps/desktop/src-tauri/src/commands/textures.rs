//! `read_texture`: the texture-override change summary's backend half.
//! Read-only — never persists
//! anything, never emits `session://changed`.

use rim_analyzer::domain::ModId;
use rim_session::use_cases::ReadTexture;

use crate::dto::texture::TextureDto;
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Reads `mod_id`'s texture file at `texture_path` (a normalized key, per
/// `rim_analyzer::extract::textures::normalize`) back as a base64 `data:`
/// URL, for the change summary's texture-override panel to display.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::invalid_input`] when `mod_id` isn't active,
/// `texture_path` isn't among its files, the file is over the 8 MB cap,
/// or its content isn't a recognized PNG/JPEG — or a `MergeSourceFailed`
/// [`CommandError`] when reading the located file fails.
pub(crate) async fn read_texture_inner(
    state: &AppState,
    mod_id: String,
    texture_path: String,
) -> Result<TextureDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let use_case = ReadTexture::new(adapters.asset_locator);
        let output = use_case.execute(session, &ModId::new(&mod_id), &texture_path)?;
        Ok((&output).into())
    })
    .await
}

/// See [`read_texture_inner`].
///
/// # Errors
///
/// See [`read_texture_inner`].
#[tauri::command]
pub async fn read_texture(
    state: tauri::State<'_, AppState>,
    mod_id: String,
    texture_path: String,
) -> Result<TextureDto, CommandError> {
    read_texture_inner(&state, mod_id, texture_path).await
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use rim_analyzer::analysis::SourceIndex;
    use rim_resolve::test_support::ReportBuilder;
    use rim_session::ports::{TextureBytes, TextureFormat};
    use rim_session::test_support::{FakeAssetLocator, session_fixture};

    use super::*;
    use crate::error::CommandErrorCode;
    use crate::test_support::session_with_temp_paths;

    fn state_with(session: rim_session::Session) -> AppState {
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        state
    }

    /// [`state_with`], with `Adapters::asset_locator` swapped for
    /// `locator` — the `Arc<dyn AssetLocator + Send + Sync>` seam lets a
    /// command test exercise an in-memory fake instead of the real
    /// `FileAssetLocator` `AppState::default` builds.
    fn state_with_locator(session: rim_session::Session, locator: FakeAssetLocator) -> AppState {
        let mut state = state_with(session);
        state.adapters.asset_locator = Arc::new(locator);
        state
    }

    #[tokio::test]
    async fn reads_a_texture_through_the_in_memory_fake() {
        let path = PathBuf::from("Mods/a/Textures/things/wall.png");
        let locator =
            FakeAssetLocator::new(BTreeMap::from([("things/wall".to_string(), path.clone())]))
                .with_bytes(
                    path,
                    TextureBytes {
                        format: TextureFormat::Png,
                        bytes: vec![0x89, 0x50, 0x4E, 0x47],
                    },
                );
        let state = state_with_locator(session_fixture(&["a"]), locator);

        let dto = read_texture_inner(&state, "a".to_string(), "things/wall".to_string())
            .await
            .expect("must succeed");

        assert_eq!(dto.format, crate::dto::texture::TextureFormatDto::Png);
        assert!(dto.data_url.starts_with("data:image/png;base64,"));
    }

    #[tokio::test]
    async fn an_unknown_mod_is_invalid_input() {
        let state = state_with_locator(session_fixture(&["a"]), FakeAssetLocator::default());

        let result =
            read_texture_inner(&state, "not.active".to_string(), "things/wall".to_string()).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn a_texture_that_cannot_be_located_is_invalid_input() {
        let state = state_with_locator(session_fixture(&["a"]), FakeAssetLocator::default());

        let result =
            read_texture_inner(&state, "a".to_string(), "things/missing".to_string()).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    /// End-to-end against the real `FileAssetLocator`/`ReadTexture` (the
    /// default adapters — never swapped) over a scratch temp directory —
    /// proves the command reaches the actual filesystem, not just the
    /// fake.
    #[tokio::test]
    async fn reads_a_real_texture_through_the_default_adapters() {
        let temp = tempfile::tempdir().expect("tempdir");
        let textures_dir = temp.path().join("Textures").join("things");
        std::fs::create_dir_all(&textures_dir).expect("create Textures/things");
        let png: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        std::fs::write(textures_dir.join("Wall.png"), png).expect("seed the texture");

        let report = ReportBuilder::new()
            .mod_with("a", |m| m.loaded_folders = vec![temp.path().to_path_buf()])
            .build();
        let (_temp_paths, session) =
            session_with_temp_paths(report, SourceIndex::default(), &["a"]);
        let state = state_with(session);

        let dto = read_texture_inner(&state, "a".to_string(), "things/wall".to_string())
            .await
            .expect("the real adapter must locate and read the seeded texture");

        assert_eq!(dto.format, crate::dto::texture::TextureFormatDto::Png);
        assert_eq!(dto.bytes, png.len());
    }
}
