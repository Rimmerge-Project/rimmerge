//! [`ScannedMod`] and [`ScanOutput`]: everything extracted from a scan,
//! the input the analysis layer builds edges and conflicts from.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::assembly::AssemblyInfo;
use super::conflict::UndecodableTexture;
use super::inactive_mod::InactiveMod;
use super::load_order::LoadOrder;
use super::locator::XmlLocator;
use super::mod_entry::Mod;
use super::mod_id::ModId;
use super::patch::PatchOp;
use super::ref_site::RefSite;
use super::warning::Warning;

/// One indexed def: its type tag (`ThingDef`, `example.PartDef`, ...) and
/// `defName`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DefEntry {
    pub def_type: String,
    pub def_name: String,
    /// `MayRequire` packageIds (comma-separated): every one must be
    /// active for this def to actually load. Compared case-insensitively
    /// via [`super::mod_id::ModId`] at match time, not lowercased here.
    pub may_require: Vec<String>,
    /// `MayRequireAnyOf` packageIds: at least one must be active for this
    /// def to actually load.
    pub may_require_any_of: Vec<String>,
    /// The `ParentName` attribute, when this def inherits from a template.
    pub parent_name: Option<String>,
    /// Where this def's element lives on disk.
    pub locator: XmlLocator,
}

/// An `Abstract="True"` (or otherwise `Name`-attributed) template node —
/// never a def of its own, but the target of `ParentName` chains and of
/// `[@Name="X"]` patches. A concrete def that also carries a `Name`
/// attribute appears both here and in [`ScannedMod::defs`] — RimWorld
/// allows a def to be its own children's template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateEntry {
    /// The tag name this template's children inherit from when they
    /// declare `ParentName` (e.g. `HediffDef`).
    pub def_type: String,
    /// The `Name` attribute.
    pub name: String,
    /// The `ParentName` attribute, when this template itself inherits
    /// from another template.
    pub parent_name: Option<String>,
    /// `MayRequire` packageIds (comma-separated): every one must be active
    /// for this node to register with `Verse.XmlInheritance` at all, so an
    /// unsatisfied gate means the template is not a candidate parent for
    /// anybody (`TryRegister`'s own early return). **No `may_require_any_of`
    /// sibling on purpose**: `TryRegister` reads the `MayRequire` attribute
    /// only — `MayRequireAnyOf` gates whether the *def* loads
    /// (`LoadedModManager.ParseAndProcessXML`'s own later loop), never
    /// whether the name registers — so a field for it here would invite a
    /// gate the engine does not apply.
    pub may_require: Vec<String>,
    /// The `<graphicData><graphicClass>` this template declares, if any
    /// — what a concrete def inheriting from it ends up with when it
    /// declares only its own `texPath`.
    ///
    /// The strict `Graphic_Collection` folder rule in `analysis::conflicts`
    /// needs this: reading only the `graphicClass` written literally beside
    /// the `texPath` would silently skip the whole plant/stone/leather
    /// family, where vanilla templates declare a collection class and
    /// hundreds of defs inherit it. `RecursiveNodeCopyOverwriteElements`
    /// merges the parent's `<graphicData>` into the child's, so the child's
    /// own `texPath` and the template's `graphicClass` end up in the same
    /// block.
    pub graphic_class: Option<String>,
    /// Whether the node carries `Abstract="True"`.
    pub is_abstract: bool,
    /// Where this template's element lives on disk.
    pub locator: XmlLocator,
}

/// The full extraction result for one mod: its metadata plus everything
/// found in its loaded folders.
#[derive(Debug, Clone)]
pub struct ScannedMod {
    pub info: Mod,
    pub defs: Vec<DefEntry>,
    /// `Name`-attributed template nodes this mod ships (see [`TemplateEntry`]).
    pub templates: Vec<TemplateEntry>,
    pub patch_ops: Vec<PatchOp>,
    /// Every normalized texture key this mod ships (path relative to a
    /// `Textures/` folder, extension stripped, lowercased, `/`-separated)
    /// mapped to the summed size in bytes of every file on disk that
    /// normalizes to it — a mod shipping both `Wall.png` and `Wall.dds`
    /// yields one key whose value is the sum of both files' sizes. The single
    /// authoritative representation of "which texture keys this mod ships and
    /// how big they are", rather than a parallel byte map kept in sync with a
    /// separate key set. See
    /// [`ScanCost::texture_files`]/[`ScanCost::texture_bytes`] for the raw
    /// (non-deduplicated-by-key) file count and byte total.
    pub textures: BTreeMap<String, u64>,
    /// Sound paths relative to a `Sounds/` folder, normalized the same way
    /// as `textures` (see [`crate::extract::sounds::normalize`]).
    pub sounds: BTreeSet<String>,
    /// Keyed translation keys this mod defines, from every
    /// `Languages/*/Keyed/**/*.xml` file's root element's children, whatever
    /// the root is named (see [`crate::extract::languages::index`]). Merged
    /// across every language folder the mod ships (`infra::mod_scan` walks
    /// `Keyed` under every `Languages/<lang>/` dir it finds, `English` and
    /// `Japanese` alike, into this one set) — a key is the same key
    /// regardless of which language defines its text, so a mod that only
    /// translates a key in a non-English language still "defines" it here.
    /// Facts only: which key collides across mods is `analysis::conflicts`'
    /// job.
    pub translation_keys: BTreeSet<String>,
    /// Every fully-qualified type string named anywhere in this mod's
    /// `Defs/**/*.xml` files (see
    /// [`crate::extract::defs::DefsFile::inline_types`]) — the union across
    /// every file, not per-file.
    pub inline_types: BTreeSet<String>,
    /// This mod's own `About/Manifest.xml`, unresolved — see
    /// [`super::ManifestOrder`]'s own doc comment for why. Default
    /// (all-empty) for a mod that ships no `Manifest.xml`, or one that fails
    /// to parse (a [`Warning`] is recorded in that case, same convention as a
    /// malformed `LoadFolders.xml`).
    pub manifest_order: super::ManifestOrder,
    /// The union, across every file, of
    /// [`crate::extract::defs::DefsFile::inline_node_path_hashes`].
    pub inline_node_path_hashes: HashSet<u64>,
    pub assemblies: Vec<AssemblyInfo>,
    /// Mod ids named in this mod's `LoadFolders.xml` `IfModActive` and
    /// `IfModActiveAll` gates for the current game version (source for
    /// `IfModActive` awareness edges).
    pub if_mod_active_targets: Vec<ModId>,
    /// The union, across every file in this mod's own loaded folders, of
    /// [`crate::extract::defs::DefsFile::texture_path_candidates`].
    pub texture_path_candidates: Vec<super::TexturePathCandidate>,
    /// Byte-and-file-count totals from the `Textures/` and `Assemblies/`
    /// walks, accumulated across every loaded folder this mod scans (and, in
    /// `--all-folders` diagnostic mode, across every folder that mode visits)
    /// — never overwritten between folders.
    pub scan_cost: ScanCost,
    /// Summed across every `Defs/**/*.xml` file this mod ships — see
    /// [`crate::extract::defs::DefsFile::nameless_def_count`]'s own doc
    /// comment.
    pub nameless_def_count: usize,
    /// Normalized texture keys (same convention as [`Self::textures`])
    /// this mod's own active asset bundles resolve, read from each
    /// `AssetBundles/<bundle>.manifest` sidecar under this mod's loaded
    /// folders (see `extract::asset_index::bundle_asset_key`). Kept
    /// separate from `textures` deliberately: a bundle texture is never a
    /// [`super::TextureOverride`] of a loose file, since a loose file
    /// always wins the lookup — merging the two sets would fabricate an
    /// override that never happens in game.
    pub bundle_textures: BTreeSet<String>,
    /// Every `.dds` file this mod actually loads (after same-relative-path
    /// shadowing across its own loaded folders — see
    /// `infra::mod_scan::shadowed_paths`'s own doc comment for the general
    /// rule) that RimWorld's own DDS loader cannot decode — see
    /// `extract::textures::classify_dds`.
    pub undecodable_textures: Vec<UndecodableTexture>,
    /// The union, across every `Defs/**/*.xml` file this mod ships, of
    /// [`crate::extract::defs::DefsFile::nested_may_require`].
    pub nested_may_require: Vec<(String, super::XmlLocator)>,
}

/// Byte-and-file-count totals `infra::mod_scan::scan_folder` accumulates for
/// one mod's `Textures/` and `Assemblies/` walks. Every count rides the
/// directory walk the scan already does — no extra filesystem read — since
/// `walkdir::DirEntry::metadata()` on Windows returns the size the directory
/// enumeration itself already cached.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ScanCost {
    /// Every file under a loaded `Textures/` folder with a recognized
    /// image extension — a raw file count, **not** deduplicated by
    /// normalized key the way [`ScannedMod::textures`]`.len()` is (a mod
    /// shipping both `Wall.png` and `Wall.dds` counts 2 here, 1 there).
    pub texture_files: u64,
    /// Sum of every counted texture file's size in bytes.
    pub texture_bytes: u64,
    /// How many of [`Self::texture_files`] have a `.dds` extension
    /// (case-insensitive).
    pub dds_files: u64,
    /// Sum of every `Assemblies/**/*.dll`'s size in bytes. The DLL
    /// *count* is [`ScannedMod::assemblies`]`.len()` — this is only the
    /// byte total, the part that isn't already available elsewhere.
    pub assembly_bytes: u64,
}

/// Which phase of [`crate::infra::scan_with_progress`] a [`ScanProgress`]
/// report describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanStage {
    /// Walking the game/workshop directories to find mod folders — before
    /// any individual mod is scanned.
    Discovering,
    /// Scanning each active mod's files (`About.xml`, defs, patches,
    /// textures, assemblies) — the bulk of the work, reported per mod.
    Scanning,
    /// Post-scan housekeeping over the whole set (e.g. reading the game's
    /// own managed-assembly names) — after every mod has been scanned.
    Analyzing,
}

/// One progress report from [`crate::infra::scan_with_progress`].
/// `done`/`total` are only meaningful within the same `stage`: a new stage
/// starts its own `0..=total` range, not a continuation of the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScanProgress {
    pub stage: ScanStage,
    pub done: usize,
    pub total: usize,
}

/// Everything scanning produced, ready for the analysis layer.
pub struct ScanOutput {
    pub scanned_mods: Vec<ScannedMod>,
    pub load_order: LoadOrder,
    /// Active mods (per `ModsConfig.xml`) that have no directory on disk.
    pub missing_mods: Vec<ModId>,
    /// Lowercased assembly-name stems shipped in the game's own
    /// `RimWorldWin64_Data/Managed/` folder. Core/DLC ship no
    /// `Assemblies/` folder of their own, so their assembly names have to
    /// be seeded from the game install directly rather than discovered by
    /// scanning mod folders.
    pub vanilla_assembly_names: HashSet<String>,
    /// `Assembly-CSharp.dll`'s own type hierarchy (every `Verse`/
    /// `RimWorld`-namespaced class and its base type, where resolvable) —
    /// `analysis::inheritance`'s own source for the *vanilla* half of a
    /// `ParentTypeMismatch` subclass check, the mirror of every active
    /// mod's own `ScannedMod::assemblies`-carried hierarchy. Empty when
    /// `Assembly-CSharp.dll` couldn't be read or parsed (a `Warning` is
    /// recorded either way) — `analysis::inheritance::is_subclass`'s own
    /// "can't resolve, don't flag" rule treats an empty index safely.
    pub vanilla_type_hierarchy: Vec<(String, Option<String>)>,
    pub warnings: Vec<Warning>,
    /// Per-mod mirror of every scanned
    /// [`crate::extract::defs::DefsFile::child_value_hashes`], keyed by the
    /// owning mod. Kept on [`ScanOutput`] rather than folded into
    /// [`ScannedMod`] itself — deliberately, unlike every other
    /// `DefsFile`-sourced field (`inline_node_path_hashes`,
    /// `texture_path_candidates`, ...) — because [`ScannedMod`] is a `pub`
    /// domain type other crates' own tests construct directly as full struct
    /// literals (`rim-resolve`'s ledger/tag test fixtures); a required field
    /// added there ripples into every one of those literals, in crates this
    /// feature has no reason to touch. This data has exactly one real
    /// consumer (`analysis::source_index::build`, which needs it *per mod* to
    /// resolve only the winning owner's own content — see
    /// `analysis::edges::child_value_targets`'s own doc comment for why that
    /// precision matters) and reaches it just as well from here.
    pub child_value_hashes_by_mod: BTreeMap<ModId, HashSet<u64>>,
    /// Every mod discovery found on disk that wasn't resolved into
    /// `scanned_mods` (or, with [`crate::infra::ScanConfig::active_mods`]
    /// set, not resolved against that override) — sorted by id
    /// (`infra::discovery::Discovered::inactive_excluding`'s own contract;
    /// the two backing maps are `HashMap`s, so sorting at that one
    /// construction site is what keeps this deterministic). **Trusted
    /// pre-sorted, not re-sorted here or anywhere downstream**
    /// (`analysis::report_builder::build_ref` clones this field through to
    /// `Report.inactive_mods` verbatim) — a hand-built `ScanOutput` (a test)
    /// that wants to exercise sortedness has to pass an already-sorted list
    /// itself, the same way `report_builder`'s own `hand_made_scan` test
    /// helper does with two entries given in reverse-of-sorted insertion
    /// order.
    pub inactive_mods: Vec<InactiveMod>,
    /// The total number of mod directories discovery found on disk — active,
    /// inactive, and (unlike `missing_mods`, which is the opposite gap)
    /// counted regardless of whether the active list even names them.
    /// `scanned_mods.len() + inactive_mods.len()` for a well-formed install
    /// with no missing mods **and** no active-list entry naming an
    /// already-counted folder twice — the latter genuinely happens: a
    /// Workshop-only mod listed under both its bare id and its
    /// `_steam`-suffixed id in the same `<activeMods>` resolves
    /// (`Discovered::lookup`) to the same on-disk folder both times, so
    /// `discovered_mod_count` stays `1` while `scanned_mods.len()` is `2` for
    /// that install. A missing active mod's directory contributes to neither
    /// side either, so the three counts don't have to sum to this one in
    /// general.
    pub discovered_mod_count: usize,
    /// Every `textures/...` container-path string found in the game's own
    /// `RimWorldWin64_Data/globalgamemanagers` (see
    /// `extract::asset_index::resource_container_paths`), with the
    /// `textures/` prefix stripped and lowercased — Core's own built-in
    /// resource textures, which ship no loose file at all and so are
    /// otherwise invisible to this analyzer.
    /// Empty when the file couldn't be read or parsed as expected; see
    /// `analysis::indices::MIN_CORE_RESOURCE_TEXTURES` for how a caller
    /// building [`Indices`](crate::analysis::indices::Indices) treats a
    /// too-small result as unusable rather than trusting a near-empty
    /// index.
    pub core_resource_textures: BTreeSet<String>,
    /// Per-mod mirror of every scanned candidate reference site (see
    /// [`RefSite`]), keyed by the owning mod — kept off [`ScannedMod`]
    /// itself for the same reason [`Self::child_value_hashes_by_mod`] is:
    /// [`ScannedMod`] is a `pub` domain type other crates' own tests
    /// construct directly as full struct literals, and a required field
    /// added there ripples into every one of those literals in crates
    /// this feature has no reason to touch. `analysis::references` is
    /// this field's one real consumer.
    pub ref_sites_by_mod: BTreeMap<ModId, Vec<RefSite>>,
}
