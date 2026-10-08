//! Use cases: one struct each, constructor-injected with the ports they
//! need (generics; `Arc<dyn Trait>` only at composition roots, per the
//! rust skill). Each wraps a [`crate::Session`] mutation with whatever
//! persistence the mutation requires.

mod activate_mods;
mod add_assignment_section;
mod apply;
mod assignment_coverage;
mod assignment_instances;
mod check_for_update;
mod clear_assignment_row;
mod contributes_nothing;
mod copy_from;
mod create_assignment;
mod create_patch;
mod deactivate_mods;
mod decide;
mod decide_merge;
mod decide_patch;
mod decide_patch_merge;
mod def_graphic;
mod def_sources;
mod delete_assignment;
mod delete_patch;
mod delete_rule;
mod export_assignment;
mod export_folder;
mod export_order;
mod export_patch;
mod find_def_cache_carrier;
mod get_recommended_rules;
mod import_game_log;
mod import_order;
mod import_profile_decisions;
mod import_rimsort;
mod inspect_def;
mod launch_game;
mod list_items;
mod load_project;
mod merge_coverage;
mod notifications;
mod plan_merge;
mod preflight_apply;
mod preview_order_import;
mod promote_imported_rule;
mod prune_patch_decisions;
mod read_mod_about;
mod read_mod_preview;
mod read_texture;
mod refresh_rule_databases;
mod remove_assignment_section;
mod render_merge_mod;
mod render_patch;
mod rescan;
mod revert_decision;
mod revert_patch_decision;
mod run_launch_network_checks;
mod select_order;
mod set_assignment_row;
mod set_manual_tag;
mod skip_recommended_rules;
mod update_app_settings;
mod update_assignment;
mod update_patch;
mod update_settings;
mod upsert_rule;
mod verify_order;

pub use activate_mods::ActivateMods;
pub use add_assignment_section::{AddAssignmentSection, AddAssignmentSectionError};
pub use apply::{Apply, ApplyError, ApplyOptions, ApplyOutcome};
pub use assignment_coverage::{AssignmentCoverage, AssignmentCoverageError};
pub use assignment_instances::{AssignmentInstances, AssignmentInstancesError};
pub use check_for_update::{
    CheckForUpdate, CheckForUpdateOutcome, UpdateCheckRequest, UpdateCheckRunOutcome,
    UpdateCheckSkipReason,
};
pub use clear_assignment_row::{ClearAssignmentRow, ClearAssignmentRowError};
pub use contributes_nothing::{
    ContributesNothing, ContributesNothingError, ContributesNothingReport,
};
pub use copy_from::{CopyFrom, CopyFromError, CopyFromOutcome, DroppedItemSlotValue};
pub use create_assignment::{
    AssignmentCandidate, CandidateSummary, CreateAssignment, CreateAssignmentError,
    CreateAssignmentInput, ExcludedTarget, ProposeAssignmentError,
};
pub use create_patch::{CreatePatch, CreatePatchError, CreatePatchInput};
pub use deactivate_mods::DeactivateMods;
pub use decide::{Decide, DecideError, NoRuleStore};
pub use decide_merge::{DecideMerge, DecideMergeError};
pub use decide_patch::{DecidePatch, DecidePatchError};
pub use decide_patch_merge::{DecidePatchMerge, DecidePatchMergeError};
pub use def_graphic::{
    Availability, DefGraphic, DefSubject, DefTexture, DefaultVariant, Face, Faces,
    GraphicEnvironment, GraphicModelError, GraphicSet, GraphicSlot, GraphicSpec, GraphicVariant,
    ImageSource, MAX_SLOTS, ReadDefTexture, ReadDefTextureError, ResolveDefGraphic,
    ResolveDefGraphicError, SlotFacts, SlotPlan, SlotSource, SlotSpec, TextureKey, TextureKeyError,
    TextureView, VariantLabel, VariantSpec, ViewRef, availability, build_set, plan_slots,
    resolve_def_graphic,
};
pub(crate) use def_sources::read_owner_def_raw;
pub use delete_assignment::{DeleteAssignment, DeleteAssignmentError};
pub use delete_patch::{DeletePatch, DeletePatchError};
pub use delete_rule::DeleteRule;
pub use export_assignment::{
    AssignmentExportOptions, AssignmentExportOutcome, AssignmentSkip, ExportAssignment,
    ExportAssignmentError,
};
pub use export_order::{ExportOrder, ExportOrderError, ExportSource};
pub use export_patch::{ExportOptions, ExportOutcome, ExportPatch, ExportPatchError};
pub use find_def_cache_carrier::FindDefCacheCarrier;
pub use get_recommended_rules::{
    FetchedRecommendedRules, GetRecommendedRules, GetRecommendedRulesError, ImportFailure,
    ImportStep, RecommendedRulesContext, RecommendedRulesProgress, RecommendedRulesReport,
};
pub use import_game_log::{
    AttributedClass, DdsFailureSummary, DependencyWarningSummary, EnclosingOp, FamilyAttribution,
    GameLogSummary, ImportGameLog, ImportGameLogError, LoadEvent, LogAttribution,
    PatchFailureSummary, StackTraceDetail, TimerSummary, UnpairedStackTrace,
};
pub use import_order::{ImportBlocker, ImportLoss, ImportOrder, ImportOrderError, ValidatedImport};
pub use import_profile_decisions::{
    ImportProfileDecisions, ImportProfileDecisionsError, ImportReport,
};
pub use import_rimsort::{ImportRimSort, ImportRimSortError};
pub use inspect_def::{
    DefInspection, InspectDef, InspectDefError, PatchOpSummary, Patcher, TemplateAmbiguity, Toucher,
};
pub use launch_game::{GameLaunchFacts, IfNotApplied, LaunchGame, LaunchGameError};
pub use list_items::{ListItem, ListItems, ListItemsFilter, ListItemsPage, item_types};
pub use load_project::{LoadProject, LoadProjectError};
pub use merge_coverage::{
    DefOverrideCoverage, MergeCoverage, MergeCoverageReport, PatchCollisionCoverage,
    PreviewStateTally, SuggestionOutcome, SuggestionOutcomeTally,
};
pub use notifications::{
    AcknowledgeGameVersion, CompleteWelcome, DismissNotification, ListNotifications,
    MuteNotificationKind, ProfileNotificationFacts, SyncGameVersionAcknowledgement,
    UnmuteNotificationKind,
};
pub(crate) use plan_merge::stored_choices;
pub use plan_merge::{MergeContext, PlanMerge, PlanMergeError};
pub use preflight_apply::{ApplyPreflight, PreflightApply};
pub use preview_order_import::{
    ImportPreview, ImportTarget, PreviewImportError, PreviewOrderImport, ReadyImport,
};
pub use promote_imported_rule::PromoteImportedRule;
pub use prune_patch_decisions::{PrunePatchDecisions, PrunePatchDecisionsError};
pub use read_mod_about::{
    AboutOutcome, AboutUnreadable, DescriptionText, ModInfoWithAbout, ModLinkKind, ReadModAbout,
};
pub use read_mod_preview::{ModPreview, PreviewUnreadable, ReadModPreview, ReadModPreviewError};
pub use read_texture::{ReadTexture, ReadTextureError, ReadTextureOutput};
pub use refresh_rule_databases::{RefreshRuleDatabases, RuleDatabaseView};
pub use remove_assignment_section::{RemoveAssignmentSection, RemoveAssignmentSectionError};
pub use render_merge_mod::{
    MergeEntryKind, MergeModEntry, MergeModRender, RenderMergeMod, RenderMergeModError,
    RenderTarget,
};
pub use render_patch::{RenderPatch, RenderPatchError};
pub use rescan::Rescan;
pub use revert_decision::RevertDecision;
pub use revert_patch_decision::{RevertPatchDecision, RevertPatchDecisionError};
pub use run_launch_network_checks::{LaunchNetworkChecksOutcome, RunLaunchNetworkChecks};
pub use select_order::SelectOrder;
pub use set_assignment_row::{SetAssignmentRow, SetAssignmentRowError};
pub use set_manual_tag::SetManualTag;
pub use skip_recommended_rules::SkipRecommendedRules;
pub use update_app_settings::{EnableRecommendedSources, ResetNetworkPolicy, UpdateAppSettings};
pub use update_assignment::{
    SchemaChange, UpdateAssignment, UpdateAssignmentError, UpdateAssignmentInput,
    UpdateAssignmentOutcome,
};
pub use update_patch::{UpdatePatch, UpdatePatchError, UpdatePatchInput, UpdatePatchOutcome};
pub use update_settings::{ResetSettings, UpdateSettings};
pub use upsert_rule::UpsertRule;
pub use verify_order::{
    CounterfactualStats, ReorderConflict, ReorderConflictDirection, VerifyOptions, VerifyOrder,
    VerifyOrderReport, reorder_conflicts,
};
