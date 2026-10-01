//! Who registers which `ParentName` target, by RimWorld's own rules.
//!
//! Three consumers need the same question answered: for a given `Name`,
//! which active mods actually register it with `Verse.XmlInheritance`,
//! and what does a given child's own `ParentName` reference actually
//! resolve to? `edges::parent_template_edges` (an ordering edge, "is
//! there exactly one foreign owner to order after") and
//! `broken_inheritance` (a ledger finding, "what does *this* child
//! actually get, and is it wrong") are both built from
//! `TemplateRegistrations`, so the two can never drift apart on what
//! "registered" means.
//!
//! The rules come from the decompiled engine; in summary:
//!
//! - `LoadedModManager` applies every patch *before* `ParseAndProcessXML`
//!   registers anything, then calls
//!   `XmlInheritance.TryRegister(node, assetlookup[node]?.mod)` for every
//!   top-level node of the unified document.
//! - A node a patch added has no `assetlookup` entry, so it registers
//!   with `mod == null` — and `GetBestParentFor` falls back to a
//!   `mod == null` registration for any child that finds nothing at or
//!   before itself. A patch-registered name therefore never constrains
//!   order, and its own element type is unknowable from an over-collected
//!   `injected_template_names` set, so `broken_inheritance` never
//!   reports a type mismatch against one.
//! - `TryRegister` returns early, registering nothing, when the node's
//!   own `MayRequire` is not satisfied. It reads `MayRequire` only —
//!   `MayRequireAnyOf` gates whether the *def* loads, never whether the
//!   name registers.
//! - **`GetBestParentFor`'s real resolution** (mirrored by
//!   `resolve_parent`): among every registrant of a name, a non-vanilla
//!   asking mod gets the registrant with the greatest `mod.loadOrder`
//!   among those `<=` its own, falling back to a vanilla registrant when
//!   none qualifies; a vanilla asking mod gets a vanilla registrant first
//!   (any one, since every vanilla mod's own `loadOrder` precedes every
//!   real mod's), else the lowest-positioned registrant of any kind
//!   (`GetBestParentFor`'s own `node.mod == null` inversion); either way,
//!   a `mod == null` (patch-injected) registration is the final fallback
//!   once no inline registrant qualifies at all.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::domain::{
    BrokenInheritance, Conflict, DefRefParts, InheritanceProblem, ModId, ScannedMod, Warning,
};

use super::indices::{
    ActiveMods, DefKey, DisplayNameIndex, may_require_satisfied, patch_op_active,
};
use super::source_index::ChildrenIndex;

/// Every `Name` an active mod registers with `Verse.XmlInheritance`, and
/// how — inline in a mod's own `Defs/` XML (attributed to that mod, whose
/// `loadOrder` is what the at-or-before test compares) or through a
/// patch's `<value>` (attributed to nothing, `mod == null`).
///
/// Built once per run in [`super::report_builder`] and shared; nothing
/// here is derived lazily, so two consumers asking the same question
/// always get the same answer.
#[derive(Debug, Default)]
pub struct TemplateRegistrations {
    /// `Name` -> the active mods registering it from their own `Defs/`
    /// XML with a satisfied `MayRequire` gate, in scan order, each mod
    /// listed once. `BTreeMap` for the same determinism reason
    /// [`super::indices::Indices::template_owners`] has one.
    inline_owners: BTreeMap<String, Vec<ModId>>,
    /// [`Self::inline_owners`], widened with each registrant's own
    /// element (def) type — [`resolve_parent`]'s own source of truth for
    /// [`crate::domain::InheritanceProblem::ParentTypeMismatch`]. A mod
    /// registering the same `Name` twice keeps only the **first**
    /// element's own type (`XmlInheritance.TryRegister`'s "already used
    /// in this mod" refusal: the second registration is refused
    /// outright, never overwriting the first).
    inline_registrations: BTreeMap<String, Vec<(ModId, String)>>,
    /// The subset of [`Self::inline_owners`]' keys with at least one
    /// vanilla (Core/DLC) registration. Vanilla is first by tier, so its
    /// `loadOrder` is `<=` every mod's and its registration always wins
    /// the at-or-before test — a different parent may be chosen when a
    /// later mod registers the same name, but the lookup never fails.
    vanilla_registered: BTreeSet<String>,
    /// Every active vanilla (Core/DLC) mod's own id — [`resolve_parent`]'s
    /// own way to tell, for one specific registrant in
    /// [`Self::inline_registrations`], whether *it* is the vanilla one,
    /// as opposed to [`Self::vanilla_registered`], which only says
    /// whether *some* registrant of a name is vanilla.
    vanilla_owners: BTreeSet<ModId>,
    /// Names some active, actually-running mutating patch operation
    /// registers (`mod == null`). See
    /// [`crate::domain::PatchOp::injected_template_names`] for why this
    /// deliberately over-collects.
    patch_registered: BTreeSet<String>,
}

impl TemplateRegistrations {
    /// Indexes every active mod's inline templates and patch `<value>`
    /// names. `active`/`name_map` gate the patch side through the same
    /// [`patch_op_active`] every other patch-derived fact uses — an
    /// operation behind a closed `PatchOperationFindMod` branch or an
    /// unsatisfied `MayRequire` never runs, so it registers nothing.
    #[must_use]
    pub fn build(scanned: &[ScannedMod], active: &ActiveMods, name_map: &DisplayNameIndex) -> Self {
        let mut registrations = Self::default();
        for scanned_mod in scanned {
            let id = &scanned_mod.info.id;
            let is_vanilla = scanned_mod.info.source.is_vanilla();
            if is_vanilla {
                registrations.vanilla_owners.insert(id.clone());
            }
            // Name -> this mod's own first-registered element type: a
            // second same-`Name` registration in one mod is refused, so
            // only the first is ever real.
            let mut own_names: BTreeMap<String, String> = BTreeMap::new();
            for template in &scanned_mod.templates {
                // `MayRequireAnyOf` is deliberately not consulted: see
                // this module's own doc comment and
                // `TemplateEntry::may_require`.
                if !may_require_satisfied(&template.may_require, &[], active) {
                    continue;
                }
                own_names
                    .entry(template.name.clone())
                    .or_insert_with(|| template.def_type.clone());
            }
            for (name, def_type) in own_names {
                if is_vanilla {
                    registrations.vanilla_registered.insert(name.clone());
                }
                registrations
                    .inline_owners
                    .entry(name.clone())
                    .or_default()
                    .push(id.clone());
                registrations
                    .inline_registrations
                    .entry(name)
                    .or_default()
                    .push((id.clone(), def_type));
            }
            for op in &scanned_mod.patch_ops {
                if !op.is_mutating || !patch_op_active(op, active, name_map) {
                    continue;
                }
                registrations
                    .patch_registered
                    .extend(op.injected_template_names.iter().cloned());
            }
        }
        registrations
    }

    /// Whether any active mod registers `name` at all. `false` is the
    /// unresolved-parent case: `GetBestParentFor` logs `XML error: Could
    /// not find parent node named "<name>"` and the child def, plus
    /// everything inheriting from it, loads without any of the parent's
    /// inherited fields.
    #[must_use]
    pub fn is_registered(&self, name: &str) -> bool {
        self.inline_owners.contains_key(name) || self.patch_registered.contains(name)
    }

    /// Whether `name` has a registration that satisfies `child`
    /// *regardless of load order* — the child's own copy (`loadOrder <=`
    /// itself, trivially), a vanilla copy (first by tier), or a
    /// patch-added one (`mod == null`, `GetBestParentFor`'s own
    /// fallback). When this is true no ordering constraint exists, even
    /// if some other mod also registers the name.
    #[must_use]
    pub fn resolves_in_any_order(&self, name: &str, child: &ModId) -> bool {
        self.vanilla_registered.contains(name)
            || self.patch_registered.contains(name)
            || self
                .inline_owners
                .get(name)
                .is_some_and(|owners| owners.iter().any(|owner| owner == child))
    }

    /// The active mods other than `child` registering `name` inline.
    /// Only meaningful once [`Self::resolves_in_any_order`] has ruled the
    /// name out: with exactly one such owner it must load at or before
    /// `child`, and with two or more the requirement is an any-of rather
    /// than a single edge.
    #[must_use]
    pub fn foreign_owners(&self, name: &str, child: &ModId) -> Vec<&ModId> {
        self.inline_owners
            .get(name)
            .map(|owners| owners.iter().filter(|owner| *owner != child).collect())
            .unwrap_or_default()
    }
}

/// One `ParentName` reference: a def or template in some mod, and the
/// template name it inherits from.
///
/// Shared by [`super::edges::parent_template_edges`] and
/// [`broken_inheritance`] so the edge producer and the ledger finding
/// can never disagree about which references count. Borrows throughout
/// — [`Self::label`] is the only allocation, and only the finding path
/// needs it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParentReference<'a> {
    /// The referencing node's own tag (`ThingDef`, `HediffDef`, ...).
    pub def_type: &'a str,
    /// The referencing node's `defName`, or its `Name` attribute when it
    /// is a template.
    pub child_name: &'a str,
    /// Whether `child_name` is a `Name` attribute rather than a `defName`.
    pub child_is_template: bool,
    /// The `ParentName` attribute's value.
    pub parent_name: &'a str,
}

impl ParentReference<'_> {
    /// Human-facing address of the referencing node, in
    /// [`crate::domain::DefTarget::match_key`]'s convention:
    /// `ThingDef/XBM_Fernwood` for a def, `ThingDef/@GeneTailBase` for
    /// a `Name`-attributed template, so the two never read alike.
    #[must_use]
    pub fn label(&self) -> String {
        if self.child_is_template {
            format!("{}/@{}", self.def_type, self.child_name)
        } else {
            format!("{}/{}", self.def_type, self.child_name)
        }
    }
}

/// Every `ParentName` reference in `scanned_mod`, with the `MayRequire`
/// gate `XmlInheritance.TryRegister` applies to the *child* node: an
/// unsatisfied gate means the child never registers, so it never resolves
/// a parent and never errors. `MayRequireAnyOf` is deliberately not
/// consulted — `TryRegister` reads `MayRequire` alone (see this module's
/// own doc comment).
pub fn parent_references<'a>(
    scanned_mod: &'a ScannedMod,
    active: &'a ActiveMods,
) -> impl Iterator<Item = ParentReference<'a>> {
    let defs = scanned_mod
        .defs
        .iter()
        .filter(|def| may_require_satisfied(&def.may_require, &[], active))
        .filter_map(|def| {
            Some(ParentReference {
                def_type: &def.def_type,
                child_name: &def.def_name,
                child_is_template: false,
                parent_name: def.parent_name.as_deref()?,
            })
        });
    let templates = scanned_mod
        .templates
        .iter()
        .filter(|template| may_require_satisfied(&template.may_require, &[], active))
        .filter_map(|template| {
            Some(ParentReference {
                def_type: &template.def_type,
                child_name: &template.name,
                child_is_template: true,
                parent_name: template.parent_name.as_deref()?,
            })
        });
    defs.chain(templates)
}

/// One registered `Name`'s resolution against a specific asking mod's own
/// load position — [`resolve_parent`]'s own return shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParentResolution<'a> {
    /// No active mod registers this name at a position the asking mod
    /// could actually resolve against, and no active patch registers it
    /// either.
    Missing,
    /// Resolves to a specific mod's own inline registration, with that
    /// registration's own element (def) type known.
    Inline { owner: &'a ModId, def_type: &'a str },
    /// Resolves only to the `mod == null` patch-injected fallback — its
    /// own element type is unknowable (`PatchOp::injected_template_names`
    /// deliberately over-collects), so this is never reported as a type
    /// mismatch.
    PatchInjected,
}

/// `Verse.XmlInheritance.GetBestParentFor`, mirrored exactly — see this
/// module's own doc comment for the rule in words. `positions` is every
/// active mod's own index in `scanned`'s order (which is load order —
/// see `infra::mod.rs::resolve_active_mods`), built once by
/// [`broken_inheritance`] and passed in rather than rebuilt per call.
fn resolve_parent<'a>(
    registrations: &'a TemplateRegistrations,
    positions: &BTreeMap<ModId, usize>,
    parent_name: &str,
    asking_id: &ModId,
    asking_is_vanilla: bool,
) -> ParentResolution<'a> {
    let inline: &[(ModId, String)] = registrations
        .inline_registrations
        .get(parent_name)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let vanilla_registrant = || {
        inline
            .iter()
            .find(|(owner, _)| registrations.vanilla_owners.contains(owner))
    };
    let lowest_positioned = || {
        inline
            .iter()
            .filter(|(owner, _)| positions.contains_key(owner))
            .min_by_key(|(owner, _)| positions[owner])
    };

    let inline_pick = if asking_is_vanilla {
        vanilla_registrant().or_else(lowest_positioned)
    } else {
        match positions.get(asking_id) {
            Some(&asking_position) => inline
                .iter()
                .filter(|(owner, _)| positions.get(owner).is_some_and(|&p| p <= asking_position))
                .max_by_key(|(owner, _)| positions[owner])
                .or_else(vanilla_registrant),
            None => vanilla_registrant().or_else(lowest_positioned),
        }
    };

    match inline_pick {
        Some((owner, def_type)) => ParentResolution::Inline { owner, def_type },
        None if registrations.patch_registered.contains(parent_name) => {
            ParentResolution::PatchInjected
        }
        None => ParentResolution::Missing,
    }
}

/// How many `.Extends` hops [`TypeHierarchyIndex::is_subclass`] walks
/// before giving up and reporting "can't resolve" — generous headroom
/// over any real RimWorld def-type hierarchy (Verse's own deepest chains
/// run under 10 levels), just a defensive bound against a cyclic or
/// pathological `Extends` chain in hostile input.
const MAX_HIERARCHY_DEPTH: usize = 32;

/// Base type full names [`TypeHierarchyIndex::is_subclass`] treats as a
/// confirmed, fully-resolved terminus — every one lives in the BCL
/// (`mscorlib`/`System.Private.CoreLib`), never in `Assembly-CSharp.dll`
/// or a mod's own assembly, so reaching one by name (a real `TypeRef`,
/// ground-truthed against `Exports.dll`'s own `class BaseWidget` — see
/// `synthetic_assemblies::exports_own_base_widget_extends_system_object`)
/// means the chain has been walked in full: nothing further to resolve,
/// and no match was found along the way.
const KNOWN_HIERARCHY_ROOTS: [&str; 5] = [
    "System.Object",
    "System.ValueType",
    "System.Enum",
    "System.MulticastDelegate",
    "System.Attribute",
];

/// Whether `resolved_full` (a fully-qualified `Namespace.Name`, as every
/// [`TypeHierarchyIndex`] value is stored) names the same type as
/// `xml_type` (an XML def-type tag, almost always bare — `"ThingDef"`,
/// never `"Verse.ThingDef"` — but occasionally fully qualified when a
/// mod's own class name is otherwise ambiguous, e.g.
/// `"Example.Weapons.ExpandableProjectileDef"`). A bare `xml_type` compares
/// against `resolved_full`'s own simple name only, matching how
/// RimWorld's own `GenTypes.GetTypeInAnyAssembly` resolves an
/// unqualified tag; a dotted one compares exactly.
fn xml_type_matches(resolved_full: &str, xml_type: &str) -> bool {
    if xml_type.contains('.') {
        resolved_full == xml_type
    } else {
        resolved_full.rsplit('.').next() == Some(xml_type)
    }
}

/// Every scanned type's own base type, merged from every active mod's
/// own assemblies plus vanilla's (`Assembly-CSharp.dll`) — the source
/// [`Self::is_subclass`] answers "is `child` a subclass of `parent`"
/// from. Built once per run and shared, the same convention
/// [`TemplateRegistrations`] follows.
#[derive(Debug, Default)]
pub struct TypeHierarchyIndex {
    /// Every type's own full name -> its base type's own full name,
    /// where resolvable (`None` for no base at all, a generic `TypeSpec`
    /// base, or an unresolvable target — see `extract::pe_metadata
    /// ::tables::resolve_extends_target`'s own doc comment).
    full: BTreeMap<String, Option<String>>,
    /// Every type's own *simple* name (the text after its last `.`, or
    /// the whole name when there is none) -> every full name sharing it
    /// — [`Self::resolve_xml_type`]'s own index for a bare XML tag. More
    /// than one candidate for a simple name means an ambiguous lookup,
    /// treated as unresolvable rather than guessing.
    by_simple_name: BTreeMap<String, Vec<String>>,
}

impl TypeHierarchyIndex {
    /// `scanned`'s own assemblies (every active mod, first occurrence of
    /// a given full name wins — scan order is already deterministic)
    /// plus `vanilla`'s (`ScanOutput::vanilla_type_hierarchy`).
    #[must_use]
    pub fn build(scanned: &[ScannedMod], vanilla: &[(String, Option<String>)]) -> Self {
        let mut index = Self::default();
        for scanned_mod in scanned {
            for assembly in &scanned_mod.assemblies {
                for (full_name, base) in &assembly.type_hierarchy {
                    index.insert(full_name.clone(), base.clone());
                }
            }
        }
        for (full_name, base) in vanilla {
            index.insert(full_name.clone(), base.clone());
        }
        index
    }

    fn insert(&mut self, full_name: String, base: Option<String>) {
        if self.full.contains_key(&full_name) {
            return;
        }
        let simple_name = full_name
            .rsplit('.')
            .next()
            .unwrap_or(&full_name)
            .to_string();
        self.by_simple_name
            .entry(simple_name)
            .or_default()
            .push(full_name.clone());
        self.full.insert(full_name, base);
    }

    /// `xml_type` resolved to the exact index key it names — itself, if
    /// `xml_type` is already a full name this index carries, else the
    /// single unambiguous full name sharing its simple name. `None` for
    /// no match at all, or more than one candidate (an ambiguous bare
    /// tag this index can't safely pick between).
    fn resolve_xml_type(&self, xml_type: &str) -> Option<String> {
        if self.full.contains_key(xml_type) {
            return Some(xml_type.to_string());
        }
        match self.by_simple_name.get(xml_type) {
            Some(candidates) if candidates.len() == 1 => Some(candidates[0].clone()),
            _ => None,
        }
    }

    /// Whether `child`'s own type extends `parent`'s, walking `child`'s
    /// own `.Extends` chain up to `MAX_HIERARCHY_DEPTH` hops. `Some(true)`
    /// once `parent` is found anywhere in the chain; `Some(false)` once the
    /// chain is confirmed to reach a `KNOWN_HIERARCHY_ROOTS` terminus
    /// without ever matching (a real answer, not a guess: every hop up to
    /// that point resolved to a real name); `None` — "can't resolve, don't
    /// flag" — the moment any hop's own base can't be named at all (an
    /// inactive or unparsed mod's own type, a generic `TypeSpec` base, or
    /// `child` itself not found in this index).
    #[must_use]
    pub fn is_subclass(&self, child: &str, parent: &str) -> Option<bool> {
        let mut current = self.resolve_xml_type(child)?;
        for _ in 0..MAX_HIERARCHY_DEPTH {
            let base = self.full.get(&current)?.as_ref()?;
            if xml_type_matches(base, parent) {
                return Some(true);
            }
            if KNOWN_HIERARCHY_ROOTS.contains(&base.as_str()) {
                return Some(false);
            }
            current = base.clone();
        }
        None
    }
}

/// A bound on [`BrokenInheritance::affected`], applied after a full
/// (unbounded work, bounded output) walk of `children_by_template` — see
/// [`affected_concrete_defs`]. Generous: real-install measurements never
/// approached it (a handful of affected defs for the one missing-parent
/// case found), so this exists only to cap what a hostile or
/// pathological install could report, never to change an ordinary
/// finding's own content.
const MAX_AFFECTED_DEFS: usize = 500;

/// Whether `(owner, key)`'s own def or template entry has a satisfied
/// `MayRequire` gate — the same gate [`parent_references`] applies to a
/// reference's own child, reapplied here per descendant during the
/// `children_by_template` walk, since that index carries no gate
/// information of its own (see its own doc comment: it's built once,
/// gate-agnostic, and shared with [`super::source_index::SourceIndex`]).
fn gate_open(
    scanned_by_id: &BTreeMap<&ModId, &ScannedMod>,
    owner: &ModId,
    key: &DefKey,
    active: &ActiveMods,
) -> bool {
    let Some(scanned_mod) = scanned_by_id.get(owner) else {
        return false;
    };
    let (def_type, name) = key;
    if let Some(def) = scanned_mod
        .defs
        .iter()
        .find(|def| &def.def_type == def_type && &def.def_name == name)
    {
        return may_require_satisfied(&def.may_require, &def.may_require_any_of, active);
    }
    if let Some(template) = scanned_mod
        .templates
        .iter()
        .find(|template| &template.def_type == def_type && &template.name == name)
    {
        return may_require_satisfied(&template.may_require, &[], active);
    }
    false
}

/// Every active, gate-open concrete def reachable from `(start_owner,
/// start_key)` — itself, if [`ChildrenIndex::owners_by_def`] says it's
/// concrete, plus every concrete descendant reached transitively through
/// [`ChildrenIndex::children_by_template`]. Bounded output
/// ([`MAX_AFFECTED_DEFS`]); the walk itself completes in full so
/// `truncated` is an exact count, not a guess — real installs are small
/// enough (thousands of defs, not millions) for this to cost nothing
/// worth measuring.
///
/// A concrete def that is *also* registered as a further parent template
/// under a **different** `Name` than its own `defName` (a rare "hybrid"
/// shape) has its own further descendants missed: `children_by_template`
/// is keyed by `Name`, and a dual-attributed node's own contributed
/// identity in that index is always its `defName`-based one (the defs
/// loop runs first — see `source_index::build_children_index`'s own doc
/// comment), so a lookup keyed on that `defName` finds nothing unless it
/// happens to also be some other node's own registered `Name`. Disclosed,
/// not fixed: the common shape (an abstract template chain ending in
/// concrete leaves) is unaffected, and this can only under-count, never
/// fabricate an affected def that isn't real.
pub(crate) fn affected_concrete_defs(
    children_index: &ChildrenIndex,
    scanned_by_id: &BTreeMap<&ModId, &ScannedMod>,
    active: &ActiveMods,
    start_owner: &ModId,
    start_key: DefKey,
) -> (Vec<(String, String)>, usize) {
    let mut affected: BTreeSet<DefKey> = BTreeSet::new();
    let mut visited: BTreeSet<(ModId, DefKey)> = BTreeSet::new();
    let mut queue: VecDeque<(ModId, DefKey)> = VecDeque::new();
    queue.push_back((start_owner.clone(), start_key));

    while let Some((owner, key)) = queue.pop_front() {
        if !visited.insert((owner.clone(), key.clone())) {
            continue;
        }
        if children_index.owners_by_def.contains_key(&key) {
            affected.insert(key.clone());
        }
        if let Some(children) = children_index.children_by_template.get(&key) {
            for (child_owner, child_key) in children {
                if gate_open(scanned_by_id, child_owner, child_key, active) {
                    queue.push_back((child_owner.clone(), child_key.clone()));
                }
            }
        }
    }

    let truncated = affected.len().saturating_sub(MAX_AFFECTED_DEFS);
    let affected = affected.into_iter().take(MAX_AFFECTED_DEFS).collect();
    (affected, truncated)
}

/// Every def or template whose `ParentName` resolves incorrectly — either
/// to nothing at all, or to a registration of the wrong element type —
/// under `Verse.XmlInheritance`'s own resolution rule
/// ([`resolve_parent`]), replacing the old `unresolved_parent_templates`
/// scan warning with a real ledger finding that names every concrete def
/// actually affected.
///
/// One [`Conflict::BrokenInheritance`] per `(mod, parent_name)`: every
/// active def/template in that mod referencing the same name resolves
/// identically (resolution only depends on the *asking mod's* own load
/// position, never on which specific child within it is asking), so a
/// missing-parent problem is necessarily shared by the whole group, and
/// among several children a type-mismatch problem can affect only some
/// of them — either way the lexicographically first (by
/// [`ParentReference::label`]) problem-having child is kept as the
/// representative `child`, the same "smallest label wins, byte-identical
/// across runs" rule the old warning used.
///
/// Also returns every `ParentTypeMismatch` this crate found real evidence
/// for but couldn't confirm — [`TypeHierarchyIndex::is_subclass`]
/// returning `None` for at least one affected child — as informational
/// [`Warning`]s rather than [`Conflict::BrokenInheritance`]s. A type
/// mismatch is suppressed outright (neither a conflict nor a warning)
/// when: the two type names differ only by case (`BackstoryDef`/
/// `BackStoryDef`, a documented real-install false positive, not a
/// load-time failure); the asking child's own mod is vanilla (Core/DLC
/// templates and children are ground truth — see this function's own
/// doc comment); or [`TypeHierarchyIndex::is_subclass`] confirms the
/// child's own type genuinely extends the resolved parent's (a mod
/// framework's own `ExpandableProjectileDef : ThingDef`-style
/// specialization, safe by construction). It survives as a full
/// [`Conflict`] only when the subclass check *confirms* — by walking the
/// child's own chain to a known terminus without ever matching — that
/// the two types are genuinely unrelated.
#[must_use]
pub fn broken_inheritance(
    scanned: &[ScannedMod],
    active: &ActiveMods,
    registrations: &TemplateRegistrations,
    children_index: &ChildrenIndex,
    vanilla_type_hierarchy: &[(String, Option<String>)],
) -> (Vec<Conflict>, Vec<Warning>) {
    let positions: BTreeMap<ModId, usize> = scanned
        .iter()
        .enumerate()
        .map(|(position, scanned_mod)| (scanned_mod.info.id.clone(), position))
        .collect();
    let scanned_by_id: BTreeMap<&ModId, &ScannedMod> = scanned
        .iter()
        .map(|scanned_mod| (&scanned_mod.info.id, scanned_mod))
        .collect();
    let hierarchy = TypeHierarchyIndex::build(scanned, vanilla_type_hierarchy);

    // Keyed on (mod, parent_name) only — see this function's own doc
    // comment for why that's always enough, never `(mod, parent_name,
    // problem_kind)`: the two problem kinds can never both occur for the
    // same pair.
    let mut representatives: BTreeMap<(ModId, String), (ParentReference<'_>, InheritanceProblem)> =
        BTreeMap::new();
    // The informational-only counterpart, keyed the same way — see this
    // function's own doc comment.
    let mut low_confidence: BTreeMap<(ModId, String), ParentReference<'_>> = BTreeMap::new();

    for scanned_mod in scanned {
        let asking_id = &scanned_mod.info.id;
        let asking_is_vanilla = scanned_mod.info.source.is_vanilla();
        for reference in parent_references(scanned_mod, active) {
            let resolution = resolve_parent(
                registrations,
                &positions,
                reference.parent_name,
                asking_id,
                asking_is_vanilla,
            );
            let problem = match resolution {
                ParentResolution::Missing => InheritanceProblem::MissingParent,
                ParentResolution::PatchInjected => continue,
                ParentResolution::Inline { def_type, .. } if def_type == reference.def_type => {
                    continue;
                }
                ParentResolution::Inline { def_type, .. }
                    if def_type.eq_ignore_ascii_case(reference.def_type) =>
                {
                    continue;
                }
                ParentResolution::Inline { .. } if asking_is_vanilla => continue,
                ParentResolution::Inline { owner, def_type } => {
                    match hierarchy.is_subclass(reference.def_type, def_type) {
                        Some(true) => continue,
                        Some(false) => InheritanceProblem::ParentTypeMismatch {
                            parent_type: def_type.to_string(),
                            parent_owner: Some(owner.clone()),
                        },
                        None => {
                            let key = (asking_id.clone(), reference.parent_name.to_string());
                            low_confidence.entry(key).or_insert(reference);
                            continue;
                        }
                    }
                }
            };
            let key = (asking_id.clone(), reference.parent_name.to_string());
            representatives
                .entry(key)
                .and_modify(|(current, _)| {
                    if reference.label() < current.label() {
                        *current = reference;
                    }
                })
                .or_insert((reference, problem));
        }
    }
    // A key with a confirmed `Conflict` never also carries a low-
    // confidence warning — the rare case where the same (mod,
    // parent_name) produced both outcomes for different children keeps
    // only the stronger, confirmed signal.
    low_confidence.retain(|key, _| !representatives.contains_key(key));

    let conflicts = representatives
        .into_iter()
        .map(|((mod_id, parent_name), (child, problem))| {
            let start_key: DefKey = (child.def_type.to_string(), child.child_name.to_string());
            let (affected, truncated) =
                affected_concrete_defs(children_index, &scanned_by_id, active, &mod_id, start_key);
            Conflict::BrokenInheritance(BrokenInheritance {
                mod_id,
                parent_name,
                child: DefRefParts {
                    def_type: child.def_type.to_string(),
                    name: child.child_name.to_string(),
                    is_template: child.child_is_template,
                },
                problem,
                affected,
                truncated,
            })
        })
        .collect();

    let warnings = low_confidence
        .into_iter()
        .map(|((mod_id, parent_name), child)| {
            Warning::new(
                Some(mod_id),
                format!(
                    "{} inherits '{parent_name}', which resolves to a registration of a \
                     different element type — whether that type is a compatible subclass \
                     could not be confirmed, so this is informational only",
                    child.label()
                ),
            )
        })
        .collect();

    (conflicts, warnings)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashSet};
    use std::path::PathBuf;

    use super::*;
    use crate::domain::{
        AssemblyInfo, DeclaredOrder, DefEntry, LoadOrder, Mod, PatchOp, ScanCost, ScanOutput,
        Source, TemplateEntry, XmlLocator,
    };

    fn scanned_mod(id: &str, name: &str) -> ScannedMod {
        ScannedMod {
            info: Mod {
                id: ModId::new(id),
                name: name.to_string(),
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
            },
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            assemblies: Vec::new(),
            sounds: BTreeSet::new(),
            translation_keys: BTreeSet::new(),
            inline_types: BTreeSet::new(),
            manifest_order: Default::default(),
            texture_path_candidates: Vec::new(),
            inline_node_path_hashes: HashSet::new(),
            if_mod_active_targets: Vec::new(),
            scan_cost: ScanCost::default(),
            nameless_def_count: 0,
            bundle_textures: Default::default(),
            undecodable_textures: Vec::new(),
            nested_may_require: Vec::new(),
        }
    }

    fn template(name: &str, may_require: &[&str]) -> TemplateEntry {
        TemplateEntry {
            graphic_class: None,
            def_type: "ThingDef".to_string(),
            name: name.to_string(),
            parent_name: None,
            may_require: may_require.iter().map(|s| (*s).to_string()).collect(),
            is_abstract: true,
            locator: XmlLocator::for_test(),
        }
    }

    fn injecting_op(names: &[&str]) -> PatchOp {
        PatchOp {
            class: "PatchOperationAdd".to_string(),
            xpath: Some("Defs".to_string()),
            target: None,
            find_mod_context: Vec::new(),
            conditional_xpath: None,
            find_mod_names: Vec::new(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: BTreeSet::new(),
            injected_paths: BTreeSet::new(),
            injected_template_names: names.iter().map(|s| (*s).to_string()).collect(),
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

    /// A stub assembly declaring `ThingDef` and `PawnKindDef` as two
    /// distinct, direct `System.Object` descendants — genuinely unrelated
    /// to each other, so [`TypeHierarchyIndex::is_subclass`] *confirms*
    /// rather than merely fails to resolve.
    fn unrelated_types_assembly() -> AssemblyInfo {
        AssemblyInfo {
            file_name: "stub".to_string(),
            name: "stub".to_string(),
            references: Vec::new(),
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: vec![
                ("ThingDef".to_string(), Some("System.Object".to_string())),
                ("PawnKindDef".to_string(), Some("System.Object".to_string())),
            ],
            parse_failed: false,
        }
    }

    fn build(scanned: &[ScannedMod]) -> TemplateRegistrations {
        let active = ActiveMods::build(scanned);
        let name_map = crate::analysis::edges::build_name_map(scanned).0;
        TemplateRegistrations::build(scanned, &active, &name_map)
    }

    #[test]
    fn a_vanilla_registration_resolves_for_every_child() {
        let mut core = scanned_mod("core", "Core");
        core.info.source = Source::Core;
        core.templates = vec![template("WallBase", &[])];
        let mods = vec![core, scanned_mod("child", "Child")];

        let registrations = build(&mods);

        assert!(registrations.is_registered("WallBase"));
        assert!(registrations.resolves_in_any_order("WallBase", &ModId::new("child")));
    }

    #[test]
    fn a_patch_registered_name_resolves_for_every_child() {
        let mut patcher = scanned_mod("patcher", "Patcher");
        patcher.patch_ops = vec![injecting_op(&["PatchedBase"])];
        let mods = vec![patcher, scanned_mod("child", "Child")];

        let registrations = build(&mods);

        assert!(registrations.is_registered("PatchedBase"));
        assert!(registrations.resolves_in_any_order("PatchedBase", &ModId::new("child")));
        assert!(
            registrations
                .foreign_owners("PatchedBase", &ModId::new("child"))
                .is_empty(),
            "a patch registration belongs to no mod (mod == null), so it is never an owner"
        );
    }

    /// `TryRegister`'s own early return: a `MayRequire` naming an
    /// inactive mod registers nothing at all, so the name is simply
    /// absent rather than owned by the gated mod.
    #[test]
    fn a_may_require_gated_template_registers_nothing() {
        let mut owner = scanned_mod("owner", "Owner");
        owner.templates = vec![template("GatedBase", &["not.installed"])];
        let mods = vec![owner, scanned_mod("child", "Child")];

        let registrations = build(&mods);

        assert!(!registrations.is_registered("GatedBase"));
        assert!(
            registrations
                .foreign_owners("GatedBase", &ModId::new("child"))
                .is_empty()
        );
    }

    #[test]
    fn a_satisfied_may_require_gate_still_registers() {
        let mut owner = scanned_mod("owner", "Owner");
        owner.templates = vec![template("GatedBase", &["child"])];
        let mods = vec![owner, scanned_mod("child", "Child")];

        let registrations = build(&mods);

        assert!(registrations.is_registered("GatedBase"));
        assert_eq!(
            registrations.foreign_owners("GatedBase", &ModId::new("child")),
            vec![&ModId::new("owner")]
        );
    }

    // -- broken_inheritance --------------------------------------------

    /// A distinct [`XmlLocator`] per call — `XmlLocator::for_test()` alone
    /// always returns the identical locator, which would make
    /// `children_by_template`'s own `(parent_key, locator)` dedup collapse
    /// two different synthetic children sharing one parent into one.
    fn locator_for(ordinal: u32) -> XmlLocator {
        XmlLocator::new(
            std::sync::Arc::from(std::path::Path::new("test.xml")),
            vec![ordinal],
        )
    }

    fn def(def_type: &str, def_name: &str, parent_name: Option<&str>, ordinal: u32) -> DefEntry {
        DefEntry {
            def_type: def_type.to_string(),
            def_name: def_name.to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            parent_name: parent_name.map(str::to_string),
            locator: locator_for(ordinal),
        }
    }

    fn template_of_type(name: &str, def_type: &str, parent_name: Option<&str>) -> TemplateEntry {
        TemplateEntry {
            graphic_class: None,
            def_type: def_type.to_string(),
            name: name.to_string(),
            parent_name: parent_name.map(str::to_string),
            may_require: Vec::new(),
            is_abstract: true,
            locator: XmlLocator::for_test(),
        }
    }

    /// `mods`' own order is load order — the same convention every other
    /// producer in this crate assumes (see `infra::mod.rs::resolve_active_mods`).
    fn scan_output(mods: Vec<ScannedMod>) -> ScanOutput {
        let order = mods.iter().map(|m| m.info.id.clone()).collect();
        let discovered_mod_count = mods.len();
        ScanOutput {
            load_order: LoadOrder::new(order),
            scanned_mods: mods,
            missing_mods: Vec::new(),
            vanilla_assembly_names: HashSet::new(),
            vanilla_type_hierarchy: Vec::new(),
            warnings: Vec::new(),
            child_value_hashes_by_mod: BTreeMap::new(),
            inactive_mods: Vec::new(),
            discovered_mod_count,
            core_resource_textures: BTreeSet::new(),
            ref_sites_by_mod: BTreeMap::new(),
        }
    }

    fn broken_inheritance_for(mods: Vec<ScannedMod>) -> Vec<Conflict> {
        broken_inheritance_and_warnings_for(mods).0
    }

    fn broken_inheritance_and_warnings_for(mods: Vec<ScannedMod>) -> (Vec<Conflict>, Vec<Warning>) {
        let active = ActiveMods::build(&mods);
        let name_map = crate::analysis::edges::build_name_map(&mods).0;
        let registrations = TemplateRegistrations::build(&mods, &active, &name_map);
        let scan = scan_output(mods);
        let children_index = crate::analysis::source_index::build_children_index(&scan);
        broken_inheritance(
            &scan.scanned_mods,
            &active,
            &registrations,
            &children_index,
            &scan.vanilla_type_hierarchy,
        )
    }

    fn only_broken_inheritance(conflicts: Vec<Conflict>) -> Vec<BrokenInheritance> {
        conflicts
            .into_iter()
            .map(|c| match c {
                Conflict::BrokenInheritance(b) => b,
                other => panic!("expected only BrokenInheritance, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn missing_parent_lists_every_concrete_descendant() {
        let mut child = scanned_mod("child", "Child");
        child.templates = vec![template_of_type("Base", "ThingDef", Some("Nobody"))];
        child.defs = vec![
            def("ThingDef", "Leaf1", Some("Base"), 0),
            def("ThingDef", "Leaf2", Some("Base"), 1),
        ];

        let found = only_broken_inheritance(broken_inheritance_for(vec![child]));

        assert_eq!(found.len(), 1);
        let finding = &found[0];
        assert_eq!(finding.parent_name, "Nobody");
        assert_eq!(finding.problem, InheritanceProblem::MissingParent);
        assert_eq!(finding.truncated, 0);
        assert_eq!(
            finding.affected,
            vec![
                ("ThingDef".to_string(), "Leaf1".to_string()),
                ("ThingDef".to_string(), "Leaf2".to_string()),
            ]
        );
    }

    #[test]
    fn missing_parent_on_an_abstract_template_names_its_concrete_children_not_itself() {
        let mut child = scanned_mod("child", "Child");
        child.templates = vec![template_of_type("Base", "ThingDef", Some("Nobody"))];
        child.defs = vec![def("ThingDef", "Leaf1", Some("Base"), 0)];

        let found = only_broken_inheritance(broken_inheritance_for(vec![child]));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].child.name, "Base");
        assert!(found[0].child.is_template);
        assert!(
            !found[0]
                .affected
                .contains(&("ThingDef".to_string(), "Base".to_string())),
            "the abstract template itself is never a concrete def"
        );
        assert_eq!(
            found[0].affected,
            vec![("ThingDef".to_string(), "Leaf1".to_string())]
        );
    }

    #[test]
    fn registration_only_in_a_later_mod_is_a_missing_parent() {
        let mut asking = scanned_mod("asking", "Asking");
        asking.defs = vec![def("ThingDef", "X", Some("LaterBase"), 0)];
        let mut later = scanned_mod("later", "Later");
        later.templates = vec![template_of_type("LaterBase", "ThingDef", None)];

        // `asking` loads first, `later` second — `later`'s own registration
        // is strictly after `asking`'s own load position, so it can never
        // satisfy `asking`'s own `ParentName` under `GetBestParentFor`.
        let found = only_broken_inheritance(broken_inheritance_for(vec![asking, later]));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].parent_name, "LaterBase");
        assert_eq!(found[0].problem, InheritanceProblem::MissingParent);
    }

    #[test]
    fn vanilla_registration_is_never_missing() {
        let mut child = scanned_mod("child", "Child");
        child.defs = vec![def("ThingDef", "X", Some("WallBase"), 0)];
        let mut core = scanned_mod("core", "Core");
        core.info.source = Source::Core;
        core.templates = vec![template_of_type("WallBase", "ThingDef", None)];

        // Core loads *after* `child` in this fixture's own scan order —
        // deliberately, to prove the vanilla fallback ignores position
        // entirely rather than merely happening to sort first.
        let found = only_broken_inheritance(broken_inheritance_for(vec![child, core]));

        assert!(
            found.is_empty(),
            "a vanilla registrant resolves for every child regardless of position: {found:?}"
        );
    }

    #[test]
    fn patch_injected_name_suppresses_the_finding() {
        let mut patcher = scanned_mod("patcher", "Patcher");
        patcher.patch_ops = vec![injecting_op(&["PatchedBase"])];
        let mut child = scanned_mod("child", "Child");
        child.defs = vec![def("ThingDef", "X", Some("PatchedBase"), 0)];

        let found = only_broken_inheritance(broken_inheritance_for(vec![patcher, child]));

        assert!(found.is_empty());
    }

    /// Two registrations of `N` exist — an earlier `ThingDef` and a later
    /// `PawnKindDef` — and the asking child loads strictly between them.
    /// The nearest-at-or-before rule must pick the earlier (`ThingDef`)
    /// one, not either "any" registration or the later one just because
    /// it happens to share the child's own type. Both types are given a
    /// stub, unrelated base chain (each its own direct `System.Object`
    /// descendant) so the subclass check *confirms* they're unrelated —
    /// without it, an unresolvable check would downgrade this to a
    /// warning instead (see `unresolvable_type_mismatch_downgrades_to_a_warning`).
    #[test]
    fn type_mismatch_uses_the_nearest_registration_not_any() {
        let mut early = scanned_mod("early", "Early");
        early.templates = vec![template_of_type("N", "ThingDef", None)];
        let mut middle = scanned_mod("middle", "Middle");
        middle.defs = vec![def("PawnKindDef", "MiddleThing", Some("N"), 0)];
        middle.assemblies = vec![unrelated_types_assembly()];
        let mut late = scanned_mod("late", "Late");
        late.templates = vec![template_of_type("N", "PawnKindDef", None)];

        let found = only_broken_inheritance(broken_inheritance_for(vec![early, middle, late]));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].mod_id, ModId::new("middle"));
        match &found[0].problem {
            InheritanceProblem::ParentTypeMismatch {
                parent_type,
                parent_owner,
            } => {
                assert_eq!(parent_type, "ThingDef");
                assert_eq!(parent_owner.as_ref(), Some(&ModId::new("early")));
            }
            other => panic!("expected ParentTypeMismatch, got {other:?}"),
        }
    }

    /// A type mismatch with no evidence either way — neither type
    /// appears in any scanned assembly's own `type_hierarchy` — downgrades
    /// to an informational [`Warning`], never a [`Conflict`]. Real-install
    /// case: dozens of vanilla/DLC-vs-mod-framework pairs a naive
    /// "tags differ" rule would over-count as `Hard` problems.
    #[test]
    fn unresolvable_type_mismatch_downgrades_to_a_warning() {
        let mut early = scanned_mod("early", "Early");
        early.templates = vec![template_of_type("N", "ExampleModType", None)];
        let mut child = scanned_mod("child", "Child");
        child.defs = vec![def("PawnKindDef", "MyThing", Some("N"), 0)];

        let (conflicts, warnings) = broken_inheritance_and_warnings_for(vec![early, child]);

        assert!(
            only_broken_inheritance(conflicts).is_empty(),
            "no assembly evidence resolves either type — this must never surface as a conflict"
        );
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].mod_id, Some(ModId::new("child")));
    }

    /// A confirmed subclass (the child's own type genuinely extends the
    /// resolved parent's, per a scanned assembly's own `type_hierarchy`)
    /// suppresses the finding entirely — real-install shape:
    /// `Example.Weapons.ExpandableProjectileDef : ThingDef`.
    #[test]
    fn a_confirmed_subclass_type_mismatch_is_suppressed() {
        let mut early = scanned_mod("early", "Early");
        early.templates = vec![template_of_type("N", "ThingDef", None)];
        let mut child = scanned_mod("child", "Child");
        child.defs = vec![def("Example.SpecializedThingDef", "MyThing", Some("N"), 0)];
        child.assemblies = vec![AssemblyInfo {
            file_name: "stub".to_string(),
            name: "stub".to_string(),
            references: Vec::new(),
            version: None,
            runtime_patches: Vec::new(),
            type_hierarchy: vec![(
                "Example.SpecializedThingDef".to_string(),
                Some("Verse.ThingDef".to_string()),
            )],
            parse_failed: false,
        }];

        let (conflicts, warnings) = broken_inheritance_and_warnings_for(vec![early, child]);

        assert!(only_broken_inheritance(conflicts).is_empty());
        assert!(
            warnings.is_empty(),
            "a confirmed subclass is safe by construction, not even worth a warning: {warnings:?}"
        );
    }

    /// A type mismatch that differs only by case never surfaces at all —
    /// real-install shape: `BackstoryDef` (child) inheriting a
    /// `BackStoryDef`-typed vanilla template.
    #[test]
    fn a_case_only_type_difference_is_suppressed() {
        let mut early = scanned_mod("early", "Early");
        early.templates = vec![template_of_type("N", "BackStoryDef", None)];
        let mut child = scanned_mod("child", "Child");
        child.defs = vec![def("BackstoryDef", "MyStory", Some("N"), 0)];

        let (conflicts, warnings) = broken_inheritance_and_warnings_for(vec![early, child]);

        assert!(only_broken_inheritance(conflicts).is_empty());
        assert!(warnings.is_empty());
    }

    /// A type mismatch whose asking child is owned by a vanilla (Core or
    /// DLC) mod is ground truth — never flagged, resolved or not.
    /// Real-install shape: `ludeon.rimworld.royalty`'s own `FleckDef`
    /// inheriting a `ThingDef`-typed vanilla template.
    #[test]
    fn a_vanilla_owned_childs_type_mismatch_is_ground_truth() {
        let mut early = scanned_mod("early", "Early");
        early.templates = vec![template_of_type("N", "ThingDef", None)];
        let mut dlc = scanned_mod("ludeon.rimworld.royalty", "Royalty");
        dlc.info.source = Source::Dlc;
        dlc.defs = vec![def("FleckDef", "MyFleck", Some("N"), 0)];

        let (conflicts, warnings) = broken_inheritance_and_warnings_for(vec![early, dlc]);

        assert!(only_broken_inheritance(conflicts).is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn gate_closed_child_is_ignored() {
        let mut child = scanned_mod("child", "Child");
        let mut gated = def("ThingDef", "X", Some("Nobody"), 0);
        gated.may_require = vec!["not.installed".to_string()];
        child.defs = vec![gated];

        let found = broken_inheritance_for(vec![child]);

        assert!(found.is_empty());
    }

    #[test]
    fn affected_list_is_bounded_and_counts_truncation() {
        let mut child = scanned_mod("child", "Child");
        child.templates = vec![template_of_type("Base", "ThingDef", Some("Nobody"))];
        let total = MAX_AFFECTED_DEFS + 10;
        child.defs = (0..total)
            .map(|i| def("ThingDef", &format!("Leaf{i:04}"), Some("Base"), i as u32))
            .collect();

        let found = only_broken_inheritance(broken_inheritance_for(vec![child]));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].affected.len(), MAX_AFFECTED_DEFS);
        assert_eq!(found[0].truncated, 10);
    }
}
