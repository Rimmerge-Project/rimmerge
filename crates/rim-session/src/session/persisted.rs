//! [`Session::reload_persisted`]: bringing a freshly scanned session level
//! with what is on disk now.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::Session;
use crate::ports::{
    AssignmentProjectStore, DecisionStore, PatchProjectStore, PersistedKind, RuleStore,
    RulesLoadWarning, StoreError,
};

/// The stores [`Session::reload_persisted`] re-reads, borrowed for one call.
#[derive(Clone, Copy)]
pub struct PersistedStores<'a> {
    /// The decisions file.
    pub decisions: &'a dyn DecisionStore,
    /// The rules file.
    pub rules: &'a dyn RuleStore,
    /// The compat patch projects.
    pub patches: &'a dyn PatchProjectStore,
    /// The assignment (patch maker) projects.
    pub assignments: &'a dyn AssignmentProjectStore,
}

/// What one [`Session::reload_persisted`] call found: the kinds that
/// differed, and the files it could not re-read.
#[derive(Default)]
struct Reloaded {
    changed: Vec<PersistedKind>,
    failures: Vec<RulesLoadWarning>,
}

impl Reloaded {
    fn note(&mut self, kind: PersistedKind, outcome: Result<bool, StoreError>) {
        match outcome {
            Ok(true) => self.changed.push(kind),
            Ok(false) => {}
            Err(error) => self.failures.push(RulesLoadWarning::PersistedReloadFailed {
                kind,
                reason: error.0,
            }),
        }
    }

    fn has_changed(&self, kind: PersistedKind) -> bool {
        self.changed.contains(&kind)
    }
}

impl Session {
    /// Re-reads the rules, decisions, patches and assignments from `stores`
    /// and sets whichever differs from what this session holds.
    ///
    /// [`crate::use_cases::LoadProject`] reads those files before it builds
    /// a session, outside the session lock, so an edit saved while a scan
    /// ran is in the file but not in the new session, and the next save
    /// from that session would erase it. The desktop calls this on the new
    /// session **under the session write lock, just before the swap**:
    /// every edit saves while holding that lock, so an edit that landed
    /// earlier is on disk by now.
    ///
    /// The sort and the ledger are recomputed once, and only when the rules
    /// or the decisions changed (that is [`Session::recompute_sort`], which
    /// also clears every selected-order cache). A patch that was added,
    /// removed or whose content changed drops its own scoped ledger and
    /// previews, and an unchanged patch keeps its caches; a changed
    /// assignment drops the cached coverage. When nothing changed, nothing is touched.
    ///
    /// A file that cannot be re-read keeps the copy the scan read and adds a
    /// [`RulesLoadWarning::PersistedReloadFailed`]: the scan is never lost
    /// and the swap never fails. The reload-failure warnings are replaced by
    /// this call's; the rules file's own warnings are replaced only when the
    /// rules were re-read successfully (a failed re-read keeps the scan's),
    /// and the mod-knowledge notes are never touched. Returns the kinds that
    /// changed.
    pub fn reload_persisted(&mut self, stores: PersistedStores<'_>) -> Vec<PersistedKind> {
        let dir = self.paths.profile_dir.clone();
        let mut reloaded = Reloaded::default();

        let decisions = self.reload_decisions(stores.decisions, &dir);
        reloaded.note(PersistedKind::Decisions, decisions);
        let rules = self.reload_rules(stores.rules, &dir);
        reloaded.note(PersistedKind::Rules, rules);
        let patches = self.reload_patches(stores.patches, &dir);
        reloaded.note(PersistedKind::Patches, patches);
        let assignments = self.reload_assignments(stores.assignments, &dir);
        reloaded.note(PersistedKind::Assignments, assignments);

        if reloaded.has_changed(PersistedKind::Rules)
            || reloaded.has_changed(PersistedKind::Decisions)
        {
            self.recompute_sort();
        }
        self.reload_failures = reloaded.failures;
        reloaded.changed
    }

    fn reload_decisions(
        &mut self,
        store: &dyn DecisionStore,
        dir: &Path,
    ) -> Result<bool, StoreError> {
        let loaded = store.load(dir)?;
        let is_changed = loaded != self.decisions;
        self.decisions = loaded;
        Ok(is_changed)
    }

    fn reload_rules(&mut self, store: &dyn RuleStore, dir: &Path) -> Result<bool, StoreError> {
        let loaded = store.load(dir)?;
        self.rules_file_warnings = loaded.warnings;
        let is_changed = loaded.rules != self.rules;
        self.rules = loaded.rules;
        Ok(is_changed)
    }

    fn reload_patches(
        &mut self,
        store: &dyn PatchProjectStore,
        dir: &Path,
    ) -> Result<bool, StoreError> {
        let loaded: BTreeMap<_, _> = store
            .load_all(dir)?
            .into_iter()
            .map(|project| (project.id().clone(), project))
            .collect();
        if loaded == self.patches {
            return Ok(false);
        }
        let affected: BTreeSet<_> = self
            .patches
            .keys()
            .chain(loaded.keys())
            .filter(|id| self.patches.get(*id) != loaded.get(*id))
            .cloned()
            .collect();
        self.patches = loaded;
        for id in &affected {
            self.invalidate_patch_caches(id);
        }
        Ok(true)
    }

    fn reload_assignments(
        &mut self,
        store: &dyn AssignmentProjectStore,
        dir: &Path,
    ) -> Result<bool, StoreError> {
        let loaded: BTreeMap<_, _> = store
            .load_all(dir)?
            .into_iter()
            .map(|project| (project.id().clone(), project))
            .collect();
        if loaded == self.assignments {
            return Ok(false);
        }
        self.assignments = loaded;
        // Coverage reflects the project's own rows, so it goes stale with
        // them; the instance cache reads only the install and survives.
        self.assignment_coverage.clear();
        Ok(true)
    }
}

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "persisted_tests.rs"]
mod tests;
