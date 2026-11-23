//! Library organization: nested mod groups and per-mod colours.
//!
//! A bounded context of its own. [`ModOrganization`] is a forest of groups
//! (each with **direct** members only) plus a colour per mod, keyed by
//! [`OrganizedModId`] (the base id, so `x` and `x_steam` are one mod). It
//! is pure: no IO, no `Session`, and nothing in the sorter or the ledger
//! can read it. Persistence is behind [`crate::ports::OrganizationStore`];
//! the session holds it as an [`OrganizationState`].

mod color;
mod copy;
mod edit;
mod forest;
mod group;
mod ids;
mod resolve;
mod state;

pub use color::{ColorToken, UnknownColor};
pub use copy::{CopyMode, CopyPlan, CopyReport, Discarded, GroupColorClash, plan_copy};
pub use edit::{EditOutcome, OrganizationEdit};
pub use forest::{
    ColorChange, DeleteMode, DeleteOutcome, GroupNode, GroupScope, MembershipAdd, MembershipMove,
    MembershipRemoval, ModOrganization, OrganizationDocumentError, OrganizationError,
};
pub use group::{Group, GroupParts};
pub use ids::{GroupId, GroupName, InvalidGroupName, InvalidOrganizedModId, OrganizedModId};
pub use resolve::{GroupScopeResolution, ResolvedScope, resolve_scope};
pub use state::{
    OrganizationIssue, OrganizationLoad, OrganizationQueryError, OrganizationState,
    OrganizationUnavailable,
};

/// The bounds on an organization, read by the aggregate and by the
/// `rim-io` reader so a hostile file cannot build an unbounded structure.
#[derive(Debug, Clone, Copy)]
pub struct OrganizationLimits;

impl OrganizationLimits {
    /// The deepest nesting: a top-level group is level 1.
    pub const MAX_DEPTH: usize = 8;
    /// The most groups.
    pub const MAX_GROUPS: usize = 1_000;
    /// The largest group id (and so the room for ids over a profile's
    /// lifetime: ids are never reused). A file naming a larger one is
    /// refused with a clear error instead of running the id space out.
    pub const MAX_GROUP_ID: u32 = 1_000_000;
    /// The most direct members of one group.
    pub const MAX_MEMBERS_PER_GROUP: usize = crate::MAX_MOD_LIST_ROWS;
    /// The most memberships over all groups.
    pub const MAX_TOTAL_MEMBERSHIPS: usize = 50_000;
    /// The most mods that carry a colour.
    pub const MAX_MOD_COLORS: usize = 20_000;
    /// The longest group name, in characters.
    pub const MAX_NAME_CHARS: usize = 64;
    /// The longest mod id, in characters.
    pub const MAX_ID_CHARS: usize = 128;
    /// The most bytes a reader accepts of `organization.json` (4 MiB).
    pub const MAX_FILE_BYTES: usize = 4 * 1024 * 1024;
}

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "organization/organization_tests.rs"]
mod tests;
