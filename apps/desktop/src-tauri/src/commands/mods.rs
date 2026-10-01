//! `list_mods`/`get_mod`/`list_mod_names`/`get_mod_info`/`read_mod_preview`:
//! the mods page's commands.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_session::use_cases::{ReadModAbout, ReadModPreview};
use rim_session::{FindingFilter, MAX_PAGE_SIZE};

use crate::dto::mod_info::{ModInfoDto, ModPreviewDto, mod_info_dto};
use crate::dto::mods::{
    ModDetailDto, ModEdgeDto, ModFilterDto, ModNamesDto, ModPageDto, mod_page_dto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Searches and pages the active mod list. Filtering/sorting/paging lives
/// in `rim_session::Session::mods`; this only maps the DTO in and out.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::invalid_input`] when `filter.tag` isn't a valid tag
/// slug.
pub(crate) async fn list_mods_inner(
    state: &AppState,
    filter: ModFilterDto,
) -> Result<ModPageDto, CommandError> {
    with_session(state, move |session| {
        let filter: rim_session::ModFilter = filter.try_into()?;
        let page = session.mods(&filter);
        Ok(mod_page_dto(page, session.report()))
    })
    .await
}

/// See [`list_mods_inner`].
///
/// # Errors
///
/// See [`list_mods_inner`].
#[tauri::command]
pub async fn list_mods(
    state: tauri::State<'_, AppState>,
    filter: ModFilterDto,
) -> Result<ModPageDto, CommandError> {
    list_mods_inner(&state, filter).await
}

/// One mod's full detail: declared order, edges grouped by direction and
/// evaluated against the selected order, tags, and live finding keys.
/// Resolves `mod_id` by [`ModId::base`] on both sides, so an active
/// `_steam` variant is found by its base id and vice versa; every
/// subsequent per-mod lookup (edges, tags, findings) then uses the
/// resolved canonical id rather than the raw request, so those can't miss
/// the same way.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::mod_not_found`] when `mod_id` isn't active.
pub(crate) async fn get_mod_inner(
    state: &AppState,
    mod_id: String,
) -> Result<ModDetailDto, CommandError> {
    with_session(state, move |session| {
        let target = ModId::new(&mod_id).base();
        let source = session.selected();
        let order = session.orders().get(source).clone();

        let mod_entry = session
            .report()
            .mods
            .iter()
            .find(|m| m.id.base() == target)
            .cloned()
            .ok_or_else(|| CommandError::mod_not_found(&mod_id))?;
        // The resolved, canonical id — used for every lookup below
        // instead of the raw request, so a `_steam`-suffixed active id
        // reached via its base (or vice versa) doesn't then miss again.
        let id = mod_entry.id.clone();

        let (edges_in, edges_out) = {
            let report = session.report();
            let mut edges_in = Vec::new();
            let mut edges_out = Vec::new();
            for report_edge in &report.edges {
                let status = rim_resolve::evaluate::edge_status(&report_edge.edge, &order);
                if report_edge.edge.after == id {
                    edges_in.push(ModEdgeDto::incoming(&report_edge.edge, status));
                } else if report_edge.edge.before == id {
                    edges_out.push(ModEdgeDto::outgoing(&report_edge.edge, status));
                }
            }
            (edges_in, edges_out)
        };

        let finding_page = session.findings(
            source,
            &FindingFilter {
                mod_id: Some(id.clone()),
                offset: 0,
                limit: MAX_PAGE_SIZE,
                ..FindingFilter::default()
            },
        );
        let finding_keys_total = finding_page.total;
        let finding_keys = finding_page.items.iter().map(ToString::to_string).collect();

        Ok(ModDetailDto {
            mod_id: mod_entry.id.as_str().to_string(),
            name: mod_entry.name.clone(),
            authors: mod_entry.authors.clone(),
            url: mod_entry.url.clone(),
            source: mod_entry.source.into(),
            supported_versions: mod_entry.supported_versions.clone(),
            declared: (&mod_entry.declared).into(),
            tags: session
                .tagging()
                .tags_of(&id)
                .iter()
                .map(ToString::to_string)
                .collect(),
            hard_dependents: mod_entry.hard_dependents,
            is_framework_candidate: mod_entry.is_framework_candidate,
            generated: mod_entry.generated.as_ref().map(Into::into),
            workshop_id: mod_entry.workshop_id,
            edges_in,
            edges_out,
            finding_keys,
            finding_keys_total,
        })
    })
    .await
}

/// See [`get_mod_inner`].
///
/// # Errors
///
/// See [`get_mod_inner`].
#[tauri::command]
pub async fn get_mod(
    state: tauri::State<'_, AppState>,
    mod_id: String,
) -> Result<ModDetailDto, CommandError> {
    get_mod_inner(&state, mod_id).await
}

/// Every active **or inactive** mod's id mapped to its display name, so a
/// render site can show names instead of ids without a per-render lookup
/// command — an inactive
/// mod (the Mods page's own Inactive tab, or a `plan_activate_mods`/
/// `plan_deactivate_mods` dialog) needs this too, not only an active one.
/// A `_steam`-suffixed active id also gets its base id entered as a
/// second key (never overwriting an already-active base id's own entry),
/// so a caller holding either form's text resolves the same name.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn list_mod_names_inner(state: &AppState) -> Result<ModNamesDto, CommandError> {
    with_session(state, |session| {
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        for mod_entry in &session.report().mods {
            names.insert(mod_entry.id.as_str().to_string(), mod_entry.name.clone());
        }
        for mod_entry in &session.report().mods {
            let base = ModId::base(&mod_entry.id);
            if base != mod_entry.id {
                names
                    .entry(base.as_str().to_string())
                    .or_insert_with(|| mod_entry.name.clone());
            }
        }
        for mod_entry in &session.report().inactive_mods {
            names
                .entry(mod_entry.id.as_str().to_string())
                .or_insert_with(|| mod_entry.name.clone());
        }
        Ok(ModNamesDto(names))
    })
    .await
}

/// See [`list_mod_names_inner`].
///
/// # Errors
///
/// See [`list_mod_names_inner`].
#[tauri::command]
pub async fn list_mod_names(
    state: tauri::State<'_, AppState>,
) -> Result<ModNamesDto, CommandError> {
    list_mod_names_inner(&state).await
}

/// One mod's full information for the mod info panel: identity, declared
/// order, scan facts (cost, loaded folders, dependents), placement,
/// findings, pending state, and the lazily-read `About.xml` details
/// (description, version, icon path). Resolves `mod_id` by [`ModId::base`]
/// across active, inactive, and missing mods.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::mod_not_found`] when `mod_id` names nothing in the
/// current report at all.
pub(crate) async fn get_mod_info_inner(
    state: &AppState,
    mod_id: String,
) -> Result<ModInfoDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let source = session.selected();
        let use_case = ReadModAbout::new(adapters.mod_about_reader);
        let with_about = use_case.execute(session, &ModId::new(&mod_id), source)?;
        let game_dir = session.paths().game_dir.clone();
        let workshop_dir = session.paths().workshop_dir.clone();
        Ok(mod_info_dto(&with_about, &game_dir, &workshop_dir))
    })
    .await
}

/// See `get_mod_info_inner`.
///
/// # Errors
///
/// See `get_mod_info_inner`.
#[tauri::command]
pub async fn get_mod_info(
    state: tauri::State<'_, AppState>,
    mod_id: String,
) -> Result<ModInfoDto, CommandError> {
    get_mod_info_inner(&state, mod_id).await
}

/// Reads `mod_id`'s own `About/Preview.png` back, for the mod info
/// panel's image. The path is derived entirely from session data — this
/// command takes only `mod_id`, never a path.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::mod_not_found`] when `mod_id` names nothing in the
/// current report at all.
pub(crate) async fn read_mod_preview_inner(
    state: &AppState,
    mod_id: String,
) -> Result<ModPreviewDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let use_case = ReadModPreview::new(adapters.asset_locator);
        let preview = use_case.execute(
            session,
            &ModId::new(&mod_id),
            rim_session::ports::AboutImage::Preview,
        )?;
        Ok((&preview).into())
    })
    .await
}

/// See `read_mod_preview_inner`.
///
/// # Errors
///
/// See `read_mod_preview_inner`.
#[tauri::command]
pub async fn read_mod_preview(
    state: tauri::State<'_, AppState>,
    mod_id: String,
) -> Result<ModPreviewDto, CommandError> {
    read_mod_preview_inner(&state, mod_id).await
}

/// Reads `mod_id`'s own `About/ModIcon.png` back, for the mod info
/// panel's icon. See `read_mod_preview_inner`'s own doc comment — same
/// shape, a different `About/` image.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// or [`CommandError::mod_not_found`] when `mod_id` names nothing in the
/// current report at all.
pub(crate) async fn read_mod_icon_inner(
    state: &AppState,
    mod_id: String,
) -> Result<ModPreviewDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let use_case = ReadModPreview::new(adapters.asset_locator);
        let preview = use_case.execute(
            session,
            &ModId::new(&mod_id),
            rim_session::ports::AboutImage::Icon,
        )?;
        Ok((&preview).into())
    })
    .await
}

/// See `read_mod_icon_inner`.
///
/// # Errors
///
/// See `read_mod_icon_inner`.
#[tauri::command]
pub async fn read_mod_icon(
    state: tauri::State<'_, AppState>,
    mod_id: String,
) -> Result<ModPreviewDto, CommandError> {
    read_mod_icon_inner(&state, mod_id).await
}

/// Opens `mod_id`'s own workshop or homepage link in the system browser.
/// The URL is re-derived here from session data through
/// [`rim_session::use_cases::ModInfoWithAbout::link_url`], the same
/// resolution `get_mod_info` itself uses to show the link — the frontend
/// names only `mod_id` and `link`, never a URL.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::mod_not_found`] when `mod_id` names nothing in the
/// current report at all, or [`CommandError::invalid_input`] when this
/// mod has no such link (no workshop id, no homepage, or a homepage
/// that's present but not openable).
pub(crate) async fn open_mod_link_inner(
    state: &AppState,
    mod_id: String,
    link: crate::dto::links::ModLinkKindDto,
) -> Result<(), CommandError> {
    let adapters = state.adapters.clone();
    let url = with_session(state, move |session| {
        let source = session.selected();
        let use_case = ReadModAbout::new(adapters.mod_about_reader);
        let with_about = use_case.execute(session, &ModId::new(&mod_id), source)?;
        with_about
            .link_url(link.into())
            .ok_or_else(|| CommandError::invalid_input("this mod has no such openable link"))
    })
    .await?;
    state
        .link_opener
        .open(&url)
        .map_err(|error| CommandError::internal(error.to_string()))
}

/// See `open_mod_link_inner`.
///
/// # Errors
///
/// See `open_mod_link_inner`.
#[tauri::command]
pub async fn open_mod_link(
    state: tauri::State<'_, AppState>,
    mod_id: String,
    link: crate::dto::links::ModLinkKindDto,
) -> Result<(), CommandError> {
    open_mod_link_inner(&state, mod_id, link).await
}

#[cfg(test)]
mod tests {
    use rim_session::test_support::session_fixture;

    use super::*;

    #[tokio::test]
    async fn get_mod_finds_an_active_steam_suffixed_mod_by_its_base_id() {
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session_fixture(&["x_steam", "y"]));

        let detail = get_mod_inner(&state, "x".to_string())
            .await
            .expect("x_steam must be found by its base id");

        assert_eq!(detail.mod_id, "x_steam");
    }

    #[tokio::test]
    async fn get_mod_returns_mod_not_found_for_an_inactive_mod() {
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session_fixture(&["a"]));

        let result = get_mod_inner(&state, "nobody".to_string()).await;

        assert_eq!(
            result.unwrap_err().code,
            crate::error::CommandErrorCode::ModNotFound
        );
    }

    #[tokio::test]
    async fn list_mod_names_adds_a_base_id_entry_for_a_steam_suffixed_active_mod() {
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session_fixture(&["x_steam", "y"]));

        let names = list_mod_names_inner(&state)
            .await
            .expect("list_mod_names must succeed")
            .0;

        assert_eq!(names.get("x_steam").map(String::as_str), Some("x_steam"));
        assert_eq!(
            names.get("x").map(String::as_str),
            Some("x_steam"),
            "a _steam id's base must resolve to the same name"
        );
        assert_eq!(names.get("y").map(String::as_str), Some("y"));
    }

    #[tokio::test]
    async fn list_mod_names_also_covers_an_inactive_mod() {
        // `useModLabel` reads this command for the
        // Inactive tab and for `plan_activate_mods`/`plan_deactivate_mods`
        // dialog rows too, not only active ones.
        let state = AppState::default();
        let report =
            rim_session::test_support::report_fixture_with_inactive(&["a"], &["inactive.mod"]);
        *state.session.write().expect("lock") = Some(rim_session::Session::new(
            rim_session::ProjectPaths {
                game_dir: "game".into(),
                workshop_dir: "workshop".into(),
                mods_config: "ModsConfig.xml".into(),
                profile_dir: "profile".into(),
            },
            report,
            Vec::new(),
            rim_analyzer::analysis::SourceIndex::default(),
            rim_session::ports::StoredRules::default(),
            rim_resolve::domain::DecisionSet::new(),
            rim_session::ports::ModsConfigFile {
                version: "1.6".to_string(),
                active_mods: vec![ModId::new("a")],
                known_expansions: Vec::new(),
            },
            Vec::new(),
            Vec::new(),
        ));

        let names = list_mod_names_inner(&state)
            .await
            .expect("list_mod_names must succeed")
            .0;

        assert_eq!(
            names.get("inactive.mod").map(String::as_str),
            Some("inactive.mod")
        );
    }

    #[tokio::test]
    async fn list_mods_caps_the_page_at_max_page_size() {
        let ids: Vec<String> = (0..MAX_PAGE_SIZE + 5).map(|i| format!("mod{i}")).collect();
        let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session_fixture(&id_refs));

        let page = list_mods_inner(
            &state,
            ModFilterDto {
                limit: 10_000,
                ..ModFilterDto::default()
            },
        )
        .await
        .expect("list_mods must succeed");

        assert_eq!(page.total, MAX_PAGE_SIZE + 5);
        assert_eq!(page.items.len(), MAX_PAGE_SIZE);
    }

    mod mod_info {
        use std::path::PathBuf;
        use std::sync::Arc;

        use rim_analyzer::analysis::SourceIndex;
        use rim_session::mod_info::ModInfo;
        use rim_session::ports::{AboutImage, TextureBytes, TextureFormat};
        use rim_session::test_support::{FakeAssetLocator, FakeModAboutReader};

        use super::*;
        use crate::dto::mod_info::ModInfoDto;
        use crate::error::CommandErrorCode;

        fn state_with(session: rim_session::Session) -> AppState {
            let state = AppState::default();
            *state.session.write().expect("lock") = Some(session);
            state
        }

        #[tokio::test]
        async fn resolves_an_active_mod() {
            let state = state_with(session_fixture(&["a"]));

            let dto = get_mod_info_inner(&state, "a".to_string())
                .await
                .expect("must resolve");

            assert!(matches!(dto, ModInfoDto::Active(_)));
        }

        #[tokio::test]
        async fn resolves_an_inactive_mod() {
            let report =
                rim_session::test_support::report_fixture_with_inactive(&["a"], &["inactive.mod"]);
            let session = rim_session::Session::new(
                rim_session::ProjectPaths {
                    game_dir: "game".into(),
                    workshop_dir: "workshop".into(),
                    mods_config: "ModsConfig.xml".into(),
                    profile_dir: "profile".into(),
                },
                report,
                Vec::new(),
                SourceIndex::default(),
                rim_session::ports::StoredRules::default(),
                rim_resolve::domain::DecisionSet::new(),
                rim_session::ports::ModsConfigFile {
                    version: "1.6".to_string(),
                    active_mods: vec![ModId::new("a")],
                    known_expansions: Vec::new(),
                },
                Vec::new(),
                Vec::new(),
            );
            let state = state_with(session);

            let dto = get_mod_info_inner(&state, "inactive.mod".to_string())
                .await
                .expect("must resolve");

            assert!(matches!(dto, ModInfoDto::Inactive(_)));
        }

        #[tokio::test]
        async fn an_unknown_mod_is_mod_not_found() {
            let state = state_with(session_fixture(&["a"]));

            let result = get_mod_info_inner(&state, "nobody".to_string()).await;

            assert_eq!(result.unwrap_err().code, CommandErrorCode::ModNotFound);
        }

        #[tokio::test]
        async fn about_details_come_through_the_injected_reader() {
            let report = rim_resolve::test_support::ReportBuilder::new()
                .mod_with("a", |m| {
                    m.path = PathBuf::from("game").join("Mods").join("a")
                })
                .build();
            let mut session = rim_session::Session::new(
                rim_session::ProjectPaths {
                    game_dir: "game".into(),
                    workshop_dir: "workshop".into(),
                    mods_config: "ModsConfig.xml".into(),
                    profile_dir: "profile".into(),
                },
                report,
                Vec::new(),
                SourceIndex::default(),
                rim_session::ports::StoredRules::default(),
                rim_resolve::domain::DecisionSet::new(),
                rim_session::ports::ModsConfigFile {
                    version: "1.6".to_string(),
                    active_mods: vec![ModId::new("a")],
                    known_expansions: Vec::new(),
                },
                Vec::new(),
                Vec::new(),
            );
            let root = session
                .mod_info(&ModId::new("a"), rim_resolve::domain::OrderSource::Current)
                .expect("resolves");
            let ModInfo::Active(active) = root else {
                panic!("expected Active");
            };
            let reader = FakeModAboutReader::default().with_result(
                active.root.clone(),
                Ok(rim_analyzer::extract::about_xml::AboutDetails {
                    package_id: ModId::new("a"),
                    description: Some("hello <b>world</b>".to_string()),
                    mod_version: Some("1.0".to_string()),
                    mod_icon_path: None,
                    url: None,
                }),
            );
            let mut state = state_with(session);
            state.adapters.mod_about_reader = Arc::new(reader);

            let dto = get_mod_info_inner(&state, "a".to_string())
                .await
                .expect("must resolve");

            let ModInfoDto::Active(active) = dto else {
                panic!("expected Active");
            };
            assert!(matches!(
                active.about,
                crate::dto::mod_info::ModAboutDto::Read { .. }
            ));
            assert!(
                !active.root.starts_with("game"),
                "root must be shown relative to game_dir, not absolute-looking: {}",
                active.root
            );
        }

        #[tokio::test]
        async fn a_not_found_about_file_maps_to_not_on_disk_not_a_command_failure() {
            let mut state = state_with(session_fixture(&["a"]));
            // The real `FileModAboutReader` swapped for a fake with no
            // result seeded for any root — it returns `NotFound`, which
            // the command must surface as a field on a *successful*
            // response, never as `Err(CommandError)`.
            state.adapters.mod_about_reader = Arc::new(FakeModAboutReader::default());

            let dto = get_mod_info_inner(&state, "a".to_string())
                .await
                .expect("a reader failure must still be a successful command response");

            let ModInfoDto::Active(active) = dto else {
                panic!("expected Active");
            };
            assert!(matches!(
                active.about,
                crate::dto::mod_info::ModAboutDto::NotOnDisk
            ));
        }

        #[tokio::test]
        async fn an_io_error_maps_to_unreadable_not_a_command_failure() {
            let session = session_fixture(&["a"]);
            let root = session
                .report()
                .mods
                .iter()
                .find(|m| m.id == ModId::new("a"))
                .expect("mod a exists")
                .path
                .clone();
            let reader = FakeModAboutReader::default().with_result(
                root,
                Err(rim_session::ports::AboutReadError::Io("boom".to_string())),
            );
            let mut state = state_with(session);
            state.adapters.mod_about_reader = Arc::new(reader);

            let dto = get_mod_info_inner(&state, "a".to_string())
                .await
                .expect("a reader failure must still be a successful command response");

            let ModInfoDto::Active(active) = dto else {
                panic!("expected Active");
            };
            assert!(matches!(
                active.about,
                crate::dto::mod_info::ModAboutDto::Unreadable { .. }
            ));
        }

        #[tokio::test]
        async fn read_mod_preview_absent_by_default() {
            let state = state_with(session_fixture(&["a"]));

            let dto = read_mod_preview_inner(&state, "a".to_string())
                .await
                .expect("must resolve");

            assert_eq!(dto, crate::dto::mod_info::ModPreviewDto::Absent);
        }

        #[tokio::test]
        async fn read_mod_preview_returns_the_located_image() {
            let session = session_fixture(&["a"]);
            let root = session
                .report()
                .mods
                .iter()
                .find(|m| m.id == ModId::new("a"))
                .expect("mod a exists")
                .path
                .clone();
            let preview_path = root.join("About").join("Preview.png");
            let locator = FakeAssetLocator::default()
                .with_about_image(root, AboutImage::Preview, Some(preview_path.clone()))
                .with_bytes(
                    preview_path,
                    TextureBytes {
                        format: TextureFormat::Png,
                        bytes: vec![0x89, 0x50, 0x4E, 0x47],
                    },
                );
            let mut state = state_with(session);
            state.adapters.asset_locator = Arc::new(locator);

            let dto = read_mod_preview_inner(&state, "a".to_string())
                .await
                .expect("must resolve");

            assert!(matches!(
                dto,
                crate::dto::mod_info::ModPreviewDto::Image { .. }
            ));
        }

        #[tokio::test]
        async fn read_mod_preview_unknown_mod_is_mod_not_found() {
            let state = state_with(session_fixture(&["a"]));

            let result = read_mod_preview_inner(&state, "nobody".to_string()).await;

            assert_eq!(result.unwrap_err().code, CommandErrorCode::ModNotFound);
        }

        #[tokio::test]
        async fn read_mod_icon_returns_the_located_icon() {
            let session = session_fixture(&["a"]);
            let root = session
                .report()
                .mods
                .iter()
                .find(|m| m.id == ModId::new("a"))
                .expect("mod a exists")
                .path
                .clone();
            let icon_path = root.join("About").join("ModIcon.png");
            let locator = FakeAssetLocator::default()
                .with_about_image(root, AboutImage::Icon, Some(icon_path.clone()))
                .with_bytes(
                    icon_path,
                    TextureBytes {
                        format: TextureFormat::Png,
                        bytes: vec![0x89, 0x50, 0x4E, 0x47],
                    },
                );
            let mut state = state_with(session);
            state.adapters.asset_locator = Arc::new(locator);

            let dto = read_mod_icon_inner(&state, "a".to_string())
                .await
                .expect("must resolve");

            assert!(matches!(
                dto,
                crate::dto::mod_info::ModPreviewDto::Image { .. }
            ));
        }

        #[tokio::test]
        async fn read_mod_icon_unknown_mod_is_mod_not_found() {
            let state = state_with(session_fixture(&["a"]));

            let result = read_mod_icon_inner(&state, "nobody".to_string()).await;

            assert_eq!(result.unwrap_err().code, CommandErrorCode::ModNotFound);
        }

        #[tokio::test]
        async fn open_mod_link_opens_the_workshop_url_through_the_recording_fake() {
            let report = rim_resolve::test_support::ReportBuilder::new()
                .mod_with("a", |m| m.workshop_id = Some(42))
                .build();
            let session = rim_session::Session::new(
                rim_session::ProjectPaths {
                    game_dir: "game".into(),
                    workshop_dir: "workshop".into(),
                    mods_config: "ModsConfig.xml".into(),
                    profile_dir: "profile".into(),
                },
                report,
                Vec::new(),
                SourceIndex::default(),
                rim_session::ports::StoredRules::default(),
                rim_resolve::domain::DecisionSet::new(),
                rim_session::ports::ModsConfigFile {
                    version: "1.6".to_string(),
                    active_mods: vec![ModId::new("a")],
                    known_expansions: Vec::new(),
                },
                Vec::new(),
                Vec::new(),
            );
            let mut state = state_with(session);
            let opener = Arc::new(crate::test_support::RecordingLinkOpener::default());
            state.link_opener = opener.clone();

            open_mod_link_inner(
                &state,
                "a".to_string(),
                crate::dto::links::ModLinkKindDto::Workshop,
            )
            .await
            .expect("must succeed");

            assert_eq!(
                opener.calls(),
                vec!["https://steamcommunity.com/sharedfiles/filedetails/?id=42".to_string()]
            );
        }

        #[tokio::test]
        async fn open_mod_link_without_a_matching_link_is_invalid_input() {
            let state = state_with(session_fixture(&["a"]));

            let result = open_mod_link_inner(
                &state,
                "a".to_string(),
                crate::dto::links::ModLinkKindDto::Workshop,
            )
            .await;

            assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
        }

        #[tokio::test]
        async fn open_mod_link_unknown_mod_is_mod_not_found() {
            let state = state_with(session_fixture(&["a"]));

            let result = open_mod_link_inner(
                &state,
                "nobody".to_string(),
                crate::dto::links::ModLinkKindDto::Workshop,
            )
            .await;

            assert_eq!(result.unwrap_err().code, CommandErrorCode::ModNotFound);
        }
    }
}
