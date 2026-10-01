//! What the emitter is given: the about spec, dependencies, provenance, and shipped assets.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rim_analyzer::domain::ModId;
pub use rim_resolve::domain::GeneratedModIdentity;

use super::rendered::RenderedDefsFile;
use crate::plan::MergePlan;

/// One `TextureOverride` choice, ready to copy into the generated mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetCopy {
    /// The shared, normalized texture key (display only here).
    pub texture_path: String,
    /// The owner whose file this copies.
    pub from: ModId,
    /// The real file to copy from.
    pub source: PathBuf,
    /// Where it lands under the generated mod's `Textures/` folder.
    pub relative_target: PathBuf,
}

/// How [`render`](crate::emit::render) resolves `About.xml`'s `modDependencies`/`loadAfter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dependencies<'a> {
    /// The union of every op's `depends_on` and every
    /// asset's `from` (Core never appears in either — it's never in a
    /// `depends_on` set to begin with). What the profile merge mod uses.
    FromContent,
    /// A compat patch's behaviour: `modDependencies` and `loadAfter` are
    /// exactly this set, nothing more, nothing less — so a published
    /// patch always declares (and depends on) precisely the mods its
    /// scope names, regardless of which of them a given op happens to
    /// touch. [`render`](crate::emit::render) refuses with [`EmitError::UndeclaredDependency`](crate::emit::rendered::EmitError::UndeclaredDependency)
    /// if any op or asset in this input depends on a mod outside it — a
    /// patch can never ship an op gated on a mod it doesn't declare.
    Exactly(&'a BTreeSet<ModId>),
    /// An assignment export's behaviour: `modDependencies` is exactly
    /// `depends_on` (the same "declares and depends on precisely this set"
    /// contract as [`Self::Exactly`], and [`render`](crate::emit::render)
    /// refuses the same way if any op or asset depends on a mod outside it
    /// — never reachable in practice for an assignment render, since it
    /// carries no plans/assets, but kept for the same safety-net reasoning
    /// `Exactly`'s own doc comment gives); `loadAfter` is the *union* of
    /// `depends_on` and `load_after_only` — a target mod gated only through
    /// a row's own `MayRequire`, never a hard dependency, still needs a
    /// load-order constraint relative to this export's own mod (R in both
    /// sets, T in `loadAfter` only).
    ExactlyWithLoadAfter {
        /// `modDependencies`, and the base of `loadAfter`.
        depends_on: &'a BTreeSet<ModId>,
        /// Extra `loadAfter` members beyond `depends_on` — never added to
        /// `modDependencies`.
        load_after_only: &'a BTreeSet<ModId>,
    },
}

/// Parameterizes `About/About.xml`'s identity-ish fields. The profile
/// merge mod and a compat patch export render very different values here
/// (a hash-derived identity and a fixed author/description versus a
/// user-chosen identity, author, and description, plus a restricted
/// dependency set) — `emit` renders whichever it's given without caring
/// which is which.
#[derive(Debug, Clone, Copy)]
pub struct AboutSpec<'a> {
    /// The generated mod's identity (package id, folder name, display
    /// name).
    pub identity: &'a GeneratedModIdentity,
    /// `<author>`.
    pub author: &'a str,
    /// `<description>`.
    pub description: &'a str,
    /// How to resolve `modDependencies`/`loadAfter`.
    pub dependencies: Dependencies<'a>,
}

/// What generated this render — feeds `rimmerge.json`, the marker
/// `rim_analyzer::infra::discovery` reads back.
#[derive(Debug, Clone, Copy)]
pub enum Provenance<'a> {
    /// The profile merge mod: the base `rimmerge.json` shape, plus
    /// `"kind":"merge"`.
    ProfileMerge {
        /// An ISO-8601 timestamp — this crate has no clock of its own
        /// (pure, no IO), so the caller supplies it.
        generated_at: &'a str,
        /// The active profile's hash.
        profile_hash: &'a str,
        /// A hex digest of the `decisions.json` bytes this render came
        /// from.
        decisions_sha256: &'a str,
    },
    /// A compat patch export. Deliberately carries no timestamp, so two
    /// exports of the same decisions and scope are byte-identical.
    CompatPatch {
        /// The patch project's id.
        patch_id: &'a str,
        /// The active profile's hash.
        profile_hash: &'a str,
        /// The patch's declared scope.
        scope: &'a BTreeSet<ModId>,
        /// A hex digest of this patch's own decisions
        /// (`PatchProject::decisions_sha256`).
        decisions_sha256: &'a str,
    },
    /// An exported assignment project. Deliberately carries no timestamp, mirroring
    /// [`Self::CompatPatch`], so two exports of the same rows are
    /// byte-identical.
    Assignment {
        /// The assignment project's id — written to `rimmerge.json`'s
        /// `assignmentId` key, [`rim_resolve::domain::GeneratedMarker::patch_id`]'s
        /// wire name for this kind (`rim_analyzer::extract::rimmerge_marker`
        /// already parses it back into that same slot).
        assignment_id: &'a str,
        /// The active profile's hash.
        profile_hash: &'a str,
        /// This export's declared scope: R ∪ T.
        scope: &'a BTreeSet<ModId>,
        /// A hex digest of the project's own content
        /// (`AssignmentProject::content_sha256`: refs, targets, schema,
        /// and rows) — `decisions_sha256`'s counterpart for a project
        /// with no `DecisionSet` of its own.
        content_sha256: &'a str,
    },
}

/// Everything [`render`](crate::emit::render) needs.
#[derive(Debug, Clone, Copy)]
pub struct EmitInput<'a> {
    /// `About/About.xml`'s identity-ish fields.
    pub about: AboutSpec<'a>,
    /// The game's major.minor version, for `supportedVersions`.
    pub game_version: &'a str,
    /// Every plan to emit — must already be complete
    /// (`unresolved.is_empty()`); [`render`](crate::emit::render) rejects one that isn't.
    pub plans: &'a [MergePlan],
    /// Every already-rendered `Defs/` file to write verbatim (an
    /// assignment project's own instances) — empty for a profile merge
    /// or a compat patch, which never emit `Defs/`.
    pub defs: &'a [RenderedDefsFile],
    /// Every texture to copy in.
    pub assets: &'a [AssetCopy],
    /// Display names for every mod that might need one (`About.xml`'s
    /// `modDependencies`) — a source id missing here falls back to its
    /// own id as its display name rather than failing the whole render.
    pub mod_names: &'a BTreeMap<ModId, String>,
    /// What generated this render, for `rimmerge.json`.
    pub provenance: Provenance<'a>,
    /// This build's own version string, for `rimmerge.json`.
    pub rimmerge_version: &'a str,
}
