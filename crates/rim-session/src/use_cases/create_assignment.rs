//! [`CreateAssignment`]: proposes assignment-def candidates for a chosen
//! reference/target set in two phases — cheap listing
//! ([`CreateAssignment::list_candidates`]), then the real per-type
//! inference ([`CreateAssignment::infer_candidate`]) — then persists the
//! user's confirmed pick (`execute`).
//!
//! **Why two phases rather than one `propose` call**: running every
//! candidate's own (expensive) instance read and target-shape inference up
//! front is sub-second for a hand-built fixture, but tens of seconds
//! against a real, richly-populated reference mod (one large content mod
//! alone can yield dozens of candidate def types). Splitting the
//! read-heavy work into a second, per-type call the wizard/CLI only ever
//! makes once (for the type the user actually picks) is a real, measured
//! latency fix, not a style preference.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{LoadOrder, ModId, Selector, Source};
use rim_merge::tree::Content;
use rim_resolve::domain::{
    AssignmentId, AssignmentProject, AssignmentSchema, FieldRole, PatchIdentityError,
    PatchModIdentity, TargetShape,
};

use super::assignment_instances::{AssignmentInstances, AssignmentInstancesError};
use super::def_sources::{self, DefSourceLookupError};
use crate::Session;
use crate::assignment_refs::effective_refs;
use crate::ports::{AssignmentProjectStore, DefSourceReader, StoreError};

/// One candidate assignment def type [`CreateAssignment::infer_candidate`]
/// infers, with its fully inferred schema (fields *and* target shapes —
/// unlike [`rim_merge::assign::infer`]'s own bare adapter, which leaves
/// `target_shapes` empty for exactly this caller to fill in).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignmentCandidate {
    /// The candidate def type, e.g. `example.PartAssignmentDef`.
    pub def_type: String,
    /// The inferred schema, `target_shapes` included.
    pub schema: AssignmentSchema,
    /// Referenced targets excluded from a `TargetKey` field's own
    /// [`TargetShape::infer`] sample because their own inherited tree
    /// couldn't be read — see [`UnreadableTarget`]'s own doc comment.
    /// Never fails the whole candidate: a shape learned from every *other*
    /// referenced target is still useful, and the wizard can show these as
    /// a caveat rather than lose the candidate entirely.
    pub unreadable_targets: Vec<UnreadableTarget>,
    /// Referenced targets never even attempted, because their own owner
    /// isn't a member of the selected `targets` (T) or Core — see
    /// [`ExcludedTarget`]'s own doc comment. A different fact from
    /// [`Self::unreadable_targets`]: these were never read at all (the
    /// point of honouring T is fewer reads, not more failures).
    pub excluded_targets: Vec<ExcludedTarget>,
}

/// One referenced target [`CreateAssignment::propose`] could not read back
/// while learning a `TargetKey` field's [`TargetShape`] — a stale file, a
/// broken `ParentName` chain, or any other single-target read failure
/// (`TargetReadError`, private: its `Display` text is all a caller ever
/// needs, in [`Self::reason`]). Reported, never silently dropped
/// *and* never enough on its own to fail the whole `propose` call: the
/// field's shape is still learned from every other referenced target that
/// *did* read back cleanly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableTarget {
    /// The `TargetKey` field whose sample this target would have joined.
    pub field: rim_resolve::domain::FieldPath,
    /// The target's own def type (the field's resolved type).
    pub def_type: String,
    /// The target's own `defName`.
    pub def_name: String,
    /// Why the read failed.
    pub reason: String,
}

/// One referenced target [`CreateAssignment::infer_candidate`] never even
/// tried to read, because its own owner isn't a member of the selected
/// `targets` (T) or Core (samples must be read only from referenced defs
/// owned by the selected T, plus Core). Reading a candidate target's full
/// inherited tree
/// (`ParentName` chain included) is the expensive part of inference on a
/// real install — most of a `TargetKey` field's referenced values are
/// never owned by T at all, so skipping them here rather than reading
/// them and discarding the result is the whole point of honouring T.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedTarget {
    /// The `TargetKey` field whose sample this target would have joined.
    pub field: rim_resolve::domain::FieldPath,
    /// The target's own def type (the field's resolved type).
    pub def_type: String,
    /// The target's own `defName`.
    pub def_name: String,
}

/// One assignment-def candidate's cheap summary
/// [`CreateAssignment::list_candidates`] returns — no instance reads at
/// all, just [`rim_analyzer::analysis::SourceIndex::owners_by_def`]'s own
/// already-in-memory maps (sub-second on the real install). The wizard's
/// own first step: pick refs/targets, see this list, pick one type, *then*
/// [`CreateAssignment::infer_candidate`] runs the real (expensive)
/// inference for that one type alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateSummary {
    /// The candidate def type, e.g. `example.PartAssignmentDef`.
    pub def_type: String,
    /// How many active instances of this type exist across the whole
    /// install (every owner, not only the effective reference set) — the
    /// same count [`super::AssignmentInstances::execute`] would read if
    /// this type were chosen.
    pub instance_count: usize,
    /// Every mod that owns at least one instance of this type, across the
    /// whole install.
    pub owners: BTreeSet<ModId>,
}

/// Everything that can go wrong proposing assignment-def candidates.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProposeAssignmentError {
    /// Reading every active instance of a candidate def type failed — the
    /// one failure mode that still fails the whole call ("never a partial
    /// schema": every instance is required evidence for
    /// the field-classification vote itself, unlike one target among many
    /// feeding a learned shape's sample, which [`UnreadableTarget`]
    /// excludes instead).
    #[error(transparent)]
    Instances(#[from] AssignmentInstancesError),
}

/// A single referenced target's own read failure, on its way to becoming
/// an [`UnreadableTarget`] rather than propagating — see
/// [`resolved_top_level_children`]'s own call site.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
enum TargetReadError {
    #[error(transparent)]
    Source(#[from] DefSourceLookupError),
    #[error("resolving a target's inherited tree: {0}")]
    Inherit(String),
}

/// A new assignment project's requested shape, straight from the wizard —
/// `schema` is the (possibly user-edited) confirmed schema of whichever
/// [`AssignmentCandidate`] the user picked from [`CreateAssignment::propose`].
#[derive(Debug, Clone, PartialEq)]
pub struct CreateAssignmentInput {
    /// The project's label in lists.
    pub name: String,
    /// The published mod's package id.
    pub package_id: String,
    /// The published mod's display name.
    pub display_name: String,
    /// The selected reference set (before the dependency closure).
    pub refs: BTreeSet<ModId>,
    /// Closure members the user opted out of.
    pub excluded_refs: BTreeSet<ModId>,
    /// The target set T.
    pub targets: BTreeSet<ModId>,
    /// The confirmed schema.
    pub schema: AssignmentSchema,
}

/// Everything that can go wrong creating an assignment project.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CreateAssignmentError {
    /// `package_id`/`display_name` failed [`PatchModIdentity::new`]'s own
    /// validation.
    #[error(transparent)]
    Identity(#[from] PatchIdentityError),
    /// `refs` was empty.
    #[error("at least one reference mod must be selected")]
    EmptyRefs,
    /// `targets` was empty, and the schema has at least one
    /// [`FieldRole::TargetKey`] field (a standalone/"new def" schema has
    /// none, and is exempt from this check).
    #[error("at least one target must be selected")]
    EmptyTargets,
    /// `package_id` collides with an active mod's base id or another
    /// project's own package id.
    #[error("package id {0} is already used by an active mod or another project")]
    PackageIdTaken(ModId),
    /// Persisting the new project failed.
    #[error("saving the assignment project: {0}")]
    Store(StoreError),
}

/// Every top-level child tag of `def_name`'s fully inherited tree —
/// the inference read: the raw node, its template chain, then
/// [`rim_merge::inherit::resolve`] over both. A single target's own
/// failure here becomes an [`UnreadableTarget`] at the call site, never a
/// whole-`propose` error — see that type's own doc comment.
fn resolved_top_level_children<Reader: DefSourceReader>(
    reader: &Reader,
    session: &mut Session,
    order: &LoadOrder,
    def_type: &str,
    def_name: &str,
) -> Result<BTreeSet<String>, TargetReadError> {
    let (owner, raw) = def_sources::def_owner_and_raw(
        reader,
        session,
        order,
        def_type,
        def_name,
        Selector::DefName,
    )?;
    let templates = def_sources::template_set(
        reader,
        session,
        order,
        def_type,
        &owner,
        raw.parent_name.as_deref(),
    )?;
    let resolved = rim_merge::inherit::resolve(&raw, &templates)
        .map_err(|error| TargetReadError::Inherit(error.to_string()))?;
    Ok(match &resolved.root.content {
        Content::Children(children) => children.iter().map(|child| child.tag.clone()).collect(),
        _ => BTreeSet::new(),
    })
}

/// Every distinct value `field`'s [`FieldRole::TargetKey`] instances name,
/// across `instances`.
fn referenced_values(
    instances: &[(ModId, rim_resolve::domain::InstanceValues)],
    field: &rim_resolve::domain::FieldPath,
) -> BTreeSet<String> {
    instances
        .iter()
        .filter_map(|(_, values)| values.get(field))
        .flat_map(|occurrence| occurrence.values.iter().cloned())
        .collect()
}

/// Whether `owner` is registered as [`Source::Core`] in the active scan —
/// [`owner_in_targets`]'s own "plus Core" half.
fn is_core(session: &Session, owner: &ModId) -> bool {
    session
        .report()
        .mods
        .iter()
        .find(|m| m.id.base() == owner.base())
        .is_some_and(|m| m.source == Source::Core)
}

/// Whether `(def_type, name)`'s own owners (via `owners_by_def`) include a
/// member of `targets` or Core — the inference read-gate: a
/// `TargetKey` field's referenced value is only ever read back (an
/// expensive `ParentName`-chain resolve) when it could plausibly be a real
/// target, never merely to discard the result.
fn owner_in_targets(
    session: &Session,
    def_type: &str,
    name: &str,
    targets: &BTreeSet<ModId>,
) -> bool {
    session
        .sources()
        .owners_by_def
        .get(&(def_type.to_string(), name.to_string()))
        .is_some_and(|owners| {
            owners
                .iter()
                .any(|owner| targets.contains(&owner.base()) || is_core(session, owner))
        })
}

/// Field inference over `instances`, with the `resolve`/
/// `dll_owner`/`existing_def_type` closures built straight off
/// `session.sources()` — the same construction
/// [`super::update_assignment::UpdateAssignment`]'s own re-inference
/// uses. `existing_def_type` feeds `AssignmentSchema::infer_fields`'s
/// tag-reconstruction tie discriminator;
/// pre-lowercased once here rather than per candidate type, since a tie
/// is rare but this closure is cheap to build regardless.
fn infer_schema(
    session: &Session,
    instances: &[(ModId, rim_resolve::domain::InstanceValues)],
    effective: &BTreeSet<ModId>,
    def_type: &str,
) -> AssignmentSchema {
    let sources = session.sources();
    let resolve = |value: &str| sources.defs_by_name.get(value).cloned().unwrap_or_default();
    let dll_owner = |type_name: &str| sources.dll_owner_of(type_name).cloned();
    let existing_types_lower: BTreeSet<String> = sources
        .owners_by_def
        .keys()
        .map(|(t, _)| t.to_ascii_lowercase())
        .collect();
    let existing_def_type =
        |type_name: &str| existing_types_lower.contains(&type_name.to_ascii_lowercase());
    rim_merge::assign::infer(
        def_type,
        instances,
        &resolve,
        &dll_owner,
        &existing_def_type,
        effective,
    )
}

/// Which of the two callers is asking `CreateAssignment` to infer a
/// candidate — decides only whether a no-`TargetKey` (free-standing)
/// schema is gated on the project's target set being empty; see
/// [`CreateAssignment::infer_candidate`] and
/// [`CreateAssignment::infer_candidate_for_explicit_add`]'s own doc
/// comments for why the two need different answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CandidateIntent {
    /// The tool is proposing/ranking a candidate for the user to choose
    /// during project creation.
    Proposal,
    /// The user already named this exact def type as one to add.
    ExplicitAdd,
}

/// Infers every assignment-def candidate for a reference/target selection,
/// then persists the user's confirmed pick.
pub struct CreateAssignment<Store, Reader> {
    store: Store,
    reader: Reader,
}

impl<Store: AssignmentProjectStore, Reader: DefSourceReader> CreateAssignment<Store, Reader> {
    /// Builds the use case from its ports.
    #[must_use]
    pub fn new(store: Store, reader: Reader) -> Self {
        Self { store, reader }
    }

    /// The effective reference set (`selected` plus the
    /// transitive closure of its declared `modDependencies`, Core/DLC
    /// excluded, minus `excluded_refs`) alone — for a caller (the wizard)
    /// that wants to display "plus X (dependency of Y)" *before* any
    /// candidate has been chosen, when there is no
    /// [`AssignmentCandidate::schema`]`.refs` yet to read it off of.
    #[must_use]
    pub fn effective_refs(
        &self,
        session: &Session,
        selected: &BTreeSet<ModId>,
        excluded_refs: &BTreeSet<ModId>,
    ) -> BTreeSet<ModId> {
        effective_refs(selected, session.report())
            .difference(excluded_refs)
            .cloned()
            .collect()
    }

    /// Phase 1 (cheap, no instance reads): every def type any member of
    /// the effective reference set (`selected` plus its declared-
    /// dependency closure, minus `excluded_refs`) owns,
    /// with its whole-install instance count and owner set, from
    /// [`rim_analyzer::analysis::SourceIndex::owners_by_def`] alone —
    /// sub-second even on a large real install. `targets` is accepted
    /// (unused here) to match this method's own call shape — T only
    /// affects [`Self::infer_candidate`]'s target-shape sampling and
    /// no-target-key gate, never which def types exist at all.
    ///
    /// The wizard's own first step: show this list, let the user pick one
    /// `def_type`, then call [`Self::infer_candidate`] for it alone —
    /// never run every candidate's own (expensive) inference up front.
    #[must_use]
    pub fn list_candidates(
        &self,
        session: &Session,
        selected: &BTreeSet<ModId>,
        excluded_refs: &BTreeSet<ModId>,
        targets: &BTreeSet<ModId>,
    ) -> Vec<CandidateSummary> {
        let _ = targets;
        let effective: BTreeSet<ModId> = effective_refs(selected, session.report())
            .difference(excluded_refs)
            .cloned()
            .collect();

        let mut by_type: BTreeMap<String, (usize, BTreeSet<ModId>)> = BTreeMap::new();
        for ((def_type, _), owners) in &session.sources().owners_by_def {
            let entry = by_type.entry(def_type.clone()).or_default();
            entry.0 += 1;
            entry.1.extend(owners.iter().cloned());
        }

        by_type
            .into_iter()
            .filter(|(_, (_, owners))| owners.iter().any(|owner| effective.contains(&owner.base())))
            .map(|(def_type, (instance_count, owners))| CandidateSummary {
                def_type,
                instance_count,
                owners,
            })
            .collect()
    }

    /// Phase 2: the real (expensive) inference for exactly one candidate
    /// def type chosen from [`Self::list_candidates`]'s own result — every
    /// active instance of `def_type` read, its field schema inferred, and
    /// (for each [`FieldRole::TargetKey`] field) its target shape learned
    /// from referenced values owned by `targets` or Core alone (honouring
    /// T rather than reading every referenced value regardless of
    /// ownership).
    ///
    /// A def type with at least one `FieldRole::TargetKey` field is always
    /// a valid candidate; a def type with **none** is a valid candidate
    /// only when `targets` is empty — a "new def" (standalone) candidate,
    /// e.g. authoring fresh `example.PartDef`s rather than patching an
    /// existing def (a "good to have" extension of the assignment model:
    /// the resulting project's rows are free-standing instances
    /// keyed by their own `defName`, never a `TargetRef` — see
    /// [`rim_resolve::domain::Section::is_standalone`]). `None`
    /// when neither condition holds.
    ///
    /// This is the **proposal** path: the tool is choosing/ranking a
    /// candidate for the user during project creation, and a
    /// free-standing type genuinely can't yet be tied to targets the user
    /// is about to pick, so it stays gated on `targets.is_empty()`. An
    /// explicit, already-named request (`AddAssignmentSection`) uses
    /// [`Self::infer_candidate_for_explicit_add`] instead — see that
    /// method's own doc comment for why the gate doesn't apply there.
    ///
    /// # Errors
    ///
    /// See [`ProposeAssignmentError`].
    pub fn infer_candidate(
        &self,
        session: &mut Session,
        selected: &BTreeSet<ModId>,
        excluded_refs: &BTreeSet<ModId>,
        targets: &BTreeSet<ModId>,
        def_type: &str,
    ) -> Result<Option<AssignmentCandidate>, ProposeAssignmentError> {
        self.infer_candidate_impl(
            session,
            selected,
            excluded_refs,
            targets,
            def_type,
            CandidateIntent::Proposal,
        )
    }

    /// Same inference as [`Self::infer_candidate`], for a def type the
    /// caller has already named explicitly rather than one the tool is
    /// proposing among several (`AddAssignmentSection`). The only
    /// difference: a no-`TargetKey` (free-standing)
    /// schema is **always** a valid candidate here, regardless of whether
    /// the project's own target set `targets` is empty. "This project
    /// already has targets set" is a reason to refuse a candidate the
    /// tool is choosing *for* the user (the proposal path,
    /// [`Self::infer_candidate`]) — it is not a reason to refuse a type
    /// the user named outright, and refusing it there would block adding
    /// a free-standing section *after* a target-keyed one already exists.
    ///
    /// # Errors
    ///
    /// See [`ProposeAssignmentError`].
    pub fn infer_candidate_for_explicit_add(
        &self,
        session: &mut Session,
        selected: &BTreeSet<ModId>,
        excluded_refs: &BTreeSet<ModId>,
        targets: &BTreeSet<ModId>,
        def_type: &str,
    ) -> Result<Option<AssignmentCandidate>, ProposeAssignmentError> {
        self.infer_candidate_impl(
            session,
            selected,
            excluded_refs,
            targets,
            def_type,
            CandidateIntent::ExplicitAdd,
        )
    }

    /// Shared inference behind [`Self::infer_candidate`] and
    /// [`Self::infer_candidate_for_explicit_add`] — identical except for
    /// whether a no-`TargetKey` schema is gated on `targets.is_empty()`,
    /// decided by `intent`.
    fn infer_candidate_impl(
        &self,
        session: &mut Session,
        selected: &BTreeSet<ModId>,
        excluded_refs: &BTreeSet<ModId>,
        targets: &BTreeSet<ModId>,
        def_type: &str,
        intent: CandidateIntent,
    ) -> Result<Option<AssignmentCandidate>, ProposeAssignmentError> {
        let effective: BTreeSet<ModId> = effective_refs(selected, session.report())
            .difference(excluded_refs)
            .cloned()
            .collect();
        let order = session.orders().get(session.selected()).clone();

        let instances = AssignmentInstances::new(&self.reader).execute(session, def_type)?;
        let mut schema = infer_schema(session, &instances, &effective, def_type);
        let is_valid_candidate =
            schema.has_target_key() || targets.is_empty() || intent == CandidateIntent::ExplicitAdd;
        if !is_valid_candidate {
            return Ok(None);
        }

        let (unreadable_targets, excluded_targets) =
            self.fill_target_shapes(session, &order, &instances, targets, &mut schema);
        Ok(Some(AssignmentCandidate {
            def_type: def_type.to_string(),
            schema,
            unreadable_targets,
            excluded_targets,
        }))
    }

    /// Computes and records every [`FieldRole::TargetKey`] field's
    /// [`TargetShape`] in place, returning every referenced target
    /// excluded from a shape's sample because it couldn't be read back
    /// (the inference read; see [`UnreadableTarget`]'s own doc
    /// comment for why this never fails the whole candidate) and every
    /// referenced target never even attempted because its owner isn't in
    /// `targets`/Core (see [`ExcludedTarget`]'s own doc comment). A
    /// standalone (no-`TargetKey`) schema has nothing to loop over here —
    /// both results come back empty.
    fn fill_target_shapes(
        &self,
        session: &mut Session,
        order: &LoadOrder,
        instances: &[(ModId, rim_resolve::domain::InstanceValues)],
        targets: &BTreeSet<ModId>,
        schema: &mut AssignmentSchema,
    ) -> (Vec<UnreadableTarget>, Vec<ExcludedTarget>) {
        let target_key_fields: Vec<(rim_resolve::domain::FieldPath, String)> = schema
            .fields
            .iter()
            .filter_map(|(path, spec)| match &spec.role {
                FieldRole::TargetKey { def_type } => Some((path.clone(), def_type.clone())),
                _ => None,
            })
            .collect();

        let mut unreadable = Vec::new();
        let mut excluded = Vec::new();
        for (path, key_type) in target_key_fields {
            let referenced_children = self.referenced_target_shapes(
                session,
                order,
                instances,
                &path,
                &key_type,
                targets,
                &mut unreadable,
                &mut excluded,
            );
            let shape = TargetShape::infer(key_type, &referenced_children);
            schema.target_shapes.insert(path, shape);
        }
        (unreadable, excluded)
    }

    /// One `TargetKey` field's own shape sample: every referenced value
    /// that resolves to the field's own type **and** whose owner is a
    /// member of `targets` or Core ([`owner_in_targets`] — the inference
    /// read-gate, honouring T so a candidate's shape is learned only
    /// from targets that could plausibly matter, never from every
    /// referenced value regardless of ownership), each read back and
    /// reduced to its top-level children. A value outside `targets`/Core
    /// is recorded into `excluded` and never read at all; a value inside
    /// but whose read fails is recorded into `unreadable` — both excluded
    /// from the sample, neither failing the whole candidate.
    #[allow(clippy::too_many_arguments)]
    fn referenced_target_shapes(
        &self,
        session: &mut Session,
        order: &LoadOrder,
        instances: &[(ModId, rim_resolve::domain::InstanceValues)],
        path: &rim_resolve::domain::FieldPath,
        key_type: &str,
        targets: &BTreeSet<ModId>,
        unreadable: &mut Vec<UnreadableTarget>,
        excluded: &mut Vec<ExcludedTarget>,
    ) -> Vec<BTreeSet<String>> {
        let mut referenced_children = Vec::new();
        for name in referenced_values(instances, path) {
            let resolves_to_key_type = session
                .sources()
                .defs_by_name
                .get(&name)
                .is_some_and(|resolved| resolved.iter().any(|(t, _)| t == key_type));
            if !resolves_to_key_type {
                continue;
            }
            if !owner_in_targets(session, key_type, &name, targets) {
                excluded.push(ExcludedTarget {
                    field: path.clone(),
                    def_type: key_type.to_string(),
                    def_name: name,
                });
                continue;
            }
            match resolved_top_level_children(&self.reader, session, order, key_type, &name) {
                Ok(children) => referenced_children.push(children),
                Err(error) => unreadable.push(UnreadableTarget {
                    field: path.clone(),
                    def_type: key_type.to_string(),
                    def_name: name,
                    reason: error.to_string(),
                }),
            }
        }
        referenced_children
    }

    /// Validates `input` against the session's current active mods and
    /// reserved package ids, builds the project, saves it, and makes it
    /// visible on `session`.
    ///
    /// # Errors
    ///
    /// See [`CreateAssignmentError`]. Nothing changes on `session` when
    /// this returns an error — the project is only made visible once it
    /// has been persisted.
    pub fn execute(
        &self,
        session: &mut Session,
        input: CreateAssignmentInput,
    ) -> Result<AssignmentId, CreateAssignmentError> {
        let identity = PatchModIdentity::new(&input.package_id, &input.display_name)?;
        if input.refs.is_empty() {
            return Err(CreateAssignmentError::EmptyRefs);
        }
        // A standalone ("new def") candidate — no `TargetKey` field at
        // all, per `Self::infer_candidate`'s own gate — has nothing to
        // require a target set for; every other candidate still needs
        // T non-empty exactly as before.
        if input.targets.is_empty() && input.schema.has_target_key() {
            return Err(CreateAssignmentError::EmptyTargets);
        }

        let active = session.active_base_ids();
        let reserved = session.reserved_package_ids();
        let package_base = identity.package_id().base();
        if active.contains(&package_base) || reserved.contains(&package_base) {
            return Err(CreateAssignmentError::PackageIdTaken(
                identity.package_id().clone(),
            ));
        }

        let created_at = jiff::Timestamp::now();
        let profile_hash = session.paths().profile_hash().to_string();
        let id = AssignmentId::derive(&profile_hash, identity.package_id(), created_at);
        let mut project = AssignmentProject::new(
            id.clone(),
            input.name,
            identity,
            input.refs,
            input.targets,
            input.schema,
            created_at,
        );
        project.set_excluded_refs(input.excluded_refs);

        if let Err(error) = self.store.save(&session.paths().profile_dir, &project) {
            return Err(CreateAssignmentError::Store(error));
        }
        session.upsert_assignment(project);
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{DefSourceError, ElementExpectation};
    use crate::test_support::{InMemoryAssignmentProjectStore, session_with_sources_and_mods};
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::{DefEntry, XmlLocator};
    use rim_resolve::test_support::ReportBuilder;
    use std::collections::BTreeMap;
    use std::path::Path;
    use std::sync::Arc;

    struct FakeReader {
        by_ordinal: BTreeMap<u32, String>,
    }

    impl DefSourceReader for FakeReader {
        fn read_element(
            &self,
            locator: &XmlLocator,
            _expected: &ElementExpectation,
        ) -> Result<String, DefSourceError> {
            self.by_ordinal
                .get(&locator.element_path[0])
                .cloned()
                .ok_or_else(|| DefSourceError::Io {
                    file: locator.file.to_path_buf(),
                    message: "not seeded".to_string(),
                })
        }
    }

    fn locator(ordinal: u32) -> XmlLocator {
        XmlLocator::new(Arc::from(Path::new("Defs/fixture.xml")), vec![ordinal])
    }

    /// A minimal fixture: `example.framework` owns 5 `example.PartAssignmentDef`
    /// instances, each naming one of 5 races owned by `target.races`
    /// (each race a plain `ThingDef` with a `<race/>` marker child) —
    /// enough to clear `MIN_RESOLVED_DISTINCT`/`TYPE_COVERAGE_MIN` and
    /// exercise 3.3's own end-to-end shape.
    fn fixture() -> (SourceIndex, FakeReader) {
        let mut index = SourceIndex::default();
        let mut ordinal = 0u32;
        let mut by_ordinal = BTreeMap::new();

        for i in 0..5 {
            let group_name = format!("Group_R{i}");
            let race_name = format!("Race{i}");
            index.defs.insert(
                (
                    ModId::new("example.framework"),
                    ("example.PartAssignmentDef".to_string(), group_name.clone()),
                ),
                vec![DefEntry {
                    def_type: "example.PartAssignmentDef".to_string(),
                    def_name: group_name.clone(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: locator(ordinal),
                }],
            );
            index.owners_by_def.insert(
                ("example.PartAssignmentDef".to_string(), group_name.clone()),
                vec![ModId::new("example.framework")],
            );
            by_ordinal.insert(ordinal,
                format!("<example.PartAssignmentDef><defName>{group_name}</defName><speciesNames><li>{race_name}</li></speciesNames></example.PartAssignmentDef>"
                ));
            ordinal += 1;

            index.defs.insert(
                (
                    ModId::new("target.races"),
                    ("ThingDef".to_string(), race_name.clone()),
                ),
                vec![DefEntry {
                    def_type: "ThingDef".to_string(),
                    def_name: race_name.clone(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: locator(ordinal),
                }],
            );
            index.owners_by_def.insert(
                ("ThingDef".to_string(), race_name.clone()),
                vec![ModId::new("target.races")],
            );
            index
                .defs_by_name
                .entry(race_name.clone())
                .or_default()
                .push(("ThingDef".to_string(), ModId::new("target.races")));
            by_ordinal.insert(
                ordinal,
                format!("<ThingDef><defName>{race_name}</defName><race/></ThingDef>"),
            );
            ordinal += 1;
        }

        (index, FakeReader { by_ordinal })
    }

    fn report() -> rim_analyzer::domain::Report {
        ReportBuilder::new()
            .mod_("example.framework")
            .mod_("target.races")
            .build()
    }

    #[test]
    fn list_candidates_names_example_part_assignment_def_with_its_count_and_owner() {
        let (sources, reader) = fixture();
        let session = session_with_sources_and_mods(
            sources,
            report(),
            &["example.framework", "target.races"],
        );
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
        let targets: BTreeSet<ModId> = [ModId::new("target.races")].into_iter().collect();

        // Immutable `&Session` — `list_candidates` is cheap and read-only,
        // no `AssignmentInstances`/`DefSourceReader` call at all.
        let candidates = use_case.list_candidates(&session, &refs, &BTreeSet::new(), &targets);

        let summary = candidates
            .into_iter()
            .find(|c| c.def_type == "example.PartAssignmentDef")
            .expect("example.PartAssignmentDef must be listed as a candidate");
        assert_eq!(summary.instance_count, 5);
        assert_eq!(
            summary.owners,
            BTreeSet::from([ModId::new("example.framework")])
        );
    }

    #[test]
    fn infer_candidate_matches_3_3s_expectations() {
        let (sources, reader) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report(),
            &["example.framework", "target.races"],
        );
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
        let targets: BTreeSet<ModId> = [ModId::new("target.races")].into_iter().collect();

        let candidate = use_case
            .infer_candidate(
                &mut session,
                &refs,
                &BTreeSet::new(),
                &targets,
                "example.PartAssignmentDef",
            )
            .expect("infer_candidate must succeed")
            .expect("example.PartAssignmentDef must be a valid candidate (has a TargetKey field)");

        assert_eq!(candidate.def_type, "example.PartAssignmentDef");
        assert!(
            candidate.unreadable_targets.is_empty(),
            "{:?}",
            candidate.unreadable_targets
        );
        assert!(
            candidate.excluded_targets.is_empty(),
            "every referenced race is owned by target.races, which is in T: {:?}",
            candidate.excluded_targets
        );
        let race_names_field: rim_resolve::domain::FieldPath = "speciesNames".parse().unwrap();
        assert_eq!(
            candidate.schema.fields[&race_names_field].role,
            FieldRole::TargetKey {
                def_type: "ThingDef".to_string()
            }
        );
        let shape = candidate
            .schema
            .target_shapes
            .get(&race_names_field)
            .expect("a TargetKey field must get a computed shape");
        assert!(
            shape.required_children.contains("race"),
            "every referenced race carries <race/>: {shape:?}"
        );
    }

    /// A single referenced target's own
    /// read failure must not fail the whole `infer_candidate` call — the
    /// field's shape is still learned from every other referenced target,
    /// and the failed one is reported as a caveat on the candidate.
    #[test]
    fn a_single_unreadable_target_is_reported_as_a_caveat_not_an_infer_failure() {
        let (sources, reader) = fixture();
        // Race0's own locator is ordinal 1 (`fixture`'s own loop: group at
        // `ordinal`, race def at `ordinal + 1`) — drop it, simulating a
        // stale/missing file for exactly one referenced target.
        let mut by_ordinal = reader.by_ordinal.clone();
        by_ordinal.remove(&1);
        let reader = FakeReader { by_ordinal };
        let mut session = session_with_sources_and_mods(
            sources,
            report(),
            &["example.framework", "target.races"],
        );
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
        let targets: BTreeSet<ModId> = [ModId::new("target.races")].into_iter().collect();

        let candidate = use_case
            .infer_candidate(
                &mut session,
                &refs,
                &BTreeSet::new(),
                &targets,
                "example.PartAssignmentDef",
            )
            .expect("one unreadable target must not fail the whole infer_candidate call")
            .expect("the PartAssignmentDef candidate must still be inferred");

        assert_eq!(
            candidate.unreadable_targets.len(),
            1,
            "{:?}",
            candidate.unreadable_targets
        );
        let unreadable = &candidate.unreadable_targets[0];
        assert_eq!(unreadable.def_type, "ThingDef");
        assert_eq!(unreadable.def_name, "Race0");

        let race_names_field: rim_resolve::domain::FieldPath = "speciesNames".parse().unwrap();
        let shape = candidate
            .schema
            .target_shapes
            .get(&race_names_field)
            .expect("a shape must still be learned from the other 4 targets");
        assert!(
            shape.required_children.contains("race"),
            "the shape is still learned from every OTHER referenced target: {shape:?}"
        );
    }

    /// Honouring T: a referenced race owned by a mod outside
    /// `targets` (and not Core) is never even read back — excluded from
    /// the sample, not merely failing to read.
    #[test]
    fn a_referenced_target_owned_outside_t_is_excluded_not_read() {
        let (sources, reader) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report(),
            &["example.framework", "target.races"],
        );
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
        // An empty T excludes every referenced race (none is Core), so
        // every one of the five ends up excluded, never read.
        let candidate = use_case
            .infer_candidate(
                &mut session,
                &refs,
                &BTreeSet::new(),
                &BTreeSet::new(),
                "example.PartAssignmentDef",
            )
            .expect("infer_candidate must succeed")
            .expect("example.PartAssignmentDef must still be a valid candidate");

        assert_eq!(
            candidate.excluded_targets.len(),
            5,
            "{:?}",
            candidate.excluded_targets
        );
        assert!(candidate.unreadable_targets.is_empty());
        let race_names_field: rim_resolve::domain::FieldPath = "speciesNames".parse().unwrap();
        let shape = &candidate.schema.target_shapes[&race_names_field];
        assert!(
            shape.required_children.is_empty(),
            "no referenced target was ever read, so nothing informs the shape: {shape:?}"
        );
    }

    #[test]
    fn creates_and_persists_a_valid_assignment() {
        let (sources, reader) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report(),
            &["example.framework", "target.races"],
        );
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
        let targets: BTreeSet<ModId> = [ModId::new("target.races")].into_iter().collect();
        let schema = use_case
            .infer_candidate(
                &mut session,
                &refs,
                &BTreeSet::new(),
                &targets,
                "example.PartAssignmentDef",
            )
            .expect("infer_candidate must succeed")
            .expect("the PartAssignmentDef candidate")
            .schema;

        let id = use_case
            .execute(
                &mut session,
                CreateAssignmentInput {
                    name: "Example race patch".to_string(),
                    package_id: "mypatch.parts".to_string(),
                    display_name: "Sample Part Patch".to_string(),
                    refs,
                    excluded_refs: BTreeSet::new(),
                    targets,
                    schema,
                },
            )
            .expect("a valid project must be created");

        let project = session.assignment(&id).expect("must be visible");
        assert_eq!(project.name(), "Example race patch");
        assert!(use_case.store.last_saved(&id).is_some());
    }

    #[test]
    fn rejects_an_empty_ref_set() {
        let (sources, reader) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report(),
            &["example.framework", "target.races"],
        );
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);

        let result = use_case.execute(
            &mut session,
            CreateAssignmentInput {
                name: "n".to_string(),
                package_id: "mypatch.parts".to_string(),
                display_name: "d".to_string(),
                refs: BTreeSet::new(),
                excluded_refs: BTreeSet::new(),
                targets: [ModId::new("target.races")].into_iter().collect(),
                schema: AssignmentSchema {
                    def_type: "example.PartAssignmentDef".to_string(),
                    refs: BTreeSet::new(),
                    fields: BTreeMap::new(),
                    target_shapes: BTreeMap::new(),
                },
            },
        );

        assert_eq!(result, Err(CreateAssignmentError::EmptyRefs));
    }

    #[test]
    fn a_failed_save_never_makes_the_project_visible() {
        let (sources, reader) = fixture();
        let mut session = session_with_sources_and_mods(
            sources,
            report(),
            &["example.framework", "target.races"],
        );
        let store = InMemoryAssignmentProjectStore::new();
        store.fail_next_save();
        let use_case = CreateAssignment::new(store, reader);

        let result = use_case.execute(
            &mut session,
            CreateAssignmentInput {
                name: "n".to_string(),
                package_id: "mypatch.parts".to_string(),
                display_name: "d".to_string(),
                refs: [ModId::new("example.framework")].into_iter().collect(),
                excluded_refs: BTreeSet::new(),
                targets: [ModId::new("target.races")].into_iter().collect(),
                schema: AssignmentSchema {
                    def_type: "example.PartAssignmentDef".to_string(),
                    refs: BTreeSet::new(),
                    fields: BTreeMap::new(),
                    target_shapes: BTreeMap::new(),
                },
            },
        );

        assert!(matches!(result, Err(CreateAssignmentError::Store(_))));
        assert_eq!(session.assignments().count(), 0);
    }

    // -- standalone ("new def") candidates: a def type with no TargetKey
    //    field at all (see `CreateAssignment::infer_candidate`'s own doc
    //    comment). ------------------------------------------------------

    /// `example.framework` owns 5 `example.PartDef`-like instances, each with
    /// only a scalar `hediffName` field — nothing here ever resolves to a
    /// def outside R, so no field can ever classify as `TargetKey` (rule
    /// 2's reference gate has nothing to vote on at all).
    fn standalone_fixture() -> (SourceIndex, FakeReader) {
        let mut index = SourceIndex::default();
        let mut by_ordinal = BTreeMap::new();

        for i in 0..5u32 {
            let part_name = format!("Part{i}");
            index.defs.insert(
                (
                    ModId::new("example.framework"),
                    ("example.PartDef".to_string(), part_name.clone()),
                ),
                vec![DefEntry {
                    def_type: "example.PartDef".to_string(),
                    def_name: part_name.clone(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: locator(i),
                }],
            );
            index.owners_by_def.insert(
                ("example.PartDef".to_string(), part_name.clone()),
                vec![ModId::new("example.framework")],
            );
            by_ordinal.insert(i,
                format!("<example.PartDef><defName>{part_name}</defName><hediffName>Hediff{i}</hediffName></example.PartDef>"
                ));
        }

        (index, FakeReader { by_ordinal })
    }

    fn standalone_report() -> rim_analyzer::domain::Report {
        ReportBuilder::new().mod_("example.framework").build()
    }

    #[test]
    fn list_candidates_includes_a_def_type_with_no_target_key_field() {
        let (sources, reader) = standalone_fixture();
        let session =
            session_with_sources_and_mods(sources, standalone_report(), &["example.framework"]);
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();

        let candidates =
            use_case.list_candidates(&session, &refs, &BTreeSet::new(), &BTreeSet::new());

        // Phase 1 lists every def type R owns, regardless of whether it
        // will later turn out to have a TargetKey field — that gate only
        // applies once `infer_candidate` actually infers the schema.
        assert!(
            candidates.iter().any(|c| c.def_type == "example.PartDef"),
            "{candidates:?}"
        );
    }

    #[test]
    fn infer_candidate_accepts_a_no_target_key_schema_when_targets_is_empty() {
        let (sources, reader) = standalone_fixture();
        let mut session =
            session_with_sources_and_mods(sources, standalone_report(), &["example.framework"]);
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();

        let candidate = use_case
            .infer_candidate(
                &mut session,
                &refs,
                &BTreeSet::new(),
                &BTreeSet::new(),
                "example.PartDef",
            )
            .expect("infer_candidate must succeed")
            .expect("a no-TargetKey schema must be a valid standalone candidate when T is empty");

        assert!(
            !candidate.schema.has_target_key(),
            "{:?}",
            candidate.schema.fields
        );
        assert!(candidate.unreadable_targets.is_empty());
        assert!(candidate.excluded_targets.is_empty());
    }

    #[test]
    fn infer_candidate_rejects_a_no_target_key_schema_when_targets_is_non_empty() {
        let (sources, reader) = standalone_fixture();
        let mut session =
            session_with_sources_and_mods(sources, standalone_report(), &["example.framework"]);
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
        let targets: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();

        let candidate = use_case
            .infer_candidate(
                &mut session,
                &refs,
                &BTreeSet::new(),
                &targets,
                "example.PartDef",
            )
            .expect("infer_candidate must succeed");

        assert!(
            candidate.is_none(),
            "a no-TargetKey schema is never offered once a target set is selected: {candidate:?}"
        );
    }

    /// `infer_candidate_for_explicit_add`'s own half of the free-standing
    /// section gate: unlike
    /// [`infer_candidate_rejects_a_no_target_key_schema_when_targets_is_non_empty`]
    /// right above, an *explicitly requested* free-standing type must
    /// still be a valid candidate once a target set is selected — this is
    /// `CreateAssignment`'s own unit-level pin of the gate;
    /// `AddAssignmentSection`'s use-case-level regression lives in
    /// `add_assignment_section.rs`'s
    /// `adds_a_free_standing_section_when_the_project_already_has_a_non_empty_target_set`.
    #[test]
    fn infer_candidate_for_explicit_add_accepts_a_no_target_key_schema_even_when_targets_is_non_empty()
     {
        let (sources, reader) = standalone_fixture();
        let mut session =
            session_with_sources_and_mods(sources, standalone_report(), &["example.framework"]);
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
        let targets: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();

        let candidate = use_case
            .infer_candidate_for_explicit_add(
                &mut session,
                &refs,
                &BTreeSet::new(),
                &targets,
                "example.PartDef",
            )
            .expect("infer_candidate_for_explicit_add must succeed")
            .expect(
                "an explicitly requested no-TargetKey schema stays valid once a target set is \
                 selected",
            );

        assert!(!candidate.schema.has_target_key());
    }

    #[test]
    fn creates_and_persists_a_standalone_project_with_no_targets() {
        let (sources, reader) = standalone_fixture();
        let mut session =
            session_with_sources_and_mods(sources, standalone_report(), &["example.framework"]);
        let use_case = CreateAssignment::new(InMemoryAssignmentProjectStore::new(), reader);
        let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
        let schema = use_case
            .infer_candidate(
                &mut session,
                &refs,
                &BTreeSet::new(),
                &BTreeSet::new(),
                "example.PartDef",
            )
            .expect("infer_candidate must succeed")
            .expect("the PartDef candidate")
            .schema;

        let id = use_case
            .execute(
                &mut session,
                CreateAssignmentInput {
                    name: "New body part".to_string(),
                    package_id: "sample.newpart".to_string(),
                    display_name: "Sample's New Part".to_string(),
                    refs,
                    excluded_refs: BTreeSet::new(),
                    targets: BTreeSet::new(),
                    schema,
                },
            )
            .expect("a standalone project needs no non-empty target set");

        let project = session.assignment(&id).expect("must be visible");
        assert!(
            project
                .section("example.PartDef")
                .expect("the created section")
                .is_standalone()
        );
    }
}
