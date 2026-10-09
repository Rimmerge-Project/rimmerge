//! `rim-session`: the application layer for Rimmerge.
//!
//! Owns the [`Session`] aggregate (one loaded project's full state), the
//! use cases that mutate it, and the ports (traits) infrastructure
//! adapters implement — scanning, config/decision/rule persistence, and
//! RimSort import. No filesystem, no XML, no Tauri: every side effect
//! happens behind a port, injected by the composition root (a CLI or
//! Tauri command).

mod active_set;
pub mod app_link;
pub mod app_settings;
mod assignment_refs;
mod changes;
mod finding_index;
mod game_launch;
mod import_sources;
mod merge_workspace;
mod mod_index;
pub mod mod_info;
mod mod_inventory;
mod mod_knowledge;
pub mod mod_list;
mod paths;
mod recommended_rules;
mod replay_pool;
mod session;
mod settings;

pub mod def_conflict_view;
pub mod effective_fields;
pub mod notifications;
pub mod ports;
pub mod use_cases;

pub use active_set::{
    ActivatePlan, ActiveSet, ActiveSetDiff, ActiveSetError, DeactivatePlan, PendingActiveChanges,
    plan_activate, plan_deactivate,
};
pub use app_settings::{
    AppSettings, NetworkPolicy, ReminderPolicy, StaleAfterDays, StaleAfterDaysError,
};
pub use changes::{AssetKind, ChangeFilter, ChangeKind, ChangePage, ChangeRow};
pub use def_conflict_view::{
    DefConflictKind, DefConflictView, DefConflictViewError, FieldRow, FieldRowKind,
    InjectedNodeRelation, Preference, Problem, Toucher as DefConflictToucher, ToucherRole,
};
pub use finding_index::{FindingFilter, FindingIndex, FindingKind, FindingPage, MAX_PAGE_SIZE};
pub use game_launch::{
    GameLaunchStatus, GameProcess, LaunchRouteKind, LaunchUnavailable, OrderOnDisk,
    UnappliedReason, game_launch_status,
};
pub use import_sources::{resolve_from_cache, resolve_from_rimsort_dir, should_import_from_cache};
pub use merge_workspace::{
    MergeFieldFilter, MergeFieldPage, MergeFieldTotals, MergePreview, PreviewSlot,
};
pub use mod_index::{ModFilter, ModPage, ModSummary};
pub use mod_inventory::{InventoryEntry, ModInventory};
pub use mod_knowledge::ModKnowledge;
pub use paths::ProjectPaths;
pub use recommended_rules::{
    FirstRun, RecommendedRulesFacts, RecommendedRulesStep, SourceNeed, StepSkip, Unavailable,
    recommended_rules_step,
};
pub use session::{PatchDecideError, RuleKey, Session, UnknownAssignment, UnknownPatch};
pub use settings::{Settings, SortProvenance, filter_imported_rules};

#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
