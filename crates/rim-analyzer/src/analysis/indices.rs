//! Cross-mod lookup tables shared by edge-building and conflict
//! detection, built once per run from every scanned mod.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::domain::{FindModGate, ModId, PatchOp, ScannedMod, Source};

/// A `(def_type, def_name)` key, as owned by one or more mods.
pub type DefKey = (String, String);

/// The mods actually scanned this run — the single "is this id active"
/// authority, keyed for both an exact match and a `_steam`-suffix-agnostic
/// one (see [`ModId::base`]).
///
/// Deliberately built from `ScannedMod`s, never from the raw
/// `LoadOrder`/`ModsConfig.xml` list: that list can name mods with no
/// directory on disk, which must never count as "active" for gating.
#[derive(Debug, Clone, Default)]
pub struct ActiveMods {
    by_base: HashMap<ModId, ModId>,
}

impl ActiveMods {
    #[must_use]
    pub fn build(scanned: &[ScannedMod]) -> Self {
        let mut by_base: HashMap<ModId, ModId> = HashMap::new();
        for scanned_mod in scanned {
            let id = scanned_mod.info.id.clone();
            let base = id.base();
            by_base
                .entry(base)
                .and_modify(|resolved: &mut ModId| {
                    // Prefer the non-suffixed copy when both a local and a
                    // Workshop copy of the same packageId are active — an
                    // edge case, but a bare declared id should resolve to
                    // the mod without a synthetic suffix when there's a
                    // choice.
                    if resolved.as_str().ends_with("_steam") && !id.as_str().ends_with("_steam") {
                        *resolved = id.clone();
                    }
                })
                .or_insert(id);
        }
        Self { by_base }
    }

    /// Whether `id` — with or without a `_steam` suffix — names a
    /// currently active mod.
    #[must_use]
    pub fn contains(&self, id: &ModId) -> bool {
        self.by_base.contains_key(&id.base())
    }

    /// The exact active [`ModId`] `id` resolves to (matching a scanned
    /// [`Mod::id`](crate::domain::Mod::id) and [`LoadOrder`](crate::domain::LoadOrder)
    /// entry), if any.
    #[must_use]
    pub fn resolve(&self, id: &ModId) -> Option<&ModId> {
        self.by_base.get(&id.base())
    }

    /// Builds an [`ActiveMods`] directly from a set of already-active (base)
    /// ids, without needing a full `ScannedMod` per id — for a caller
    /// (`rim-session`'s patch replay) that only has
    /// `Session::active_base_ids()` on hand, not the raw scan [`ScannedMod`]
    /// list [`Self::build`] needs. Every id is already base-normalized either
    /// way (`ModId::base()` strips a `_steam` suffix), so `Self::build`'s own
    /// "prefer the non-suffixed copy" tie-break has nothing to resolve here —
    /// this only ever needs [`Self::contains`] to agree, which it does
    /// regardless of which exact variant ends up as the map's own value.
    #[must_use]
    pub fn from_ids<I: IntoIterator<Item = ModId>>(ids: I) -> Self {
        Self {
            by_base: ids.into_iter().map(|id| (id.base(), id)).collect(),
        }
    }
}

pub struct Indices {
    /// Every mod that owns each `(def_type, def_name)`, in scan order
    /// (one entry per mod, even if it defines the same def more than
    /// once). Defs gated off by an unsatisfied `MayRequire`/
    /// `MayRequireAnyOf` are excluded — RimWorld never loads them.
    /// `BTreeMap`, not `HashMap`: this gets iterated to build
    /// [`Conflict`](crate::domain::Conflict) lists, and `HashMap`'s
    /// per-process random iteration order would make two runs over the
    /// same install produce differently-ordered reports.
    pub def_owners: BTreeMap<DefKey, Vec<ModId>>,
    /// Every mod that ships an assembly with this name. `BTreeMap` for
    /// the same determinism reason as `def_owners`.
    pub assembly_owners: BTreeMap<String, Vec<ModId>>,
    /// Every mod that ships a texture at this normalized path. `BTreeMap`
    /// for the same determinism reason as `def_owners`.
    pub texture_owners: BTreeMap<String, Vec<ModId>>,
    /// Assembly names shipped by at least one Core/Dlc mod, or by the
    /// game's own `RimWorldWin64_Data/Managed/` folder. Only ever
    /// membership-tested, never iterated into report output, so a
    /// `HashSet` is fine here.
    pub vanilla_assembly_names: HashSet<String>,
    pub mod_source: HashMap<ModId, Source>,
    pub mod_authors: HashMap<ModId, Vec<String>>,
    /// Each mod's own `About.xml` declarations (`loadAfter`/`modDependencies`/
    /// ...) — used by `analysis::edges::assembly::designated_provider` to
    /// infer which owner of a shared assembly name is the real upstream
    /// source, versus which owners merely bundle their own copy. Only ever
    /// looked up by key, never iterated into report output, so a
    /// `HashMap` is fine here.
    pub mod_declared: HashMap<ModId, crate::domain::DeclaredOrder>,
    /// Each mod's own copy version of each assembly it ships, keyed by
    /// `(mod, assembly name)`. Only ever looked up by key (for
    /// [`crate::domain::DuplicateAssembly::versions`]), never iterated
    /// into report output, so a `HashMap` is fine here.
    pub assembly_versions: HashMap<(ModId, String), crate::domain::AssemblyVersion>,
    /// Every mod that registers a `Name`-attributed template of this name.
    /// `BTreeMap` for the same determinism reason as `def_owners`.
    pub template_owners: BTreeMap<String, Vec<ModId>>,
    /// Every mod that defines this `Languages/*/Keyed` key. `BTreeMap` for
    /// the same determinism reason as `def_owners`.
    pub translation_key_owners: BTreeMap<String, Vec<ModId>>,
    /// Every mod that ships a sound at this normalized path. `BTreeMap` for
    /// the same determinism reason as `def_owners`.
    pub sound_owners: BTreeMap<String, Vec<ModId>>,
    /// Every mod whose shipped assemblies declare a runtime patch
    /// targeting this `(type, method)`. `BTreeMap` for the same determinism
    /// reason as `def_owners`.
    pub runtime_patch_owners: BTreeMap<(String, String), Vec<ModId>>,
    /// Every distinct shipped assembly *name* declaring a runtime patch
    /// targeting this `(type, method)` — used to collapse bundled copies of
    /// the same patch DLL before
    /// [`super::conflicts::runtime_patch_collisions`]'s `> 1` owner test: two
    /// mods each bundling their own copy of the identically-named assembly
    /// aren't two independently-authored patches, just one patch shipped
    /// twice. `BTreeMap`/`BTreeSet` for the same determinism reason as
    /// `def_owners`.
    pub runtime_patch_assembly_names: BTreeMap<(String, String), BTreeSet<String>>,
    /// Every mod whose shipped assemblies declare a `Transpiler` role
    /// (attribute or convention-named,
    /// [`crate::domain::RuntimePatchKind::Transpiler`]) targeting this
    /// `(type, method)` — the narrower sibling of `runtime_patch_owners` this
    /// finding needs: not every mod patching a target is a transpiler, and
    /// the ordering risk only applies to the ones that are. `BTreeMap` for
    /// the same determinism reason as `def_owners`.
    pub transpiler_owners: BTreeMap<(String, String), Vec<ModId>>,
    /// The transpiler sibling of `runtime_patch_assembly_names`: every
    /// distinct shipped assembly *name* declaring a `Transpiler` role on
    /// this `(type, method)` — used to collapse bundled copies of the
    /// same patch DLL before `super::conflicts::transpiler_collisions`'s
    /// `> 1` owner test, the same bundled-copy reasoning
    /// `runtime_patch_assembly_names` already documents.
    pub transpiler_assembly_names: BTreeMap<(String, String), BTreeSet<String>>,
    /// Every fully-qualified type string written inline anywhere in any
    /// active mod's own `Defs/` (see
    /// [`crate::domain::ScannedMod::inline_types`]) — deliberately whole-file
    /// (not `MayRequire`-filtered like `def_owners`): a type gated off by an
    /// unsatisfied `MayRequire` still counts as "pre-existing" evidence here,
    /// since proving it would need per-element gating this index doesn't do.
    /// Only ever membership-tested, never iterated into report output, so a
    /// `HashSet` is fine here.
    pub inline_type_names: HashSet<String>,
    /// A hash ([`crate::domain::hash_node_path`]) of
    /// `"{def_type}/{def_name}/{relative_path}"` for every descendant element
    /// (bounded depth — see
    /// [`crate::extract::defs::MAX_INLINE_NODE_PATH_DEPTH`]) of every
    /// concrete, named def written inline anywhere in any active mod's own
    /// `Defs/`, plus `"{def_type}/@{name}/{relative_path}"` for every
    /// `Name`-attributed template (see
    /// `extract::defs::DefsFile::inline_node_path_hashes`'s own doc comment)
    /// — the per-*node* sibling of `inline_type_names`' per-*type-string*
    /// check, needed because a type string existing inline somewhere doesn't
    /// prove a specific XML node at a specific address does. Same conventions
    /// as `inline_type_names`, deliberately: whole-file, not
    /// `MayRequire`-filtered (a gated-off def still counts as pre-existing
    /// evidence — proving otherwise would need per-element gating neither
    /// index does, and demoting is the safe direction either way), and only
    /// ever membership-tested, never iterated into report output, so a
    /// `HashSet` doesn't cost this crate's determinism guarantee. Hashed
    /// rather than the raw path text — on a real install, ~16MB of `u64`s
    /// versus ~120MB+ of owned `String`s: see `hash_node_path`'s own doc
    /// comment for why a hash collision is safe here (it can only ever
    /// produce a false "pre-exists", which demotes a `Hard` edge to advisory
    /// — never the reverse).
    pub inline_node_paths: HashSet<u64>,
    /// The union of every active mod's own
    /// [`ScannedMod::bundle_textures`](crate::domain::ScannedMod::bundle_textures)
    /// plus Core's own resource-container keys (`core_resource_textures`,
    /// passed into [`Self::build`]) — every texture a loose-file existence
    /// check alone would miss. Consulted
    /// **only** by `conflicts::textures::texture_path_resolves`/
    /// `collection_graphic_rows`, deliberately kept out of `texture_owners`:
    /// a bundle/Core texture is never a [`TextureOverride`](crate::domain::TextureOverride)
    /// of a loose file, since a loose file always wins the lookup.
    pub non_loose_textures: BTreeSet<String>,
    /// `core_resource_textures.len()`, as passed into [`Self::build`] — see
    /// [`MIN_CORE_RESOURCE_TEXTURES`] for how a low count disables
    /// `missing_texture_path` for the whole scan.
    pub core_resource_texture_count: usize,
}

/// The minimum number of Core resource-container-path strings
/// [`Indices::build`]'s `core_resource_textures` argument must carry before
/// `conflicts::textures::missing_texture_paths` trusts it at all — well
/// under the ~3,455 a real install's own Core index actually has. Below
/// this, the scan almost certainly read the wrong/a corrupted file, and
/// the check would be ~99.6% false positives against it, so it disables
/// itself instead rather than trusting a near-empty index.
pub const MIN_CORE_RESOURCE_TEXTURES: usize = 1_000;

impl Indices {
    /// `vanilla_assembly_names_seed` supplies names that can never be
    /// discovered by scanning mod folders (Core/DLC ship no `Assemblies/`
    /// folder of their own) — see
    /// [`ScanOutput::vanilla_assembly_names`](crate::domain::ScanOutput::vanilla_assembly_names).
    /// `active` is the single, externally-computed definition of "active"
    /// for this run (see [`ActiveMods`]) — this never recomputes its own.
    /// `core_resource_textures` is
    /// [`ScanOutput::core_resource_textures`](crate::domain::ScanOutput::core_resource_textures),
    /// already stripped of its `textures/` prefix.
    #[must_use]
    pub fn build(
        scanned: &[ScannedMod],
        vanilla_assembly_names_seed: &HashSet<String>,
        active: &ActiveMods,
        core_resource_textures: &BTreeSet<String>,
    ) -> Self {
        let mut def_owners: BTreeMap<DefKey, Vec<ModId>> = BTreeMap::new();
        let mut assembly_owners: BTreeMap<String, Vec<ModId>> = BTreeMap::new();
        let mut texture_owners: BTreeMap<String, Vec<ModId>> = BTreeMap::new();
        let mut vanilla_assembly_names = vanilla_assembly_names_seed.clone();
        let mut mod_source = HashMap::new();
        let mut mod_authors = HashMap::new();
        let mut mod_declared = HashMap::new();
        let mut assembly_versions = HashMap::new();
        let mut template_owners: BTreeMap<String, Vec<ModId>> = BTreeMap::new();
        let mut translation_key_owners: BTreeMap<String, Vec<ModId>> = BTreeMap::new();
        let mut sound_owners: BTreeMap<String, Vec<ModId>> = BTreeMap::new();
        let mut runtime_patch_owners: BTreeMap<(String, String), Vec<ModId>> = BTreeMap::new();
        let mut runtime_patch_assembly_names: BTreeMap<(String, String), BTreeSet<String>> =
            BTreeMap::new();
        let mut transpiler_owners: BTreeMap<(String, String), Vec<ModId>> = BTreeMap::new();
        let mut transpiler_assembly_names: BTreeMap<(String, String), BTreeSet<String>> =
            BTreeMap::new();
        let mut inline_type_names: HashSet<String> = HashSet::new();
        let mut inline_node_paths: HashSet<u64> = HashSet::new();
        let mut non_loose_textures: BTreeSet<String> = core_resource_textures.clone();

        for scanned_mod in scanned {
            let id = &scanned_mod.info.id;
            mod_source.insert(id.clone(), scanned_mod.info.source);
            mod_authors.insert(id.clone(), scanned_mod.info.authors.clone());
            mod_declared.insert(id.clone(), scanned_mod.info.declared.clone());
            non_loose_textures.extend(scanned_mod.bundle_textures.iter().cloned());

            for key in scanned_mod
                .defs
                .iter()
                .filter(|d| may_require_satisfied(&d.may_require, &d.may_require_any_of, active))
                .map(|d| (d.def_type.clone(), d.def_name.clone()))
                .collect::<HashSet<_>>()
            {
                def_owners.entry(key).or_default().push(id.clone());
            }
            for assembly in &scanned_mod.assemblies {
                if let Some(version) = assembly.version {
                    assembly_versions.insert((id.clone(), assembly.name.clone()), version);
                }
            }
            for name in scanned_mod
                .assemblies
                .iter()
                .map(|a| a.name.clone())
                .collect::<HashSet<_>>()
            {
                if scanned_mod.info.source.is_vanilla() {
                    vanilla_assembly_names.insert(name.clone());
                }
                assembly_owners.entry(name).or_default().push(id.clone());
            }
            for path in scanned_mod.textures.keys() {
                texture_owners
                    .entry(path.clone())
                    .or_default()
                    .push(id.clone());
            }
            for name in scanned_mod
                .templates
                .iter()
                .map(|t| t.name.clone())
                .collect::<HashSet<_>>()
            {
                template_owners.entry(name).or_default().push(id.clone());
            }
            for key in &scanned_mod.translation_keys {
                translation_key_owners
                    .entry(key.clone())
                    .or_default()
                    .push(id.clone());
            }
            for path in &scanned_mod.sounds {
                sound_owners
                    .entry(path.clone())
                    .or_default()
                    .push(id.clone());
            }
            for target in scanned_mod
                .assemblies
                .iter()
                .flat_map(|a| a.runtime_patches.iter())
                .map(|p| (p.type_name.clone(), p.method_name.clone()))
                .collect::<HashSet<_>>()
            {
                runtime_patch_owners
                    .entry(target)
                    .or_default()
                    .push(id.clone());
            }
            // Per assembly, not per mod: which shipped DLL actually
            // declared each target, so `runtime_patch_collisions` can
            // tell a bundled copy of the same DLL from a genuinely
            // independent patch.
            for assembly in &scanned_mod.assemblies {
                for patch in &assembly.runtime_patches {
                    runtime_patch_assembly_names
                        .entry((patch.type_name.clone(), patch.method_name.clone()))
                        .or_default()
                        .insert(assembly.name.clone());
                }
            }
            for target in scanned_mod
                .assemblies
                .iter()
                .flat_map(|a| a.runtime_patches.iter())
                .filter(|p| p.kind == crate::domain::RuntimePatchKind::Transpiler)
                .map(|p| (p.type_name.clone(), p.method_name.clone()))
                .collect::<HashSet<_>>()
            {
                transpiler_owners
                    .entry(target)
                    .or_default()
                    .push(id.clone());
            }
            for assembly in &scanned_mod.assemblies {
                for patch in &assembly.runtime_patches {
                    if patch.kind != crate::domain::RuntimePatchKind::Transpiler {
                        continue;
                    }
                    transpiler_assembly_names
                        .entry((patch.type_name.clone(), patch.method_name.clone()))
                        .or_default()
                        .insert(assembly.name.clone());
                }
            }
            inline_type_names.extend(scanned_mod.inline_types.iter().cloned());
            inline_node_paths.extend(scanned_mod.inline_node_path_hashes.iter().copied());
        }

        Self {
            def_owners,
            assembly_owners,
            texture_owners,
            vanilla_assembly_names,
            mod_source,
            mod_authors,
            mod_declared,
            assembly_versions,
            template_owners,
            translation_key_owners,
            sound_owners,
            runtime_patch_owners,
            runtime_patch_assembly_names,
            transpiler_owners,
            transpiler_assembly_names,
            inline_type_names,
            inline_node_paths,
            non_loose_textures,
            core_resource_texture_count: core_resource_textures.len(),
        }
    }

    /// Whether every mod in `owners` shares at least one author (case-insensitive)
    /// with every other mod in `owners`.
    #[must_use]
    pub fn shared_author(&self, owners: &[ModId]) -> bool {
        let mut author_sets = owners.iter().filter_map(|id| self.mod_authors.get(id));
        let Some(first) = author_sets.next() else {
            return false;
        };
        let mut common: HashSet<String> = first.iter().map(|a| a.to_lowercase()).collect();
        for set in author_sets {
            let this: HashSet<String> = set.iter().map(|a| a.to_lowercase()).collect();
            common.retain(|a| this.contains(a));
            if common.is_empty() {
                return false;
            }
        }
        !common.is_empty()
    }
}

/// Whether a `MayRequire`/`MayRequireAnyOf` gate is satisfied by the
/// active mod set: `MayRequire` needs every named packageId active,
/// `MayRequireAnyOf` needs at least one (an empty list is trivially
/// satisfied for each — no condition was declared).
#[must_use]
pub fn may_require_satisfied(
    may_require: &[String],
    may_require_any_of: &[String],
    active: &ActiveMods,
) -> bool {
    let all_required_active = may_require
        .iter()
        .all(|id| active.contains(&ModId::new(id)));
    let any_of_active = may_require_any_of.is_empty()
        || may_require_any_of
            .iter()
            .any(|id| active.contains(&ModId::new(id)));
    all_required_active && any_of_active
}

/// A `{display name -> id}` lookup that normalizes on **both** insert and
/// lookup, so [`gate_open`]'s "always lowercase the gate's own lookup"
/// contract can never be silently violated by a caller building the map by
/// hand.
///
/// **Why this exists**: `gate_open` always lowercases its own lookup key, so
/// a bare `HashMap<String, ModId>` parameter would let a caller fill it with
/// verbatim-cased keys — silently excluding a large share of indexed patch
/// operations from `verify` on a real install, and making
/// `ContributesNothing` give false "safe to disable" advice. The shared,
/// correctly-lowercased producers are `build_name_map` here and
/// `rim_session::use_cases::verify_order::build_gate_name_map` for the
/// session layer's own `Mod`-based equivalent. This type makes a mis-cased
/// map a compile error: the only way to get a key into a `DisplayNameIndex`
/// is [`Self::insert`], which always lowercases, and the only way to look one
/// up is [`Self::get`]/[`Self::contains_key`], which do too.
///
/// [`Self::insert`] always overwrites (plain `HashMap` semantics) —
/// "first mod to claim a name wins" is each *producer*'s own tie-break
/// (see `build_name_map`'s doc comment), not this type's, since the two
/// current producers differ only in what they iterate, never in whether
/// the first winner is kept.
///
/// **Deliberately a distinct type from
/// `rim_merge::patch_eval::ReplayContext::mod_names_by_display`**
/// (`&BTreeMap<String, ModId>`, verbatim-keyed, looked up with an exact
/// `.get` — `rim-session`'s `verify_order.rs`/`contributes_nothing.rs` build
/// both maps side by side, on purpose, with opposite contracts): keeping the
/// two as different container *types* (this newtype vs. a bare `BTreeMap`)
/// means a caller can never pass one where the other is expected, the same
/// class of mix-up described above.
#[derive(Debug, Clone, Default)]
pub struct DisplayNameIndex(HashMap<String, ModId>);

impl DisplayNameIndex {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts `display_name -> id`, lowercasing `display_name` first.
    /// Always overwrites an existing entry — see the type's own doc
    /// comment for why "first wins" is a producer concern, not this
    /// method's.
    pub fn insert(&mut self, display_name: &str, id: ModId) {
        self.0.insert(display_name.to_lowercase(), id);
    }

    /// The id `display_name` resolves to, lowercasing the lookup key
    /// first.
    #[must_use]
    pub fn get(&self, display_name: &str) -> Option<&ModId> {
        self.0.get(&display_name.to_lowercase())
    }

    /// Whether `display_name` (any case) is present.
    #[must_use]
    pub fn contains_key(&self, display_name: &str) -> bool {
        self.0.contains_key(&display_name.to_lowercase())
    }
}

/// Whether a `<match>`/`<nomatch>` [`FindModGate`] is open: `AnyActive`
/// when at least one named display name resolves to an active mod,
/// `NoneActive` when none of them do.
///
/// **Known, deliberate divergence from the real engine (checked, not
/// assumed): this lookup is case-insensitive ([`DisplayNameIndex`] lowercases
/// both sides), but RimWorld's own `Verse.PatchOperationFindMod.ApplyWorker`
/// resolves a name via `ModLister.HasActiveModWithName`, whose body is
/// `mod.Active && mod.Name == name` — ordinary C# `==`, case-sensitive**
/// (ground-truthed against the decompiled `Assembly-CSharp.dll`). This
/// gate's own lowercasing is left as-is rather than tightened to match: a
/// real-install measurement across over a thousand distinct `FindMod` gate
/// names found exactly **one** case-sensitivity mismatch (`"a real content
/// mod's display name"`), a real-world typo-tolerance win this gate's
/// leniency buys for negligible real cost, against a large, unscoped,
/// real-install-re-measurement-heavy change to every producer that calls
/// [`patch_op_active`] (`rim-analyzer`'s edge builders, plus `rim-session`'s
/// `verify_order`/`contributes_nothing`) for a single affected name.
/// **`rim_merge::patch_eval::ReplayContext::mod_names_by_display` is already
/// exact** (see that field's own doc comment) — for `rim-session`'s
/// `VerifyOrder`, which pairs this gate (as a candidate pre-filter) with that
/// exact-match replay, the one real-install mismatch is provably harmless:
/// the gate merely admits the op as a *candidate*, and the exact-match replay
/// still resolves the actual outcome correctly (`nomatch`), so the final
/// prediction is unaffected. **Not separately re-measured for
/// `rim-analyzer`'s own edge producers** (`patch_target_edges`/
/// `patch_injected_node_edges`/`patch_removed_node_edges` and friends) —
/// there this gate's answer *is* the final word, with no downstream
/// exact-match step to correct it, so a case-sensitivity mismatch there (if
/// the one real name ever participates in a gated mutating op) could in
/// principle misattribute an edge; disclosed as an unmeasured, theoretical
/// gap rather than silently assumed identical to the `VerifyOrder` case.
/// **Never lowercase `mod_names_by_display` to "align" the two** — this gate
/// is the one that's lenient, not the ground truth.
fn gate_open(gate: &FindModGate, name_map: &DisplayNameIndex) -> bool {
    let any_active = |names: &[String]| names.iter().any(|name| name_map.contains_key(name));
    match gate {
        FindModGate::AnyActive(names) => any_active(names),
        FindModGate::NoneActive(names) => !any_active(names),
    }
}

/// Whether every gate in a `find_mod_context` chain is open — vacuously
/// true when there are no enclosing `PatchOperationFindMod`s at all.
fn all_gates_open(gates: &[FindModGate], name_map: &DisplayNameIndex) -> bool {
    gates.iter().all(|gate| gate_open(gate, name_map))
}

/// Whether a patch op would actually run given the active mods: its own
/// `MayRequire`/`MayRequireAnyOf` gate is satisfied (when its own node is
/// one the game actually reads that attribute on), every enclosing
/// `PatchOperationFindMod` level's `<match>`/`<nomatch>` gate is open, and
/// every enclosing mod-setting-gated custom toggle class resolves it as
/// reachable at that toggle's own declared default (`op.toggle_active`,
/// `extract::patches::toggle_default`) — shared with `rim-merge`'s own
/// replay so the two layers never disagree about which ops actually run.
///
/// **`MayRequire`/`MayRequireAnyOf` only gate an op whose own node is a
/// `<li>` list item** (`op.is_list_item`) — ground-truthed against the
/// decompiled engine: `ModContentPack.LoadPatches` builds a top-level
/// `<Operation>` via `DirectXmlToObject.ObjectFromXml<PatchOperation>`,
/// which never reads `MayRequire` at all, and a `<match>`/`<nomatch>` node
/// that is itself one operation (not a list) is read the identical way.
/// Only `DirectXmlToObject.ListFromXml` — the reader for a
/// `PatchOperationSequence`'s `<operations>`, and for a `<match>`/
/// `<nomatch>` branch that itself holds a list — actually honours the
/// attribute. A non-list-item op's own `may_require`/`may_require_any_of`
/// is therefore vacuously satisfied here, whatever it says: the game runs
/// the op regardless.
#[must_use]
pub fn patch_op_active(op: &PatchOp, active: &ActiveMods, name_map: &DisplayNameIndex) -> bool {
    let may_require_gate_open =
        !op.is_list_item || may_require_satisfied(&op.may_require, &op.may_require_any_of, active);
    op.toggle_active && may_require_gate_open && all_gates_open(&op.find_mod_context, name_map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DeclaredOrder, DefEntry, Mod, PatchOp, XmlLocator};
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn scanned_mod(id: &str, author: &str, def_names: &[&str]) -> ScannedMod {
        ScannedMod {
            info: Mod {
                id: ModId::new(id),
                name: id.to_string(),
                authors: vec![author.to_string()],
                url: None,
                path: PathBuf::from(id),
                source: Source::Local,
                supported_versions: Vec::new(),
                declared: DeclaredOrder::default(),
                loaded_folders: Vec::new(),
                hard_dependents: 0,
                soft_dependents: 0,
                awareness_dependents: 0,
                is_framework_candidate: false,
                generated: None,
                workshop_id: None,
                load_folders_version_matched: None,
            },
            defs: def_names
                .iter()
                .map(|n| DefEntry {
                    def_type: "ThingDef".to_string(),
                    def_name: (*n).to_string(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: XmlLocator::for_test(),
                })
                .collect(),
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            assemblies: Vec::new(),
            sounds: std::collections::BTreeSet::new(),
            translation_keys: std::collections::BTreeSet::new(),
            inline_types: std::collections::BTreeSet::new(),
            manifest_order: Default::default(),
            texture_path_candidates: Vec::new(),
            inline_node_path_hashes: std::collections::HashSet::new(),
            if_mod_active_targets: Vec::new(),
            scan_cost: crate::domain::ScanCost::default(),
            nameless_def_count: 0,
            bundle_textures: Default::default(),
            undecodable_textures: Vec::new(),
            nested_may_require: Vec::new(),
        }
    }

    fn active_of(scanned: &[ScannedMod]) -> ActiveMods {
        ActiveMods::build(scanned)
    }

    #[test]
    fn def_owners_lists_every_mod_defining_the_same_key() {
        let scanned = vec![
            scanned_mod("a", "Alice", &["Wall"]),
            scanned_mod("b", "Bob", &["Wall"]),
        ];
        let indices = Indices::build(
            &scanned,
            &HashSet::new(),
            &active_of(&scanned),
            &BTreeSet::new(),
        );
        let owners = indices
            .def_owners
            .get(&("ThingDef".to_string(), "Wall".to_string()))
            .unwrap();
        assert_eq!(owners, &vec![ModId::new("a"), ModId::new("b")]);
    }

    #[test]
    fn def_gated_off_by_unsatisfied_may_require_is_excluded_from_ownership() {
        let mut gated = scanned_mod("a", "Alice", &[]);
        gated.defs = vec![DefEntry {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
            may_require: vec!["not.active".to_string()],
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: XmlLocator::for_test(),
        }];
        let scanned = vec![gated];
        let indices = Indices::build(
            &scanned,
            &HashSet::new(),
            &active_of(&scanned),
            &BTreeSet::new(),
        );
        assert!(
            !indices
                .def_owners
                .contains_key(&("ThingDef".to_string(), "Wall".to_string()))
        );
    }

    #[test]
    fn shared_author_true_only_when_every_owner_overlaps() {
        let scanned = vec![
            scanned_mod("a", "Alice", &[]),
            scanned_mod("b", "Alice", &[]),
        ];
        let indices = Indices::build(
            &scanned,
            &HashSet::new(),
            &active_of(&scanned),
            &BTreeSet::new(),
        );
        assert!(indices.shared_author(&[ModId::new("a"), ModId::new("b")]));
    }

    #[test]
    fn shared_author_false_when_authors_differ() {
        let scanned = vec![scanned_mod("a", "Alice", &[]), scanned_mod("b", "Bob", &[])];
        let indices = Indices::build(
            &scanned,
            &HashSet::new(),
            &active_of(&scanned),
            &BTreeSet::new(),
        );
        assert!(!indices.shared_author(&[ModId::new("a"), ModId::new("b")]));
    }

    #[test]
    fn vanilla_assembly_names_includes_the_seed_set() {
        let scanned = vec![scanned_mod("a", "Alice", &[])];
        let seed = HashSet::from(["assembly-csharp".to_string()]);
        let indices = Indices::build(&scanned, &seed, &active_of(&scanned), &BTreeSet::new());
        assert!(indices.vanilla_assembly_names.contains("assembly-csharp"));
    }

    fn template_entry(name: &str, def_type: &str) -> crate::domain::TemplateEntry {
        crate::domain::TemplateEntry {
            graphic_class: None,
            may_require: Vec::new(),
            def_type: def_type.to_string(),
            name: name.to_string(),
            parent_name: None,
            is_abstract: true,
            locator: XmlLocator::for_test(),
        }
    }

    #[test]
    fn template_owners_lists_every_mod_registering_the_same_name() {
        let mut a = scanned_mod("a", "Alice", &[]);
        a.templates = vec![template_entry("WallBase", "ThingDef")];
        let mut b = scanned_mod("b", "Bob", &[]);
        b.templates = vec![template_entry("WallBase", "ThingDef")];
        let scanned = vec![a, b];
        let indices = Indices::build(
            &scanned,
            &HashSet::new(),
            &active_of(&scanned),
            &BTreeSet::new(),
        );
        assert_eq!(
            indices.template_owners.get("WallBase"),
            Some(&vec![ModId::new("a"), ModId::new("b")])
        );
    }

    #[test]
    fn translation_key_owners_lists_every_mod_defining_the_same_key() {
        let mut a = scanned_mod("a", "Alice", &[]);
        a.translation_keys = BTreeSet::from(["Greeting".to_string()]);
        let mut b = scanned_mod("b", "Bob", &[]);
        b.translation_keys = BTreeSet::from(["Greeting".to_string()]);
        let scanned = vec![a, b];
        let indices = Indices::build(
            &scanned,
            &HashSet::new(),
            &active_of(&scanned),
            &BTreeSet::new(),
        );
        assert_eq!(
            indices.translation_key_owners.get("Greeting"),
            Some(&vec![ModId::new("a"), ModId::new("b")])
        );
    }

    #[test]
    fn sound_owners_lists_every_mod_shipping_the_same_path() {
        let mut a = scanned_mod("a", "Alice", &[]);
        a.sounds = BTreeSet::from(["shot_fire".to_string()]);
        let mut b = scanned_mod("b", "Bob", &[]);
        b.sounds = BTreeSet::from(["shot_fire".to_string()]);
        let scanned = vec![a, b];
        let indices = Indices::build(
            &scanned,
            &HashSet::new(),
            &active_of(&scanned),
            &BTreeSet::new(),
        );
        assert_eq!(
            indices.sound_owners.get("shot_fire"),
            Some(&vec![ModId::new("a"), ModId::new("b")])
        );
    }

    #[test]
    fn runtime_patch_owners_lists_every_mod_patching_the_same_target() {
        use crate::domain::{AssemblyInfo, RuntimePatchKind, RuntimePatchTarget};
        let target = RuntimePatchTarget {
            type_name: "Verse.Pawn".to_string(),
            method_name: "Kill".to_string(),
            kind: RuntimePatchKind::Unknown,
        };
        let assembly = |patches: Vec<RuntimePatchTarget>| AssemblyInfo {
            file_name: "Mod".to_string(),
            name: "mod".to_string(),
            references: Vec::new(),
            version: None,
            runtime_patches: patches,
            type_hierarchy: Vec::new(),
            parse_failed: false,
        };
        let mut a = scanned_mod("a", "Alice", &[]);
        a.assemblies = vec![assembly(vec![target.clone()])];
        let mut b = scanned_mod("b", "Bob", &[]);
        b.assemblies = vec![assembly(vec![target])];
        let scanned = vec![a, b];
        let indices = Indices::build(
            &scanned,
            &HashSet::new(),
            &active_of(&scanned),
            &BTreeSet::new(),
        );
        assert_eq!(
            indices
                .runtime_patch_owners
                .get(&("Verse.Pawn".to_string(), "Kill".to_string())),
            Some(&vec![ModId::new("a"), ModId::new("b")])
        );
    }

    #[test]
    fn inline_type_names_collects_across_every_mod() {
        let mut a = scanned_mod("a", "Alice", &[]);
        a.inline_types = BTreeSet::from(["Example.Weapons.HeavyWeapon".to_string()]);
        let scanned = vec![a];
        let indices = Indices::build(
            &scanned,
            &HashSet::new(),
            &active_of(&scanned),
            &BTreeSet::new(),
        );
        assert!(
            indices
                .inline_type_names
                .contains("Example.Weapons.HeavyWeapon")
        );
        assert!(!indices.inline_type_names.contains("Unrelated.Type"));
    }

    #[test]
    fn may_require_satisfied_requires_every_may_require_id_active() {
        let active = ActiveMods::from_ids([ModId::new("a.b")]);
        assert!(may_require_satisfied(&["a.b".to_string()], &[], &active));
        assert!(!may_require_satisfied(
            &["a.b".to_string(), "c.d".to_string()],
            &[],
            &active
        ));
    }

    #[test]
    fn may_require_satisfied_requires_at_least_one_any_of_id_active() {
        let active = ActiveMods::from_ids([ModId::new("a.b")]);
        assert!(may_require_satisfied(
            &[],
            &["a.b".to_string(), "c.d".to_string()],
            &active
        ));
        assert!(!may_require_satisfied(
            &[],
            &["c.d".to_string(), "e.f".to_string()],
            &active
        ));
    }

    #[test]
    fn may_require_satisfied_matches_a_steam_suffixed_active_id() {
        let active = ActiveMods::from_ids([ModId::new("a.b_steam")]);
        assert!(may_require_satisfied(&["a.b".to_string()], &[], &active));
    }

    fn bare_op(find_mod_context: Vec<FindModGate>) -> PatchOp {
        PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
            xpath: None,
            target: None,
            find_mod_context,
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
            conditional_xpath: None,
            is_list_item: false,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: XmlLocator::for_test(),
        }
    }

    fn name_map_with(names: &[&str]) -> DisplayNameIndex {
        let mut map = DisplayNameIndex::new();
        for name in names {
            map.insert(name, ModId::new(*name));
        }
        map
    }

    #[test]
    fn gate_all_open_false_when_one_gate_is_closed() {
        let name_map = name_map_with(&["active"]);
        let gates = vec![
            FindModGate::AnyActive(vec!["active".to_string()]),
            FindModGate::AnyActive(vec!["inactive".to_string()]),
        ];
        assert!(!all_gates_open(&gates, &name_map));
    }

    #[test]
    fn gate_all_open_true_when_the_single_gate_matches() {
        let name_map = name_map_with(&["active"]);
        let gates = vec![FindModGate::AnyActive(vec!["active".to_string()])];
        assert!(all_gates_open(&gates, &name_map));
    }

    #[test]
    fn gate_any_active_with_no_names_is_closed() {
        let gates = vec![FindModGate::AnyActive(vec![])];
        assert!(!all_gates_open(&gates, &DisplayNameIndex::new()));
    }

    #[test]
    fn gate_no_gates_at_all_is_vacuously_open() {
        assert!(all_gates_open(&[], &DisplayNameIndex::new()));
    }

    #[test]
    fn nomatch_op_excluded_when_listed_mod_is_active() {
        let name_map = name_map_with(&["other mod"]);
        let op = bare_op(vec![FindModGate::NoneActive(vec!["Other Mod".to_string()])]);
        assert!(!patch_op_active(&op, &ActiveMods::default(), &name_map));
    }

    #[test]
    fn nomatch_op_included_when_no_listed_mod_is_active() {
        let name_map = DisplayNameIndex::new();
        let op = bare_op(vec![FindModGate::NoneActive(vec!["Other Mod".to_string()])]);
        assert!(patch_op_active(&op, &ActiveMods::default(), &name_map));
    }

    /// The `DisplayNameIndex` newtype test: a caller handing
    /// `patch_op_active`/`gate_open` a bare, verbatim-keyed `HashMap<String,
    /// ModId>` (which silently excludes a large share of indexed patch ops
    /// from `verify` on a real install) is a compile error: `patch_op_active`
    /// takes a `DisplayNameIndex`, and its only constructor path
    /// ([`DisplayNameIndex::insert`]) always lowercases. What's left to test
    /// is the normalization contract itself: an entry inserted under one case
    /// is found under a *different* case, both at insert (`"Other Mod"`
    /// stored, looked up as `"other mod"`) and at lookup (stored lowercase
    /// already, looked up under the gate's own original mixed case) — either
    /// direction failing would silently reintroduce the same class of miss
    /// this type exists to close off.
    #[test]
    fn display_name_index_matches_regardless_of_case_on_either_side() {
        let mut name_map = DisplayNameIndex::new();
        name_map.insert("Other Mod", ModId::new("other.mod"));
        let op = bare_op(vec![FindModGate::AnyActive(vec!["Other Mod".to_string()])]);

        assert!(
            patch_op_active(&op, &ActiveMods::default(), &name_map),
            "an entry inserted under one case must satisfy a gate naming it under any other case"
        );
    }

    // -- is_list_item gates MayRequire/MayRequireAnyOf --

    /// The game never reads `MayRequire` on a top-level `<Operation>` node
    /// (`ModContentPack.LoadPatches` builds it via
    /// `DirectXmlToObject.ObjectFromXml`, which doesn't read the
    /// attribute) — so a non-list-item op with an unsatisfied `MayRequire`
    /// must still run. Regression: before this change, `patch_op_active`
    /// gated on every op's own `may_require` regardless of `is_list_item`,
    /// and this assertion failed.
    #[test]
    fn a_non_list_items_unsatisfied_may_require_does_not_gate_patch_op_active() {
        let op = PatchOp {
            may_require: vec!["not.installed".to_string()],
            is_list_item: false,
            ..bare_op(vec![])
        };

        assert!(
            patch_op_active(&op, &ActiveMods::default(), &DisplayNameIndex::new()),
            "a top-level <Operation> (or a <match>/<nomatch> single-operation \
             node)'s own MayRequire is never read by the game, so it must run \
             regardless of whether the named mod is active"
        );
    }

    /// The same shape, `MayRequireAnyOf` — both attributes are unread on a
    /// non-list-item node, not just `MayRequire`.
    #[test]
    fn a_non_list_items_unsatisfied_may_require_any_of_does_not_gate_patch_op_active() {
        let op = PatchOp {
            may_require_any_of: vec!["not.installed".to_string()],
            is_list_item: false,
            ..bare_op(vec![])
        };

        assert!(patch_op_active(
            &op,
            &ActiveMods::default(),
            &DisplayNameIndex::new()
        ));
    }

    /// A `<li>` list item — a `PatchOperationSequence`'s `<operations>`
    /// child, or a list-shaped `<match>`/`<nomatch>` branch child — is the
    /// one shape `DirectXmlToObject.ListFromXml` actually reads
    /// `MayRequire` on, so its own unsatisfied gate still closes it.
    #[test]
    fn a_list_items_unsatisfied_may_require_still_gates_patch_op_active() {
        let op = PatchOp {
            may_require: vec!["not.installed".to_string()],
            is_list_item: true,
            ..bare_op(vec![])
        };

        assert!(!patch_op_active(
            &op,
            &ActiveMods::default(),
            &DisplayNameIndex::new()
        ));
    }

    /// A `<li>` list item whose `MayRequire` names an active mod still
    /// runs — the gate only *closes* the op, it doesn't otherwise change
    /// behaviour.
    #[test]
    fn a_list_items_satisfied_may_require_runs() {
        let active = ActiveMods::from_ids([ModId::new("installed")]);
        let op = PatchOp {
            may_require: vec!["installed".to_string()],
            is_list_item: true,
            ..bare_op(vec![])
        };

        assert!(patch_op_active(&op, &active, &DisplayNameIndex::new()));
    }
}
