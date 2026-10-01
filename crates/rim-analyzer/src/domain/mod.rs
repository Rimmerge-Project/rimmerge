//! Domain types: pure data and value objects with no filesystem or XML
//! dependency. Everything here is constructed by `extract`/`infra` and
//! consumed by `analysis`.

mod assembly;
mod conflict;
mod constraint;
mod declared_order;
mod def_reference;
mod edge;
mod folder_policy;
mod inactive_mod;
mod load_order;
mod locator;
mod manifest_order;
mod mod_cost;
mod mod_entry;
mod mod_id;
mod node_path_hash;
mod override_facts;
mod patch;
mod ref_site;
mod report;
mod scan;
mod source;
mod version;
mod warning;

pub use assembly::{
    AssemblyInfo, AssemblyReference, AssemblyVersion, RuntimePatchKind, RuntimePatchTarget,
};
pub use conflict::{
    AffectedDef, BrokenInheritance, Conflict, DanglingCause, DanglingDefReference, DefOverride,
    DefRefParts, DiscardedAddition, DuplicateAssembly, DuplicateTemplateName, InheritanceProblem,
    InheritanceProblemKind, KeyedTranslationCollision, LikelyDuplicateMod, MissingTexturePath,
    ModReferenceKind, NearMissModReference, NearMissRule, PatchCollision, PatchCollisionEntry,
    PatchCollisionSeverity, RefSiteReferrer, RefSiteSummary, RuntimePatchCollision, SoundOverride,
    TextureOverride, TranspilerCollision, UndecodableTexture,
};
pub use constraint::{Constraint, ConstraintStatus};
pub use declared_order::{DeclaredOrder, ModDependency};
pub use def_reference::TexturePathCandidate;
pub use edge::{Edge, EdgeKind, EdgeReport, EdgeStatus, EdgeStrength};
pub use folder_policy::FolderPolicy;
pub use inactive_mod::InactiveMod;
pub use load_order::LoadOrder;
pub use locator::XmlLocator;
pub use manifest_order::ManifestOrder;
pub use mod_cost::ModCost;
pub use mod_entry::{GeneratedKind, GeneratedMarker, Mod};
pub use mod_id::ModId;
pub(crate) use node_path_hash::hash_node_path;
pub use override_facts::{ModsById, last_loaded};
pub use patch::{
    ConditionalBranch, DefTarget, FindModGate, MAX_VALUE_DIGEST_DEPTH, MAX_VALUE_DIGEST_ENTRIES,
    PatchOp, Selector, ValueDigest,
};
pub use ref_site::{MIN_RESOLVED_DISTINCT, RefSite, RefSiteOwner, RefSiteShape};
pub use report::{
    IncompatiblePair, MissingDependency, REPORT_SCHEMA_VERSION, Report, ReportMetadata,
    UnresolvedFindMod,
};
pub use scan::{
    DefEntry, ScanCost, ScanOutput, ScanProgress, ScanStage, ScannedMod, TemplateEntry,
};
pub use source::Source;
pub use version::{GameVersion, GameVersionParseError, parse_tag_name};
pub use warning::Warning;
