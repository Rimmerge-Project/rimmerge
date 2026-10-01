//! The patch project: its scope, identity, and decisions.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rim_analyzer::domain::ModId;

use super::identity::{DEFAULT_PATCH_AUTHOR, PatchId, PatchModIdentity};
use super::scope::PatchScope;
use crate::domain::decision::{Decision, DecisionSet};
use crate::domain::finding::FindingKey;
use crate::domain::merge::{FieldPath, MergeChoice};
use crate::domain::resolution::Action;

/// Why [`PatchProject::decide`] rejected a decision.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatchDecisionError {
    /// `key` isn't a finding between (at least two of) this patch's scope
    /// members. Boxed: `FindingKey`'s own largest variant
    /// (`DiscardedAddition`, two `ModId`s plus a `DefKey` plus a path
    /// string) pushed this error past clippy's `result_large_err`
    /// threshold otherwise.
    #[error("{0} is not a finding between this patch's scope members")]
    OutOfScope(Box<FindingKey>),
    /// The action isn't one a compat patch can publish (only `Merge`,
    /// `ShipAsset`, and `Ignore` are patchable).
    #[error("a patch can only Merge, ShipAsset, or Ignore — not {0}")]
    NotPatchable(UnpatchableAction),
    /// A `Merge` decision's per-field choice names an owner outside this
    /// patch's scope.
    #[error("{path}: choice names {mod_id}, which is not in this patch's scope")]
    ChoiceOutsideScope {
        /// The field whose choice named the out-of-scope owner.
        path: FieldPath,
        /// The out-of-scope owner the choice named.
        mod_id: ModId,
    },
    /// A `ShipAsset` decision copies from an owner outside this patch's
    /// scope.
    #[error("ShipAsset from {0}, which is not in this patch's scope")]
    AssetOutsideScope(ModId),
}

/// What [`PatchProject::set_scope`] did to the project's existing
/// decisions — never deletes anything (rerere semantics: an orphaned
/// decision stays on file, listed as prunable).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScopeChange {
    /// Decisions whose key the new scope no longer admits.
    pub now_orphaned: Vec<FindingKey>,
    /// Still-admitted decisions whose `Merge` choice (a `From { mod_id }`)
    /// or `ShipAsset { from }` names a mod the new scope removed. The next
    /// preview reports these as a caveat (`rim-merge`/`rim-session`)
    /// rather than silently exporting a value from a mod the
    /// patch no longer depends on.
    pub choices_naming_removed: Vec<(FindingKey, Option<FieldPath>)>,
}

/// The [`Action`] kinds a compat patch can never carry — the closed set
/// [`PatchDecisionError::NotPatchable`] names, so an interface can translate
/// the rejection instead of echoing an English identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnpatchableAction {
    /// [`Action::Accept`].
    Accept,
    /// [`Action::Reorder`].
    Reorder,
    /// [`Action::PreferWinner`].
    PreferWinner,
    /// [`Action::ChooseCandidate`].
    ChooseCandidate,
    /// [`Action::DropEdge`].
    DropEdge,
    /// [`Action::KeepEdge`].
    KeepEdge,
    /// [`Action::AddTag`].
    AddTag,
    /// [`Action::RemoveTag`].
    RemoveTag,
    /// [`Action::ExcludeFromCluster`].
    ExcludeFromCluster,
    /// [`Action::RemoveMod`].
    RemoveMod,
    /// [`Action::PromoteRule`].
    PromoteRule,
    /// [`Action::DropRule`].
    DropRule,
}

impl std::fmt::Display for UnpatchableAction {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Accept => "Accept",
            Self::Reorder => "Reorder",
            Self::PreferWinner => "PreferWinner",
            Self::ChooseCandidate => "ChooseCandidate",
            Self::DropEdge => "DropEdge",
            Self::KeepEdge => "KeepEdge",
            Self::AddTag => "AddTag",
            Self::RemoveTag => "RemoveTag",
            Self::ExcludeFromCluster => "ExcludeFromCluster",
            Self::RemoveMod => "RemoveMod",
            Self::PromoteRule => "PromoteRule",
            Self::DropRule => "DropRule",
        })
    }
}

/// The unpatchable kind of `action` — exhaustive except the three patchable
/// variants, so a future `Action` variant is a compile error here (as
/// intended: a new action needs an explicit decision about whether a patch
/// can ever carry it).
fn unpatchable_action(action: &Action) -> Option<UnpatchableAction> {
    match action {
        Action::Merge { .. } | Action::ShipAsset { .. } | Action::Ignore => None,
        Action::Accept => Some(UnpatchableAction::Accept),
        Action::Reorder { .. } => Some(UnpatchableAction::Reorder),
        Action::PreferWinner { .. } => Some(UnpatchableAction::PreferWinner),
        Action::ChooseCandidate { .. } => Some(UnpatchableAction::ChooseCandidate),
        Action::DropEdge { .. } => Some(UnpatchableAction::DropEdge),
        Action::KeepEdge { .. } => Some(UnpatchableAction::KeepEdge),
        Action::AddTag { .. } => Some(UnpatchableAction::AddTag),
        Action::RemoveTag { .. } => Some(UnpatchableAction::RemoveTag),
        Action::ExcludeFromCluster { .. } => Some(UnpatchableAction::ExcludeFromCluster),
        Action::RemoveMod { .. } => Some(UnpatchableAction::RemoveMod),
        Action::PromoteRule { .. } => Some(UnpatchableAction::PromoteRule),
        Action::DropRule { .. } => Some(UnpatchableAction::DropRule),
    }
}

/// A named, user-identified compat patch: a validated
/// [`PatchModIdentity`], a [`PatchScope`] of two or more active mods, and
/// its **own** [`DecisionSet`] — independent of the profile's; the two
/// never look each other up automatically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchProject {
    id: PatchId,
    name: String,
    identity: PatchModIdentity,
    author: String,
    description: String,
    scope: PatchScope,
    decisions: DecisionSet,
    export_dir: Option<PathBuf>,
    created_at: jiff::Timestamp,
    updated_at: jiff::Timestamp,
}

/// Plain, already-persisted data for every field a [`PatchProject`]
/// carries — the shape [`PatchProject::from_stored`] trusts, in contrast
/// to [`PatchProject::new`]'s constructor arguments for a brand-new
/// project. No invariants of its own (every field public); a store
/// adapter (`rim-io`'s `JsonPatchProjectStore`) builds one straight from
/// its own deserialized record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPatchProject {
    /// The project's stable id.
    pub id: PatchId,
    /// The project's label in lists.
    pub name: String,
    /// The project's published identity.
    pub identity: PatchModIdentity,
    /// The `About.xml` `<author>` this patch will export.
    pub author: String,
    /// The `About.xml` `<description>` this patch will export.
    pub description: String,
    /// The project's scope, as of this save.
    pub scope: PatchScope,
    /// May legitimately include a decision [`PatchScope::admits`] no
    /// longer admits under `scope` — see [`PatchProject::from_stored`].
    pub decisions: Vec<Decision>,
    /// The last folder this patch was exported to, if any.
    pub export_dir: Option<PathBuf>,
    /// When the project was created.
    pub created_at: jiff::Timestamp,
    /// When the project was last changed.
    pub updated_at: jiff::Timestamp,
}

impl PatchProject {
    /// Builds a new, empty (no decisions, no export yet) patch project.
    /// `author` defaults to `"Rimmerge"`; `description` defaults to empty —
    /// the description template names each scope member's *display name*,
    /// which this
    /// pure-domain crate has no way to look up (it only ever sees
    /// `ModId`s), so filling it in is `rim-session`'s `CreatePatch` use
    /// case's job, via [`PatchProject::set_description`].
    #[must_use]
    pub fn new(
        id: PatchId,
        name: String,
        identity: PatchModIdentity,
        scope: PatchScope,
        created_at: jiff::Timestamp,
    ) -> Self {
        Self {
            id,
            name,
            identity,
            author: DEFAULT_PATCH_AUTHOR.to_string(),
            description: String::new(),
            scope,
            decisions: DecisionSet::new(),
            export_dir: None,
            created_at,
            updated_at: created_at,
        }
    }

    /// The project's stable id.
    #[must_use]
    pub fn id(&self) -> &PatchId {
        &self.id
    }

    /// The project's label in lists — distinct from
    /// [`PatchModIdentity::display_name`], the published mod's own name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The project's current identity.
    #[must_use]
    pub fn identity(&self) -> &PatchModIdentity {
        &self.identity
    }

    /// The `About.xml` `<author>` this patch will export.
    #[must_use]
    pub fn author(&self) -> &str {
        &self.author
    }

    /// The `About.xml` `<description>` this patch will export.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The project's current scope.
    #[must_use]
    pub fn scope(&self) -> &PatchScope {
        &self.scope
    }

    /// This patch's own decisions — never the profile's.
    #[must_use]
    pub fn decisions(&self) -> &DecisionSet {
        &self.decisions
    }

    /// The last folder this patch was exported to, if any.
    #[must_use]
    pub fn export_dir(&self) -> Option<&Path> {
        self.export_dir.as_deref()
    }

    /// When the project was created.
    #[must_use]
    pub fn created_at(&self) -> jiff::Timestamp {
        self.created_at
    }

    /// When the project was last changed.
    #[must_use]
    pub fn updated_at(&self) -> jiff::Timestamp {
        self.updated_at
    }

    /// Renames the project's list label. Does not touch
    /// [`PatchModIdentity::display_name`] (the published mod's own name;
    /// see [`PatchProject::set_identity`]).
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    /// Sets the `About.xml` `<author>`.
    pub fn set_author(&mut self, author: String) {
        self.author = author;
    }

    /// Sets the `About.xml` `<description>`.
    pub fn set_description(&mut self, description: String) {
        self.description = description;
    }

    /// Sets (or clears, with `None`) the last export target, remembered
    /// for the folder picker.
    pub fn set_export_dir(&mut self, export_dir: Option<PathBuf>) {
        self.export_dir = export_dir;
    }

    /// Replaces the project's published identity.
    pub fn set_identity(&mut self, identity: PatchModIdentity) {
        self.identity = identity;
    }

    /// Validates and records a decision, replacing any earlier one on the
    /// same key.
    ///
    /// # Errors
    ///
    /// Returns [`PatchDecisionError`] (storing nothing) when `decision`'s
    /// action isn't `Merge`/`ShipAsset`/`Ignore`
    /// ([`PatchDecisionError::NotPatchable`]), `decision.key` isn't
    /// admitted by this patch's scope
    /// ([`PatchDecisionError::OutOfScope`]), a `Merge` choice names an
    /// owner outside scope ([`PatchDecisionError::ChoiceOutsideScope`]), or
    /// a `ShipAsset` copies from outside scope
    /// ([`PatchDecisionError::AssetOutsideScope`]).
    pub fn decide(&mut self, decision: Decision) -> Result<Option<Decision>, PatchDecisionError> {
        if let Some(action) = unpatchable_action(&decision.action) {
            return Err(PatchDecisionError::NotPatchable(action));
        }
        if !self.scope.admits(&decision.key) {
            return Err(PatchDecisionError::OutOfScope(Box::new(
                decision.key.clone(),
            )));
        }
        if let Action::Merge { choices, .. } = &decision.action {
            for (path, choice) in choices {
                if let MergeChoice::From { mod_id } = choice
                    && !self.scope.admits_owner(mod_id)
                {
                    return Err(PatchDecisionError::ChoiceOutsideScope {
                        path: path.clone(),
                        mod_id: mod_id.clone(),
                    });
                }
            }
        }
        if let Action::ShipAsset { from, .. } = &decision.action
            && !self.scope.contains(from)
        {
            return Err(PatchDecisionError::AssetOutsideScope(from.clone()));
        }

        Ok(self
            .decisions
            .insert(decision)
            .unwrap_or_else(|never| match never {}))
    }

    /// Reconstitutes a project from its own previously-persisted state,
    /// trusting it completely except for
    /// [`PatchDecisionError::NotPatchable`] — an action kind a patch can
    /// never carry, regardless of scope, which the store's own writer
    /// should never have produced in the first place.
    ///
    /// This is deliberately **not** `stored.decisions.into_iter().try_for_each(|d|
    /// self.decide(d))`: `PatchProject::set_scope` legitimately shrinks a
    /// project's scope after some of its decisions were made, and its own
    /// contract (see [`ScopeChange`]) is to keep an orphaned decision on
    /// file rather than delete it. Replaying every stored decision through
    /// [`Self::decide`] — which rejects anything [`PatchScope::admits`]
    /// no longer admits — would reject the *whole* project over a single
    /// decision the file format is explicitly designed to keep, turning a
    /// legitimate scope shrink into "your saved patch no longer loads."
    /// Callers that need to know what's now orphaned use
    /// [`Self::orphaned`]/[`Self::prune_orphaned`] on the loaded project,
    /// exactly as for a shrink that happened in memory.
    ///
    /// # Errors
    ///
    /// Returns [`PatchDecisionError::NotPatchable`] naming the first
    /// unpatchable action found among `stored.decisions` — the store's
    /// own file format never intentionally writes one (only `Merge`/
    /// `ShipAsset`/`Ignore` ever reach `decisions.json`'s twin), so this
    /// only fires against a hand-edited or corrupted file.
    pub fn from_stored(stored: StoredPatchProject) -> Result<Self, PatchDecisionError> {
        let mut decisions = DecisionSet::new();
        for decision in stored.decisions {
            if let Some(action) = unpatchable_action(&decision.action) {
                return Err(PatchDecisionError::NotPatchable(action));
            }
            decisions
                .insert(decision)
                .unwrap_or_else(|never| match never {});
        }

        Ok(Self {
            id: stored.id,
            name: stored.name,
            identity: stored.identity,
            author: stored.author,
            description: stored.description,
            scope: stored.scope,
            decisions,
            export_dir: stored.export_dir,
            created_at: stored.created_at,
            updated_at: stored.updated_at,
        })
    }

    /// Removes the decision on `key`, if one exists.
    pub fn revert(&mut self, key: &FindingKey) -> Option<Decision> {
        self.decisions.remove(key)
    }

    /// Replaces the scope, reporting what happened to the project's
    /// existing decisions against the *new* scope — never deletes any of
    /// them (see [`ScopeChange`]'s own doc comment).
    pub fn set_scope(&mut self, scope: PatchScope) -> ScopeChange {
        let mut now_orphaned = Vec::new();
        let mut choices_naming_removed = Vec::new();

        for decision in self.decisions.iter() {
            if !scope.admits(&decision.key) {
                now_orphaned.push(decision.key.clone());
                continue;
            }
            match &decision.action {
                Action::Merge { choices, .. } => {
                    for (path, choice) in choices {
                        if let MergeChoice::From { mod_id } = choice
                            && !scope.admits_owner(mod_id)
                        {
                            choices_naming_removed.push((decision.key.clone(), Some(path.clone())));
                        }
                    }
                }
                Action::ShipAsset { from, .. } => {
                    if !scope.contains(from) {
                        choices_naming_removed.push((decision.key.clone(), None));
                    }
                }
                Action::Accept
                | Action::Reorder { .. }
                | Action::PreferWinner { .. }
                | Action::ChooseCandidate { .. }
                | Action::DropEdge { .. }
                | Action::KeepEdge { .. }
                | Action::AddTag { .. }
                | Action::RemoveTag { .. }
                | Action::ExcludeFromCluster { .. }
                | Action::RemoveMod { .. }
                | Action::PromoteRule { .. }
                | Action::DropRule { .. }
                | Action::Ignore => {}
            }
        }

        self.scope = scope;
        ScopeChange {
            now_orphaned,
            choices_naming_removed,
        }
    }

    /// Decisions whose key is either no longer live (absent from `live`)
    /// or no longer admitted by the current scope — kept on file, never
    /// silently dropped, but worth surfacing as prunable.
    pub fn orphaned<'a>(
        &'a self,
        live: &'a BTreeSet<FindingKey>,
    ) -> impl Iterator<Item = &'a Decision> {
        self.decisions.iter().filter(move |decision| {
            !(live.contains(&decision.key) && self.scope.admits(&decision.key))
        })
    }

    /// Removes and returns every currently-orphaned decision.
    pub fn prune_orphaned(&mut self, live: &BTreeSet<FindingKey>) -> Vec<Decision> {
        let keys: Vec<FindingKey> = self.orphaned(live).map(|d| d.key.clone()).collect();
        keys.into_iter()
            .filter_map(|key| self.decisions.remove(&key))
            .collect()
    }

    /// Canonical hash of *what was decided*, not when — delegates to
    /// [`DecisionSet::content_sha256`], the same canonical algorithm the
    /// profile merge mod's own render uses, so both kinds of generated
    /// mod's `rimmerge.json` carry a hash built the same way. See that
    /// method's own doc comment for the exact byte encoding and the
    /// file-format-change rule it extends.
    #[must_use]
    pub fn decisions_sha256(&self) -> String {
        self.decisions.content_sha256()
    }
}
