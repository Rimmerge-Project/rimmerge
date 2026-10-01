//! [`SourceIndex`]: every def, template, and patch op a scan found,
//! looked back up by key instead of by which mod happened to own it —
//! the lookup a later merge editor needs to find a def's XML again after
//! a scan, without re-walking every mod's files.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use crate::domain::{
    DefEntry, DefTarget, ModId, PatchOp, RefSiteOwner, RefSiteShape, ScanOutput, ScannedMod,
    Selector, TemplateEntry, XmlLocator,
};

use super::indices::{DefKey, MIN_CORE_RESOURCE_TEXTURES};
use super::texture_index::TextureIndex;

/// Every `(file, first ordinal)` top-level `<Operation>` ancestor seen for
/// one `(def_type, def_name, selector)` key while building
/// [`SourceIndex::ops_by_mod`] — see that field's own doc comment.
type TopLevelAncestors = BTreeSet<(Arc<Path>, u32)>;

/// One patch op plus the mod that ships it — [`SourceIndex::patch_ops_by_def`]'s
/// value needs both: the op alone can't say who to gate a replay on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedPatchOp {
    pub mod_id: ModId,
    pub op: PatchOp,
}

impl IndexedPatchOp {
    /// Whether this op's own node is a top-level `<Operation>` (a direct
    /// child of `<Patch>`), not a nested descendant reached through a
    /// `PatchOperationSequence`/`FindMod`/`Conditional`'s `<operations>`
    /// list or `<match>`/`<nomatch>` branch.
    ///
    /// [`crate::domain::XmlLocator::element_path`]'s own doc comment gives
    /// the shape: `extract::patches::walk`'s initial call seeds every
    /// top-level operation's path with exactly one ordinal (`[index]`). A
    /// nested descendant's path is at least *two* ordinals deep, not
    /// three: a `<match>`/`<nomatch>` branch whose own node carries a
    /// `Class` attribute is walked at the wrapper's own path (e.g. `[0,
    /// 1]` for a single-level `PatchOperationFindMod`'s `match`); a branch
    /// that instead lists several `<operations>`/`<li>` children goes one
    /// ordinal deeper still (see `walk_operation`/`walk_branch`). So
    /// `element_path.len() == 1` identifies a top-level op exactly.
    ///
    /// This is **not** the dedup rule [`SourceIndex::ops_by_mod`] counts
    /// by. Several flattened entries — including nested descendants whose
    /// own `is_top_level` is `false` — can share one top-level ancestor
    /// (the same enclosing `<Operation>`, identified by `(file, element_path.first())`,
    /// the same rule `rim-session::use_cases::plan_merge::top_level_operations`
    /// re-derives a top-level locator from). `ops_by_mod` counts distinct
    /// ancestors, not distinct `is_top_level` entries — see its own doc
    /// comment. This predicate is kept for a caller that needs to know
    /// whether *this one* entry is itself the top-level node, not what it
    /// counts toward.
    #[must_use]
    pub fn is_top_level(&self) -> bool {
        self.op.locator.element_path.len() == 1
    }
}

/// Every def, template, and mutating patch op a scan found, keyed for
/// lookup instead of by scan order. Built once, right after a scan
/// completes, from a [`ScanOutput`] that is about to be dropped —
/// nothing here holds a reference back into it.
#[derive(Debug, Clone, Default)]
pub struct SourceIndex {
    /// Every concrete def, per owner. A `Vec` because one mod can define
    /// the same key twice in different files (**first** wins in-mod —
    /// `DefDatabase<T>.AddAllInMods` logs `Mod X has multiple <T>s named
    /// Y. Skipping.` on the second one — but both are kept here, in
    /// scan order; the caller decides).
    pub defs: BTreeMap<(ModId, DefKey), Vec<DefEntry>>,
    /// `Name`-attributed nodes, per `(def_type, Name)`, owners in scan
    /// (load) order.
    pub templates: BTreeMap<(String, String), Vec<(ModId, TemplateEntry)>>,
    /// Every mutating op whose xpath parsed to a `DefTarget`, keyed by the
    /// def it targets (any `sub_path` folds into the same key), in load
    /// order then file order then document order — exactly the replay
    /// order a merge evaluator needs.
    pub patch_ops_by_def: BTreeMap<(String, String, Selector), Vec<IndexedPatchOp>>,
    /// Mutating ops whose xpath named no def at all, counted per mod —
    /// un-scopable, so a caller reports them as a caveat instead of
    /// silently ignoring them. An op whose head names *several* defs is
    /// not one of these: [`crate::extract::xpath_target::parse_all`] returns every name,
    /// and `build` indexes the op under each, so `patch_ops_by_def`
    /// fully accounts for it.
    pub unscoped_op_counts: BTreeMap<ModId, usize>,
    /// Every mod that defines `(def_type, def_name)`, in scan (load)
    /// order — the inverse of [`Self::defs`], so "who owns this def" is a
    /// lookup, not a scan of every `(ModId, DefKey)` key. One entry per
    /// mod regardless of how many times that mod itself defines the same
    /// key (see `defs`'s own doc comment on why a mod can define a key
    /// more than once).
    ///
    /// **Never the complete "who owns this def" answer on its own**: a def
    /// that exists only via a whole-def `<xpath>Defs</xpath>` patch injection
    /// (no literal `Defs/**/*.xml` file of its own) has *no* entry here by
    /// construction — `extract::defs::index` only ever parses literal def
    /// files, never a patch operation's own injected content (those defs have
    /// no owner in `Indices.def_owners`, the reason
    /// [`super::edges::injected_def_owners`] exists at all). See
    /// [`Self::injected_def_owners`] for that other half, and mind that it
    /// has no raw source [`Self::defs`] can hand back either — a caller that
    /// needs "does this def exist at all" must check both; one that needs
    /// "can I read this def's raw XML" only ever gets an answer from this
    /// one.
    pub owners_by_def: BTreeMap<DefKey, Vec<ModId>>,
    /// Every mod that patch-injects `(def_type, def_name)` as a whole,
    /// unconditionally-gated-active `<xpath>Defs</xpath>` operation —
    /// [`super::edges::injected_def_owners`], recomputed here (once per scan,
    /// from the same [`ScanOutput`] this whole index is built from) since
    /// `rim-session` — the only downstream consumer that needs this — holds a
    /// [`SourceIndex`], never the raw `ScanOutput`/`ActiveMods`/name-map
    /// `injected_def_owners` itself needs. A def with an entry *only* here
    /// and none in [`Self::owners_by_def`] is real (RimWorld genuinely
    /// creates it), so `VerifyOrder`'s zero-owner `DeadTarget` fast path must
    /// not fire for it — but it also has no raw `Defs/**/*.xml` source
    /// [`Self::defs`] can read, so it cannot be replayed either; the caller's
    /// own job, not this index's.
    pub injected_def_owners: BTreeMap<DefKey, Vec<ModId>>,
    /// Per mod, every `(def_type, def_name, selector)` its mutating ops
    /// target, with a count of *distinct top-level `<Operation>`
    /// ancestors* that touch it — the inverse of
    /// [`Self::patch_ops_by_def`]. `patch_ops_by_def` flattens every op
    /// *and* its nested `PatchOperationSequence`/`FindMod`/`Conditional`
    /// descendants into separate entries (right for collision detection,
    /// wrong for a count a user reads as "how many operations patch
    /// this"). Filtering that list down to entries where
    /// [`IndexedPatchOp::is_top_level`] is `true` would silently drop
    /// every op nested under a container — most patches use one — so this
    /// map instead dedups on each entry's own top-level ancestor,
    /// `(file, element_path.first())`: the same rule
    /// `rim-session::use_cases::plan_merge::top_level_operations`
    /// re-derives a top-level locator from. Two nested descendants under
    /// the same ancestor (e.g. both branches of one `PatchOperationFindMod`)
    /// still count once; a top-level leaf and a nested descendant under a
    /// *different* ancestor count as two, even when both target the same
    /// key.
    pub ops_by_mod: BTreeMap<ModId, BTreeMap<(String, String, Selector), usize>>,
    /// Per `(def_type, Name)` template, every concrete def or template
    /// whose `ParentName` names it, in load order — "what inherits from
    /// this". Direct children only; the chain is walked on demand. A
    /// template child is keyed the same shape as a concrete def
    /// (`def_type`, its own `Name`) even though it has no `defName` of
    /// its own — the pair alone (not a [`Selector`]) is enough to look it
    /// back up in either [`Self::defs`] or [`Self::templates`]. A node
    /// that is a concrete def and *also* `Name`-attributed is one physical
    /// element indexed in both `scanned_mod.defs` and `scanned_mod.templates`
    /// (same locator — see `extract::defs`'s own doc comment), so `build`
    /// dedups a template's children on `(mod, locator)`: one such node is
    /// one child of its parent, not two, keeping the def's own
    /// `(def_type, def_name)` identity over the template's `(def_type,
    /// Name)` alias for that same node (the defs loop runs first).
    pub children_by_template: BTreeMap<(String, String), Vec<(ModId, DefKey)>>,
    /// Every `defName`, with each `def_type`/owner pair that defines it, in
    /// load order — the name index for resolving a value an assignment
    /// instance names (e.g. Example's `speciesNames`) to the def type(s) and
    /// mod(s) that actually define it, without knowing which type to look
    /// under first. `defName`s are unique only *within* a def type, not
    /// across all of them, so a name commonly legitimately resolves to more
    /// than one type (a hediff and a race sharing a name is the real-install
    /// case that motivates keeping every type here rather than picking one) —
    /// the caller (`rim-merge`'s `assign::infer`) takes the field-wide
    /// majority. One entry per `(def_type, owner)` pair per name, deduped
    /// exactly like [`Self::owners_by_def`] (a mod defining the same key
    /// twice in different files still counts once): built in the same per-mod
    /// walk over `scanned_mod.defs`, off the same `def_keys_this_mod` dedup
    /// guard, so it costs no second scan. Built only from `scanned_mod.defs`
    /// (concrete defs, each with a real `defName`) — never from
    /// `scanned_mod.templates`, unlike [`Self::children_by_template`]'s
    /// deliberate inclusion of both: a `Name`-only abstract template never
    /// appears here at all, and a concrete def that also carries a `Name`
    /// attribute (same physical node in both lists, same locator) still
    /// contributes exactly one entry, under its own `defName`, never a second
    /// one under its `Name`. The owner is the raw active [`ModId`] (a
    /// `_steam`-suffixed copy stays suffixed, exactly like every other field
    /// in this struct) — a caller comparing it against a reference or target
    /// set must go through [`ModId::base`] itself, the same as everywhere
    /// else in this crate.
    pub defs_by_name: BTreeMap<String, Vec<(String, ModId)>>,
    /// Every mod that ships an assembly with this (lowercased) name, in load
    /// order — a narrower mirror of
    /// [`super::indices::Indices::assembly_owners`] (the DLL-namespace
    /// ownership signal), kept here too because `Indices` itself needs a raw
    /// `&[ScannedMod]` this crate's own downstream consumers (`rim-session`,
    /// which only ever sees a [`SourceIndex`], never the scan it was built
    /// from) have no way to rebuild. See [`Self::dll_owner_of`].
    pub assembly_owners: BTreeMap<String, Vec<ModId>>,
    /// A direct copy of [`ScanOutput::child_value_hashes_by_mod`] — carried
    /// onto this index (rather than left for a caller to fetch off a
    /// `ScanOutput` this crate's own downstream consumers never hold) purely
    /// so [`super::edges::child_value_targets`] has something to resolve a
    /// `[race/intelligence="Humanlike"]`-shaped head against, both here
    /// (building [`Self::patch_ops_by_def`]) and from any other caller that
    /// only ever sees a [`SourceIndex`].
    pub child_value_hashes_by_mod: BTreeMap<ModId, HashSet<u64>>,
    /// Which texture keys exist and who ships them — see [`TextureIndex`].
    /// Built from the same scan as everything else here; not part of the
    /// serialized report.
    pub textures: TextureIndex,
    /// Race `ThingDef` name -> the `PawnKindDef`s whose **raw** `race`
    /// declares it, sorted. A template's `race` reaches every concrete
    /// descendant (via [`Self::children_by_template`]). Raw means a `race`
    /// set only by a patch is missed, and a descendant that overrides its
    /// template's `race` is still listed under the template's: a caller
    /// that needs the truth re-reads each kind's effective `race`.
    pub kinds_by_race: BTreeMap<String, Vec<String>>,
}

/// Just the two maps [`SourceIndex::build`]'s first pass computes from
/// `scanned_mod.defs`/`.templates` alone — `owners_by_def` and
/// `children_by_template`. Pulled out as its own function so
/// [`super::inheritance::broken_inheritance`] can build the identical
/// index without either running the rest of [`build`]'s (heavier,
/// patch-op-dependent) second pass or reimplementing this walk a second
/// time and risking the two drifting apart on what "child" means.
#[derive(Debug, Default)]
pub struct ChildrenIndex {
    pub owners_by_def: BTreeMap<DefKey, Vec<ModId>>,
    pub children_by_template: BTreeMap<(String, String), Vec<(ModId, DefKey)>>,
}

/// See [`ChildrenIndex`]. Walks `scan.load_order`, exactly like [`build`]'s
/// own first pass (**not** `scan.scanned_mods`'s own order, which a caller
/// is free to hand over in any order at all — [`build`]'s own doc comment
/// on why load order is what decides "first wins" here applies verbatim).
pub fn build_children_index(scan: &ScanOutput) -> ChildrenIndex {
    let by_id: HashMap<&ModId, &ScannedMod> = scan
        .scanned_mods
        .iter()
        .map(|scanned_mod| (&scanned_mod.info.id, scanned_mod))
        .collect();
    let mut index = ChildrenIndex::default();
    for mod_id in scan.load_order.as_slice() {
        let Some(scanned_mod) = by_id.get(mod_id).copied() else {
            continue;
        };
        // Same two dedup guards as `build`'s own first pass — see that
        // loop's own comments for why each is needed.
        let mut def_keys_this_mod: BTreeSet<DefKey> = BTreeSet::new();
        let mut children_by_template_seen: BTreeSet<((String, String), XmlLocator)> =
            BTreeSet::new();
        for def in &scanned_mod.defs {
            let def_key: DefKey = (def.def_type.clone(), def.def_name.clone());
            if def_keys_this_mod.insert(def_key.clone()) {
                index
                    .owners_by_def
                    .entry(def_key.clone())
                    .or_default()
                    .push(mod_id.clone());
            }
            if let Some(parent_name) = &def.parent_name {
                let parent_key = (def.def_type.clone(), parent_name.clone());
                if children_by_template_seen.insert((parent_key.clone(), def.locator.clone())) {
                    index
                        .children_by_template
                        .entry(parent_key)
                        .or_default()
                        .push((mod_id.clone(), def_key));
                }
            }
        }
        for template in &scanned_mod.templates {
            let parent_name = match &template.parent_name {
                Some(parent_name) => parent_name,
                None => continue,
            };
            let parent_key = (template.def_type.clone(), parent_name.clone());
            if children_by_template_seen.insert((parent_key.clone(), template.locator.clone())) {
                index
                    .children_by_template
                    .entry(parent_key)
                    .or_default()
                    .push((
                        mod_id.clone(),
                        (template.def_type.clone(), template.name.clone()),
                    ));
            }
        }
    }
    index
}

/// Loose owners per key in load order, plus the bundle and Core-resource
/// keys — the same folds as `Indices::build`'s `texture_owners` and
/// `non_loose_textures`.
fn build_texture_index(scan: &ScanOutput, by_id: &HashMap<&ModId, &ScannedMod>) -> TextureIndex {
    let mut loose: BTreeMap<String, Vec<ModId>> = BTreeMap::new();
    let mut non_loose: BTreeSet<String> = scan.core_resource_textures.clone();
    for mod_id in scan.load_order.as_slice() {
        let Some(scanned_mod) = by_id.get(mod_id).copied() else {
            continue;
        };
        for key in scanned_mod.textures.keys() {
            loose.entry(key.clone()).or_default().push(mod_id.clone());
        }
        non_loose.extend(scanned_mod.bundle_textures.iter().cloned());
    }
    let is_core_index_trusted = scan.core_resource_textures.len() >= MIN_CORE_RESOURCE_TEXTURES;
    TextureIndex::new(loose, non_loose, is_core_index_trusted)
}

/// See [`SourceIndex::kinds_by_race`].
fn build_kinds_by_race(
    scan: &ScanOutput,
    owners_by_def: &BTreeMap<DefKey, Vec<ModId>>,
    children_by_template: &BTreeMap<(String, String), Vec<(ModId, DefKey)>>,
) -> BTreeMap<String, Vec<String>> {
    let mut kinds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let race_sites = scan.ref_sites_by_mod.values().flatten().filter(|site| {
        &*site.def_type == PAWN_KIND_DEF
            && &*site.field_path == "race"
            && site.shape == RefSiteShape::Scalar
    });
    for site in race_sites {
        let entry = kinds.entry(site.value.clone()).or_default();
        match &*site.owner {
            RefSiteOwner::Def { def_name, .. } => {
                entry.insert(def_name.clone());
            }
            RefSiteOwner::Template { name, .. } => {
                entry.extend(concrete_descendants(
                    name,
                    owners_by_def,
                    children_by_template,
                ));
            }
            RefSiteOwner::Patch { .. } => {}
        }
    }
    kinds
        .into_iter()
        .filter(|(_, names)| !names.is_empty())
        .map(|(race, names)| (race, names.into_iter().collect()))
        .collect()
}

const PAWN_KIND_DEF: &str = "PawnKindDef";

/// Every concrete `PawnKindDef` below the template `root`, through
/// templates and `Name`-attributed defs alike.
fn concrete_descendants(
    root: &str,
    owners_by_def: &BTreeMap<DefKey, Vec<ModId>>,
    children_by_template: &BTreeMap<(String, String), Vec<(ModId, DefKey)>>,
) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut pending: Vec<&str> = vec![root];
    while let Some(name) = pending.pop() {
        if !visited.insert(name) {
            continue;
        }
        let children = children_by_template
            .get(&(PAWN_KIND_DEF.to_owned(), name.to_owned()))
            .into_iter()
            .flatten();
        for (_, (def_type, child_name)) in children {
            if owners_by_def.contains_key(&(def_type.clone(), child_name.clone())) {
                found.insert(child_name.clone());
            }
            pending.push(child_name);
        }
    }
    found
}

impl SourceIndex {
    /// The active mod owning `type_name`'s namespace, by shipped DLL name —
    /// the DLL-namespace ownership signal, exposed here so a `rim-session`
    /// use case can build the `dll_owner` closure
    /// [`rim_merge::assign::infer`]/`dependencies` need without any new port:
    /// every session already holds a [`SourceIndex`]. Delegates to
    /// [`super::edges::dll_owner_of`], the exact rule `uses_type_edges` uses
    /// to build `UsesType` edges, over [`Self::assembly_owners`] alone (this
    /// index carries no `vanilla_assembly_names`/`texture_owners`/etc. —
    /// `dll_owner_of`'s own rule never reads those).
    #[must_use]
    pub fn dll_owner_of(&self, type_name: &str) -> Option<&ModId> {
        super::edges::dll_owner_of(type_name, &self.assembly_owners)
    }

    /// Who ships the assembly `type_name`'s namespace names, telling "no
    /// owner" from "several owners": the strict query a log's stack frame is
    /// attributed by (see [`super::edges::namespace_ownership`]). Unlike
    /// [`Self::dll_owner_of`] it never falls through an ambiguous longest
    /// prefix to a shorter unique one.
    #[must_use]
    pub fn namespace_ownership(&self, type_name: &str) -> super::edges::AssemblyOwnership<'_> {
        super::edges::namespace_ownership(type_name, &self.assembly_owners)
    }

    /// Who ships the assembly named exactly `assembly_name`
    /// (case-insensitive): none, one, or several (see
    /// [`super::edges::assembly_ownership`]).
    #[must_use]
    pub fn assembly_ownership(&self, assembly_name: &str) -> super::edges::AssemblyOwnership<'_> {
        super::edges::assembly_ownership(assembly_name, &self.assembly_owners)
    }
}

/// Builds a [`SourceIndex`] from a completed scan, walking mods in load
/// order so every per-def op list already comes out in replay order.
#[must_use]
pub fn build(scan: &ScanOutput) -> SourceIndex {
    let by_id: HashMap<&ModId, &crate::domain::ScannedMod> = scan
        .scanned_mods
        .iter()
        .map(|scanned_mod| (&scanned_mod.info.id, scanned_mod))
        .collect();

    // `child_value_hashes_by_mod` is a direct copy, not recomputed —
    // `ScanOutput` already carries this per-mod (see that field's own doc
    // comment for why it lives there rather than on `ScannedMod`), so
    // there's nothing to fold across mods here the way
    // `assembly_owners`/`owners_by_def` are.
    let mut index = SourceIndex {
        child_value_hashes_by_mod: scan.child_value_hashes_by_mod.clone(),
        textures: build_texture_index(scan, &by_id),
        ..SourceIndex::default()
    };
    // Recomputed here rather than threaded in from `report_builder`'s own
    // (identical) computation: `source_index::build`'s existing signature
    // (`&ScanOutput` alone) is `rim-io`'s only call site
    // (`AnalyzerScanner::scan_and_analyze`, right after `build_ref`, per
    // this module's own top-of-file doc comment on why it must happen
    // before `scan` is dropped) and widening `build_ref`'s own return
    // type just to smuggle these two intermediate values out would ripple
    // into every existing caller of it for a cost this doesn't need to
    // pay — `ActiveMods::build`/`edges::build_name_map` are both pure,
    // in-memory folds over the same already-scanned `ScannedMod` list,
    // negligible next to the disk-scanning work the rest of a scan does.
    // The name-map's own warnings are already captured once, correctly,
    // by `report_builder::build_ref`'s own call — discarded here, not
    // duplicated into anything user-visible.
    let active = super::indices::ActiveMods::build(&scan.scanned_mods);
    let (name_map, _name_map_warnings) = super::edges::build_name_map(&scan.scanned_mods);
    index.injected_def_owners =
        super::edges::injected_def_owners(&scan.scanned_mods, &active, &name_map);
    // Shared with `analysis::inheritance::broken_inheritance` — see
    // `build_children_index`'s own doc comment for why this isn't
    // recomputed inline in the loop below the way it used to be.
    let children_index = build_children_index(scan);
    index.owners_by_def = children_index.owners_by_def;
    index.children_by_template = children_index.children_by_template;
    index.kinds_by_race =
        build_kinds_by_race(scan, &index.owners_by_def, &index.children_by_template);

    // Two passes, not one interleaved loop — deliberate: resolving such a
    // head via `edges::parent_name_targets` in the patch-ops pass below needs
    // a *complete* `children_by_template`/`owners_by_def`, built from every
    // mod's defs/templates, not just whichever mods happened to load before
    // the one whose patch is being indexed. Splitting into two full passes
    // over `load_order` is the only way to guarantee that: a single
    // interleaved loop can never see a later mod's own template registration
    // while indexing an earlier mod's patch ops.
    for mod_id in scan.load_order.as_slice() {
        let Some(scanned_mod) = by_id.get(mod_id).copied() else {
            continue;
        };

        // One entry per distinct assembly name this mod ships — a
        // `BTreeSet` (not `Indices::build`'s own `HashSet`) purely to
        // keep this crate's own "no `HashMap`/`HashSet` iteration reaches
        // output" rule trivially provable here too, even though iteration
        // order over a single mod's own distinct names can't actually
        // affect `assembly_owners`' final shape (each name is a separate
        // map key).
        let assembly_names_this_mod: BTreeSet<&str> = scanned_mod
            .assemblies
            .iter()
            .map(|assembly| assembly.name.as_str())
            .collect();
        for name in assembly_names_this_mod {
            index
                .assembly_owners
                .entry(name.to_string())
                .or_default()
                .push(mod_id.clone());
        }

        // Dedup within this one mod only: a mod can define the same key
        // twice across different files, and `defs_by_name` lists a mod
        // once, not once per file — `defs` above already keeps every raw
        // entry for that. `owners_by_def`/`children_by_template`
        // themselves come from `children_index` above now, with the
        // identical dedup rule applied there instead.
        let mut def_keys_this_mod: BTreeSet<DefKey> = BTreeSet::new();
        for def in &scanned_mod.defs {
            let def_key: DefKey = (def.def_type.clone(), def.def_name.clone());
            let key = (mod_id.clone(), def_key.clone());
            index.defs.entry(key).or_default().push(def.clone());
            if def_keys_this_mod.insert(def_key.clone()) {
                index
                    .defs_by_name
                    .entry(def.def_name.clone())
                    .or_default()
                    .push((def.def_type.clone(), mod_id.clone()));
            }
        }

        for template in &scanned_mod.templates {
            let key = (template.def_type.clone(), template.name.clone());
            index
                .templates
                .entry(key)
                .or_default()
                .push((mod_id.clone(), template.clone()));
        }
    }

    // Second pass: `index.owners_by_def`/`.children_by_template` are now
    // complete (every mod's own defs/templates were indexed above,
    // regardless of load order), so a `[@ParentName="X"]`-headed op can
    // finally be resolved against every real child, not just whichever
    // happened to load first.
    for mod_id in scan.load_order.as_slice() {
        let Some(scanned_mod) = by_id.get(mod_id).copied() else {
            continue;
        };

        let mut unscoped = 0usize;
        // Per key, every top-level `<Operation>` ancestor that touches it
        // — `(file, element_path.first())` — deduped across both a
        // top-level leaf op and any nested descendant sharing the same
        // ancestor (`SourceIndex::ops_by_mod`'s own doc comment).
        let mut ancestors_by_key: BTreeMap<(String, String, Selector), TopLevelAncestors> =
            BTreeMap::new();
        for op in &scanned_mod.patch_ops {
            if !op.is_mutating {
                continue;
            }
            let mut targets = super::patch_op_targets(op);
            // A `[@ParentName="X"]` head names no def directly — `xpath_target`
            // alone can never resolve it (see `edges::parent_name_targets`'s
            // own doc comment) — so `patch_op_targets` always returns empty
            // for one, same as for any other genuinely unscoped op. Tried
            // only as a fallback, exactly mirroring `patch_op_targets`'s own
            // "ordinary parse first, fall back only on empty" shape, and
            // only when there's a raw xpath at all to resolve against.
            if targets.is_empty()
                && let Some(xpath) = op.xpath.as_deref()
            {
                targets = super::edges::parent_name_targets(
                    xpath,
                    &index.owners_by_def,
                    &index.children_by_template,
                );
            }
            // A `[race/intelligence="Humanlike"]`-shaped head names no def
            // directly either — same "tried only as a fallback" shape as the
            // `@ParentName=` case just above, and just as disjoint a query
            // (neither can ever also match the other's xpath shape), so
            // trying it unconditionally after costs nothing on every op this
            // doesn't apply to.
            if targets.is_empty()
                && let Some(xpath) = op.xpath.as_deref()
            {
                targets = super::edges::child_value_targets(
                    xpath,
                    &index.owners_by_def,
                    &index.child_value_hashes_by_mod,
                );
            }
            // A whole-`<Defs>`-root injection (`<xpath>Defs</xpath>` or
            // `/Defs`) names no def in its own xpath at all — every def it
            // creates lives only in `PatchOp::injected_paths`'s own
            // whole-def shape, `"{element_tag}/{defName_text}"` per
            // top-level `<value>` child that carries a `<defName>`
            // (`extract::patches::injected_paths_of`'s own doc comment).
            // Without this fallback, a same-mod "Remove the def, then
            // re-`Add xpath=Defs` it" idiom (the real re-creator shape
            // `analysis::edges::patches::remover_recreates_region` already
            // detects on the *remover*'s own side) is invisible to
            // `patch_ops_by_def` for the recreated def: the recreating
            // `Add` never becomes one of that def's own replay
            // contributions, so `rim_merge::patch_eval::replay` never sees
            // it and a later, unrelated op on the same def predicts a false
            // `DeadTarget` against a tree the game itself never leaves
            // empty. Only ever tried for this exact xpath text — the same
            // literal-string constraint `injected_paths_of` itself already
            // documents (no case or slash variants occur on a real
            // install) — so it can never compete with the class-restricted
            // first shape in the same field, which requires a genuinely
            // different, resolvable xpath to have produced any entries at
            // all.
            if targets.is_empty() && matches!(op.xpath.as_deref(), Some("Defs" | "/Defs")) {
                targets = op
                    .injected_paths
                    .iter()
                    .filter_map(|path| {
                        let (def_type, def_name) = path.split_once('/')?;
                        Some(DefTarget {
                            def_type: def_type.to_string(),
                            def_name: def_name.to_string(),
                            selector: Selector::DefName,
                            sub_path: None,
                        })
                    })
                    .collect();
            }
            if targets.is_empty() {
                unscoped += 1;
                continue;
            }
            // Every mutating op — top-level or nested — has a non-empty
            // `element_path` (`extract::patches::walk_operation` always
            // seeds at least one ordinal); its first ordinal identifies
            // the top-level `<Operation>` it lives under.
            let Some(&first_ordinal) = op.locator.element_path.first() else {
                continue;
            };
            for target in targets {
                let key = (target.def_type, target.def_name, target.selector);
                ancestors_by_key
                    .entry(key.clone())
                    .or_default()
                    .insert((Arc::clone(&op.locator.file), first_ordinal));
                let indexed_op = IndexedPatchOp {
                    mod_id: mod_id.clone(),
                    op: op.clone(),
                };
                index
                    .patch_ops_by_def
                    .entry(key)
                    .or_default()
                    .push(indexed_op);
            }
        }
        if unscoped > 0 {
            index.unscoped_op_counts.insert(mod_id.clone(), unscoped);
        }
        if !ancestors_by_key.is_empty() {
            let counts = ancestors_by_key
                .into_iter()
                .map(|(key, ancestors)| (key, ancestors.len()))
                .collect();
            index.ops_by_mod.insert(mod_id.clone(), counts);
        }
    }

    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::edges::AssemblyOwnership;
    use crate::domain::{
        AssemblyInfo, DeclaredOrder, DefTarget, LoadOrder, Mod, ScannedMod, Source, XmlLocator,
    };
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    fn mod_info(id: &str) -> Mod {
        Mod {
            id: ModId::new(id),
            name: id.to_string(),
            authors: Vec::new(),
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
        }
    }

    fn def(def_type: &str, def_name: &str) -> DefEntry {
        DefEntry {
            def_type: def_type.to_string(),
            def_name: def_name.to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: None,
            locator: XmlLocator::for_test(),
        }
    }

    fn def_with_parent(def_type: &str, def_name: &str, parent_name: &str) -> DefEntry {
        DefEntry {
            parent_name: Some(parent_name.to_string()),
            ..def(def_type, def_name)
        }
    }

    fn template(def_type: &str, name: &str) -> TemplateEntry {
        TemplateEntry {
            graphic_class: None,
            may_require: Vec::new(),
            def_type: def_type.to_string(),
            name: name.to_string(),
            parent_name: None,
            is_abstract: true,
            locator: XmlLocator::for_test(),
        }
    }

    fn template_with_parent(def_type: &str, name: &str, parent_name: &str) -> TemplateEntry {
        TemplateEntry {
            graphic_class: None,
            may_require: Vec::new(),
            parent_name: Some(parent_name.to_string()),
            ..template(def_type, name)
        }
    }

    fn mutating_op(def_type: &str, def_name: &str, class: &str) -> PatchOp {
        PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: class.to_string(),
            xpath: None,
            target: Some(DefTarget {
                def_type: def_type.to_string(),
                def_name: def_name.to_string(),
                selector: Selector::DefName,
                sub_path: None,
            }),
            find_mod_context: Vec::new(),
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

    /// A nested descendant op (e.g. under a `PatchOperationSequence`'s
    /// `<operations>` list, or a `PatchOperationFindMod`'s `<match>`
    /// branch) — same shape as [`mutating_op`], but with a multi-ordinal
    /// `element_path` so [`IndexedPatchOp::is_top_level`] reports `false`
    /// for it, mirroring what `extract::patches::walk_operation`/
    /// `walk_branch` actually produce for a real nested node.
    fn nested_mutating_op(def_type: &str, def_name: &str, class: &str) -> PatchOp {
        PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
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
            locator: XmlLocator::new(Arc::from(Path::new("test.xml")), vec![0, 0, 0]),
            ..mutating_op(def_type, def_name, class)
        }
    }

    fn unscoped_op() -> PatchOp {
        PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            class: "PatchOperationAdd".to_string(),
            xpath: Some("weird xpath".to_string()),
            target: None,
            find_mod_context: Vec::new(),
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

    fn scanned(
        id: &str,
        defs: Vec<DefEntry>,
        templates: Vec<TemplateEntry>,
        patch_ops: Vec<PatchOp>,
    ) -> ScannedMod {
        ScannedMod {
            info: mod_info(id),
            defs,
            templates,
            patch_ops,
            textures: BTreeMap::new(),
            assemblies: Vec::<AssemblyInfo>::new(),
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

    fn scan(mods: Vec<ScannedMod>, order: &[&str]) -> ScanOutput {
        let discovered_mod_count = mods.len();
        ScanOutput {
            load_order: LoadOrder::new(order.iter().map(|id| ModId::new(*id)).collect()),
            scanned_mods: mods,
            missing_mods: Vec::new(),
            vanilla_assembly_names: std::collections::HashSet::new(),
            vanilla_type_hierarchy: Vec::new(),
            warnings: Vec::new(),
            child_value_hashes_by_mod: BTreeMap::new(),
            inactive_mods: Vec::new(),
            discovered_mod_count,
            core_resource_textures: BTreeSet::new(),
            ref_sites_by_mod: BTreeMap::new(),
        }
    }

    #[test]
    fn indexes_defs_per_owner_and_key() {
        let a = scanned("a", vec![def("ThingDef", "Wall")], vec![], vec![]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        let key = (
            ModId::new("a"),
            ("ThingDef".to_string(), "Wall".to_string()),
        );
        assert_eq!(index.defs.get(&key).map(Vec::len), Some(1));
    }

    #[test]
    fn indexes_templates_with_owners_in_load_order() {
        let a = scanned("a", vec![], vec![template("HediffDef", "Base")], vec![]);
        let b = scanned("b", vec![], vec![template("HediffDef", "Base")], vec![]);
        let output = scan(vec![b, a], &["a", "b"]);

        let index = build(&output);

        let owners = index
            .templates
            .get(&("HediffDef".to_string(), "Base".to_string()))
            .unwrap();
        assert_eq!(
            owners.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
            vec![ModId::new("a"), ModId::new("b")]
        );
    }

    /// The whole point of the index: ops on the same def from different
    /// mods come out ordered by load order, not by scan/declaration
    /// order — the exact order a replay must apply them in.
    #[test]
    fn patch_ops_are_ordered_by_load_order_then_original_op_order() {
        let a = scanned(
            "a",
            vec![],
            vec![],
            vec![
                mutating_op("BiomeDef", "Forest", "PatchOperationReplace"),
                mutating_op("BiomeDef", "Forest", "PatchOperationAdd"),
            ],
        );
        let b = scanned(
            "b",
            vec![],
            vec![],
            vec![mutating_op("BiomeDef", "Forest", "PatchOperationReplace")],
        );
        // Declared in scan order b, a — load order says a loads first.
        let output = scan(vec![b, a], &["a", "b"]);

        let index = build(&output);

        let ops = index
            .patch_ops_by_def
            .get(&(
                "BiomeDef".to_string(),
                "Forest".to_string(),
                Selector::DefName,
            ))
            .unwrap();
        assert_eq!(
            ops.iter().map(|op| op.mod_id.clone()).collect::<Vec<_>>(),
            vec![ModId::new("a"), ModId::new("a"), ModId::new("b")]
        );
        // Mod `a`'s own two ops keep their original relative order.
        assert_eq!(ops[0].op.class, "PatchOperationReplace");
        assert_eq!(ops[1].op.class, "PatchOperationAdd");
    }

    #[test]
    fn non_mutating_ops_are_excluded_from_patch_ops_by_def() {
        let mut op = mutating_op("ThingDef", "Wall", "PatchOperationFindMod");
        op.is_mutating = false;
        let a = scanned("a", vec![], vec![], vec![op]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        assert!(index.patch_ops_by_def.is_empty());
    }

    #[test]
    fn unscoped_ops_are_counted_per_mod_not_indexed_by_def() {
        let a = scanned("a", vec![], vec![], vec![unscoped_op(), unscoped_op()]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        assert!(index.patch_ops_by_def.is_empty());
        assert_eq!(index.unscoped_op_counts.get(&ModId::new("a")), Some(&2));
    }

    /// An op whose head names several defs is indexed under **every** one
    /// of them (RimWorld applies it to each), and is fully scoped — never
    /// counted as unscoped.
    #[test]
    fn an_op_whose_head_names_two_defs_is_indexed_under_both_and_is_not_unscoped() {
        let mut op = mutating_op("BiomeDef", "Forest", "PatchOperationReplace");
        op.xpath =
            Some(r#"Defs/BiomeDef[defName="Forest" or defName="Other"]/plantDensity"#.to_string());
        let a = scanned("a", vec![], vec![], vec![op]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        for def_name in ["Forest", "Other"] {
            let key = (
                "BiomeDef".to_string(),
                def_name.to_string(),
                Selector::DefName,
            );
            assert_eq!(
                index.patch_ops_by_def.get(&key).map(Vec::len),
                Some(1),
                "{def_name}"
            );
        }
        assert!(index.unscoped_op_counts.is_empty());
    }

    /// End to end through `build` itself — not just
    /// `edges::parent_name_targets`'s own unit tests — proving the two-pass
    /// split actually delivers a complete `children_by_template` to the
    /// second pass: a `[@ParentName="X"]`-headed op is indexed under its
    /// real child, `Wall`, even though `Wall`'s own def (with
    /// `ParentName="WallBase"`) is declared by a mod loading *after* the
    /// patching mod — the shape a single interleaved pass could never
    /// resolve, since `children_by_template` wouldn't contain `Wall` yet when
    /// `x`'s own patch op was processed.
    #[test]
    fn a_parent_name_headed_op_is_indexed_under_a_later_loading_mods_own_child() {
        let op = PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            xpath: Some(r#"Defs/ThingDef[@ParentName="WallBase"]/statBases"#.to_string()),
            target: None,
            ..mutating_op("ThingDef", "unused", "PatchOperationAdd")
        };
        let x = scanned("x", vec![], vec![], vec![op]);
        // `core` (the def's real owner) loads *after* `x` (the patcher) —
        // `patch_ops_by_def` must still find it.
        let core = scanned(
            "core",
            vec![def_with_parent("ThingDef", "Wall", "WallBase")],
            vec![template("ThingDef", "WallBase")],
            vec![],
        );
        let output = scan(vec![x, core], &["x", "core"]);

        let index = build(&output);

        let key = (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        );
        assert_eq!(
            index.patch_ops_by_def.get(&key).map(Vec::len),
            Some(1),
            "{:?}",
            index.patch_ops_by_def
        );
        assert!(index.unscoped_op_counts.is_empty());
    }

    /// A whole-`<Defs>`-root injection (`<xpath>Defs</xpath>`, no
    /// `op.target` of its own) is still indexed under the def its own
    /// `injected_paths` names — the fallback `patch_ops_by_def` needs so a
    /// same-mod "Remove the def, then re-`Add xpath=Defs` it" idiom's own
    /// recreating `Add` becomes one of that def's real replay
    /// contributions (otherwise `rim_merge::patch_eval::replay` never sees
    /// it and predicts a false `DeadTarget` against a tree the game itself
    /// never leaves empty).
    #[test]
    fn a_whole_defs_root_injection_is_indexed_under_the_def_its_injected_paths_names() {
        let op = PatchOp {
            xpath: Some("Defs".to_string()),
            target: None,
            injected_paths: std::collections::BTreeSet::from(["ThingDef/Wall".to_string()]),
            ..mutating_op("ThingDef", "unused", "PatchOperationAdd")
        };
        let a = scanned("a", vec![], vec![], vec![op]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        let key = (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        );
        assert_eq!(
            index.patch_ops_by_def.get(&key).map(Vec::len),
            Some(1),
            "{:?}",
            index.patch_ops_by_def
        );
        assert!(index.unscoped_op_counts.is_empty());
    }

    /// The same shape, but with no `<defName>`-bearing entry in
    /// `injected_paths` at all (an op whose own `<value>` injects nothing
    /// this shape can key by) — stays genuinely unscoped, never crashing on
    /// a malformed or absent entry.
    #[test]
    fn a_whole_defs_root_injection_with_no_injected_paths_entry_stays_unscoped() {
        let op = PatchOp {
            xpath: Some("Defs".to_string()),
            target: None,
            injected_paths: std::collections::BTreeSet::new(),
            ..mutating_op("ThingDef", "unused", "PatchOperationAdd")
        };
        let a = scanned("a", vec![], vec![], vec![op]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        assert!(index.patch_ops_by_def.is_empty());
        assert_eq!(index.unscoped_op_counts.get(&ModId::new("a")), Some(&1));
    }

    /// A `@Name` leaf mixed into the same disjunction indexes under the
    /// template selector, not the def one.
    #[test]
    fn a_head_mixing_def_name_and_name_attribute_indexes_under_both_selectors() {
        let mut op = mutating_op("ThingDef", "Wall", "PatchOperationReplace");
        op.xpath = Some(r#"Defs/ThingDef[defName="Wall" or @Name="WallBase"]/comps"#.to_string());
        let a = scanned("a", vec![], vec![], vec![op]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        assert!(index.patch_ops_by_def.contains_key(&(
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName
        )));
        assert!(index.patch_ops_by_def.contains_key(&(
            "ThingDef".to_string(),
            "WallBase".to_string(),
            Selector::NameAttr
        )));
    }

    /// An ordinary single-def xpath must never spuriously count as
    /// unscoped just because `op.target` is `Some`.
    #[test]
    fn an_ordinary_single_def_op_is_not_counted_as_unscoped() {
        let mut op = mutating_op("BiomeDef", "Forest", "PatchOperationReplace");
        op.xpath = Some(r#"Defs/BiomeDef[defName="Forest"]/plantDensity"#.to_string());
        let a = scanned("a", vec![], vec![], vec![op]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        assert!(index.unscoped_op_counts.is_empty());
    }

    #[test]
    fn a_mod_named_in_load_order_but_never_scanned_contributes_nothing() {
        let output = scan(vec![], &["missing"]);

        let index = build(&output);

        assert!(index.defs.is_empty());
        assert!(index.templates.is_empty());
        assert!(index.patch_ops_by_def.is_empty());
    }

    /// A whole-def-injection op (`target: None`, `injected_paths` holding the
    /// `"{element_tag}/{defName}"` shape —
    /// [`super::edges::injected_def_owners`]'s own convention, mirrored here)
    /// populates [`SourceIndex::injected_def_owners`] but never
    /// [`SourceIndex::owners_by_def`]/[`SourceIndex::defs`] — the whole point
    /// of the field: this def has no literal `Defs/**/*.xml` source `build`
    /// ever indexes, since `extract::defs::index` only ever parses def files,
    /// never a patch operation's own injected content.
    #[test]
    fn injected_def_owners_lists_a_mod_that_patch_injects_a_whole_def_but_owners_by_def_does_not() {
        let mut a = scanned("a", vec![], vec![], vec![]);
        a.patch_ops = vec![PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            xpath: Some("Defs".to_string()),
            target: None,
            injected_paths: BTreeSet::from(["ThingDef/NewThing".to_string()]),
            ..mutating_op("ThingDef", "NewThing", "PatchOperationAdd")
        }];
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        let key: DefKey = ("ThingDef".to_string(), "NewThing".to_string());
        assert_eq!(
            index.injected_def_owners.get(&key),
            Some(&vec![ModId::new("a")])
        );
        assert!(
            !index.owners_by_def.contains_key(&key),
            "an injected-only def must never appear in owners_by_def — it has no raw source"
        );
    }

    /// An injecting op gated off by an unsatisfied `MayRequire` — the
    /// same "compat patch only when the other mod is present" shape
    /// `injected_def_owners` itself already guards against — contributes
    /// no ownership here either, since this index just calls straight
    /// through to that same gated function.
    #[test]
    fn injected_def_owners_excludes_a_gated_off_op() {
        let mut a = scanned("a", vec![], vec![], vec![]);
        a.patch_ops = vec![PatchOp {
            injected_template_names: std::collections::BTreeSet::new(),
            xpath: Some("Defs".to_string()),
            target: None,
            injected_paths: BTreeSet::from(["ThingDef/NewThing".to_string()]),
            may_require: vec!["not.active".to_string()],
            // A list item is the one shape the game actually reads
            // MayRequire on; a top-level op's own attribute is unread.
            is_list_item: true,
            ..mutating_op("ThingDef", "NewThing", "PatchOperationAdd")
        }];
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        assert!(index.injected_def_owners.is_empty());
    }

    // -- inverse maps ----------

    /// `owners_by_def` is `defs`'s own inverse: a def owned by three mods
    /// lists all three, in load order — not scan/declaration order.
    #[test]
    fn owners_by_def_lists_every_owner_of_a_def_in_load_order() {
        let a = scanned("a", vec![def("ThingDef", "Wall")], vec![], vec![]);
        let b = scanned("b", vec![def("ThingDef", "Wall")], vec![], vec![]);
        let c = scanned("c", vec![def("ThingDef", "Wall")], vec![], vec![]);
        // Declared in scan order c, a, b — load order says a, b, c.
        let output = scan(vec![c, a, b], &["a", "b", "c"]);

        let index = build(&output);

        let key: DefKey = ("ThingDef".to_string(), "Wall".to_string());
        assert_eq!(
            index.owners_by_def.get(&key),
            Some(&vec![ModId::new("a"), ModId::new("b"), ModId::new("c")])
        );
        // Agrees with the forward map: every mod listed here has a
        // `(mod, key)` entry in `defs`, and vice versa.
        for owner in &index.owners_by_def[&key] {
            assert!(index.defs.contains_key(&(owner.clone(), key.clone())));
        }
        let owners_from_defs: BTreeSet<ModId> = index
            .defs
            .keys()
            .filter(|(_, def_key)| *def_key == key)
            .map(|(mod_id, _)| mod_id.clone())
            .collect();
        assert_eq!(
            owners_from_defs,
            index.owners_by_def[&key].iter().cloned().collect()
        );
    }

    /// A mod defining the same key twice (two files) is listed once in
    /// `owners_by_def`, even though `defs` keeps both raw entries.
    #[test]
    fn owners_by_def_lists_a_mod_once_despite_two_definitions_in_the_same_mod() {
        let a = scanned(
            "a",
            vec![def("ThingDef", "Wall"), def("ThingDef", "Wall")],
            vec![],
            vec![],
        );
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        let key: DefKey = ("ThingDef".to_string(), "Wall".to_string());
        assert_eq!(index.owners_by_def.get(&key), Some(&vec![ModId::new("a")]));
        assert_eq!(
            index.defs.get(&(ModId::new("a"), key)).map(Vec::len),
            Some(2)
        );
    }

    /// `defs_by_name` agrees with `owners_by_def`: every def appears under
    /// its `defName` with the right `def_type` and owner, in load order.
    #[test]
    fn defs_by_name_agrees_with_owners_by_def() {
        let a = scanned("a", vec![def("ThingDef", "Wall")], vec![], vec![]);
        let b = scanned("b", vec![def("ThingDef", "Wall")], vec![], vec![]);
        // Declared in scan order b, a — load order says a, b.
        let output = scan(vec![b, a], &["a", "b"]);

        let index = build(&output);

        let by_name = index
            .defs_by_name
            .get("Wall")
            .expect("Wall must be indexed");
        assert_eq!(
            by_name,
            &vec![
                ("ThingDef".to_string(), ModId::new("a")),
                ("ThingDef".to_string(), ModId::new("b")),
            ]
        );
        // Same owners `owners_by_def` reports for the same key.
        let key: DefKey = ("ThingDef".to_string(), "Wall".to_string());
        assert_eq!(
            by_name.iter().map(|(_, id)| id.clone()).collect::<Vec<_>>(),
            index.owners_by_def[&key]
        );
    }

    /// A `defName` that exists under two different def types (e.g. a
    /// hediff and a race sharing a name — the real-install case the
    /// field-wide type vote exists for) lists both, each with its own
    /// type and owner.
    #[test]
    fn defs_by_name_lists_every_type_a_name_resolves_to() {
        let a = scanned(
            "a",
            vec![def("HediffDef", "Clamp"), def("ThingDef", "Clamp")],
            vec![],
            vec![],
        );
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        let by_name = index
            .defs_by_name
            .get("Clamp")
            .expect("Clamp must be indexed");
        assert_eq!(
            by_name,
            &vec![
                ("HediffDef".to_string(), ModId::new("a")),
                ("ThingDef".to_string(), ModId::new("a")),
            ]
        );
    }

    /// A mod defining the same key twice (two files) is listed once, same
    /// dedup as `owners_by_def`.
    #[test]
    fn defs_by_name_lists_a_mod_once_despite_two_definitions_in_the_same_mod() {
        let a = scanned(
            "a",
            vec![def("ThingDef", "Wall"), def("ThingDef", "Wall")],
            vec![],
            vec![],
        );
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        assert_eq!(
            index.defs_by_name.get("Wall"),
            Some(&vec![("ThingDef".to_string(), ModId::new("a"))])
        );
    }

    /// A `Name`-only abstract template (no `defName` of its own) must never
    /// appear in `defs_by_name` — it is built only from `scanned_mod.defs`,
    /// never from `scanned_mod.templates`, unlike `children_by_template`'s
    /// deliberate inclusion of both. A concrete def that also carries a
    /// `Name` attribute (the same physical node in both lists, same
    /// locator — `extract::defs`'s own doc comment) still contributes
    /// exactly one entry, under its own `defName`, never a second one under
    /// its `Name`.
    #[test]
    fn defs_by_name_excludes_template_only_names_and_counts_a_dual_attributed_def_once() {
        let shared_locator = XmlLocator::for_test();
        let wall = DefEntry {
            locator: shared_locator.clone(),
            ..def("ThingDef", "Wall")
        };
        let wall_as_template = TemplateEntry {
            graphic_class: None,
            may_require: Vec::new(),
            locator: shared_locator,
            ..template("ThingDef", "WallBase")
        };
        let abstract_only = template("ThingDef", "AbstractBase");
        let a = scanned(
            "a",
            vec![wall],
            vec![wall_as_template, abstract_only],
            vec![],
        );
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        assert_eq!(
            index.defs_by_name.get("Wall"),
            Some(&vec![("ThingDef".to_string(), ModId::new("a"))]),
            "a def with both Name and defName contributes exactly one entry, under defName"
        );
        assert!(
            !index.defs_by_name.contains_key("WallBase"),
            "a template's own Name must never appear in defs_by_name"
        );
        assert!(
            !index.defs_by_name.contains_key("AbstractBase"),
            "a Name-only abstract template must never appear in defs_by_name"
        );
    }

    /// `assembly_owners` is keyed by assembly name (deduped per mod), and
    /// `dll_owner_of` finds the sole owner of the *longest* matching
    /// namespace prefix — the exact rule `analysis::edges::uses_type_edges`
    /// builds `UsesType` edges with, reachable off a plain `SourceIndex` with
    /// no `Indices` of its own.
    #[test]
    fn dll_owner_of_finds_the_longest_matching_namespace_owner() {
        let framework_dll = AssemblyInfo {
            file_name: "Example".to_string(),
            name: "example".to_string(),
            references: Vec::new(),
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: Vec::new(),
            parse_failed: false,
        };
        let framework = ScannedMod {
            assemblies: vec![framework_dll],
            ..scanned("example.framework", vec![], vec![], vec![])
        };
        let output = scan(vec![framework], &["example.framework"]);

        let index = build(&output);

        assert_eq!(
            index.assembly_owners.get("example"),
            Some(&vec![ModId::new("example.framework")])
        );
        assert_eq!(
            index.dll_owner_of("example.PartDef"),
            Some(&ModId::new("example.framework")),
            "the longest dot-separated prefix ('example') matches the shipped assembly name"
        );
        assert_eq!(
            index.dll_owner_of("Verse.Pawn"),
            None,
            "an engine namespace is never owned by any mod"
        );
        assert_eq!(
            index.dll_owner_of("someother.Thing"),
            None,
            "no active mod ships an assembly named 'someother'"
        );
    }

    fn mod_shipping(id: &str, assembly_names: &[&str]) -> ScannedMod {
        let assemblies = assembly_names
            .iter()
            .map(|name| AssemblyInfo {
                file_name: (*name).to_string(),
                name: name.to_lowercase(),
                references: Vec::new(),
                version: None,
                runtime_patches: Vec::new(),
                type_hierarchy: Vec::new(),
                parse_failed: false,
            })
            .collect();
        ScannedMod {
            assemblies,
            ..scanned(id, vec![], vec![], vec![])
        }
    }

    fn ownership_index() -> SourceIndex {
        let mods = vec![
            mod_shipping("solo", &["Solo"]),
            mod_shipping("twin.a", &["Twin.Lib"]),
            mod_shipping("twin.b", &["Twin.Lib"]),
            mod_shipping("twin.short", &["Twin"]),
            mod_shipping("verse.named", &["Verse"]),
        ];
        let ids = ["solo", "twin.a", "twin.b", "twin.short", "verse.named"];
        build(&scan(mods, &ids))
    }

    #[test]
    fn namespace_ownership_names_the_sole_owner_of_the_longest_prefix() {
        let index = ownership_index();

        assert_eq!(
            index.namespace_ownership("Solo.Deep.Thing"),
            AssemblyOwnership::Sole(&ModId::new("solo"))
        );
    }

    #[test]
    fn namespace_ownership_reports_every_owner_of_an_ambiguous_longest_prefix() {
        let index = ownership_index();

        let ownership = index.namespace_ownership("Twin.Lib.Thing");

        assert_eq!(
            ownership,
            AssemblyOwnership::Shared(&[ModId::new("twin.a"), ModId::new("twin.b")]),
            "the shorter unique prefix 'twin' must not take over"
        );
        assert_eq!(
            index.dll_owner_of("Twin.Lib.Thing"),
            Some(&ModId::new("twin.short")),
            "dll_owner_of keeps its fall-through for its other consumers"
        );
    }

    #[test]
    fn namespace_ownership_is_unowned_without_a_matching_assembly() {
        let index = ownership_index();

        assert_eq!(
            index.namespace_ownership("Nobody.Thing"),
            AssemblyOwnership::Unowned
        );
        assert_eq!(
            index.namespace_ownership("Solo"),
            AssemblyOwnership::Unowned,
            "a name with no namespace has no prefix to match"
        );
    }

    #[test]
    fn assembly_ownership_reads_an_exact_assembly_name_without_prefix_matching() {
        let index = ownership_index();

        assert_eq!(
            index.assembly_ownership("TWIN.lib"),
            AssemblyOwnership::Shared(&[ModId::new("twin.a"), ModId::new("twin.b")])
        );
        assert_eq!(
            index.assembly_ownership("Twin"),
            AssemblyOwnership::Sole(&ModId::new("twin.short"))
        );
        assert_eq!(
            index.assembly_ownership("Twin.Lib.Extra"),
            AssemblyOwnership::Unowned
        );
    }

    #[test]
    fn namespace_ownership_never_gives_an_engine_namespace_to_a_mod() {
        let index = ownership_index();

        assert_eq!(
            index.namespace_ownership("Verse.Pawn"),
            AssemblyOwnership::Unowned
        );
    }

    /// `children_by_template` lists both a concrete def and another
    /// template that declare `ParentName` against the same template, in
    /// load order, keyed by the template's own `(def_type, Name)`.
    #[test]
    fn children_by_template_lists_direct_def_and_template_children() {
        let a = scanned("a", vec![], vec![template("ThingDef", "Base")], vec![]);
        let b = scanned(
            "b",
            vec![def_with_parent("ThingDef", "Wall", "Base")],
            vec![],
            vec![],
        );
        let c = scanned(
            "c",
            vec![],
            vec![template_with_parent("ThingDef", "Reinforced", "Base")],
            vec![],
        );
        let output = scan(vec![a, b, c], &["a", "b", "c"]);

        let index = build(&output);

        let children = index
            .children_by_template
            .get(&("ThingDef".to_string(), "Base".to_string()))
            .expect("Base must have children indexed");
        assert_eq!(
            children,
            &vec![
                (
                    ModId::new("b"),
                    ("ThingDef".to_string(), "Wall".to_string())
                ),
                (
                    ModId::new("c"),
                    ("ThingDef".to_string(), "Reinforced".to_string())
                ),
            ]
        );
        // The template itself (no `ParentName`) is not its own child.
        assert!(!children.iter().any(|(_, key)| key.1 == "Base"));
    }

    /// A concrete def that also carries a `Name` attribute is indexed once
    /// in `defs` and once in `templates` (same locator — `extract::defs`'s
    /// own doc comment), both declaring the same `ParentName`. It must
    /// count as one child of that parent, not two.
    #[test]
    fn children_by_template_counts_a_def_with_both_name_and_parent_name_once() {
        let child_def = DefEntry {
            parent_name: Some("Base".to_string()),
            ..def("ThingDef", "ReinforcedWall")
        };
        let child_template = TemplateEntry {
            graphic_class: None,
            may_require: Vec::new(),
            parent_name: Some("Base".to_string()),
            ..template("ThingDef", "ReinforcedWallBase")
        };
        let a = scanned("a", vec![child_def], vec![child_template], vec![]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        let children = index
            .children_by_template
            .get(&("ThingDef".to_string(), "Base".to_string()))
            .expect("Base must have one child indexed");
        assert_eq!(
            children,
            &vec![(
                ModId::new("a"),
                ("ThingDef".to_string(), "ReinforcedWall".to_string())
            )]
        );
    }

    /// A patch touching two targets (a disjunctive xpath head) counts one
    /// top-level op against each target it names, not two against either.
    #[test]
    fn ops_by_mod_counts_one_top_level_op_per_target_it_touches() {
        let mut op = mutating_op("BiomeDef", "Forest", "PatchOperationReplace");
        op.xpath =
            Some(r#"Defs/BiomeDef[defName="Forest" or defName="Other"]/plantDensity"#.to_string());
        let a = scanned("a", vec![], vec![], vec![op]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        let by_mod = &index.ops_by_mod[&ModId::new("a")];
        assert_eq!(
            by_mod.get(&(
                "BiomeDef".to_string(),
                "Forest".to_string(),
                Selector::DefName
            )),
            Some(&1)
        );
        assert_eq!(
            by_mod.get(&(
                "BiomeDef".to_string(),
                "Other".to_string(),
                Selector::DefName
            )),
            Some(&1)
        );
    }

    /// A nested descendant op (under a `PatchOperationSequence`/`FindMod`) is
    /// still indexed by `patch_ops_by_def` (needed for replay), and counted
    /// in `ops_by_mod` too — by its own top-level ancestor, a *different* one
    /// from a plain top-level leaf targeting the same key, so both are
    /// counted: dropping nested descendants entirely undercounts every patch
    /// that uses a `Sequence`/`FindMod`/`Conditional` wrapper, which is most
    /// of them.
    #[test]
    fn ops_by_mod_counts_a_nested_descendant_by_its_own_top_level_ancestor() {
        let mut top = mutating_op("ThingDef", "Wall", "PatchOperationAdd");
        top.locator = XmlLocator::new(Arc::from(Path::new("test.xml")), vec![1]);
        let nested = nested_mutating_op("ThingDef", "Wall", "PatchOperationAdd"); // ancestor ordinal 0
        let a = scanned("a", vec![], vec![], vec![top, nested]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        let key = (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        );
        // Both ops are indexed for replay...
        assert_eq!(index.patch_ops_by_def[&key].len(), 2);
        // ...and both are counted: a top-level leaf at ordinal 1 and a
        // nested descendant under ordinal 0 are two distinct top-level
        // operations touching the same target.
        assert_eq!(index.ops_by_mod[&ModId::new("a")].get(&key), Some(&2));
    }

    /// Two nested descendants sharing the same top-level ancestor (e.g.
    /// the `match` and `nomatch` branches of one `PatchOperationFindMod`)
    /// count as one top-level operation, not two — the same dedup
    /// `ops_by_mod_counts_one_top_level_op_per_target_it_touches` exercises
    /// for a disjunctive top-level xpath, but across two flattened entries
    /// instead of one.
    #[test]
    fn ops_by_mod_dedups_two_nested_descendants_under_the_same_top_level_ancestor() {
        let match_branch = nested_mutating_op("ThingDef", "Wall", "PatchOperationAdd"); // [0, 0, 0]
        let mut nomatch_branch = nested_mutating_op("ThingDef", "Wall", "PatchOperationAdd");
        nomatch_branch.locator = XmlLocator::new(Arc::from(Path::new("test.xml")), vec![0, 1, 0]);
        let a = scanned("a", vec![], vec![], vec![match_branch, nomatch_branch]);
        let output = scan(vec![a], &["a"]);

        let index = build(&output);

        let key = (
            "ThingDef".to_string(),
            "Wall".to_string(),
            Selector::DefName,
        );
        assert_eq!(index.patch_ops_by_def[&key].len(), 2);
        assert_eq!(index.ops_by_mod[&ModId::new("a")].get(&key), Some(&1));
    }

    #[test]
    fn is_top_level_true_for_a_single_ordinal_path() {
        let op = mutating_op("ThingDef", "Wall", "PatchOperationAdd");
        let indexed = IndexedPatchOp {
            mod_id: ModId::new("a"),
            op,
        };
        assert!(indexed.is_top_level());
    }

    #[test]
    fn is_top_level_false_for_a_nested_sequence_or_find_mod_descendant() {
        let op = nested_mutating_op("ThingDef", "Wall", "PatchOperationAdd");
        let indexed = IndexedPatchOp {
            mod_id: ModId::new("a"),
            op,
        };
        assert!(!indexed.is_top_level());
    }

    fn race_site(owner: RefSiteOwner, value: &str) -> crate::domain::RefSite {
        crate::domain::RefSite {
            def_type: Arc::from("PawnKindDef"),
            field_path: Arc::from("race"),
            shape: RefSiteShape::Scalar,
            value: value.to_string(),
            owner: Arc::new(owner),
            may_require: Box::default(),
            may_require_any_of: Box::default(),
        }
    }

    fn def_owner(def_name: &str) -> RefSiteOwner {
        RefSiteOwner::Def {
            def_name: def_name.to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
        }
    }

    fn scan_with_sites(
        mut mods: Vec<ScannedMod>,
        sites: Vec<crate::domain::RefSite>,
    ) -> ScanOutput {
        let order: Vec<String> = mods
            .iter()
            .map(|m| m.info.id.as_str().to_string())
            .collect();
        let refs: Vec<&str> = order.iter().map(String::as_str).collect();
        let first = ModId::new(refs[0]);
        let mut output = scan(std::mem::take(&mut mods), &refs);
        output.ref_sites_by_mod.insert(first, sites);
        output
    }

    #[test]
    fn texture_index_keeps_loose_owners_in_load_order_and_unions_non_loose_keys() {
        let mut a = scanned("a", vec![], vec![], vec![]);
        a.textures.insert("things/x".to_string(), 1);
        let mut b = scanned("b", vec![], vec![], vec![]);
        b.textures.insert("things/x".to_string(), 2);
        b.bundle_textures.insert("things/bundled".to_string());
        let mut output = scan(vec![b, a], &["a", "b"]);
        output
            .core_resource_textures
            .insert("things/core".to_string());

        let textures = build(&output).textures;

        assert_eq!(
            textures.loose_owners("things/x"),
            [ModId::new("a"), ModId::new("b")]
        );
        assert!(textures.is_non_loose("things/bundled"));
        assert!(textures.is_non_loose("things/core"));
        assert!(crate::extract::graphics::TextureCatalog::contains(
            &textures,
            "things/core"
        ));
    }

    #[test]
    fn texture_index_marks_a_small_core_resource_index_untrusted() {
        let output = scan(vec![scanned("a", vec![], vec![], vec![])], &["a"]);

        assert!(!build(&output).textures.is_core_index_trusted());
    }

    #[test]
    fn texture_index_trusts_a_core_resource_index_at_the_floor() {
        let mut output = scan(vec![scanned("a", vec![], vec![], vec![])], &["a"]);
        output.core_resource_textures = (0..MIN_CORE_RESOURCE_TEXTURES)
            .map(|index| format!("things/core{index}"))
            .collect();

        assert!(build(&output).textures.is_core_index_trusted());
    }

    #[test]
    fn kinds_by_race_lists_a_kind_that_declares_the_race_directly() {
        let a = scanned("a", vec![def("PawnKindDef", "Cow")], vec![], vec![]);
        let output = scan_with_sites(vec![a], vec![race_site(def_owner("Cow"), "Cow")]);

        let index = build(&output);

        assert_eq!(
            index.kinds_by_race.get("Cow"),
            Some(&vec!["Cow".to_string()])
        );
    }

    #[test]
    fn kinds_by_race_expands_a_template_to_its_concrete_descendants() {
        let with_ordinal = |mut entry: DefEntry, ordinal: u32| {
            entry.locator.element_path = vec![ordinal];
            entry
        };
        let a = scanned(
            "a",
            vec![
                with_ordinal(def_with_parent("PawnKindDef", "Cow", "AnimalKind"), 1),
                with_ordinal(def_with_parent("PawnKindDef", "Bull", "AnimalKind"), 2),
                with_ordinal(def_with_parent("PawnKindDef", "Calf", "Bull"), 3),
            ],
            vec![template("PawnKindDef", "AnimalKind")],
            vec![],
        );
        let owner = RefSiteOwner::Template {
            name: "AnimalKind".to_string(),
            may_require: Vec::new(),
        };
        let output = scan_with_sites(vec![a], vec![race_site(owner, "Herd")]);

        let index = build(&output);

        assert_eq!(
            index.kinds_by_race.get("Herd"),
            Some(&vec![
                "Bull".to_string(),
                "Calf".to_string(),
                "Cow".to_string()
            ])
        );
    }

    #[test]
    fn kinds_by_race_ignores_other_fields_other_types_and_patch_owners() {
        let a = scanned("a", vec![def("PawnKindDef", "Cow")], vec![], vec![]);
        let other_field = crate::domain::RefSite {
            field_path: Arc::from("backstory"),
            ..race_site(def_owner("Cow"), "Cow")
        };
        let other_type = crate::domain::RefSite {
            def_type: Arc::from("ThingDef"),
            ..race_site(def_owner("Cow"), "Cow")
        };
        let patch_owner = race_site(
            RefSiteOwner::Patch {
                mod_id: ModId::new("a"),
                def_name: "Cow".to_string(),
                locator: XmlLocator::for_test(),
            },
            "Cow",
        );
        let output = scan_with_sites(vec![a], vec![other_field, other_type, patch_owner]);

        assert!(build(&output).kinds_by_race.is_empty());
    }
}
