//! `resolve_def_graphic`/`read_def_texture`: what texture a def shows,
//! and the bytes behind one of its textures. Both are read-only: they
//! never persist anything and never emit `session://changed`.

use std::str::FromStr;

use rim_resolve::domain::DefRef;
use rim_session::use_cases::{ReadDefTexture, ResolveDefGraphic, TextureKey};

use crate::dto::def_graphic::{
    DefGraphicDto, DefTextureDto, ReadDefTextureRequestDto, ResolveDefGraphicRequestDto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Resolves what `request.def_ref` shows under the selected order: its
/// slots with each texture's availability, or that it shows nothing of
/// its own. The session caches the answer until the order changes.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] when `request.def_ref` doesn't parse,
/// [`CommandError::def_not_found`] when it doesn't name anything this scan
/// indexed, or a `MergeSourceFailed` [`CommandError`] when the def's source
/// changed since the scan.
pub(crate) async fn resolve_def_graphic_inner(
    state: &AppState,
    request: ResolveDefGraphicRequestDto,
) -> Result<DefGraphicDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let def_ref = DefRef::from_str(&request.def_ref)?;
        let use_case = ResolveDefGraphic::new(adapters.def_reader);
        let graphic = use_case.execute(session, &def_ref)?;
        Ok(graphic.into())
    })
    .await
}

/// See `resolve_def_graphic_inner`.
///
/// # Errors
///
/// See `resolve_def_graphic_inner`.
#[tauri::command]
pub async fn resolve_def_graphic(
    state: tauri::State<'_, AppState>,
    request: ResolveDefGraphicRequestDto,
) -> Result<DefGraphicDto, CommandError> {
    resolve_def_graphic_inner(&state, request).await
}

/// Reads one texture of `request.def_ref`'s resolved graphic back as a
/// base64 `data:` URL, or says why there is nothing to show. The key must
/// be one the def's own resolution produced; any other is refused, so the
/// command cannot be used to read an arbitrary file. No absolute path
/// reaches the response.
///
/// # Errors
///
/// As `resolve_def_graphic_inner`, plus [`CommandError::invalid_input`]
/// when `request.texture_key` is malformed or isn't a texture of the def,
/// and [`CommandError::mod_not_found`] when the file's owner isn't a
/// scanned mod.
pub(crate) async fn read_def_texture_inner(
    state: &AppState,
    request: ReadDefTextureRequestDto,
) -> Result<DefTextureDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let def_ref = DefRef::from_str(&request.def_ref)?;
        let key = TextureKey::parse(&request.texture_key)
            .map_err(|error| CommandError::invalid_input(error.to_string()))?;
        let use_case = ReadDefTexture::new(adapters.def_reader, adapters.asset_locator);
        let texture = use_case.execute(session, &def_ref, &key)?;
        Ok((&texture).into())
    })
    .await
}

/// See `read_def_texture_inner`.
///
/// # Errors
///
/// See `read_def_texture_inner`.
#[tauri::command]
pub async fn read_def_texture(
    state: tauri::State<'_, AppState>,
    request: ReadDefTextureRequestDto,
) -> Result<DefTextureDto, CommandError> {
    read_def_texture_inner(&state, request).await
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::PathBuf;
    use std::sync::Arc;

    use rim_analyzer::analysis::{SourceIndex, TextureIndex};
    use rim_analyzer::domain::{DefEntry, ModId, XmlLocator};
    use rim_resolve::test_support::ReportBuilder;
    use rim_session::ports::{TextureBytes, TextureFormat};
    use rim_session::test_support::{FakeAssetLocator, InMemoryDefSourceReader};

    use super::*;
    use crate::error::CommandErrorCode;
    use crate::test_support::session_with_temp_paths;

    const THING_XML: &str = "<ThingDef><defName>X</defName><graphicData><texPath>Things/X</texPath><graphicClass>Graphic_Single</graphicClass></graphicData></ThingDef>";
    const PNG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];

    /// A project whose mod `a` owns `ThingDef/X` showing `things/x`, which
    /// `a` ships as a loose PNG. The locator also knows a second file,
    /// `secret/file`, that no def shows.
    fn state() -> AppState {
        let element = XmlLocator::new(PathBuf::from("def0.xml").into(), vec![0]);
        let mut sources = SourceIndex::default();
        let key = ("ThingDef".to_string(), "X".to_string());
        sources.defs.insert(
            (ModId::new("a"), key.clone()),
            vec![DefEntry {
                def_type: "ThingDef".to_string(),
                def_name: "X".to_string(),
                may_require: Vec::new(),
                may_require_any_of: Vec::new(),
                parent_name: None,
                locator: element.clone(),
            }],
        );
        sources.owners_by_def.insert(key, vec![ModId::new("a")]);
        sources.textures = TextureIndex::new(
            BTreeMap::from([("things/x".to_string(), vec![ModId::new("a")])]),
            BTreeSet::new(),
            true,
        );
        let report = ReportBuilder::new()
            .mod_with("a", |entry| {
                entry.loaded_folders = vec![PathBuf::from("Mods/a")];
            })
            .build();
        let (_temp_dir, session) = session_with_temp_paths(report, sources, &["a"]);
        let image = PathBuf::from("Mods/a/Textures/Things/X.png");
        let secret = PathBuf::from("Mods/a/Textures/Secret/File.png");
        let png = TextureBytes {
            format: TextureFormat::Png,
            bytes: PNG.to_vec(),
        };
        let locator = FakeAssetLocator::new(BTreeMap::from([
            ("things/x".to_string(), image.clone()),
            ("secret/file".to_string(), secret.clone()),
        ]))
        .with_bytes(image, png.clone())
        .with_bytes(secret, png);
        let mut state = AppState::default();
        state.adapters.def_reader = Arc::new(InMemoryDefSourceReader::new(BTreeMap::from([(
            element,
            THING_XML.to_string(),
        )])));
        state.adapters.asset_locator = Arc::new(locator);
        *state.session.write().expect("lock") = Some(session);
        state
    }

    fn resolve_request(def_ref: &str) -> ResolveDefGraphicRequestDto {
        ResolveDefGraphicRequestDto {
            def_ref: def_ref.to_string(),
        }
    }

    fn read_request(def_ref: &str, texture_key: &str) -> ReadDefTextureRequestDto {
        ReadDefTextureRequestDto {
            def_ref: def_ref.to_string(),
            texture_key: texture_key.to_string(),
        }
    }

    #[tokio::test]
    async fn resolve_lists_the_defs_texture_with_its_owner() {
        let state = state();

        let dto = resolve_def_graphic_inner(&state, resolve_request("ThingDef/X"))
            .await
            .expect("a known def resolves");

        let json = serde_json::to_value(&dto).expect("serializable");
        assert_eq!(json["kind"], "resolved");
        let face = &json["slots"][0]["variants"][0]["faces"]["face"];
        assert_eq!(face["textureKey"], "things/x");
        assert_eq!(
            face["availability"],
            serde_json::json!({ "kind": "loose", "owner": "a" })
        );
    }

    #[tokio::test]
    async fn resolve_reports_def_not_found_for_an_unknown_ref() {
        let state = state();

        let result = resolve_def_graphic_inner(&state, resolve_request("ThingDef/Nope")).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::DefNotFound);
    }

    #[tokio::test]
    async fn resolve_rejects_an_unparseable_ref() {
        let state = state();

        let result = resolve_def_graphic_inner(&state, resolve_request("not a ref")).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn resolve_without_a_project_is_no_project_loaded() {
        let state = AppState::default();

        let result = resolve_def_graphic_inner(&state, resolve_request("ThingDef/X")).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::NoProjectLoaded);
    }

    #[tokio::test]
    async fn read_returns_the_image_without_any_path() {
        let state = state();

        let dto = read_def_texture_inner(&state, read_request("ThingDef/X", "things/x"))
            .await
            .expect("the def's own texture reads");

        let json = serde_json::to_string(&dto).expect("serializable");
        assert!(json.contains("\"kind\":\"image\""), "{json}");
        assert!(json.contains("data:image/png;base64,"), "{json}");
        assert!(json.contains("\"owner\":\"a\""), "{json}");
        assert!(!json.contains("Mods/a"), "a path leaked: {json}");
        assert!(
            !json.to_lowercase().contains("textures"),
            "a path leaked: {json}"
        );
    }

    #[tokio::test]
    async fn read_refuses_a_key_the_def_does_not_show() {
        let state = state();

        let result =
            read_def_texture_inner(&state, read_request("ThingDef/X", "secret/file")).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn read_refuses_a_malformed_key() {
        let state = state();

        let result = read_def_texture_inner(&state, read_request("ThingDef/X", "../x")).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn read_for_an_unknown_def_is_def_not_found() {
        let state = state();

        let result =
            read_def_texture_inner(&state, read_request("ThingDef/Nope", "things/x")).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::DefNotFound);
    }

    #[tokio::test]
    async fn read_without_a_project_is_no_project_loaded() {
        let state = AppState::default();

        let result = read_def_texture_inner(&state, read_request("ThingDef/X", "things/x")).await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::NoProjectLoaded);
    }
}
