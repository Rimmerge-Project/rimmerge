//! What an inspection answers: the def's owners, patchers, patch operations, and template
//! ambiguity.

use std::collections::BTreeMap;

use rim_analyzer::domain::{FindModGate, ModId, PatchOp, XmlLocator};
use rim_merge::effective::EffectiveDef;
use rim_merge::plan::Caveat;
use rim_resolve::domain::{DefKey, DefRef, FindingKey, ResolutionStatus};

use crate::ports::DefSourceError;

/// Surfaced on [`DefInspection::template_ambiguity`] whenever the
/// inspected `[@Name]` template has more than one registrant. Per the
/// decompiled `Verse.XmlInheritance.GetBestParentFor`, there is no single
/// "winner" for a duplicated template `Name`, only a per-child answer, and
/// `InspectDef` has no specific child in hand when inspecting a template
/// in the abstract. Rather than silently picking one registrant and
/// calling it `DefInspection::winner`, this reports every real answer this
/// scan can compute, so a caller can see the actual ambiguity instead of
/// trusting a fabricated single owner — `winner` itself still carries a
/// representative (the last-loaded registrant,
/// `def_sources::nearest_owner`'s own `Asking::Nobody` arm), but only ever
/// as one, explicitly-disclosed choice among these.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateAmbiguity {
    /// Every mod registering this `Name`, in load order.
    pub registrants: Vec<ModId>,
    /// Every known child of this `Name` (deduplicated by its own owning
    /// mod — every child def a single mod ships resolves identically,
    /// since the real rule pivots on the *mod's* own load position, not
    /// the individual def) paired with which registrant it actually
    /// resolves to under the selected order, per the real
    /// `Verse.XmlInheritance.GetBestParentFor` rule.
    pub resolutions: BTreeMap<ModId, ModId>,
}

/// One mod that owns (or registers) the inspected def/template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toucher {
    /// The owning mod.
    pub mod_id: ModId,
    /// Its position in the selected order.
    pub position: usize,
    /// Whether this owner is a Rimmerge-generated mod (the profile merge
    /// mod, or an exported compat patch) — shown, never hidden: it's what
    /// the game actually runs.
    pub is_generated: bool,
}

/// One top-level `<Operation>`'s own identity — class, xpath, and gates —
/// enough to show what a patcher's operation targets without replaying
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchOpSummary {
    /// The `Class` attribute, e.g. `PatchOperationReplace`.
    pub class: String,
    /// The raw `<xpath>` text, verbatim.
    pub xpath: Option<String>,
    /// Whatever followed the `defName="..."]`/`Name="..."]` bracket in the
    /// xpath, e.g. `/statBases`.
    pub sub_path: Option<String>,
    /// One gate per enclosing `PatchOperationFindMod` this operation is
    /// nested under.
    pub find_mod_context: Vec<FindModGate>,
    /// `MayRequire` packageIds.
    pub may_require: Vec<String>,
    /// `MayRequireAnyOf` packageIds.
    pub may_require_any_of: Vec<String>,
    /// Where this operation lives on disk.
    pub locator_file: std::path::PathBuf,
    /// Whether this summary describes a *stand-in descendant* rather
    /// than the top-level `<Operation>` node itself — `true` exactly when
    /// [`representative_op`](crate::use_cases::def_sources::representative_op) had to reach past a non-mutating
    /// `PatchOperationSequence`/`FindMod`/`Conditional` wrapper (see its
    /// own doc comment for why: `SourceIndex::patch_ops_by_def` never
    /// indexes the wrapper itself).
    pub is_wrapped: bool,
}

impl PatchOpSummary {
    /// Built from `op`'s own fields, at `top_locator`'s file. `op` is
    /// **not** always the top-level `<Operation>` node itself — see
    /// [`representative_op`](crate::use_cases::def_sources::representative_op)'s own doc comment for when it's a
    /// stand-in descendant's, which [`Self::is_wrapped`] flags.
    pub(super) fn from_op(op: &PatchOp, top_locator: &XmlLocator) -> Self {
        Self {
            class: op.class.clone(),
            xpath: op.xpath.clone(),
            sub_path: op.target.as_ref().and_then(|t| t.sub_path.clone()),
            find_mod_context: op.find_mod_context.clone(),
            may_require: op.may_require.clone(),
            may_require_any_of: op.may_require_any_of.clone(),
            locator_file: top_locator.file.to_path_buf(),
            is_wrapped: op.locator.element_path.len() > 1,
        }
    }
}

/// One mod patching this def/template, foreign or self.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patcher {
    /// The patching mod.
    pub mod_id: ModId,
    /// Its position in the selected order.
    pub position: usize,
    /// Whether this patcher is a Rimmerge-generated mod.
    pub is_generated: bool,
    /// One summary per top-level operation this mod aims at the target.
    pub ops: Vec<PatchOpSummary>,
    /// Whether this mod's own top-level operations replayed cleanly as
    /// part of [`DefInspection::effective`]'s own full, load-ordered
    /// fold — `Err` names the fold's own stopper error, verbatim, when
    /// the stopper is one of this mod's own ops. `Ok(())` when the fold
    /// reached and cleared every one of this mod's ops (see
    /// [`Self::reached`]) *or* never got far enough to find out — the
    /// latter is "no evidence of failure", not a verified success, which
    /// is exactly what `reached` distinguishes. Derived from the same
    /// fold [`DefInspection::effective`] already ran (never a second,
    /// standalone `patch_eval::replay` call against this mod's ops
    /// alone): re-replaying in isolation against the *original* raw node
    /// could disagree with what the fold actually saw once an earlier
    /// mod's own patch had already changed the tree
    ///
    pub replay: Result<(), String>,
    /// Whether the fold actually attempted every one of this mod's own
    /// ops before stopping — `false` only when an *earlier* mod's own op
    /// was the fold's stopper, so this mod's own ops were never reached
    /// at all and [`Self::replay`]'s `Ok(())` reflects "not disproven",
    /// not "verified".
    pub reached: bool,
    /// Every caveat the fold recorded while replaying this mod's own
    /// contributions (e.g. a `PatchOperationReplace`/`Remove` that
    /// matched nothing — not a replay error, since the real game logs it
    /// and moves on too; see [`rim_merge::plan::Caveat::FailedOp`]).
    pub caveats: Vec<Caveat>,
}

/// Everything [`InspectDef::execute`](crate::use_cases::inspect_def::InspectDef::execute) answers for one [`DefRef`] under
/// one [`rim_resolve::domain::OrderSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefInspection {
    /// The ref this inspection was built for — echoed back verbatim, so a
    /// name-only ref
    /// stays name-only here too, even though resolving it needs a real
    /// `def_type` internally (see [`resolve_target`](crate::use_cases::inspect_def::targets::resolve_target)).
    pub def_ref: DefRef,
    /// Which order this inspection was computed against.
    pub source: rim_resolve::domain::OrderSource,
    /// Every owner/registrant in the selected order; the last is the
    /// winner for a concrete def. **Not, for a template, "the first" —
    /// see [`Self::winner`]/[`Self::template_ambiguity`]'s own doc
    /// comments: a duplicated template `Name` has no single winner at
    /// all.**
    pub owners: Vec<Toucher>,
    /// The effective owner: whose raw node the game actually uses, for a
    /// concrete def. For a template with more than one registrant, this
    /// is instead an explicit, disclosed *representative* (the
    /// last-loaded registrant — see `def_sources::nearest_owner`'s own
    /// `Asking::Nobody` doc comment for why), never a fabricated single
    /// answer: see [`Self::template_ambiguity`] for what each real child
    /// actually resolves to, which can genuinely differ from this value.
    pub winner: ModId,
    /// `Some` only for a template (`Selector::NameAttr`) with more than
    /// one registrant — the real per-child resolutions [`Self::winner`]
    /// alone cannot represent. `None` for a concrete def, or a template
    /// with only one registrant (unambiguous; `winner` is simply correct
    /// there, no disclosure needed).
    pub template_ambiguity: Option<TemplateAmbiguity>,
    /// Every mod whose patch ops target this def/template, in the
    /// selected order.
    pub patchers: Vec<Patcher>,
    /// The `ParentName` chain from the winner's raw node outward, each
    /// with its registering mod — empty when there is no parent.
    pub parents: Vec<(DefKey, ModId)>,
    /// Templates only: direct children (direct only — the transitive
    /// closure is a click away), each with the
    /// [`Selector`](rim_analyzer::domain::Selector) its own registration actually uses —
    /// **not** always [`Selector::DefName`](rim_analyzer::domain::Selector::DefName):
    /// a child can itself be an abstract, `Name`-only template
    /// with no `defName` of its own (a large share of Core's `Name`
    /// declarations are themselves bare `ParentName` children), and
    /// typing such a child `DefName` would make it an unresolvable, dead link.
    pub children: Vec<(ModId, DefRef)>,
    /// The node the game actually uses under the selected order, and
    /// which mod last set each of its fields.
    pub effective: EffectiveDef,
    /// [`Self::effective`]`.resolved`, rendered back to XML text.
    pub resolved_xml: String,
    /// Every finding naming this def (`FindingKey::def_ref() == Some(def_ref)`),
    /// with its current status.
    pub findings: Vec<(FindingKey, ResolutionStatus)>,
}

/// Everything that can go wrong inspecting a [`DefRef`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InspectDefError {
    /// `def_ref` names nothing this scan indexed as a def or template —
    /// an unparseable/stale/mistyped ref, or one from a mod that's since
    /// gone inactive.
    #[error("{0} is not indexed as a def or template by this scan")]
    NotFound(DefRef),
    /// The def/template/patch-op text couldn't be read back — the scan is
    /// stale relative to what's on disk now, or the file itself couldn't
    /// be read. Surfaced, never silently partial (unlike a broken
    /// `ParentName` chain — see [`DefInspection::effective`]'s own
    /// `completeness`).
    #[error(transparent)]
    Source(#[from] DefSourceError),
    /// The read-back XML text failed to parse.
    #[error("parsing merge XML: {0}")]
    Xml(String),
}
