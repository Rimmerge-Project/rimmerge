//! What the emitter produces: rendered files, the rendered mod, and emit errors.

use std::path::PathBuf;

use rim_analyzer::domain::ModId;

use crate::plan::DefKey;

/// One rendered file's content: literal text, or a source path the
/// writer copies verbatim (a texture — copying bytes here would mean
/// reading a file, which this pure crate never does).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    /// Literal file text.
    Text(String),
    /// Copy this source file's bytes verbatim.
    CopyFrom(PathBuf),
}

/// One `Defs/` file to write verbatim into the generated mod — produced
/// by [`crate::assign::render_rows`] (an assignment project's emitted
/// instances) and handed to [`render`](crate::emit::render) already rendered: this crate's
/// emitter never inspects its content, only places it in
/// [`RenderedMod::files`] before the patch files (`Defs/` loads before
/// `Patches/` in the game itself, and this mirrors that).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedDefsFile {
    /// Path relative to the mod's own folder, e.g.
    /// `Defs/rimmerge_example.PartAssignmentDef.xml` (see [`defs_file_path`]).
    pub relative_path: PathBuf,
    /// The file's literal text.
    pub content: String,
}

/// The relative path an assignment def type's instances render into —
/// `Defs/rimmerge_<DefType>.xml`, mirroring [`patch_file_path`](crate::emit::patches::patch_file_path)'s
/// `Patches/` convention and exposed for the same reason: a caller
/// predicting a rendered file's name (e.g. a merge-mod entry listing)
/// should call this rather than re-deriving the same naming scheme.
#[must_use]
pub fn defs_file_path(def_type: &str) -> PathBuf {
    PathBuf::from(format!("Defs/rimmerge_{def_type}.xml"))
}

/// One file the generated mod should have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedFile {
    /// Path relative to the mod's own folder.
    pub relative_path: PathBuf,
    /// Its content.
    pub content: FileContent,
}

/// Everything [`render`](crate::emit::render) produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedMod {
    /// The mod's folder name (relative to the game's `Mods/` directory).
    pub folder_name: String,
    /// Every file, in a stable, deterministic order — including
    /// `rimmerge.json` for a [`Provenance::CompatPatch`](crate::emit::input::Provenance::CompatPatch) render, which
    /// carries no timestamp; only a [`Provenance::ProfileMerge`](crate::emit::input::Provenance::ProfileMerge) render's
    /// `rimmerge.json` varies between two renders of the same input (its
    /// `generated_at`).
    pub files: Vec<RenderedFile>,
}

/// Why [`render`](crate::emit::render) refused to run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EmitError {
    /// A plan in [`EmitInput::plans`](crate::emit::input::EmitInput::plans) still has unresolved fields —
    /// [`render`](crate::emit::render) only ever emits complete plans (the session is
    /// responsible for filtering incomplete ones out and listing them as
    /// skipped).
    #[error(
        "plan for {key} has {unresolved} unresolved field(s); only complete plans can be emitted"
    )]
    IncompletePlan {
        /// The incomplete plan's def.
        key: DefKey,
        /// How many fields still need a choice.
        unresolved: usize,
    },
    /// An op or a `ShipAsset` in this input depends on a mod outside
    /// [`Dependencies::Exactly`](crate::emit::input::Dependencies::Exactly)'s declared set. Unreachable from the
    /// merge editor, which only ever offers a patch's own scope members as
    /// `Merge`/`ShipAsset` choices — this is a safety net against a
    /// hand-edited `patches/<id>.json` naming an out-of-scope owner.
    #[error("{mod_id} is depended on but not declared in this patch's dependency set")]
    UndeclaredDependency {
        /// The undeclared mod (the alphabetically earliest one, when more
        /// than one op or asset depends on a mod outside the declared
        /// set).
        mod_id: ModId,
    },
}
