//! Rendering the project's `Defs/` files, its dependency and scope sets, and the mod folder.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_merge::assign::{self};
use rim_merge::emit::{self, AboutSpec, Dependencies, EmitInput, Provenance, RenderedMod};
use rim_resolve::domain::{
    AssignmentId, AssignmentProject, AssignmentRow, RowKey, Section, TargetRef,
};

use super::outcome::{AssignmentSkip, ExportAssignmentError};
use super::validation::{build_gates, validate_and_clean_sections};
use crate::Session;
use crate::assignment_refs::effective_refs;
use crate::use_cases::render_merge_mod::{RIMMERGE_VERSION, major_minor};

/// [`render_defs`]'s own result: the rendered `Defs/` files, the
/// item-slot-validated sections ([`dependency_and_scope`] needs them again
/// to compute `modDependencies`), and every skip from both the pre-render
/// validation and the engine's own render.
pub(super) struct RenderedProject {
    pub(super) files: Vec<emit::RenderedDefsFile>,
    pub(super) sections: BTreeMap<String, Section>,
    pub(super) skipped: Vec<AssignmentSkip>,
}

/// The render-time half of [`ExportAssignment::execute`](crate::use_cases::export_assignment::ExportAssignment::execute): validates every
/// section's item slots ([`validate_and_clean_sections`]), computes every
/// target-keyed row's [`TargetGate`](rim_merge::assign::TargetGate) ([`build_gates`]), and renders every
/// section's own `Defs/` file in one call ([`assign::render_sections`]) —
/// folding both steps' own skips into one `skipped` list. Returns
/// [`ExportAssignmentError::NothingToExport`] when
/// nothing rendered at all (no rows anywhere, or every row skipped).
pub(super) fn render_defs(
    session: &Session,
    project: &AssignmentProject,
) -> Result<RenderedProject, ExportAssignmentError> {
    let (sections, mut skipped) = validate_and_clean_sections(session, project);
    let gates = build_gates(session, &sections);

    let (files, render_skipped) = assign::render_sections(&sections, &gates);
    skipped.extend(render_skipped);
    if files.is_empty() {
        return Err(ExportAssignmentError::NothingToExport);
    }
    Ok(RenderedProject {
        files,
        sections,
        skipped,
    })
}

/// The `modDependencies`/`loadAfter` split (`assign::dependencies`
/// for `modDependencies`, unioned with `assign::load_after_only`'s own T
/// for `loadAfter` alone) plus the marker's `scope` — the
/// *effective*, closure-applied reference set (minus `excluded_refs`)
/// unioned with T. `scope` answers "what is this generated mod about"
/// (`GeneratedMods`' own hiding rule); `depends_on`/`load_after_only`
/// answer "what does it declare to RimWorld" — related but genuinely
/// distinct, grouped in one struct only because [`render_mod`] needs all
/// three for one `EmitInput`.
pub(super) struct DependencySets {
    depends_on: BTreeSet<ModId>,
    load_after_only: BTreeSet<ModId>,
    scope: BTreeSet<ModId>,
}

/// `modDependencies` across *every* section: each
/// target-keyed section contributes through [`assign::dependencies`],
/// each free-standing one through [`assign::standalone_dependencies`] —
/// the two are separate public functions in `rim-merge`, so this is
/// where a multi-section project
/// unions their results. `own_package_id` is threaded to both so an
/// `ItemSlot` naming this project's own free-standing row from a
/// *different* section never becomes a dependency on itself.
pub(super) fn dependency_and_scope(
    session: &Session,
    project: &AssignmentProject,
    sections: &BTreeMap<String, Section>,
) -> DependencySets {
    let resolve = |value: &str| {
        session
            .sources()
            .defs_by_name
            .get(value)
            .cloned()
            .unwrap_or_default()
    };
    let dll_owner = |type_name: &str| session.sources().dll_owner_of(type_name).cloned();
    let own_package_id = project.identity().package_id();

    let mut depends_on = BTreeSet::new();
    for section in sections.values() {
        let schema = &section.schema;
        if section.is_standalone() {
            let rows: BTreeMap<String, AssignmentRow> = section
                .rows
                .iter()
                .filter_map(|(key, row)| match key {
                    RowKey::Own(name) => Some((name.clone(), row.clone())),
                    RowKey::Target(_) => None,
                })
                .collect();
            depends_on.extend(assign::standalone_dependencies(
                schema,
                &rows,
                own_package_id,
                &resolve,
                &dll_owner,
            ));
        } else {
            let rows: BTreeMap<TargetRef, AssignmentRow> = section
                .rows
                .iter()
                .filter_map(|(key, row)| match key {
                    RowKey::Target(target) => Some((target.clone(), row.clone())),
                    RowKey::Own(_) => None,
                })
                .collect();
            depends_on.extend(assign::dependencies(
                schema,
                &rows,
                own_package_id,
                &resolve,
                &dll_owner,
            ));
        }
    }
    let load_after_only = assign::load_after_only(project.targets());

    let effective = effective_refs(project.refs(), session.report());
    let mut scope: BTreeSet<ModId> = effective
        .difference(project.excluded_refs())
        .cloned()
        .collect();
    scope.extend(project.targets().iter().cloned());

    DependencySets {
        depends_on,
        load_after_only,
        scope,
    }
}

/// Assembles `project`'s own `EmitInput` (identity, dependencies, the
/// already-rendered `Defs/` files, the `Provenance::Assignment` marker)
/// and renders it. Returns the rendered mod alongside
/// [`AssignmentProject::content_sha256`] (computed once here, needed both
/// by the marker and by [`AssignmentExportOutcome::content_sha256`](crate::use_cases::export_assignment::outcome::AssignmentExportOutcome::content_sha256)).
pub(super) fn render_mod(
    session: &Session,
    id: &AssignmentId,
    project: &AssignmentProject,
    identity: &emit::GeneratedModIdentity,
    files: &[emit::RenderedDefsFile],
    dependencies: &DependencySets,
) -> Result<(RenderedMod, String), ExportAssignmentError> {
    let mod_names: BTreeMap<ModId, String> = session
        .report()
        .mods
        .iter()
        .map(|m| (m.id.clone(), m.name.clone()))
        .collect();
    let game_version = major_minor(&session.report().metadata.game_version);
    let content_sha256 = project.content_sha256();
    let profile_hash = session.paths().profile_hash().to_string();

    let input = EmitInput {
        about: AboutSpec {
            identity,
            author: project.author(),
            description: project.description(),
            dependencies: Dependencies::ExactlyWithLoadAfter {
                depends_on: &dependencies.depends_on,
                load_after_only: &dependencies.load_after_only,
            },
        },
        game_version: &game_version,
        plans: &[],
        defs: files,
        assets: &[],
        mod_names: &mod_names,
        provenance: Provenance::Assignment {
            assignment_id: id.as_str(),
            profile_hash: &profile_hash,
            scope: &dependencies.scope,
            content_sha256: &content_sha256,
        },
        rimmerge_version: RIMMERGE_VERSION,
    };
    let rendered = emit::render(&input)?;
    Ok((rendered, content_sha256))
}
