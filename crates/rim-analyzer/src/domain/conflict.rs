//! File-collision conflicts between active mods.

use serde::{Deserialize, Serialize};

use super::assembly::AssemblyVersion;
use super::locator::XmlLocator;
use super::mod_id::ModId;
use super::patch::Selector;

/// The same `(def_type, def_name)` is provided by more than one active mod.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefOverride {
    pub def_type: String,
    pub def_name: String,
    /// Owning mods in the order the scan ran in (the last one is that
    /// order's winner). Which owner wins in any other order, and the
    /// facts relative to that winner, are asked of
    /// [`DefOverride::winner_declares_relation`] and
    /// [`DefOverride::shadows_framework`] with that order's winner.
    pub owners: Vec<ModId>,
    /// One of the owners is Core or a DLC.
    pub overrides_vanilla: bool,
    /// Every owner shares at least one author with every other owner.
    pub same_author: bool,
}

/// One mutating patch operation contributing to a [`PatchCollision`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchCollisionEntry {
    pub mod_id: ModId,
    pub op_class: String,
}

/// How disruptive a [`PatchCollision`] is likely to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchCollisionSeverity {
    /// Every contributing operation only adds to the target — order
    /// between the contributing mods doesn't change the outcome.
    Additive,
    /// At least one contributing operation replaces, removes, inserts,
    /// renames, or sets an attribute on the target — load order decides
    /// which mod's change (or all of them, in conflicting ways) wins.
    Contested,
}

/// The same normalized `(def_type, def_name, selector, sub_path)` target
/// receives mutating patch operations from more than one mod. `selector`
/// is part of the identity: a `[@Name="X"]` template and a
/// `[defName="X"]` def are different targets even when `X` matches both.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchCollision {
    pub def_type: String,
    pub def_name: String,
    pub selector: Selector,
    pub sub_path: Option<String>,
    /// Contributing mods and their operation classes, in the order the
    /// scan ran in.
    pub mods: Vec<PatchCollisionEntry>,
    pub severity: PatchCollisionSeverity,
    /// Every mod, contributor or not, with an active `PatchOperationRemove`
    /// on this target's def selector at the def root or at `sub_path` or
    /// one of its ancestors: each makes a later operation on `sub_path`
    /// fail to find its node. Sorted, without duplicates, and order-free,
    /// so [`PatchCollision::winner_declares_relation`] can judge any order.
    #[serde(default)]
    pub removed_by: Vec<ModId>,
}

/// The same normalized texture path is shipped by more than one mod.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextureOverride {
    pub texture_path: String,
    pub owners: Vec<ModId>,
    pub same_author: bool,
}

/// The same assembly name is shipped by more than one active mod (the
/// first-loaded copy wins at runtime).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateAssembly {
    pub assembly_name: String,
    /// Shipping mods in load order.
    pub owners: Vec<ModId>,
    /// Every owner's own copy version, in the same order as `owners`. An
    /// owner whose DLL metadata failed to parse contributes
    /// [`AssemblyVersion::default`] (`0.0.0.0`) — indistinguishable from a
    /// genuine unversioned assembly, but it sorts lowest either way, which is
    /// the only property the version-precedence edge needs.
    pub versions: Vec<(ModId, AssemblyVersion)>,
    /// The owner that actually loads first (`owners[0]`) — named
    /// separately so a reader doesn't have to know `owners` is
    /// load-order-sorted to find it.
    pub first_loaded: ModId,
}

/// A `Name` attribute (see [`crate::domain::TemplateEntry`]) registered by
/// more than one active mod. RimWorld's XML inheritance resolves
/// `ParentName`/`[@Name="X"]` references against whichever template was
/// registered last (i.e. loads last).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateTemplateName {
    pub name: String,
    /// Registering mods, in load order.
    pub owners: Vec<ModId>,
}

/// The same `Languages/*/Keyed` translation key defined by more than one
/// active mod — the last-loaded definition wins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyedTranslationCollision {
    pub key: String,
    /// Defining mods, in load order.
    pub owners: Vec<ModId>,
}

/// The same normalized sound path shipped by more than one active mod —
/// mirrors [`TextureOverride`] for `Sounds/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoundOverride {
    pub path: String,
    pub owners: Vec<ModId>,
    /// Every owner shares at least one author with every other owner.
    pub same_author: bool,
}

/// Two or more active mods declare a runtime patch targeting the same type
/// and method — at equal runtime-patch priority, load order decides which patch
/// runs last, and neither this analyzer nor the patching library itself can say which is
/// "right".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimePatchCollision {
    pub target_type: String,
    pub target_method: String,
    /// Patching mods, in load order.
    pub owners: Vec<ModId>,
}

/// Two or more active mods each declare a transpiler-kind runtime patch
/// (attribute or convention-named) targeting the same `(type, method)` — the
/// narrow, dangerous subset of [`RuntimePatchCollision`] where load order can
/// silently break a pattern match: each transpiler rewrites the IL the *next*
/// one must still recognize, unlike a `Prefix`/`Postfix`, which composes
/// safely regardless of how many mods add one. **Disclosure only, like
/// [`RuntimePatchCollision`] — no ordering edge is derived from this.** Which
/// order is safe depends on whose IL pattern survives whose rewrite, which
/// isn't recoverable from metadata alone; most instances of this finding are
/// presumably fine; it exists so a user who *does* see a patch failure knows
/// which mods to investigate and can state the order themselves (`rimmerge
/// rule set-pair`).
///
/// `owners` is scoped to the mods that actually declare a `Transpiler`
/// role on this target — not every `RuntimePatchCollision` owner, which
/// may also include `Prefix`/`Postfix`-only patchers uninvolved in the
/// risk this finding names. Owners are collapsed by originating assembly
/// name first, the same bundled-copy rule
/// [`super::super::analysis::conflicts::runtime_patch_collisions`] uses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranspilerCollision {
    pub target_type: String,
    pub target_method: String,
    /// Every mod declaring a `Transpiler` on this target, in load order.
    pub owners: Vec<ModId>,
}

/// Two active mods that look like duplicates or forks of the same content:
/// a large overlap of identical `(def_type, def_name)` keys, with neither
/// declaring a relation to the other and no shared author to explain it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LikelyDuplicateMod {
    pub a: ModId,
    pub b: ModId,
    /// Number of identical `(def_type, def_name)` keys both mods define.
    pub shared_defs: usize,
    /// `shared_defs` as a fraction of the smaller mod's total def count.
    pub share_of_smaller: f32,
}

/// A `texPath`/`texPathFemale`/`iconPath`/`uiIconPath` field value naming a
/// texture no active mod ships — neither the exact path, a
/// `path_A`/`path_east` multi-part variant, nor a same-named file inside a
/// matching folder (see `analysis::conflicts::texture_path_resolves`'s own
/// doc comment for all three) — diagnostic only, never an ordering fact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingTexturePath {
    /// The mod whose own copy of the def carries this field — the same
    /// def key can have more than one owner (an override), so this is
    /// the one specific instance the candidate value came from, not
    /// implied by `def_type`/`def_name` alone.
    pub referrer: ModId,
    pub def_type: String,
    pub def_name: String,
    pub field: String,
    /// The normalized path (same convention as `Indices.texture_owners`'
    /// own keys) that resolved to no shipped file.
    pub path: String,
}

/// A `.dds` file an active mod actually loads that RimWorld's own
/// `ModDdsLoader` cannot decode — see
/// [`crate::extract::textures::classify_dds`] for the exact rule. The base
/// game shows the bad-texture placeholder in its place; a third-party
/// graphics mod may retry a sibling PNG, but the unmodded engine never
/// does.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UndecodableTexture {
    pub mod_id: ModId,
    /// The normalized key (same convention as [`TextureOverride::texture_path`]),
    /// never the raw file path.
    pub path: String,
    pub width: u32,
    pub height: u32,
    /// The pixel format's four-character-code tag (`"DXT5"`, `"DX10"`,
    /// ...); empty when the header was too malformed to read one.
    pub fourcc: String,
    /// Whether this mod also ships a non-`.dds` file at the same
    /// normalized key. The base game never falls back to it — a `.dds`
    /// shadows any same-key file across the whole mod regardless of
    /// whether it decodes — so this is recorded only as evidence, never as
    /// a "falls back to it" claim.
    pub has_png_sibling: bool,
}

/// The `(def_type, name)` identity of one def or template that references
/// a broken `ParentName` — `name` is a `defName` for a concrete def, or a
/// `Name` attribute for an abstract template (never both at once; see
/// `is_template`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefRefParts {
    pub def_type: String,
    pub name: String,
    /// Whether `name` is a `Name` attribute (an abstract template) rather
    /// than a `defName`.
    pub is_template: bool,
}

/// The `(def_type, def_name)` shape [`BrokenInheritance::affected`] lists —
/// always a concrete def, never a template, so this carries no
/// `is_template` flag the way [`DefRefParts`] does.
pub type AffectedDef = (String, String);

/// Why `parent_name` doesn't resolve cleanly for `BrokenInheritance::child`
/// — see [`crate::analysis::inheritance::broken_inheritance`]'s own doc
/// comment for the exact `GetBestParentFor` resolution rule this is
/// computed from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InheritanceProblem {
    /// No active mod registers `parent_name` at a position `child` could
    /// actually resolve against — the game logs `XML error: Could not
    /// find parent node named "<parent_name>"` and `child` (plus every
    /// concrete def in `affected`) loads without any of the parent's own
    /// inherited fields.
    MissingParent,
    /// `parent_name` does resolve, but the registration
    /// `GetBestParentFor` actually picks has a different element tag
    /// (def type) than `child`'s own — the child's own element/`Class`
    /// still decides its real type, but every field the wrong-typed
    /// parent would have contributed is still whatever that parent
    /// happens to carry, not what an author of `child`'s own type
    /// intended.
    ParentTypeMismatch {
        /// The resolved parent registration's own element tag.
        parent_type: String,
        /// The mod owning that registration — `None` when it's vanilla
        /// or patch-injected (`mod == null`), since neither has one
        /// specific owning mod to name.
        parent_owner: Option<ModId>,
    },
}

impl InheritanceProblem {
    /// The bare discriminant, with no payload — what
    /// `FindingKey::BrokenInheritance`'s own `problem_kind` field carries,
    /// since a finding key is never allowed to embed free-text evidence
    /// like `parent_type`.
    #[must_use]
    pub fn kind(&self) -> InheritanceProblemKind {
        match self {
            Self::MissingParent => InheritanceProblemKind::MissingParent,
            Self::ParentTypeMismatch { .. } => InheritanceProblemKind::ParentTypeMismatch,
        }
    }
}

/// See [`InheritanceProblem::kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InheritanceProblemKind {
    MissingParent,
    ParentTypeMismatch,
}

/// A def or template's `ParentName` resolves incorrectly against
/// `Verse.XmlInheritance`'s own rules — either to nothing at all, or to a
/// registration of the wrong element type. See
/// [`crate::analysis::inheritance::broken_inheritance`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokenInheritance {
    /// The mod owning `child` (and, since resolution is decided by the
    /// asking mod's own load position, every other def/template in this
    /// same mod referencing the same `parent_name` resolves identically).
    pub mod_id: ModId,
    pub parent_name: String,
    /// One referencing def/template, representative of every one this
    /// finding covers — the lexicographically first by
    /// [`crate::analysis::inheritance::ParentReference::label`], so which
    /// one shows up never depends on scan order.
    pub child: DefRefParts,
    pub problem: InheritanceProblem,
    /// Every active, gate-open concrete def that loses inherited content:
    /// `child` itself when it's concrete, plus every concrete descendant
    /// reached transitively through `children_by_template`, in `(def_type,
    /// def_name)` order. Bounded at `MAX_AFFECTED_DEFS`
    /// (`analysis::inheritance`) — see `truncated`.
    pub affected: Vec<AffectedDef>,
    /// How many more affected defs existed beyond `affected`'s own cap.
    /// `0` when nothing was truncated.
    pub truncated: usize,
}

/// Which written value [`NearMissModReference`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModReferenceKind {
    /// A `PatchOperationFindMod` name, matched against active mods'
    /// display names.
    FindModName,
    /// A `MayRequire`/`MayRequireAnyOf` id, matched against active mods'
    /// base package ids.
    MayRequireId,
}

/// Which similarity rule flagged [`NearMissModReference`] — see
/// `analysis::name_similarity` for the exact definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NearMissRule {
    /// Equal case-insensitively but not exactly (`FindModName` only).
    CaseOnly,
    /// Equal after normalizing (lowercase, drop bracketed groups, drop
    /// non-alphanumerics).
    Normalized,
    /// Both normalized forms are long enough and close enough by
    /// Damerau-Levenshtein distance.
    NearMiss,
    /// The written value's own word tokens are the candidate's tokens
    /// with one or two extra leading tokens.
    LeadingToken,
}

/// A `PatchOperationFindMod` name or `MayRequire`/`MayRequireAnyOf` id
/// that resolves to no active mod, and to no installed-but-inactive mod
/// either, but closely resembles exactly one active mod under
/// `analysis::name_similarity`'s rules — a likely typo or drifted
/// reference, not a proven load-time fact (contrast
/// [`BrokenInheritance`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearMissModReference {
    /// The mod whose own file carries the written reference.
    pub referrer: ModId,
    /// Named `reference_kind`, not `kind` — [`Conflict`]'s own
    /// `#[serde(tag = "kind")]` internal tag would otherwise collide
    /// with a same-named field on this struct at the JSON level (the
    /// struct's fields are flattened into the same object as the tag),
    /// silently losing the outer `"kind": "near_miss_mod_reference"`
    /// discriminant on serialization.
    pub reference_kind: ModReferenceKind,
    /// The value as written in the mod's own XML.
    pub written: String,
    /// The closest active mod.
    pub candidate: ModId,
    /// `candidate`'s own display name, for evidence — `candidate` alone
    /// (a package id) isn't always recognizable to a user.
    pub candidate_name: String,
    pub rule: NearMissRule,
    /// Where the written value came from, when the extracted site
    /// carries one.
    pub locator: Option<XmlLocator>,
}

/// A later `PatchOperationReplace` (`replacer`) discards an earlier
/// active mod's own addition (`adder`) inside the replaced node, and the
/// replacer's own `About.xml` declares `loadAfter`, `forceLoadAfter`, or
/// `modDependencies` on the adder — a deliberate override, not a silent
/// accident (see
/// [`crate::analysis::edges::replace_discards_addition`]'s own
/// doc comment for the full rule, and
/// [`crate::domain::EdgeKind::ReplaceDiscardsAddition`] for the
/// non-deliberate case, which surfaces as an edge instead of this
/// finding). Emitted only for the deliberate case: the non-deliberate
/// one already surfaces through the edge, and — when it's dropped —
/// through the ordinary `EdgeDropped` finding, so a second finding for
/// the identical fact would be noise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscardedAddition {
    /// The mod whose `PatchOperationReplace` discards `adder`'s own
    /// content.
    pub replacer: ModId,
    /// The mod whose earlier addition is discarded.
    pub adder: ModId,
    pub def_type: String,
    pub def_name: String,
    /// The replaced node's own display path (`P`, the replacer's own
    /// target).
    pub path: String,
    /// The adder's own op's target display path — `path` itself for the
    /// same-node case, or a descendant of it for the ancestor case.
    pub adder_path: String,
}

/// Where a referrer [`RefSiteSummary`] found its value — mirrors
/// [`super::ref_site::RefSiteOwner`], but as report-shaped evidence
/// (bounded, serializable) rather than the raw scan-time candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RefSiteReferrer {
    /// Found in a concrete def's own field tree.
    ReferencedFromDef {
        mod_id: ModId,
        def_type: String,
        def_name: String,
    },
    /// Found in a `Name`-attributed template's own field tree — the
    /// template's own `Name`, not one of its concrete descendants (see
    /// [`super::ref_site::RefSiteOwner::Template`]'s own doc comment for
    /// why the reference is attributed here rather than to a descendant).
    ReferencedFromTemplate {
        mod_id: ModId,
        def_type: String,
        name: String,
    },
    /// Found in an active mutating patch op's own `<value>`.
    ReferencedFromPatch {
        mod_id: ModId,
        def_type: String,
        def_name: String,
        locator: XmlLocator,
    },
}

/// One referrer of a [`DanglingDefReference`] — the field it was written
/// in, plus where that field lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefSiteSummary {
    pub referrer: RefSiteReferrer,
    /// Field path from the referrer's own root, `li` segments collapsed
    /// — the same convention [`super::ref_site::RefSite::field_path`]
    /// uses.
    pub field_path: String,
}

/// Why a [`DanglingDefReference`]'s own `name` never resolves in the
/// post-patch active def set — see `analysis::references`'s own doc
/// comment for how the pure half is computed and
/// `crate::infra::explain_dangling_references` for the lazy IO half.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DanglingCause {
    /// A whole-def `PatchOperationRemove` (unconditional, or
    /// conditionally guarded only on its own xpath) deleted it.
    RemovedBy { mod_id: ModId, locator: XmlLocator },
    /// Defined only in one of this mod's own *unloaded* loaded-folder
    /// candidates (a version folder the running game doesn't select, or
    /// one gated off by `IfModActive`/`IfModNotActive`).
    OnlyInUnloadedFolder { mod_id: ModId, folder: String },
    /// Defined only in an installed-but-inactive mod.
    OnlyInInactiveMod { mod_id: ModId },
    /// Defined nowhere this analyzer can see, active or not — the lazy
    /// search completed within its own byte budget and found nothing.
    DefinedNowhere,
    /// The lazy explain pass hasn't reached a verdict: either it never
    /// ran (the pure half alone can't explain anything but `RemovedBy`),
    /// or its own byte budget ran out before covering every unloaded
    /// folder/inactive mod. Distinct from `DefinedNowhere`, which means
    /// "searched in full, found nothing" — this means "not proven either
    /// way".
    Unexplained,
}

/// A name written at a recognized reference site (a list item, a scalar
/// field, a keyed-dictionary element name, or a `descriptionHyperlinks`
/// entry) in an active, gate-open def/template or an active patch's own
/// `<value>`, that no active def of *any* type actually has as its
/// `defName` in the post-patch set, and that isn't an implied
/// (engine-generated) name either — see `analysis::references`'s own doc
/// comment for the full rule. Diagnostic only, never an ordering fact:
/// cross-references resolve once, after every def and patch has loaded,
/// so the result never depends on load order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DanglingDefReference {
    pub name: String,
    /// Every distinct referrer, bounded at `MAX_REFERRERS`
    /// (`analysis::references`) — see `truncated_referrers`.
    pub referrers: Vec<RefSiteSummary>,
    /// How many more referrers existed beyond `referrers`'s own cap. `0`
    /// when nothing was truncated.
    pub truncated_referrers: usize,
    pub cause: DanglingCause,
    /// Set when the voted field's own resolved values are mostly
    /// `SoundDef`s — `SoundDef.Named` never fails hard, it logs a
    /// `Warning` and falls back to an "undefined" sound, so this
    /// finding's own rationale should say that rather than imply a load
    /// failure.
    pub likely_sound: bool,
}

/// A detected file-level collision between active mods.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Conflict {
    DefOverride(DefOverride),
    PatchCollision(PatchCollision),
    TextureOverride(TextureOverride),
    DuplicateAssembly(DuplicateAssembly),
    LikelyDuplicateMod(LikelyDuplicateMod),
    DuplicateTemplateName(DuplicateTemplateName),
    KeyedTranslationCollision(KeyedTranslationCollision),
    SoundOverride(SoundOverride),
    RuntimePatchCollision(RuntimePatchCollision),
    // Appended at the end, same "never interleave" convention `EdgeKind`
    // documents for the same reason: nothing here is `Ord`-sensitive, but
    // keeping the habit costs nothing.
    MissingTexturePath(MissingTexturePath),
    // Same "append at the end" convention.
    TranspilerCollision(TranspilerCollision),
    // Same "append at the end" convention.
    UndecodableTexture(UndecodableTexture),
    // Same "append at the end" convention.
    BrokenInheritance(BrokenInheritance),
    // Same "append at the end" convention.
    NearMissModReference(NearMissModReference),
    // Same "append at the end" convention.
    DiscardedAddition(DiscardedAddition),
    // Same "append at the end" convention.
    DanglingDefReference(DanglingDefReference),
}

impl Conflict {
    /// The number of mods involved — used to rank "top conflicts by owner
    /// count" in the text summary.
    #[must_use]
    pub fn owner_count(&self) -> usize {
        match self {
            Self::DefOverride(c) => c.owners.len(),
            Self::PatchCollision(c) => c.mods.len(),
            Self::TextureOverride(c) => c.owners.len(),
            Self::DuplicateAssembly(c) => c.owners.len(),
            Self::LikelyDuplicateMod(_) => 2,
            Self::DuplicateTemplateName(c) => c.owners.len(),
            Self::KeyedTranslationCollision(c) => c.owners.len(),
            Self::SoundOverride(c) => c.owners.len(),
            Self::RuntimePatchCollision(c) => c.owners.len(),
            // Diagnostic-only, single-def finding — no owner list at all
            // (see the struct's own doc comment), so `1` is the only
            // sensible count for the "top conflicts by owner count"
            // ranking this method exists for.
            Self::MissingTexturePath(_) => 1,
            Self::TranspilerCollision(c) => c.owners.len(),
            // Single-mod diagnostic fact, same reasoning as
            // `MissingTexturePath` above.
            Self::UndecodableTexture(_) => 1,
            // Single-mod diagnostic fact: one mod's own broken
            // `ParentName` reference, not a contested resource.
            Self::BrokenInheritance(_) => 1,
            // Single-mod diagnostic fact: one mod's own written
            // reference, compared against one candidate.
            Self::NearMissModReference(_) => 1,
            // A pair fact: the replacer and the adder, the same shape as
            // `LikelyDuplicateMod`.
            Self::DiscardedAddition(_) => 2,
            // A name-shaped fact, not a between-mods conflict — the
            // referrers are evidence, not part of the identity (see
            // `FindingKey::DanglingDefReference`'s own doc comment, one
            // crate over), so there is no single "owner" list to count.
            // `1` for the same "diagnostic-only" reasoning
            // `MissingTexturePath`/`UndecodableTexture` already use.
            Self::DanglingDefReference(_) => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_count_reads_the_right_field_per_variant() {
        let def_override = Conflict::DefOverride(DefOverride {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
            overrides_vanilla: false,
            same_author: false,
        });
        assert_eq!(def_override.owner_count(), 2);

        let patch_collision = Conflict::PatchCollision(PatchCollision {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            selector: Selector::DefName,
            sub_path: None,
            mods: vec![PatchCollisionEntry {
                mod_id: ModId::new("a"),
                op_class: "PatchOperationReplace".to_string(),
            }],
            severity: PatchCollisionSeverity::Additive,
            removed_by: Vec::new(),
        });
        assert_eq!(patch_collision.owner_count(), 1);
    }

    #[test]
    fn owner_count_reads_the_right_field_for_every_remaining_conflict_variant() {
        let duplicate_template = Conflict::DuplicateTemplateName(DuplicateTemplateName {
            name: "WallBase".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
        });
        assert_eq!(duplicate_template.owner_count(), 2);

        let keyed_collision = Conflict::KeyedTranslationCollision(KeyedTranslationCollision {
            key: "Greeting".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b"), ModId::new("c")],
        });
        assert_eq!(keyed_collision.owner_count(), 3);

        let sound_override = Conflict::SoundOverride(SoundOverride {
            path: "shot_fire".to_string(),
            owners: vec![ModId::new("a")],
            same_author: false,
        });
        assert_eq!(sound_override.owner_count(), 1);

        let runtime_patch_collision = Conflict::RuntimePatchCollision(RuntimePatchCollision {
            target_type: "Verse.Pawn".to_string(),
            target_method: "Kill".to_string(),
            owners: vec![ModId::new("a"), ModId::new("b")],
        });
        assert_eq!(runtime_patch_collision.owner_count(), 2);
    }
}
