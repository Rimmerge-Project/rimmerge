//! [`Session`]'s assignment projects: the assignment map and the cached instance/coverage results.

use rim_resolve::domain::{AssignmentId, AssignmentProject, Coverage, OrderSource};

use super::AssignmentInstanceList;
use super::Session;

/// No assignment (patch maker) project with this id is loaded in the
/// session — the [`AssignmentId`] twin of [`UnknownPatch`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("no assignment project with id {0}")]
pub struct UnknownAssignment(pub AssignmentId);

impl Session {
    /// Every assignment (patch maker) project loaded with this profile, in
    /// id order.
    pub fn assignments(&self) -> impl Iterator<Item = &AssignmentProject> {
        self.assignments.values()
    }

    /// One assignment project by id, if it's loaded.
    #[must_use]
    pub fn assignment(&self, id: &AssignmentId) -> Option<&AssignmentProject> {
        self.assignments.get(id)
    }

    /// Inserts or replaces an assignment project. `pub(crate)`: only the
    /// assignment use cases should reach for this directly (via
    /// `CreateAssignment`/`UpdateAssignment`/`SetAssignmentRow`/
    /// `ClearAssignmentRow`, or a rollback through
    /// [`Session::restore_assignment`]). Unlike [`Session::upsert_patch`],
    /// this invalidates no *ledger* cache of its own: an assignment
    /// project has no decisions/findings/scoped ledger, and nothing it
    /// changes affects [`Self::assignment_instances`] (which reads the
    /// active install's raw XML, not this project's own state). **It does
    /// drop this one id's own cached [`Self::assignment_coverage`]**:
    /// unlike `assignment_instances`, a
    /// coverage result also reflects the project's own rows (`has_row`,
    /// and `winner` computed "as if already exported"), so it goes stale
    /// on every row/schema/R/T change, not just an order change.
    pub(crate) fn upsert_assignment(&mut self, project: AssignmentProject) {
        let id = project.id().clone();
        self.assignments.insert(id.clone(), project);
        self.assignment_coverage
            .retain(|(_, cached_id, _), _| cached_id != &id);
    }

    /// Removes an assignment project. Returns the removed project, if it
    /// was loaded. Also drops that id's own cached [`Self::assignment_coverage`]
    /// (see [`Session::upsert_assignment`]'s own doc comment).
    pub(crate) fn remove_assignment(&mut self, id: &AssignmentId) -> Option<AssignmentProject> {
        self.assignment_coverage
            .retain(|(_, cached_id, _), _| cached_id != id);
        self.assignments.remove(id)
    }

    /// A snapshot of one assignment project, for a caller that wants to
    /// roll back a mutation if persisting it fails via
    /// [`Session::restore_assignment`]. `None` when no such project is
    /// loaded.
    #[must_use]
    pub fn assignment_snapshot(&self, id: &AssignmentId) -> Option<AssignmentProject> {
        self.assignments.get(id).cloned()
    }

    /// Restores a [`Session::assignment_snapshot`] — used only on the rare
    /// failed-persist rollback path. Kept as its own named method so a
    /// rollback call site reads as a rollback, not an upsert (mirrors
    /// [`Session::restore_patch`]).
    pub(crate) fn restore_assignment(&mut self, project: AssignmentProject) {
        self.upsert_assignment(project);
    }

    /// A cached read of [`Self::assignment_instances`], if one exists for
    /// `(source, def_type)`.
    #[must_use]
    pub(crate) fn cached_assignment_instances(
        &self,
        source: OrderSource,
        def_type: &str,
    ) -> Option<&AssignmentInstanceList> {
        self.assignment_instances
            .get(&(source, def_type.to_string()))
    }

    /// Caches a read of every active instance of `def_type`
    /// (`crate::use_cases::AssignmentInstances`) under `(source,
    /// def_type)` — read once per session and reused by inference and
    /// coverage. Only ever
    /// caches a *successful* read (see
    /// [`crate::use_cases::AssignmentInstances::execute`]'s own doc
    /// comment for why a failed read is never cached).
    pub(crate) fn cache_assignment_instances(
        &mut self,
        source: OrderSource,
        def_type: String,
        instances: AssignmentInstanceList,
    ) {
        self.assignment_instances
            .insert((source, def_type), instances);
    }

    /// A cached [`crate::use_cases::AssignmentCoverage`] result, if one
    /// exists for `(source, id, def_type)`.
    #[must_use]
    pub(crate) fn cached_assignment_coverage(
        &self,
        source: OrderSource,
        id: &AssignmentId,
        def_type: &str,
    ) -> Option<&Coverage> {
        self.assignment_coverage
            .get(&(source, id.clone(), def_type.to_string()))
    }

    /// Caches one assignment project section's coverage work queue
    /// (`crate::use_cases::AssignmentCoverage`) under `(source, id,
    /// def_type)`. See [`Self::assignment_coverage`]'s own doc comment for
    /// when this goes stale.
    pub(crate) fn cache_assignment_coverage(
        &mut self,
        source: OrderSource,
        id: AssignmentId,
        def_type: String,
        coverage: Coverage,
    ) {
        self.assignment_coverage
            .insert((source, id, def_type), coverage);
    }
}
