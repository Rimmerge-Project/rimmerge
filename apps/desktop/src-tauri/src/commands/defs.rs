//! `list_mod_changes`/`inspect_def`/`search_defs`: the def inspector's
//! commands. All three are
//! read-only — none of them persist anything or emit `session://changed`.

use std::str::FromStr;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::DefRef;
use rim_session::use_cases::InspectDef;

use crate::dto::defs::{
    ChangeFilterDto, ChangePageDto, DefInspectionDto, DefSearchHitDto, InspectDefRequestDto,
    build_def_inspection_dto,
};
use crate::error::CommandError;
use crate::state::{AppState, with_session};

/// Lists what `mod_id` changes: the defs it owns, the templates it
/// registers, the foreign defs it patches, and the assets it overrides.
/// Filtering/paging/aggregation is `rim_session::Session::changes`'s own
/// job; this only maps the DTO in and out.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn list_mod_changes_inner(
    state: &AppState,
    mod_id: String,
    filter: ChangeFilterDto,
) -> Result<ChangePageDto, CommandError> {
    with_session(state, move |session| {
        let target = ModId::new(&mod_id);
        let page = session.changes(&target, &(&filter).into());
        Ok((&page).into())
    })
    .await
}

/// See [`list_mod_changes_inner`].
///
/// # Errors
///
/// See [`list_mod_changes_inner`].
#[tauri::command]
pub async fn list_mod_changes(
    state: tauri::State<'_, AppState>,
    mod_id: String,
    filter: ChangeFilterDto,
) -> Result<ChangePageDto, CommandError> {
    list_mod_changes_inner(&state, mod_id, filter).await
}

/// Inspects one def or `Name`-attributed template under the selected
/// order: every owner and patcher, the template chain, and the effective
/// def the game actually runs. Reuses `rim_session::use_cases::InspectDef`
/// (and its own per-`(source, defRef)` cache) directly.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded,
/// [`CommandError::invalid_input`] when `request.def_ref` doesn't parse,
/// [`CommandError::def_not_found`] when it doesn't name anything this scan
/// indexed, or a `MergeSourceFailed` [`CommandError`] when the
/// def/template/patch source changed since the scan.
pub(crate) async fn inspect_def_inner(
    state: &AppState,
    request: InspectDefRequestDto,
) -> Result<DefInspectionDto, CommandError> {
    let adapters = state.adapters.clone();
    with_session(state, move |session| {
        let def_ref = DefRef::from_str(&request.def_ref)?;
        let use_case = InspectDef::new(adapters.def_reader);
        let inspection = use_case.execute(session, &def_ref)?;
        Ok(build_def_inspection_dto(inspection, &request.filter))
    })
    .await
}

/// See [`inspect_def_inner`].
///
/// # Errors
///
/// See [`inspect_def_inner`].
#[tauri::command]
pub async fn inspect_def(
    state: tauri::State<'_, AppState>,
    request: InspectDefRequestDto,
) -> Result<DefInspectionDto, CommandError> {
    inspect_def_inner(&state, request).await
}

/// Searches every def and `Name`-attributed template the scan indexed,
/// ranked name matches before type-only matches, capped at
/// `rim_session::MAX_PAGE_SIZE`. `rim_session::Session::search_defs` does
/// the actual ranking/filtering; this only maps the DTO out.
///
/// # Errors
///
/// Returns [`CommandError::no_project_loaded`] when no project is loaded.
pub(crate) async fn search_defs_inner(
    state: &AppState,
    query: String,
    limit: usize,
) -> Result<Vec<DefSearchHitDto>, CommandError> {
    with_session(state, move |session| {
        Ok(session
            .search_defs(&query, limit)
            .into_iter()
            .map(|(def_ref, owners)| DefSearchHitDto {
                def_ref: def_ref.to_string(),
                owners,
            })
            .collect())
    })
    .await
}

/// See [`search_defs_inner`].
///
/// # Errors
///
/// See [`search_defs_inner`].
#[tauri::command]
pub async fn search_defs(
    state: tauri::State<'_, AppState>,
    query: String,
    limit: usize,
) -> Result<Vec<DefSearchHitDto>, CommandError> {
    search_defs_inner(&state, query, limit).await
}

#[cfg(test)]
mod tests {
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{DefEntry, TemplateEntry};
    use rim_resolve::test_support::ReportBuilder;
    use rim_session::test_support::{
        InMemoryDefSourceReader, bionic_heart_fixture, session_with_sources_and_mods,
    };

    use super::*;
    use crate::dto::defs::EffectiveFieldFilterDto;
    use crate::error::CommandErrorCode;
    use crate::test_support::session_with_temp_paths;

    fn state_with(session: rim_session::Session) -> AppState {
        let state = AppState::default();
        *state.session.write().expect("lock") = Some(session);
        state
    }

    fn def_entry(def_type: &str, def_name: &str) -> DefEntry {
        DefEntry {
            def_type: def_type.to_string(),
            def_name: def_name.to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: rim_analyzer::domain::XmlLocator::new(
                std::path::PathBuf::from(format!("{def_type}_{def_name}.xml")).into(),
                vec![0],
            ),
        }
    }

    /// A mod owning `ThingDef/Wall`, so `list_mod_changes` has a real
    /// `OwnsDef` row to find. `session_with_temp_paths`'s `TempDir` is
    /// dropped immediately: none of `changes`/`search_defs` touch disk
    /// again once the session is built (both are pure over already-loaded
    /// `Report`/`SourceIndex` data).
    fn owner_state() -> AppState {
        let mut sources = SourceIndex::default();
        let owner = ModId::new("owner.mod");
        let key = ("ThingDef".to_string(), "Wall".to_string());
        sources.defs.insert(
            (owner.clone(), key.clone()),
            vec![def_entry("ThingDef", "Wall")],
        );
        sources.owners_by_def.insert(key, vec![owner]);
        let report = ReportBuilder::new().mod_("owner.mod").build();
        let (_temp_dir, session) = session_with_temp_paths(report, sources, &["owner.mod"]);
        state_with(session)
    }

    #[tokio::test]
    async fn list_mod_changes_returns_an_owns_def_row() {
        let state = owner_state();

        let page = list_mod_changes_inner(
            &state,
            "owner.mod".to_string(),
            ChangeFilterDto {
                limit: 10,
                ..ChangeFilterDto::default()
            },
        )
        .await
        .expect("list_mod_changes must succeed");

        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].def_ref.as_deref(), Some("ThingDef/Wall"));
    }

    #[tokio::test]
    async fn search_defs_finds_the_indexed_def() {
        let state = owner_state();

        let hits = search_defs_inner(&state, "wall".to_string(), 10)
            .await
            .expect("search_defs must succeed");

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].def_ref, "ThingDef/Wall");
        assert_eq!(hits[0].owners, 1);
    }

    #[tokio::test]
    async fn inspect_def_rejects_an_unparseable_ref() {
        let state = state_with(rim_session::test_support::session_fixture(&["a"]));

        let result = inspect_def_inner(
            &state,
            InspectDefRequestDto {
                def_ref: "not a valid ref".to_string(),
                filter: EffectiveFieldFilterDto::default(),
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn inspect_def_reports_def_not_found_for_an_unknown_ref() {
        let state = state_with(rim_session::test_support::session_fixture(&["a"]));

        let result = inspect_def_inner(
            &state,
            InspectDefRequestDto {
                def_ref: "ThingDef/DoesNotExist".to_string(),
                filter: EffectiveFieldFilterDto::default(),
            },
        )
        .await;

        assert_eq!(result.unwrap_err().code, CommandErrorCode::DefNotFound);
    }

    #[tokio::test]
    async fn inspect_def_round_trips_the_bionic_heart_fixture() {
        let fixture = bionic_heart_fixture();
        let (_temp_dir, session) = session_with_temp_paths(
            fixture.report,
            fixture.sources,
            &["ludeon.rimworld", "example.bionicsfork"],
        );
        let mut state = AppState::default();
        state.adapters.def_reader = std::sync::Arc::new(fixture.reader);
        *state.session.write().expect("lock") = Some(session);

        let dto = inspect_def_inner(
            &state,
            InspectDefRequestDto {
                def_ref: "HediffDef/BionicHeart".to_string(),
                filter: EffectiveFieldFilterDto {
                    only_patched: false,
                    offset: 0,
                    limit: 200,
                },
            },
        )
        .await
        .expect("a real fixture def must inspect");

        assert_eq!(dto.def_ref, "HediffDef/BionicHeart");
        assert_eq!(dto.winner, "example.bionicsfork");
        assert!(
            dto.fields.iter().any(|f| f.path == "label"),
            "the contested label field must appear: {:?}",
            dto.fields
        );
        assert!(dto.resolved_xml.contains("HediffDef"));
    }

    /// A template's direct child that is itself another abstract template
    /// (no `defName` of its own — `rim-session`'s
    /// `DefInspection.children: Vec<(ModId, DefRef)>`) must round-trip as
    /// a `Selector::NameAttr` link (`ThingDef/@MidBase`), never `DefName`
    /// (`ThingDef/MidBase`, which would 404 on the def page as a dead link).
    #[tokio::test]
    async fn inspect_def_links_an_abstract_template_child_with_a_name_attr_ref() {
        let owner = ModId::new("owner.mod");
        let mid = ModId::new("mid.mod");
        let base_locator = rim_analyzer::domain::XmlLocator::new(
            std::path::PathBuf::from("owner_base_template.xml").into(),
            vec![0],
        );
        let mid_locator = rim_analyzer::domain::XmlLocator::new(
            std::path::PathBuf::from("mid_base_template.xml").into(),
            vec![0],
        );

        let mut sources = SourceIndex::default();
        sources.templates.insert(
            ("ThingDef".to_string(), "Base".to_string()),
            vec![(
                owner.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "Base".to_string(),
                    parent_name: None,
                    is_abstract: true,
                    locator: base_locator.clone(),
                },
            )],
        );
        sources.templates.insert(
            ("ThingDef".to_string(), "MidBase".to_string()),
            vec![(
                mid.clone(),
                TemplateEntry {
                    graphic_class: None,
                    may_require: Vec::new(),
                    def_type: "ThingDef".to_string(),
                    name: "MidBase".to_string(),
                    parent_name: Some("Base".to_string()),
                    is_abstract: true,
                    locator: mid_locator.clone(),
                },
            )],
        );
        sources.children_by_template.insert(
            ("ThingDef".to_string(), "Base".to_string()),
            vec![(mid.clone(), ("ThingDef".to_string(), "MidBase".to_string()))],
        );
        // `MidBase` is deliberately absent from `owners_by_def`: it has no
        // `defName` of its own, only a `Name` — the whole point of this
        // fixture.

        let mut elements = std::collections::BTreeMap::new();
        elements.insert(base_locator,
            "<ThingDef Name=\"Base\"><statBases><MaxHitPoints>100</MaxHitPoints></statBases></ThingDef>"
                .to_string());
        elements.insert(
            mid_locator,
            "<ThingDef Name=\"MidBase\" ParentName=\"Base\"></ThingDef>".to_string(),
        );
        let report = ReportBuilder::new()
            .mod_("owner.mod")
            .mod_("mid.mod")
            .build();
        let session = session_with_sources_and_mods(sources, report, &["owner.mod", "mid.mod"]);
        let mut state = AppState::default();
        state.adapters.def_reader = std::sync::Arc::new(InMemoryDefSourceReader::new(elements));
        *state.session.write().expect("lock") = Some(session);

        let dto = inspect_def_inner(
            &state,
            InspectDefRequestDto {
                def_ref: "ThingDef/@Base".to_string(),
                filter: EffectiveFieldFilterDto::default(),
            },
        )
        .await
        .expect("the abstract Base template must inspect");

        assert_eq!(dto.children.len(), 1);
        assert_eq!(dto.children[0].def_ref, "ThingDef/@MidBase");
        assert_eq!(dto.children[0].owner, "mid.mod");
    }
}
