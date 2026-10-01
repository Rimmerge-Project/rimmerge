//! [`ExportAssignment`]: renders one assignment (patch maker) project's
//! own emitted instances and writes them to a user-chosen folder — never
//! `<game>/Mods` by default — optionally installing (copying the same
//! rendered folder into `Mods/` and activating it). Mirrors `ExportPatch`
//! exactly, sharing its
//! folder-safety/install core via `export_folder.rs`.
//!
//! **Why not `RenderTarget::Assignment(&project)`**: `RenderTarget`/`MergeModRender`
//! (`render_merge_mod.rs`) are shaped entirely around a
//! `DecisionSet`/`MergePlan` — every field (`entries: Vec<MergeModEntry>`,
//! `skipped: Vec<FindingKey>`) means something for a `Merge`/`ShipAsset`
//! decision that has no assignment-project counterpart at all. An
//! assignment project has no decisions and renders straight from its own
//! `AssignmentSchema`/rows (`rim_merge::assign::render_rows`, never a
//! `MergePlan`); forcing that shape through `RenderTarget`/`MergeModRender`
//! would mean either bloating both types for every merge/patch caller with
//! fields only this one ever populates, or lossily mapping
//! `rim_merge::assign::SkippedField` into `FindingKey`-shaped concepts it
//! doesn't fit. This use case renders directly instead
//! (`rim_merge::assign::render_rows` + `rim_merge::emit::render`), reusing
//! only what genuinely is shared: `export_folder.rs`'s folder-safety/
//! install core, and `render_merge_mod.rs`'s own `major_minor`/
//! `RIMMERGE_VERSION`.

use std::path::{Path, PathBuf};

use rim_analyzer::domain::GeneratedKind;
use rim_merge::emit::RenderedMod;
use rim_resolve::domain::AssignmentId;

use crate::ports::{AssignmentProjectStore, MergeModWriter, ModsConfigStore};
use crate::use_cases::export_folder::{self};
use crate::{Session, UnknownAssignment};
use rendering::{dependency_and_scope, render_defs, render_mod};

mod outcome;
mod rendering;
mod validation;

pub use outcome::{
    AssignmentExportOptions, AssignmentExportOutcome, AssignmentSkip, ExportAssignmentError,
};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "export_assignment/export_assignment_tests.rs"]
mod tests;

/// Renders, writes, and (when asked) installs one assignment project.
pub struct ExportAssignment<Writer, Config, Store> {
    writer: Writer,
    config_store: Config,
    store: Store,
}

impl<Writer, Config, Store> ExportAssignment<Writer, Config, Store>
where
    Writer: MergeModWriter,
    Config: ModsConfigStore,
    Store: AssignmentProjectStore,
{
    /// Builds the use case from its ports. Needs no [`crate::ports::DefSourceReader`]:
    /// a row's own values are already stored on the project, and the
    /// `modDependencies`/`loadAfter`/gate lookups this use case needs all
    /// read straight off [`Session::sources`]'s already-scanned index.
    #[must_use]
    pub fn new(writer: Writer, config_store: Config, store: Store) -> Self {
        Self {
            writer,
            config_store,
            store,
        }
    }

    /// # Errors
    ///
    /// See [`ExportAssignmentError`].
    pub fn execute(
        &self,
        session: &mut Session,
        id: &AssignmentId,
        options: AssignmentExportOptions,
    ) -> Result<AssignmentExportOutcome, ExportAssignmentError> {
        if !options.out_dir.is_absolute() {
            return Err(ExportAssignmentError::OutDirNotAbsolute(options.out_dir));
        }
        let project = session
            .assignment(id)
            .cloned()
            .ok_or_else(|| UnknownAssignment(id.clone()))?;

        let rendered_defs = render_defs(session, &project)?;

        let identity = project.identity().as_generated();
        let mods_dir = session.paths().game_dir.join("Mods");
        if export_folder::is_inside(&options.out_dir, &mods_dir) {
            return Err(ExportAssignmentError::OutDirIsModsFolder(options.out_dir));
        }
        // Both refusals happen before the first write, exactly like
        // `ExportPatch::execute`.
        export_folder::ensure_not_foreign(
            &self.writer,
            &options.out_dir,
            &identity.folder_name,
            GeneratedKind::Assignment,
            id.as_str(),
        )?;
        if options.install {
            export_folder::ensure_not_foreign(
                &self.writer,
                &mods_dir,
                &identity.folder_name,
                GeneratedKind::Assignment,
                id.as_str(),
            )?;
        }

        let dependencies = dependency_and_scope(session, &project, &rendered_defs.sections);
        let (rendered, content_sha256) = render_mod(
            session,
            id,
            &project,
            &identity,
            &rendered_defs.files,
            &dependencies,
        )?;

        let backup_dir = export_folder::backup_dir(session, "assignments", id.as_str(), "prev");
        let report = self
            .writer
            .write(&options.out_dir, &backup_dir, &rendered)?;

        let snapshot = session.assignment_snapshot(id).unwrap_or_else(|| {
            unreachable!("rendering above already confirmed the project is loaded")
        });
        let mut updated = snapshot.clone();
        updated.set_export_dir(Some(options.out_dir.clone()));
        session.upsert_assignment(updated);
        if let Err(error) = self.store.save(
            &session.paths().profile_dir,
            session
                .assignment(id)
                .unwrap_or_else(|| unreachable!("just upserted above")),
        ) {
            session.restore_assignment(snapshot);
            return Err(ExportAssignmentError::Store(error));
        }

        let (installed_path, mods_config_backup) =
            self.maybe_install(session, id, &options, &mods_dir, &rendered)?;

        let out_files = rendered
            .files
            .iter()
            .map(|file| file.relative_path.clone())
            .collect();

        Ok(AssignmentExportOutcome {
            export_path: report.mod_path,
            installed_path,
            mods_config_backup,
            skipped: rendered_defs.skipped,
            files: out_files,
            content_sha256,
        })
    }

    /// [`super::ExportPatch`]'s own `maybe_install`, unchanged in shape.
    fn maybe_install(
        &self,
        session: &mut Session,
        id: &AssignmentId,
        options: &AssignmentExportOptions,
        mods_dir: &Path,
        rendered: &RenderedMod,
    ) -> Result<(Option<PathBuf>, Option<PathBuf>), ExportAssignmentError> {
        if !options.install {
            return Ok((None, None));
        }

        let install_backup_dir =
            export_folder::backup_dir(session, "assignments", id.as_str(), "installed.prev");
        let package_id = session
            .assignment(id)
            .unwrap_or_else(|| unreachable!("still loaded: only ever removed by DeleteAssignment"))
            .identity()
            .package_id()
            .clone();
        let outcome = export_folder::install(
            &self.writer,
            &self.config_store,
            session,
            mods_dir,
            &install_backup_dir,
            rendered,
            &package_id,
        )?;

        Ok((
            Some(outcome.installed_path),
            Some(outcome.mods_config_backup),
        ))
    }
}
