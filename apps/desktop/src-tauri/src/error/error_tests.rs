//! Tests for the command error mappings.

use super::*;

#[test]
fn scan_error_maps_to_scan_failed() {
    let error: CommandError = rim_session::ports::ScanError("boom".to_string()).into();
    assert_eq!(error.code, CommandErrorCode::ScanFailed);
}

#[test]
fn config_error_maps_to_mods_config_io_failed() {
    let error: CommandError = rim_session::ports::ConfigError("boom".to_string()).into();
    assert_eq!(error.code, CommandErrorCode::ModsConfigIoFailed);
}

#[test]
fn store_error_maps_to_profile_io_failed() {
    let error: CommandError = rim_session::ports::StoreError("boom".to_string()).into();
    assert_eq!(error.code, CommandErrorCode::ProfileIoFailed);
}

#[test]
fn import_error_maps_to_rimsort_import_failed() {
    let error: CommandError = rim_session::ports::ImportError("boom".to_string()).into();
    assert_eq!(error.code, CommandErrorCode::RimsortImportFailed);
}

#[test]
fn load_project_error_delegates_to_its_inner_error_code() {
    let error: CommandError = rim_session::use_cases::LoadProjectError::Scan(
        rim_session::ports::ScanError("boom".to_string()),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::ScanFailed);
}

#[test]
fn decide_error_store_maps_to_profile_io_failed() {
    let error: CommandError = rim_session::use_cases::DecideError::Store(
        rim_session::ports::StoreError("boom".to_string()),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::ProfileIoFailed);
}

#[test]
fn decide_error_promote_needs_rule_store_maps_to_internal() {
    let error: CommandError = rim_session::use_cases::DecideError::PromoteNeedsRuleStore.into();
    assert_eq!(error.code, CommandErrorCode::Internal);
}

#[test]
fn import_rimsort_error_import_maps_to_rimsort_import_failed() {
    let error: CommandError = rim_session::use_cases::ImportRimSortError::Import(
        rim_session::ports::ImportError("boom".to_string()),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::RimsortImportFailed);
}

/// A manifest-write failure must never claim the import
/// itself failed — the rules were saved successfully at that point,
/// which is exactly the fact `Store`'s own generic "saving imported
/// rules: {0}" message would misreport if it were reused here.
#[test]
fn import_rimsort_error_manifest_says_the_rules_were_saved_not_that_the_import_failed() {
    let error: CommandError = rim_session::use_cases::ImportRimSortError::Manifest(
        rim_session::ports::StoreError("boom".to_string()),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::ProfileIoFailed);
    assert!(
        error.message.contains("saved successfully"),
        "the message must say the rules were saved, not that the import failed: {}",
        error.message
    );
}

#[test]
fn import_rimsort_error_store_maps_to_profile_io_failed() {
    let error: CommandError = rim_session::use_cases::ImportRimSortError::Store(
        rim_session::ports::StoreError("boom".to_string()),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::ProfileIoFailed);
}

#[test]
fn apply_error_config_maps_to_mods_config_io_failed() {
    let error: CommandError = rim_session::use_cases::ApplyError::Config(
        rim_session::ports::ConfigError("boom".to_string()),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::ModsConfigIoFailed);
}

#[test]
fn apply_error_decisions_maps_to_profile_io_failed() {
    let error: CommandError = rim_session::use_cases::ApplyError::Decisions(
        rim_session::ports::StoreError("boom".to_string()),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::ProfileIoFailed);
}

#[test]
fn apply_error_asset_maps_to_merge_source_failed() {
    let error: CommandError =
        rim_session::use_cases::ApplyError::Asset(rim_session::ports::DefSourceError::Io {
            file: "Textures/Foo.png".into(),
            message: "boom".to_string(),
        })
        .into();
    assert_eq!(error.code, CommandErrorCode::MergeSourceFailed);
}

#[test]
fn apply_error_merge_mod_maps_to_merge_mod_io_failed() {
    let error: CommandError = rim_session::use_cases::ApplyError::MergeMod(
        rim_session::ports::MergeModError("boom".to_string()),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::MergeModIoFailed);
}

#[test]
fn apply_error_stale_active_set_maps_to_stale_active_set_and_names_the_counts() {
    let error: CommandError = rim_session::use_cases::ApplyError::StaleActiveSet {
        added: 2,
        removed: 1,
    }
    .into();
    assert_eq!(error.code, CommandErrorCode::StaleActiveSet);
    assert!(error.message.contains('2'));
    assert!(error.message.contains('1'));
}

#[test]
fn active_set_error_maps_to_invalid_input() {
    let error: CommandError =
        rim_session::ActiveSetError::Unknown(rim_analyzer::domain::ModId::new("ghost")).into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
    assert!(error.message.contains("ghost"));
}

#[test]
fn render_merge_mod_error_asset_maps_to_merge_source_failed() {
    let error: CommandError = rim_session::use_cases::RenderMergeModError::Asset(
        rim_session::ports::DefSourceError::Io {
            file: "Textures/Foo.png".into(),
            message: "boom".to_string(),
        },
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::MergeSourceFailed);
}

#[test]
fn plan_merge_error_unsupported_finding_maps_to_invalid_input() {
    let error: CommandError = rim_session::use_cases::PlanMergeError::UnsupportedFinding(
        rim_resolve::domain::FindingKey::MissingMod {
            mod_id: rim_analyzer::domain::ModId::new("a.mod"),
        },
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn plan_merge_error_source_family_maps_to_merge_source_failed() {
    let source_error: CommandError =
        rim_session::use_cases::PlanMergeError::Source(rim_session::ports::DefSourceError::Io {
            file: "defs.xml".into(),
            message: "boom".to_string(),
        })
        .into();
    assert_eq!(source_error.code, CommandErrorCode::MergeSourceFailed);

    let inherit_error: CommandError = rim_session::use_cases::PlanMergeError::Inherit(
        rim_merge::inherit::InheritError::MissingParent {
            def_type: "HediffDef".to_string(),
            name: "Missing".to_string(),
        },
    )
    .into();
    assert_eq!(inherit_error.code, CommandErrorCode::MergeSourceFailed);

    let xml_error: CommandError =
        rim_session::use_cases::PlanMergeError::Xml("bad xml".to_string()).into();
    assert_eq!(xml_error.code, CommandErrorCode::MergeSourceFailed);

    let missing_source_error: CommandError =
        rim_session::use_cases::PlanMergeError::MissingSource("boom".to_string()).into();
    assert_eq!(
        missing_source_error.code,
        CommandErrorCode::MergeSourceFailed
    );
}

#[test]
fn decide_merge_error_unknown_field_and_owner_map_to_invalid_input() {
    let unknown_field: CommandError = rim_session::use_cases::DecideMergeError::UnknownField(
        "label".parse().expect("valid field path"),
    )
    .into();
    assert_eq!(unknown_field.code, CommandErrorCode::InvalidInput);

    let unknown_owner: CommandError = rim_session::use_cases::DecideMergeError::UnknownOwner(
        rim_analyzer::domain::ModId::new("not.an.owner"),
    )
    .into();
    assert_eq!(unknown_owner.code, CommandErrorCode::InvalidInput);
}

#[test]
fn decide_merge_error_plan_unsupported_finding_maps_to_invalid_input() {
    let error: CommandError = rim_session::use_cases::DecideMergeError::Plan(
        rim_session::use_cases::PlanMergeError::UnsupportedFinding(
            rim_resolve::domain::FindingKey::MissingMod {
                mod_id: rim_analyzer::domain::ModId::new("a.mod"),
            },
        ),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn decide_merge_error_store_maps_to_profile_io_failed() {
    let error: CommandError = rim_session::use_cases::DecideMergeError::Store(
        rim_session::ports::StoreError("boom".to_string()),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::ProfileIoFailed);
}

#[test]
fn read_texture_error_unknown_mod_and_texture_not_found_map_to_invalid_input() {
    let unknown_mod: CommandError = rim_session::use_cases::ReadTextureError::UnknownMod(
        rim_analyzer::domain::ModId::new("not.active"),
    )
    .into();
    assert_eq!(unknown_mod.code, CommandErrorCode::InvalidInput);

    let not_found: CommandError = rim_session::use_cases::ReadTextureError::TextureNotFound {
        mod_id: rim_analyzer::domain::ModId::new("a.mod"),
        texture_path: "things/wall".to_string(),
    }
    .into();
    assert_eq!(not_found.code, CommandErrorCode::InvalidInput);
}

#[test]
fn read_texture_error_too_large_is_invalid_input_and_unsupported_format_has_its_own_code() {
    let too_large: CommandError = rim_session::use_cases::ReadTextureError::Asset(
        rim_session::ports::DefSourceError::TooLarge {
            file: "wall.png".into(),
            max_bytes: 8 * 1024 * 1024,
            actual_bytes: 9 * 1024 * 1024,
        },
    )
    .into();
    assert_eq!(too_large.code, CommandErrorCode::InvalidInput);

    let unsupported: CommandError = rim_session::use_cases::ReadTextureError::Asset(
        rim_session::ports::DefSourceError::UnsupportedFormat {
            file: "wall.bmp".into(),
        },
    )
    .into();
    assert_eq!(unsupported.code, CommandErrorCode::TextureUnsupportedFormat);
}

#[test]
fn resolve_def_graphic_inspect_errors_map_like_inspect_def() {
    use rim_analyzer::domain::Selector;
    use rim_resolve::domain::{DefKey, DefRef};
    use rim_session::use_cases::{InspectDefError, ResolveDefGraphicError};

    let def_ref = DefRef::new(
        DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "X".to_string(),
        },
        Selector::DefName,
    );
    let not_found: CommandError =
        ResolveDefGraphicError::Inspect(InspectDefError::NotFound(def_ref)).into();
    assert_eq!(not_found.code, CommandErrorCode::DefNotFound);

    let xml: CommandError =
        ResolveDefGraphicError::Inspect(InspectDefError::Xml("bad".to_string())).into();
    assert_eq!(xml.code, CommandErrorCode::MergeSourceFailed);
}

#[test]
fn read_def_texture_errors_map_by_code() {
    use rim_analyzer::domain::ModId;
    use rim_session::ports::DefSourceError;
    use rim_session::use_cases::{ReadDefTextureError, TextureKey};

    let key = TextureKey::parse("things/x").expect("valid key");
    let forged: CommandError = ReadDefTextureError::KeyNotInGraphic(key).into();
    assert_eq!(forged.code, CommandErrorCode::InvalidInput);

    let owner: CommandError = ReadDefTextureError::UnknownOwner(ModId::new("gone")).into();
    assert_eq!(owner.code, CommandErrorCode::ModNotFound);

    let unsupported: CommandError =
        ReadDefTextureError::Locate(DefSourceError::UnsupportedFormat {
            file: "wall.dds".into(),
        })
        .into();
    assert_eq!(unsupported.code, CommandErrorCode::TextureUnsupportedFormat);

    let io: CommandError = ReadDefTextureError::Locate(DefSourceError::Io {
        file: "wall.png".into(),
        message: "boom".to_string(),
    })
    .into();
    assert_eq!(io.code, CommandErrorCode::MergeSourceFailed);

    // A file over the texture cap is a bad request, not an I/O failure —
    // the same code `read_texture` gives it.
    let too_large: CommandError = ReadDefTextureError::Locate(DefSourceError::TooLarge {
        file: "wall.png".into(),
        max_bytes: 8 * 1024 * 1024,
        actual_bytes: 9 * 1024 * 1024,
    })
    .into();
    assert_eq!(too_large.code, CommandErrorCode::InvalidInput);
}

#[test]
fn read_texture_error_io_maps_to_merge_source_failed() {
    let error: CommandError =
        rim_session::use_cases::ReadTextureError::Asset(rim_session::ports::DefSourceError::Io {
            file: "wall.png".into(),
            message: "boom".to_string(),
        })
        .into();
    assert_eq!(error.code, CommandErrorCode::MergeSourceFailed);
}

#[test]
fn finding_key_parse_error_maps_to_invalid_input() {
    use std::str::FromStr;
    let parse_error = rim_resolve::domain::FindingKey::from_str("not a valid key")
        .expect_err("must fail to parse");
    let error: CommandError = parse_error.into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn tag_error_maps_to_invalid_input() {
    let error: CommandError = rim_resolve::domain::Tag::new("Not Valid")
        .expect_err("must fail to validate")
        .into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn cluster_rule_id_error_maps_to_invalid_input() {
    let error: CommandError = rim_resolve::domain::ClusterRuleId::new("Not Valid")
        .expect_err("must fail to validate")
        .into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn confidence_error_maps_to_invalid_input() {
    let error: CommandError = rim_resolve::domain::Confidence::new(101)
        .expect_err("must fail to validate")
        .into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn session_lost_and_mod_not_found_and_finding_not_found_carry_their_own_codes() {
    assert_eq!(
        CommandError::session_lost().code,
        CommandErrorCode::SessionLost
    );
    assert_eq!(
        CommandError::mod_not_found("x").code,
        CommandErrorCode::ModNotFound
    );
    assert_eq!(
        CommandError::finding_not_found("x").code,
        CommandErrorCode::FindingNotFound
    );
    assert_eq!(
        CommandError::rimworld_running().code,
        CommandErrorCode::RimworldRunning
    );
    assert_eq!(
        CommandError::patch_not_found("x").code,
        CommandErrorCode::PatchNotFound
    );
    assert_eq!(
        CommandError::patch_identity_invalid("x").code,
        CommandErrorCode::PatchIdentityInvalid
    );
}

#[test]
fn unknown_patch_maps_to_patch_not_found() {
    let id: rim_resolve::domain::PatchId = "3f9a1c02be77".parse().expect("valid patch id");
    let error: CommandError = rim_session::UnknownPatch(id).into();
    assert_eq!(error.code, CommandErrorCode::PatchNotFound);
}

#[test]
fn create_patch_error_package_id_taken_maps_to_patch_identity_invalid() {
    let error: CommandError = rim_session::use_cases::CreatePatchError::PackageIdTaken(
        rim_analyzer::domain::ModId::new("vendor.moda"),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::PatchIdentityInvalid);
}

#[test]
fn create_patch_error_inactive_scope_member_maps_to_invalid_input() {
    let error: CommandError = rim_session::use_cases::CreatePatchError::InactiveScopeMember(
        rim_analyzer::domain::ModId::new("gone.mod"),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn export_patch_error_out_dir_is_mods_folder_maps_to_invalid_input_with_the_path() {
    let path = std::path::PathBuf::from("C:/RimWorld/Mods/some_folder");
    let error: CommandError =
        rim_session::use_cases::ExportPatchError::OutDirIsModsFolder(path.clone()).into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
    assert!(error.message.contains("Mods"));
}

#[test]
fn export_patch_error_unknown_maps_to_patch_not_found() {
    let id: rim_resolve::domain::PatchId = "3f9a1c02be77".parse().expect("valid patch id");
    let error: CommandError =
        rim_session::use_cases::ExportPatchError::Unknown(rim_session::UnknownPatch(id)).into();
    assert_eq!(error.code, CommandErrorCode::PatchNotFound);
}

#[test]
fn export_patch_error_nothing_to_export_maps_to_invalid_input() {
    let error: CommandError = rim_session::use_cases::ExportPatchError::NothingToExport.into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
    assert!(
        error
            .message
            .contains("has no complete merge or located asset")
    );
}

#[test]
fn export_patch_error_foreign_folder_maps_to_invalid_input_with_the_path() {
    let path = std::path::PathBuf::from("C:/exports/someone_elses_mod");
    let error: CommandError =
        rim_session::use_cases::ExportPatchError::ForeignFolder { path: path.clone() }.into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
    assert!(error.message.contains("someone_elses_mod"));
}

#[test]
fn inspect_def_error_not_found_maps_to_def_not_found() {
    use std::str::FromStr;
    let def_ref = rim_resolve::domain::DefRef::from_str("ThingDef/Wall").expect("valid ref");
    let error: CommandError = rim_session::use_cases::InspectDefError::NotFound(def_ref).into();
    assert_eq!(error.code, CommandErrorCode::DefNotFound);
}

#[test]
fn inspect_def_error_source_and_xml_map_to_merge_source_failed() {
    let source_error: CommandError =
        rim_session::use_cases::InspectDefError::Source(rim_session::ports::DefSourceError::Io {
            file: "defs.xml".into(),
            message: "boom".to_string(),
        })
        .into();
    assert_eq!(source_error.code, CommandErrorCode::MergeSourceFailed);

    let xml_error: CommandError =
        rim_session::use_cases::InspectDefError::Xml("bad xml".to_string()).into();
    assert_eq!(xml_error.code, CommandErrorCode::MergeSourceFailed);
}

#[test]
fn def_conflict_view_error_unsupported_finding_maps_to_invalid_input() {
    let error: CommandError = rim_session::DefConflictViewError::UnsupportedFinding(
        rim_resolve::domain::FindingKey::MissingMod {
            mod_id: rim_analyzer::domain::ModId::new("a.mod"),
        },
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn unknown_assignment_maps_to_assignment_not_found() {
    let id: rim_resolve::domain::AssignmentId =
        "3f9a1c02be77".parse().expect("valid assignment id");
    let error: CommandError = rim_session::UnknownAssignment(id).into();
    assert_eq!(error.code, CommandErrorCode::AssignmentNotFound);
}

#[test]
fn create_assignment_error_package_id_taken_maps_to_patch_identity_invalid() {
    let error: CommandError = rim_session::use_cases::CreateAssignmentError::PackageIdTaken(
        rim_analyzer::domain::ModId::new("vendor.moda"),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::PatchIdentityInvalid);
}

#[test]
fn create_assignment_error_empty_refs_and_targets_map_to_invalid_input() {
    let empty_refs: CommandError = rim_session::use_cases::CreateAssignmentError::EmptyRefs.into();
    assert_eq!(empty_refs.code, CommandErrorCode::InvalidInput);
    let empty_targets: CommandError =
        rim_session::use_cases::CreateAssignmentError::EmptyTargets.into();
    assert_eq!(empty_targets.code, CommandErrorCode::InvalidInput);
}

#[test]
fn export_assignment_error_nothing_to_export_maps_to_invalid_input() {
    let error: CommandError = rim_session::use_cases::ExportAssignmentError::NothingToExport.into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn export_assignment_error_unknown_maps_to_assignment_not_found() {
    let id: rim_resolve::domain::AssignmentId =
        "3f9a1c02be77".parse().expect("valid assignment id");
    let error: CommandError =
        rim_session::use_cases::ExportAssignmentError::Unknown(rim_session::UnknownAssignment(id))
            .into();
    assert_eq!(error.code, CommandErrorCode::AssignmentNotFound);
}

#[test]
fn assignment_row_error_maps_to_invalid_input() {
    let error: CommandError = rim_resolve::domain::AssignmentRowError::EmptyDefName.into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn section_error_already_exists_maps_to_invalid_input() {
    let error: CommandError =
        rim_resolve::domain::SectionError::AlreadyExists("example.PartAssignmentDef".to_string())
            .into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn section_error_section_in_use_carries_structured_detail() {
    let target = rim_resolve::domain::TargetRef {
        key_field: "speciesNames".parse().expect("valid path"),
        def: rim_resolve::domain::DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Race0".to_string(),
        },
    };
    let error: CommandError = rim_resolve::domain::SectionError::SectionInUse {
        referenced_by: vec![(
            "example.PartAssignmentDef".to_string(),
            rim_resolve::domain::RowKey::Target(target),
            "parts".parse().expect("valid path"),
        )],
    }
    .into();

    assert_eq!(error.code, CommandErrorCode::AssignmentSectionInUse);
    let referenced_by = match error.detail.expect("detail must be present") {
        CommandErrorDetail::AssignmentSectionInUse { referenced_by } => referenced_by,
    };
    assert_eq!(referenced_by.len(), 1);
    assert_eq!(referenced_by[0].def_type, "example.PartAssignmentDef");
    assert_eq!(referenced_by[0].row, "ThingDef/Race0");
    assert_eq!(referenced_by[0].path, "parts");
}

#[test]
fn add_assignment_section_error_not_owned_by_refs_maps_to_invalid_input() {
    let error: CommandError = rim_session::use_cases::AddAssignmentSectionError::NotOwnedByRefs(
        "nobody.owns.This".to_string(),
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::InvalidInput);
}

#[test]
fn remove_assignment_section_error_section_in_use_maps_through() {
    let error: CommandError = rim_session::use_cases::RemoveAssignmentSectionError::Section(
        rim_resolve::domain::SectionError::SectionInUse {
            referenced_by: Vec::new(),
        },
    )
    .into();
    assert_eq!(error.code, CommandErrorCode::AssignmentSectionInUse);
}

#[test]
fn def_conflict_view_error_not_inspected_and_not_planned_map_to_internal() {
    use std::str::FromStr;
    let def_ref = rim_resolve::domain::DefRef::from_str("ThingDef/Wall").expect("valid ref");
    let key = rim_resolve::domain::FindingKey::MissingMod {
        mod_id: rim_analyzer::domain::ModId::new("a.mod"),
    };

    let not_inspected: CommandError =
        rim_session::DefConflictViewError::NotInspected(def_ref).into();
    assert_eq!(not_inspected.code, CommandErrorCode::Internal);

    let not_planned: CommandError = rim_session::DefConflictViewError::NotPlanned(key).into();
    assert_eq!(not_planned.code, CommandErrorCode::Internal);
}
