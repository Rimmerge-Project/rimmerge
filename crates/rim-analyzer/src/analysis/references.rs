//! Dangling def-name references, tier 1: a name
//! written at a recognized reference site — a list item, a scalar field,
//! a keyed-dictionary element name, or a `descriptionHyperlinks` entry —
//! in an active, gate-open def/template or an active patch's own
//! `<value>`, that no active def of *any* type actually has as its
//! `defName` in the post-patch set, and that isn't an implied
//! (engine-generated) name either.
//!
//! **Reference sites are inferred from data, never a hand-written field
//! table** (RimWorld's Def-typed field vocabulary is open and
//! per-mod-extensible — see [`crate::domain::MIN_RESOLVED_DISTINCT`]'s
//! own doc comment): for each `(def_type, field path)` observed across
//! every active def/template/patch value, a shape (list item, scalar, or
//! keyed-element) is trusted as a reference field only once enough of its
//! distinct values resolve to some active or implied def — the same vote
//! shape `rim-resolve`'s own `AssignmentSchema::infer_fields` uses for
//! the patch-maker's reference-field classification, at the lowest layer
//! both can reach. `descriptionHyperlinks` needs no vote: its own shape
//! is confirmed engine-typed and data-independent.
//!
//! **Type-agnostic by design**: a name existing under the *wrong* def
//! type reads as resolved here — typing a field from DLL metadata is out
//! of scope for this tier (typed field resolution, from ECMA-335
//! metadata, is a possible later tier).
//!
//! **Cross-references resolve once, after every def and patch has
//! loaded** (`DirectXmlCrossRefLoader.ResolveAllWantedCrossReferences`),
//! so this check never produces an ordering fact — every emitted
//! [`crate::domain::Conflict::DanglingDefReference`] is diagnostic only.
//!
//! `RemovedBy` is the one cause this module can determine on its own
//! (from data the scan already has: an active, unconditional-or-`if
//! exists` whole-def [`crate::domain::PatchOp`] targeting the dangling
//! name). Every other name gets
//! [`crate::domain::DanglingCause::Unexplained`] here —
//! `infra::explain_dangling_references` is the lazy IO pass that upgrades
//! it to `OnlyInUnloadedFolder`/`OnlyInInactiveMod`/`DefinedNowhere` where
//! it can; this module stays pure, per the `extract`/`analysis` layering
//! contract (`crates/rim-analyzer/CLAUDE.md`).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

use crate::domain::{
    Conflict, DanglingCause, DanglingDefReference, MIN_RESOLVED_DISTINCT, ModId, RefSite,
    RefSiteOwner, RefSiteReferrer, RefSiteShape, RefSiteSummary, ScannedMod, XmlLocator,
};

use super::indices::{
    ActiveMods, DisplayNameIndex, Indices, may_require_satisfied, patch_op_active,
};
use super::inheritance::affected_concrete_defs;
use super::source_index::ChildrenIndex;

/// A `(def_type, def_name)` key — matches `indices::DefKey`, redeclared
/// locally so this module doesn't need `indices` to make it `pub`.
type DefKey = (String, String);

/// A `(def_type, field_path, shape)` vote key — see this module's own doc
/// comment. `Hyperlink` sites never enter a vote (always references), so
/// this key only ever holds `ListItem`/`Scalar`/`KeyedElement`.
type VoteKey<'a> = (&'a str, &'a str, RefSiteShape);

/// How many referrers [`DanglingDefReference::referrers`] keeps before
/// truncating — bounds report size for a name referenced very widely.
const MAX_REFERRERS: usize = 20;

/// One eligible occurrence of a candidate value at a reference site,
/// paired with the mod whose own scan produced it (not stored on
/// [`RefSiteOwner`] itself — see that type's own doc comment).
struct Occurrence<'a> {
    mod_id: &'a ModId,
    site: &'a RefSite,
}

/// `seen`/`resolved` are only ever counted and probed, never iterated
/// into output, so they are hashed: the vote loop probes `seen` once per
/// site, millions of times on a real install.
#[derive(Default)]
struct FieldVote<'a> {
    seen: HashSet<&'a str>,
    resolved: HashSet<&'a str>,
    occurrences: Vec<Occurrence<'a>>,
}

/// The whole dangling-def-reference pass — see this module's own doc
/// comment. `indices.def_owners`/`injected_owners` give the post-patch
/// active def-name set (case (a) of the dangling rule); `children_index`
/// answers whether a template has an active concrete descendant (case
/// (i) — a reference in a template's own field tree is inherited, so it's
/// attributed to the template once, never per descendant).
#[must_use]
pub fn dangling_def_references(
    scanned: &[ScannedMod],
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
    indices: &Indices,
    injected_owners: &BTreeMap<DefKey, Vec<ModId>>,
    children_index: &ChildrenIndex,
    ref_sites_by_mod: &BTreeMap<ModId, Vec<RefSite>>,
) -> Vec<Conflict> {
    let removed = whole_def_removed_names(scanned, active, name_map);
    // A whole-def removal takes a template or colour out of the merged
    // document like any other def: it generates nothing.
    let still_active = |mut names: BTreeSet<String>| {
        names.retain(|name| !removed.contains_key(name));
        names
    };
    let active_def_names = ActiveNames {
        all: post_patch_active_def_names(indices, injected_owners, &removed),
        gene_templates: still_active(active_names_of_type_family(indices, "GeneTemplateDef")),
        terrain_templates: still_active(active_names_of_type_family(indices, "TerrainTemplateDef")),
        colors: still_active(active_names_of_type(indices, "ColorDef")),
    };
    let sound_names = active_names_of_type(indices, "SoundDef");

    let mut eligibility = Eligibility {
        active,
        scanned_by_id: scanned
            .iter()
            .map(|scanned_mod| (&scanned_mod.info.id, scanned_mod))
            .collect(),
        children_index,
        op_locators: op_locator_index(scanned, active, name_map),
        template_has_descendant: HashMap::new(),
        last_owner: None,
    };

    // Every eligible occurrence, split into the ones that need a vote
    // (`ListItem`/`Scalar`/`KeyedElement`) and the ones that don't
    // (`Hyperlink` — always a reference).
    //
    // Hashed for the per-site loop below; drained in sorted key order
    // afterward, so nothing downstream ever sees hash order.
    let mut votes: HashMap<VoteKey<'_>, FieldVote<'_>> = HashMap::new();
    let mut hyperlink_occurrences: Vec<Occurrence<'_>> = Vec::new();

    for (mod_id, sites) in ref_sites_by_mod {
        let owner = SiteMod {
            id: mod_id,
            is_active: active.contains(mod_id),
        };
        for site in sites {
            if !eligibility.site_is_eligible(site, &owner) {
                continue;
            }
            if site.value.is_empty() {
                continue;
            }
            match site.shape {
                RefSiteShape::Hyperlink => {
                    hyperlink_occurrences.push(Occurrence { mod_id, site });
                }
                _ => {
                    let key = (&*site.def_type, &*site.field_path, site.shape);
                    let vote = votes.entry(key).or_default();
                    // A value already seen in this vote was already
                    // resolved (or not) the first time.
                    if vote.seen.insert(&site.value) && resolves(&site.value, &active_def_names) {
                        vote.resolved.insert(&site.value);
                    }
                    vote.occurrences.push(Occurrence { mod_id, site });
                }
            }
        }
    }

    // Per-name aggregation: which fields/sites contribute a dangling
    // occurrence of each name, and whether any contributing field's own
    // resolved values are mostly `SoundDef`s.
    let mut by_name: BTreeMap<String, Vec<Occurrence<'_>>> = BTreeMap::new();
    let mut sound_names_by_dangling: BTreeSet<String> = BTreeSet::new();

    for occurrence in hyperlink_occurrences {
        if !resolves(&occurrence.site.value, &active_def_names) {
            by_name
                .entry(occurrence.site.value.clone())
                .or_default()
                .push(occurrence);
        }
    }

    let mut votes: Vec<(VoteKey<'_>, FieldVote<'_>)> = votes.into_iter().collect();
    votes.sort_unstable_by_key(|(key, _)| *key);
    for (key, vote) in votes {
        let (def_type, field_path, shape) = &key;
        if !is_reference_field(def_type, field_path, *shape, &vote) {
            continue;
        }
        let field_is_mostly_sound = is_mostly_of_type(&vote, &sound_names);
        for occurrence in vote.occurrences {
            if resolves(&occurrence.site.value, &active_def_names) {
                continue;
            }
            if field_is_mostly_sound {
                sound_names_by_dangling.insert(occurrence.site.value.clone());
            }
            by_name
                .entry(occurrence.site.value.clone())
                .or_default()
                .push(occurrence);
        }
    }

    by_name
        .into_iter()
        .filter(|(name, _)| !is_implied_name(name, &active_def_names))
        .map(|(name, occurrences)| {
            build_conflict(
                &name,
                occurrences,
                &removed,
                sound_names_by_dangling.contains(&name),
            )
        })
        .collect()
}

fn build_conflict(
    name: &str,
    occurrences: Vec<Occurrence<'_>>,
    removed: &BTreeMap<String, (ModId, XmlLocator)>,
    likely_sound: bool,
) -> Conflict {
    let mut referrers: Vec<RefSiteSummary> = occurrences
        .iter()
        .map(|occurrence| RefSiteSummary {
            referrer: referrer_of(occurrence),
            field_path: occurrence.site.field_path.to_string(),
        })
        .collect();
    referrers.sort_by(|a, b| format!("{a:?}").cmp(&format!("{b:?}")));
    referrers.dedup();
    let truncated_referrers = referrers.len().saturating_sub(MAX_REFERRERS);
    referrers.truncate(MAX_REFERRERS);

    let cause = removed
        .get(name)
        .map(|(mod_id, locator)| DanglingCause::RemovedBy {
            mod_id: mod_id.clone(),
            locator: locator.clone(),
        })
        .unwrap_or(DanglingCause::Unexplained);

    Conflict::DanglingDefReference(DanglingDefReference {
        name: name.to_string(),
        referrers,
        truncated_referrers,
        cause,
        likely_sound,
    })
}

fn referrer_of(occurrence: &Occurrence<'_>) -> RefSiteReferrer {
    match &*occurrence.site.owner {
        RefSiteOwner::Def { def_name, .. } => RefSiteReferrer::ReferencedFromDef {
            mod_id: occurrence.mod_id.clone(),
            def_type: occurrence.site.def_type.to_string(),
            def_name: def_name.clone(),
        },
        RefSiteOwner::Template { name, .. } => RefSiteReferrer::ReferencedFromTemplate {
            mod_id: occurrence.mod_id.clone(),
            def_type: occurrence.site.def_type.to_string(),
            name: name.clone(),
        },
        RefSiteOwner::Patch {
            def_name, locator, ..
        } => RefSiteReferrer::ReferencedFromPatch {
            mod_id: occurrence.mod_id.clone(),
            def_type: occurrence.site.def_type.to_string(),
            def_name: def_name.clone(),
            locator: locator.clone(),
        },
    }
}

/// The mod whose scan produced a batch of sites, with whether it's active
/// looked up once for the batch rather than once per site.
struct SiteMod<'a> {
    id: &'a ModId,
    is_active: bool,
}

/// Everything [`Eligibility::site_is_eligible`] reads, plus a memo of
/// [`has_active_concrete_descendant`] per template: that walk depends only
/// on the template and its mod, never on which of the template's own
/// (often hundreds of) sites asks, and re-walking it per site cost
/// seconds on a real install.
struct Eligibility<'a> {
    active: &'a ActiveMods,
    scanned_by_id: BTreeMap<&'a ModId, &'a ScannedMod>,
    children_index: &'a ChildrenIndex,
    op_locators: HashMap<&'a XmlLocator, (&'a ModId, bool)>,
    template_has_descendant: HashMap<(&'a str, &'a str, &'a ModId), bool>,
    /// The owner the previous site was checked against, with its verdict.
    /// A def's, template's or patch op's sites are consecutive and share
    /// one `Arc`'d owner, so the owner half of the check (a locator lookup
    /// keyed on a path, or a template memo probe) runs once per owner
    /// instead of once per site — millions of times on a real install.
    /// Keyed on the owning mod as well as the `Arc`: the verdict reads the
    /// mod's activity, so a verdict is never reused across mods even if an
    /// owner `Arc` were ever shared by two of them.
    last_owner: Option<(&'a Arc<RefSiteOwner>, &'a ModId, bool)>,
}

impl<'a> Eligibility<'a> {
    /// Whether `site` counts as evidence at all: its owner is active and
    /// gate-open (a template also needs an active concrete descendant —
    /// case (i) of the dangling rule), and the site's own `MayRequire`/
    /// `MayRequireAnyOf` gate (case (c)) is open.
    fn site_is_eligible(&mut self, site: &'a RefSite, owner: &SiteMod<'a>) -> bool {
        if !may_require_satisfied(&site.may_require, &site.may_require_any_of, self.active) {
            return false;
        }
        if let Some((last, last_mod, verdict)) = self.last_owner
            && Arc::ptr_eq(last, &site.owner)
            && last_mod == owner.id
        {
            return verdict;
        }
        let verdict = self.owner_is_eligible(site, owner);
        self.last_owner = Some((&site.owner, owner.id, verdict));
        verdict
    }

    /// The owner half of [`Self::site_is_eligible`]; its verdict depends on
    /// the site's owner, its `def_type` and `owner` (the mod).
    fn owner_is_eligible(&mut self, site: &'a RefSite, owner: &SiteMod<'a>) -> bool {
        let active = self.active;
        match &*site.owner {
            RefSiteOwner::Def {
                may_require,
                may_require_any_of,
                ..
            } => owner.is_active && may_require_satisfied(may_require, may_require_any_of, active),
            RefSiteOwner::Template {
                name, may_require, ..
            } => {
                owner.is_active
                    && may_require_satisfied(may_require, &[], active)
                    && self.template_has_descendant(&site.def_type, name, owner.id)
            }
            RefSiteOwner::Patch { locator, .. } => match self.op_locators.get(locator) {
                Some((op_owner, op_active)) => *op_owner == owner.id && *op_active,
                None => false,
            },
        }
    }

    /// [`has_active_concrete_descendant`], memoized per template.
    fn template_has_descendant(
        &mut self,
        def_type: &'a str,
        name: &'a str,
        owner: &'a ModId,
    ) -> bool {
        if let Some(known) = self.template_has_descendant.get(&(def_type, name, owner)) {
            return *known;
        }
        let has_descendant = has_active_concrete_descendant(
            &(def_type.to_string(), name.to_string()),
            owner,
            &self.scanned_by_id,
            self.children_index,
            self.active,
        );
        self.template_has_descendant
            .insert((def_type, name, owner), has_descendant);
        has_descendant
    }
}

/// Whether the `(def_type, name)` template has at least one active,
/// gate-open concrete descendant, transitively — reuses
/// [`affected_concrete_defs`], the exact same walk
/// `Conflict::BrokenInheritance::affected` is built from, rather than a
/// second, independently-written traversal.
fn has_active_concrete_descendant(
    template_key: &DefKey,
    owner: &ModId,
    scanned_by_id: &BTreeMap<&ModId, &ScannedMod>,
    children_index: &ChildrenIndex,
    active: &ActiveMods,
) -> bool {
    let (affected, _truncated) = affected_concrete_defs(
        children_index,
        scanned_by_id,
        active,
        owner,
        template_key.clone(),
    );
    // The template's own key only lands in `affected` when it is *also*
    // registered as a concrete def (a hybrid node) — `affected_concrete_defs`
    // checks `children_index.owners_by_def`, which never contains a
    // Name-only template. So any non-empty result here is a genuine
    // concrete descendant, never the template counting itself.
    !affected.is_empty()
}

/// Every patch op's own [`XmlLocator`] mapped to its owning mod and
/// whether [`patch_op_active`] holds for it — built once so
/// [`Eligibility::site_is_eligible`] can check a [`RefSiteOwner::Patch`] site's own op
/// without a linear scan per site. Lookup-only (never iterated), so it is
/// hashed and borrows each locator: an ordered map compares file paths
/// component by component on every probe.
fn op_locator_index<'a>(
    scanned: &'a [ScannedMod],
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
) -> HashMap<&'a XmlLocator, (&'a ModId, bool)> {
    let mut out = HashMap::new();
    for scanned_mod in scanned {
        for op in &scanned_mod.patch_ops {
            out.insert(
                &op.locator,
                (&scanned_mod.info.id, patch_op_active(op, active, name_map)),
            );
        }
    }
    out
}

/// Every active def name (any type), after removing whole-def-removed
/// names and adding patch-injected ones — case (a) of the dangling rule.
/// Collapsed to name-only, both for `indices.def_owners` and for
/// `removed`: `PatchOperationRemove.ApplyWorker`'s own
/// `xml.SelectNodes(xpath)` removes every node the xpath matches across
/// the whole combined document, so a bare `defName="X"` remover deletes
/// every `X` node regardless of its element type — see
/// [`whole_def_removed_names`]'s own doc comment.
fn post_patch_active_def_names(
    indices: &Indices,
    injected_owners: &BTreeMap<DefKey, Vec<ModId>>,
    removed: &BTreeMap<String, (ModId, XmlLocator)>,
) -> HashSet<String> {
    let mut names: HashSet<String> = HashSet::new();
    for (_def_type, def_name) in indices.def_owners.keys() {
        if removed.contains_key(def_name) {
            continue;
        }
        names.insert(def_name.clone());
    }
    for (_def_type, def_name) in injected_owners.keys() {
        names.insert(def_name.clone());
    }
    names
}

/// Every active def name whose type is `base_type` or a mod's subclass of
/// it, recognised by the tag's last `.`-segment ending with `base_type`
/// (`GeneTemplateDef`, `MorphGeneTemplateDef`, `Example.MorphGeneTemplateDef`):
/// a subclass of a def type generates defs exactly as its base does, so
/// excluding it would flag real generated names.
fn active_names_of_type_family(indices: &Indices, base_type: &str) -> BTreeSet<String> {
    indices
        .def_owners
        .keys()
        .filter(|(def_type, _)| {
            def_type
                .rsplit('.')
                .next()
                .is_some_and(|tag| tag.ends_with(base_type))
        })
        .map(|(_, name)| name.clone())
        .collect()
}

/// Every active def name of exactly `def_type` — used for the
/// `likely_sound` check (`SoundDef`).
fn active_names_of_type(indices: &Indices, def_type: &str) -> BTreeSet<String> {
    indices
        .def_owners
        .keys()
        .filter(|(t, _)| t == def_type)
        .map(|(_, name)| name.clone())
        .collect()
}

/// Every name a whole-def `PatchOperationRemove` deletes from the merged
/// document — case (a) of the dangling rule. Collapsed to name-only
/// (dropping `def_type`): `PatchOperationRemove.ApplyWorker`'s own
/// `xml.SelectNodes(xpath)` removes every matching node in the whole
/// combined document, so a bare `defName="X"` remover deletes every `X`
/// node regardless of its element type. Only the *unconditional, or
/// genuinely "if exists" self-guarded* shape counts
/// ([`super::edges::is_genuinely_conditional_remove`]) — a remove nested
/// under a Conditional testing something else entirely is not proven to
/// have fired. "First remover wins" (`or_insert_with`) for the evidence
/// (`mod`/`locator`) is an arbitrary but deterministic tie-break — which
/// specific remover explains the name doesn't change *that* it's
/// removed.
fn whole_def_removed_names(
    scanned: &[ScannedMod],
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
) -> BTreeMap<String, (ModId, XmlLocator)> {
    let mut out = BTreeMap::new();
    for scanned_mod in scanned {
        let remover_id = &scanned_mod.info.id;
        for op in &scanned_mod.patch_ops {
            if !(op.is_mutating
                && op.class.ends_with("PatchOperationRemove")
                && patch_op_active(op, active, name_map))
            {
                continue;
            }
            if super::edges::is_genuinely_conditional_remove(op) {
                continue;
            }
            for target in super::patch_op_targets(op) {
                if target.sub_path.is_some() {
                    continue;
                }
                out.entry(target.def_name.clone())
                    .or_insert_with(|| (remover_id.clone(), op.locator.clone()));
            }
        }
    }
    out
}

/// Whether `name` resolves to some active or implied def — case (a)/(b)
/// combined, the "vote resolution" question (distinct from
/// [`is_implied_name`] alone, which is the *filter* applied once a name
/// is already known dangling).
fn resolves(name: &str, active_def_names: &ActiveNames) -> bool {
    active_def_names.all.contains(name) || is_implied_name(name, active_def_names)
}

/// The active def names the implied-name rules read: every name, plus the
/// names of the few def types whose members the engine combines into
/// generated defs (a name is only a gene or a carpet when its first half
/// has the generating type).
///
/// `all` is membership-only (never iterated) and probed for every distinct
/// value of every field, so it is hashed. The three small typed sets are
/// walked whole by [`is_generated_gene_name`]/[`is_generated_carpet_name`]
/// rather than probed per split point of a value.
struct ActiveNames {
    all: HashSet<String>,
    gene_templates: BTreeSet<String>,
    terrain_templates: BTreeSet<String>,
    colors: BTreeSet<String>,
}

/// Case (b) of the dangling rule: the vanilla implied-def generators,
/// modeled by prefix/suffix against active def names. Type-agnostic
/// here too for the prefix/suffix shapes — this tier doesn't distinguish
/// "an active ThingDef" from "an active TerrainDef" beyond what's needed
/// to keep the rule from over-firing on short/common names; they are
/// checked against the flat active-name set, exactly as
/// `RimWorld.DefGenerator.GenerateImpliedDefs_PreResolve` itself
/// constructs each generated name. The gene and carpet shapes combine two
/// defs of known types, so those are typed.
fn is_implied_name(name: &str, active_def_names: &ActiveNames) -> bool {
    const PREFIXES: [&str; 10] = [
        "Blueprint_Install_",
        "Blueprint_",
        "Frame_",
        "Corpse_",
        "Meat_",
        "Techprint_",
        "Administer_",
        "Make_",
        // Psytrainers: the literal `Psytrainer_` is in the engine's string
        // heap, and the generated names are that prefix plus an ability's
        // defName. Neurotrainers follow the same shape: the engine builds
        // the name by concatenation (no single literal), and every
        // `Neurotrainer_<name>` seen in game logs and mod data has an
        // active def name after the prefix.
        "Psytrainer_",
        "Neurotrainer_",
    ];
    for prefix in PREFIXES {
        if let Some(rest) = name.strip_prefix(prefix)
            && active_def_names.all.contains(rest)
        {
            return true;
        }
    }
    const SUFFIXES: [&str; 3] = ["_Rough", "_RoughHewn", "_Smooth"];
    for suffix in SUFFIXES {
        if let Some(rest) = name.strip_suffix(suffix)
            && active_def_names.all.contains(rest)
        {
            return true;
        }
    }
    for prefix in ["GeneticChemicalDependency_", "Trainable_"] {
        if let Some(rest) = name.strip_prefix(prefix)
            && active_def_names.all.contains(rest)
        {
            return true;
        }
    }
    is_generated_gene_name(name, active_def_names)
        || is_generated_carpet_name(name, active_def_names)
}

/// A gene the engine generates from a gene template: `<template>_<def>`,
/// the template's defName (a `GeneTemplateDef`), an underscore, then the
/// defName of a def the template is applied to. Requiring the first half
/// to be a `GeneTemplateDef` keeps two unrelated defs that happen to share
/// a prefix from reading as a generated name.
///
/// Walks the (few) active templates rather than every `_` position of
/// `name`: a template may itself contain `_`, and probing every split
/// point of every candidate value cost about a second on a real install,
/// where most values are labels and descriptions.
fn is_generated_gene_name(name: &str, active_def_names: &ActiveNames) -> bool {
    active_def_names.gene_templates.iter().any(|template| {
        !template.is_empty()
            && name
                .strip_prefix(template.as_str())
                .and_then(|rest| rest.strip_prefix('_'))
                .is_some_and(|applied| {
                    !applied.is_empty() && active_def_names.all.contains(applied)
                })
    })
}

/// A carpet the engine generates from a terrain template: the template's
/// defName directly followed by a colour's defName, with no separator
/// (`Carpet` + `Sandstone`; vanilla data references `CarpetSandstone`,
/// `CarpetGreyDark`). Both halves must be non-empty and have the
/// generating types. Walks the (few) active terrain templates rather than
/// every character position of `name`, for the same reason as
/// [`is_generated_gene_name`].
fn is_generated_carpet_name(name: &str, active_def_names: &ActiveNames) -> bool {
    active_def_names.terrain_templates.iter().any(|template| {
        !template.is_empty()
            && name.strip_prefix(template.as_str()).is_some_and(|colour| {
                !colour.is_empty() && active_def_names.colors.contains(colour)
            })
    })
}

/// The last `/`-segment of `field_path` — the field's own leaf tag.
fn leaf_tag(field_path: &str) -> &str {
    field_path.rsplit('/').next().unwrap_or(field_path)
}

/// Whether `(def_type, field_path, shape)` clears the reference-field
/// vote — see this module's own doc comment for the per-shape rule and
/// the `Def`/`Defs`-suffix exemption (`List`/`Scalar` only).
fn is_reference_field(
    _def_type: &str,
    field_path: &str,
    shape: RefSiteShape,
    vote: &FieldVote<'_>,
) -> bool {
    if vote.seen.is_empty() {
        return false;
    }
    // A suffix-exempted field's rule collapses to "at least one resolved
    // value" outright — no distinct-count floor and no 90% threshold —
    // the same full bypass the assignment domain's own identical
    // exemption gives (`classify_reference`'s own doc comment: "skips
    // the vote entirely"), not merely a lowered floor still gated by the
    // ratio check.
    let exempt = match shape {
        RefSiteShape::ListItem => {
            // `field_path` always ends in `/li` for this shape — the
            // leaf tag is the segment *before* it, the container's own
            // tag.
            let container = field_path.strip_suffix("/li").unwrap_or(field_path);
            leaf_tag(container).ends_with("Defs")
        }
        RefSiteShape::Scalar => leaf_tag(field_path).ends_with("Def"),
        RefSiteShape::KeyedElement => false,
        RefSiteShape::Hyperlink => return true,
    };
    if exempt {
        return !vote.resolved.is_empty();
    }
    vote.resolved.len() >= MIN_RESOLVED_DISTINCT
        && (vote.resolved.len() as f64 / vote.seen.len() as f64) >= 0.9
}

/// Whether the majority of `vote`'s own resolved values are names of
/// `type_names` (`SoundDef`, for [`DanglingDefReference::likely_sound`]).
fn is_mostly_of_type(vote: &FieldVote<'_>, type_names: &BTreeSet<String>) -> bool {
    if vote.resolved.is_empty() {
        return false;
    }
    let matching = vote
        .resolved
        .iter()
        .filter(|value| type_names.contains(**value))
        .count();
    matching * 2 > vote.resolved.len()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use crate::domain::{DeclaredOrder, Mod, ModId, ScanCost, ScannedMod, Source};
    use crate::extract::{defs, patches};

    use super::super::edges;
    use super::super::indices::Indices;
    use super::super::source_index::build_children_index;
    use super::*;

    fn test_file() -> Arc<Path> {
        Arc::from(Path::new("test.xml"))
    }

    fn base_mod(id: &str) -> ScannedMod {
        ScannedMod {
            info: Mod {
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
            },
            defs: Vec::new(),
            templates: Vec::new(),
            patch_ops: Vec::new(),
            textures: BTreeMap::new(),
            sounds: BTreeSet::new(),
            translation_keys: BTreeSet::new(),
            inline_types: BTreeSet::new(),
            manifest_order: crate::domain::ManifestOrder::default(),
            inline_node_path_hashes: HashSet::new(),
            assemblies: Vec::new(),
            if_mod_active_targets: Vec::new(),
            texture_path_candidates: Vec::new(),
            scan_cost: ScanCost::default(),
            nameless_def_count: 0,
            bundle_textures: BTreeSet::new(),
            undecodable_textures: Vec::new(),
            nested_may_require: Vec::new(),
        }
    }

    /// Builds one mod from real `Defs/**/*.xml` text, through the real
    /// extractor — never a hand-built `RefSite` — and returns it plus its
    /// own contributed [`RefSite`]s.
    fn defs_mod(id: &str, xml: &str) -> (ScannedMod, Vec<RefSite>) {
        let defs_file = defs::index(xml.as_bytes(), &test_file()).expect("valid Defs XML");
        let mut scanned_mod = base_mod(id);
        scanned_mod.defs = defs_file.defs;
        scanned_mod.templates = defs_file.templates;
        (scanned_mod, defs_file.ref_sites)
    }

    /// Same as [`defs_mod`], plus one mod owning `patches_xml`'s own
    /// `<Patch>` document — through the real patch walker, converting each
    /// [`patches::PatchValueRefSite`] into a domain [`RefSite`] the exact
    /// way `infra::mod_scan` does (see that module's own
    /// `combine_sub_path`).
    fn patches_mod(id: &str, xml: &str) -> (ScannedMod, Vec<RefSite>) {
        let (ops, value_ref_sites, _tree_depth_truncated) =
            patches::walk_with_ref_sites(xml.as_bytes(), &test_file()).expect("valid Patch XML");
        let mut scanned_mod = base_mod(id);
        let mod_id = scanned_mod.info.id.clone();
        scanned_mod.patch_ops = ops;
        let ref_sites = value_ref_sites
            .into_iter()
            .map(|site| RefSite {
                def_type: Arc::from(site.target.def_type.as_str()),
                field_path: Arc::from(combine_sub_path(
                    site.target.sub_path.as_deref(),
                    &site.field_path,
                )),
                shape: site.shape,
                value: site.value,
                owner: Arc::new(RefSiteOwner::Patch {
                    mod_id: mod_id.clone(),
                    def_name: site.target.def_name.clone(),
                    locator: site.op_locator,
                }),
                may_require: site.may_require.into_boxed_slice(),
                may_require_any_of: site.may_require_any_of.into_boxed_slice(),
            })
            .collect();
        (scanned_mod, ref_sites)
    }

    /// Mirrors `infra::mod_scan::combine_sub_path` exactly (a private
    /// function one layer over, in `infra`) — see that function's own doc
    /// comment.
    fn combine_sub_path(sub_path: Option<&str>, relative: &str) -> String {
        match (sub_path, relative.is_empty()) {
            (Some(sub), true) => sub.to_string(),
            (Some(sub), false) => format!("{sub}/{relative}"),
            (None, _) => relative.to_string(),
        }
    }

    fn find_dangling<'a>(
        conflicts: &'a [Conflict],
        name: &str,
    ) -> Option<&'a DanglingDefReference> {
        conflicts.iter().find_map(|c| match c {
            Conflict::DanglingDefReference(d) if d.name == name => Some(d),
            _ => None,
        })
    }

    /// Runs the whole pass over `mods` (in the given load order) plus
    /// every mod's own `ref_sites`, wiring every shared index the same
    /// way `report_builder::collect_conflicts` does.
    fn run(mods: Vec<(ScannedMod, Vec<RefSite>)>) -> Vec<Conflict> {
        let scanned: Vec<ScannedMod> = mods.iter().map(|(m, _)| m.clone()).collect();
        let ref_sites_by_mod: BTreeMap<ModId, Vec<RefSite>> = mods
            .into_iter()
            .map(|(m, sites)| (m.info.id, sites))
            .collect();
        let active = ActiveMods::build(&scanned);
        let (name_map, _warnings) = edges::build_name_map(&scanned);
        let indices = Indices::build(&scanned, &HashSet::new(), &active, &BTreeSet::new());
        let injected_owners = edges::injected_def_owners(&scanned, &active, &name_map);
        let load_order =
            crate::domain::LoadOrder::new(scanned.iter().map(|m| m.info.id.clone()).collect());
        let scan = crate::domain::ScanOutput {
            scanned_mods: scanned.clone(),
            load_order,
            missing_mods: Vec::new(),
            vanilla_assembly_names: HashSet::new(),
            vanilla_type_hierarchy: Vec::new(),
            warnings: Vec::new(),
            child_value_hashes_by_mod: BTreeMap::new(),
            inactive_mods: Vec::new(),
            discovered_mod_count: scanned.len(),
            core_resource_textures: BTreeSet::new(),
            ref_sites_by_mod: ref_sites_by_mod.clone(),
        };
        let children_index = build_children_index(&scan);
        dangling_def_references(
            &scanned,
            &active,
            &name_map,
            &indices,
            &injected_owners,
            &children_index,
            &ref_sites_by_mod,
        )
    }

    // -- eligibility cache ----------------------------------------------

    #[test]
    fn a_shared_owner_gets_each_mods_own_eligibility_verdict() {
        let shared_owner = Arc::new(RefSiteOwner::Def {
            def_name: "Shared".to_string(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
        });
        let site = RefSite {
            def_type: Arc::from("ThingDef"),
            field_path: Arc::from("li"),
            shape: RefSiteShape::ListItem,
            value: "Anything".to_string(),
            owner: Arc::clone(&shared_owner),
            may_require: Box::default(),
            may_require_any_of: Box::default(),
        };
        let scanned: Vec<ScannedMod> = Vec::new();
        let active = ActiveMods::build(&scanned);
        let children_index = ChildrenIndex::default();
        let mut eligibility = Eligibility {
            active: &active,
            scanned_by_id: BTreeMap::new(),
            children_index: &children_index,
            op_locators: HashMap::new(),
            template_has_descendant: HashMap::new(),
            last_owner: None,
        };
        let active_mod_id = ModId::new("active.mod");
        let inactive_mod_id = ModId::new("inactive.mod");
        let active_mod = SiteMod {
            id: &active_mod_id,
            is_active: true,
        };
        let inactive_mod = SiteMod {
            id: &inactive_mod_id,
            is_active: false,
        };

        let first = eligibility.site_is_eligible(&site, &active_mod);
        let second = eligibility.site_is_eligible(&site, &inactive_mod);

        assert!(first, "the active mod's site is evidence");
        assert!(
            !second,
            "the inactive mod's site must not inherit the previous mod's cached verdict"
        );
    }

    // -- vote ------------------------------------------------------------

    /// Nine distinct list items, all real `ResearchProjectDef`s — the
    /// shared fixture for every test in this module that needs a list
    /// field to clear both the distinct-count floor (5) and the 90%
    /// resolved threshold with exactly one dangling value alongside it
    /// (9 of 10 = 90%).
    const NINE_RESEARCH_DEFS: &str = r#"<Defs>
          <ResearchProjectDef><defName>Alpha</defName></ResearchProjectDef>
          <ResearchProjectDef><defName>Beta</defName></ResearchProjectDef>
          <ResearchProjectDef><defName>Gamma</defName></ResearchProjectDef>
          <ResearchProjectDef><defName>Delta</defName></ResearchProjectDef>
          <ResearchProjectDef><defName>Epsilon</defName></ResearchProjectDef>
          <ResearchProjectDef><defName>Zeta</defName></ResearchProjectDef>
          <ResearchProjectDef><defName>Eta</defName></ResearchProjectDef>
          <ResearchProjectDef><defName>Theta</defName></ResearchProjectDef>
          <ResearchProjectDef><defName>Iota</defName></ResearchProjectDef>
        </Defs>"#;

    const NINE_RESEARCH_LIST_ITEMS: &str = r#"
          <li>Alpha</li>
          <li>Beta</li>
          <li>Gamma</li>
          <li>Delta</li>
          <li>Epsilon</li>
          <li>Zeta</li>
          <li>Eta</li>
          <li>Theta</li>
          <li>Iota</li>
        "#;

    #[test]
    fn a_list_field_whose_values_mostly_resolve_is_a_reference_field() {
        let (research, _) = defs_mod("research", NINE_RESEARCH_DEFS);
        let referrer_xml = format!(
            r#"<Defs>
                  <ThingDef>
                    <defName>Wall</defName>
                    <researchPrerequisites>
                      {NINE_RESEARCH_LIST_ITEMS}
                      <li>GhostResearch</li>
                    </researchPrerequisites>
                  </ThingDef>
                </Defs>"#
        );
        let (referrer, referrer_sites) = defs_mod("referrer", &referrer_xml);
        let conflicts = run(vec![(research, Vec::new()), (referrer, referrer_sites)]);
        assert!(find_dangling(&conflicts, "GhostResearch").is_some());
        assert!(find_dangling(&conflicts, "Alpha").is_none());
    }

    #[test]
    fn a_tag_list_of_free_strings_is_not_a_reference_field() {
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>Wall</defName>
                    <tags>
                      <li>Wooden</li>
                      <li>Flammable</li>
                      <li>Cheap</li>
                      <li>Heavy</li>
                      <li>Ugly</li>
                    </tags>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(referrer, referrer_sites)]);
        // None of these free-text tags ever resolve, so the field never
        // clears the vote and none of them is reported as dangling.
        assert!(find_dangling(&conflicts, "Wooden").is_none());
        assert!(find_dangling(&conflicts, "Flammable").is_none());
    }

    #[test]
    fn def_suffixed_leaf_is_a_reference_field_with_one_resolved_value() {
        let (kind, _) = defs_mod(
            "kind",
            r#"<Defs><PawnKindDef><defName>Colonist</defName></PawnKindDef></Defs>"#,
        );
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>Spawner</defName>
                    <kindDef>Colonist</kindDef>
                  </ThingDef>
                  <ThingDef>
                    <defName>OtherSpawner</defName>
                    <kindDef>GhostKind</kindDef>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(kind, Vec::new()), (referrer, referrer_sites)]);
        assert!(find_dangling(&conflicts, "GhostKind").is_some());
    }

    #[test]
    fn keyed_container_element_names_are_references() {
        let (materials, _) = defs_mod(
            "materials",
            r#"<Defs>
                  <ThingDef><defName>Steel</defName></ThingDef>
                  <ThingDef><defName>WoodLog</defName></ThingDef>
                  <ThingDef><defName>Silver</defName></ThingDef>
                  <ThingDef><defName>Gold</defName></ThingDef>
                  <ThingDef><defName>Plasteel</defName></ThingDef>
                  <ThingDef><defName>Uranium</defName></ThingDef>
                  <ThingDef><defName>Jade</defName></ThingDef>
                  <ThingDef><defName>Cloth</defName></ThingDef>
                  <ThingDef><defName>Leather</defName></ThingDef>
                </Defs>"#,
        );
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>Widget</defName>
                    <costList>
                      <Steel>10</Steel>
                      <WoodLog>5</WoodLog>
                      <Silver>2</Silver>
                      <Gold>1</Gold>
                      <Plasteel>3</Plasteel>
                      <Uranium>1</Uranium>
                      <Jade>1</Jade>
                      <Cloth>1</Cloth>
                      <Leather>1</Leather>
                      <GhostMaterial>1</GhostMaterial>
                    </costList>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(materials, Vec::new()), (referrer, referrer_sites)]);
        assert!(find_dangling(&conflicts, "GhostMaterial").is_some());
        assert!(find_dangling(&conflicts, "Steel").is_none());
    }

    #[test]
    fn description_hyperlinks_are_always_references() {
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>Wall</defName>
                    <descriptionHyperlinks>
                      <ThingDef>GhostThing</ThingDef>
                    </descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#,
        );
        // A single occurrence is enough — `descriptionHyperlinks` needs no
        // vote at all.
        let conflicts = run(vec![(referrer, referrer_sites)]);
        assert!(find_dangling(&conflicts, "GhostThing").is_some());
    }

    // -- post-patch set ----------------------------------------------------

    #[test]
    fn a_def_removed_by_an_unconditional_whole_def_remove_is_dangling_with_removed_by() {
        let (defined, _) = defs_mod(
            "definer",
            r#"<Defs><ThingDef><defName>GhostDef</defName></ThingDef></Defs>"#,
        );
        let (remover, _) = patches_mod(
            "remover",
            r#"<Patch>
                  <Operation Class="PatchOperationRemove">
                    <xpath>Defs/ThingDef[defName="GhostDef"]</xpath>
                  </Operation>
                </Patch>"#,
        );
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>Wall</defName>
                    <descriptionHyperlinks><ThingDef>GhostDef</ThingDef></descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![
            (defined, Vec::new()),
            (remover, Vec::new()),
            (referrer, referrer_sites),
        ]);
        let dangling = find_dangling(&conflicts, "GhostDef").expect("GhostDef is dangling");
        assert!(matches!(
            dangling.cause,
            DanglingCause::RemovedBy { ref mod_id, .. } if mod_id.as_str() == "remover"
        ));
    }

    #[test]
    fn a_genuinely_conditional_remove_does_not_remove() {
        let (defined, _) = defs_mod(
            "definer",
            r#"<Defs><ThingDef><defName>GhostDef</defName></ThingDef></Defs>"#,
        );
        let (remover, _) = patches_mod(
            "remover",
            r#"<Patch>
                  <Operation Class="PatchOperationConditional">
                    <xpath>Defs/ThingDef[defName="SomethingElseEntirely"]</xpath>
                    <match Class="PatchOperationRemove">
                      <xpath>Defs/ThingDef[defName="GhostDef"]</xpath>
                    </match>
                  </Operation>
                </Patch>"#,
        );
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>Wall</defName>
                    <descriptionHyperlinks><ThingDef>GhostDef</ThingDef></descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![
            (defined, Vec::new()),
            (remover, Vec::new()),
            (referrer, referrer_sites),
        ]);
        assert!(find_dangling(&conflicts, "GhostDef").is_none());
    }

    #[test]
    fn a_patch_injected_def_is_defined() {
        let (injector, _) = patches_mod(
            "injector",
            r#"<Patch>
                  <Operation Class="PatchOperationAdd">
                    <xpath>Defs</xpath>
                    <value>
                      <ThingDef><defName>InjectedDef</defName></ThingDef>
                    </value>
                  </Operation>
                </Patch>"#,
        );
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>Wall</defName>
                    <descriptionHyperlinks><ThingDef>InjectedDef</ThingDef></descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(injector, Vec::new()), (referrer, referrer_sites)]);
        assert!(find_dangling(&conflicts, "InjectedDef").is_none());
    }

    #[test]
    fn implied_blueprint_corpse_meat_names_are_defined() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs><ThingDef><defName>Wall</defName></ThingDef></Defs>"#,
        );
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>Spawner</defName>
                    <descriptionHyperlinks>
                      <ThingDef>Blueprint_Wall</ThingDef>
                    </descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(base, Vec::new()), (referrer, referrer_sites)]);
        assert!(find_dangling(&conflicts, "Blueprint_Wall").is_none());
    }

    #[test]
    fn stone_terrain_suffixes_are_defined() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs><TerrainDef><defName>Granite</defName></TerrainDef></Defs>"#,
        );
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>Spawner</defName>
                    <descriptionHyperlinks>
                      <TerrainDef>Granite_Rough</TerrainDef>
                    </descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(base, Vec::new()), (referrer, referrer_sites)]);
        assert!(find_dangling(&conflicts, "Granite_Rough").is_none());
    }

    // -- implied names -------------------------------------------------------

    /// A referrer whose only reference is `name`, through a
    /// `descriptionHyperlinks` entry (always a reference, no vote).
    fn referrer_of_name(name: &str) -> (ScannedMod, Vec<RefSite>) {
        defs_mod(
            "referrer",
            &format!(
                r#"<Defs>
                  <ThingDef>
                    <defName>Spawner</defName>
                    <descriptionHyperlinks><ThingDef>{name}</ThingDef></descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#
            ),
        )
    }

    #[test]
    fn a_name_joining_two_defs_that_is_not_a_gene_template_pair_is_flagged() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <ResearchTabDef><defName>ExampleTab</defName></ResearchTabDef>
                  <HediffDef><defName>ExampleOrgan</defName></HediffDef>
                </Defs>"#,
        );
        let (referrer, sites) = referrer_of_name("ExampleTab_ExampleOrgan");
        let conflicts = run(vec![(base, Vec::new()), (referrer, sites)]);
        assert!(find_dangling(&conflicts, "ExampleTab_ExampleOrgan").is_some());
    }

    #[test]
    fn a_gene_template_name_joined_to_a_def_is_a_generated_gene() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <GeneTemplateDef><defName>Example_Template</defName></GeneTemplateDef>
                  <SkillDef><defName>ExampleSkill</defName></SkillDef>
                </Defs>"#,
        );
        let (referrer, sites) = referrer_of_name("Example_Template_ExampleSkill");
        let conflicts = run(vec![(base, Vec::new()), (referrer, sites)]);
        assert!(find_dangling(&conflicts, "Example_Template_ExampleSkill").is_none());
    }

    #[test]
    fn a_mods_subclass_of_the_gene_template_type_also_generates_genes() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <Example.MorphGeneTemplateDef><defName>ExampleMorphs</defName></Example.MorphGeneTemplateDef>
                  <XenotypeDef><defName>ExampleWolf</defName></XenotypeDef>
                </Defs>"#,
        );
        let (referrer, sites) = referrer_of_name("ExampleMorphs_ExampleWolf");
        let conflicts = run(vec![(base, Vec::new()), (referrer, sites)]);
        assert!(find_dangling(&conflicts, "ExampleMorphs_ExampleWolf").is_none());
    }

    #[test]
    fn a_generated_gene_name_whose_def_half_is_missing_is_flagged() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs><GeneTemplateDef><defName>ExampleTemplate</defName></GeneTemplateDef></Defs>"#,
        );
        let (referrer, sites) = referrer_of_name("ExampleTemplate_NoSuchSkill");
        let conflicts = run(vec![(base, Vec::new()), (referrer, sites)]);
        assert!(find_dangling(&conflicts, "ExampleTemplate_NoSuchSkill").is_some());
    }

    #[test]
    fn a_terrain_template_name_followed_by_a_colour_is_a_generated_carpet() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <TerrainTemplateDef><defName>ExampleCarpet</defName></TerrainTemplateDef>
                  <ColorDef><defName>Sand</defName></ColorDef>
                </Defs>"#,
        );
        let (referrer, sites) = referrer_of_name("ExampleCarpetSand");
        let conflicts = run(vec![(base, Vec::new()), (referrer, sites)]);
        assert!(find_dangling(&conflicts, "ExampleCarpetSand").is_none());
    }

    /// One active template's name is a prefix of another's: the split
    /// after the shorter one names no def, the split after the longer one
    /// does — the name is generated through the longer template.
    #[test]
    fn a_gene_from_a_template_whose_name_extends_another_templates_is_generated() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <GeneTemplateDef><defName>Example</defName></GeneTemplateDef>
                  <GeneTemplateDef><defName>Example_Template</defName></GeneTemplateDef>
                  <SkillDef><defName>ExampleSkill</defName></SkillDef>
                </Defs>"#,
        );
        let (referrer, sites) = referrer_of_name("Example_Template_ExampleSkill");
        let conflicts = run(vec![(base, Vec::new()), (referrer, sites)]);
        assert!(find_dangling(&conflicts, "Example_Template_ExampleSkill").is_none());
    }

    #[test]
    fn a_carpet_from_a_template_whose_name_extends_another_templates_is_generated() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <TerrainTemplateDef><defName>ExampleCarpet</defName></TerrainTemplateDef>
                  <TerrainTemplateDef><defName>ExampleCarpetFine</defName></TerrainTemplateDef>
                  <ColorDef><defName>Sand</defName></ColorDef>
                </Defs>"#,
        );
        let (referrer, sites) = referrer_of_name("ExampleCarpetFineSand");
        let conflicts = run(vec![(base, Vec::new()), (referrer, sites)]);
        assert!(find_dangling(&conflicts, "ExampleCarpetFineSand").is_none());
    }

    #[test]
    fn a_mods_subclass_of_the_terrain_template_type_also_generates_carpets() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <Example.ShoreTerrainTemplateDef><defName>ExampleShore</defName></Example.ShoreTerrainTemplateDef>
                  <ColorDef><defName>Sand</defName></ColorDef>
                </Defs>"#,
        );
        let (referrer, sites) = referrer_of_name("ExampleShoreSand");
        let conflicts = run(vec![(base, Vec::new()), (referrer, sites)]);
        assert!(find_dangling(&conflicts, "ExampleShoreSand").is_none());
    }

    #[test]
    fn a_gene_template_removed_by_a_whole_def_remove_generates_no_genes() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <GeneTemplateDef><defName>ExampleTemplate</defName></GeneTemplateDef>
                  <SkillDef><defName>ExampleSkill</defName></SkillDef>
                </Defs>"#,
        );
        let (remover, _) = patches_mod(
            "remover",
            r#"<Patch>
                  <Operation Class="PatchOperationRemove">
                    <xpath>Defs/GeneTemplateDef[defName="ExampleTemplate"]</xpath>
                  </Operation>
                </Patch>"#,
        );
        let (referrer, sites) = referrer_of_name("ExampleTemplate_ExampleSkill");
        let conflicts = run(vec![
            (base, Vec::new()),
            (remover, Vec::new()),
            (referrer, sites),
        ]);
        assert!(find_dangling(&conflicts, "ExampleTemplate_ExampleSkill").is_some());
    }

    #[test]
    fn a_terrain_template_removed_by_a_whole_def_remove_generates_no_carpets() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <TerrainTemplateDef><defName>ExampleCarpet</defName></TerrainTemplateDef>
                  <ColorDef><defName>Sand</defName></ColorDef>
                </Defs>"#,
        );
        let (remover, _) = patches_mod(
            "remover",
            r#"<Patch>
                  <Operation Class="PatchOperationRemove">
                    <xpath>Defs/TerrainTemplateDef[defName="ExampleCarpet"]</xpath>
                  </Operation>
                </Patch>"#,
        );
        let (referrer, sites) = referrer_of_name("ExampleCarpetSand");
        let conflicts = run(vec![
            (base, Vec::new()),
            (remover, Vec::new()),
            (referrer, sites),
        ]);
        assert!(find_dangling(&conflicts, "ExampleCarpetSand").is_some());
    }

    #[test]
    fn a_carpet_shaped_name_with_untyped_halves_is_flagged() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <ThingDef><defName>ExampleCarpet</defName></ThingDef>
                  <ThingDef><defName>Sand</defName></ThingDef>
                </Defs>"#,
        );
        let (referrer, sites) = referrer_of_name("ExampleCarpetSand");
        let conflicts = run(vec![(base, Vec::new()), (referrer, sites)]);
        assert!(find_dangling(&conflicts, "ExampleCarpetSand").is_some());
    }

    #[test]
    fn psytrainer_and_neurotrainer_names_of_active_defs_are_generated() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs>
                  <AbilityDef><defName>ExampleAbility</defName></AbilityDef>
                  <SkillDef><defName>ExampleSkill</defName></SkillDef>
                </Defs>"#,
        );
        for name in ["Psytrainer_ExampleAbility", "Neurotrainer_ExampleSkill"] {
            let (referrer, sites) = referrer_of_name(name);
            let conflicts = run(vec![(base.clone(), Vec::new()), (referrer, sites)]);
            assert!(find_dangling(&conflicts, name).is_none(), "{name}");
        }
    }

    #[test]
    fn psytrainer_and_neurotrainer_names_of_absent_defs_are_flagged() {
        let (base, _) = defs_mod(
            "base",
            r#"<Defs><AbilityDef><defName>ExampleAbility</defName></AbilityDef></Defs>"#,
        );
        for name in ["Psytrainer_NoSuchAbility", "Neurotrainer_NoSuchSkill"] {
            let (referrer, sites) = referrer_of_name(name);
            let conflicts = run(vec![(base.clone(), Vec::new()), (referrer, sites)]);
            assert!(find_dangling(&conflicts, name).is_some(), "{name}");
        }
    }

    // -- hyperlink referrers -------------------------------------------------

    #[test]
    fn a_hyperlink_referrer_def_carries_its_own_type_not_the_targets() {
        let (referrer, sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <HediffDef>
                    <defName>ExampleHediff</defName>
                    <descriptionHyperlinks><ThingDef>GhostThing</ThingDef></descriptionHyperlinks>
                  </HediffDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(referrer, sites)]);
        let dangling = find_dangling(&conflicts, "GhostThing").expect("GhostThing is dangling");
        assert!(matches!(
            dangling.referrers[0].referrer,
            RefSiteReferrer::ReferencedFromDef { ref def_type, .. } if def_type == "HediffDef"
        ));
    }

    #[test]
    fn a_hyperlink_in_a_template_is_checked_against_the_templates_own_type() {
        let (template, template_sites) = defs_mod(
            "template",
            r#"<Defs>
                  <HediffDef Name="ExampleBase" Abstract="True">
                    <descriptionHyperlinks><ThingDef>GhostThing</ThingDef></descriptionHyperlinks>
                  </HediffDef>
                  <HediffDef ParentName="ExampleBase"><defName>ExampleChild</defName></HediffDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(template, template_sites)]);
        let dangling = find_dangling(&conflicts, "GhostThing")
            .expect("the template has an active HediffDef child, so its reference counts");
        assert!(matches!(
            dangling.referrers[0].referrer,
            RefSiteReferrer::ReferencedFromTemplate { ref def_type, ref name, .. }
                if def_type == "HediffDef" && name == "ExampleBase"
        ));
    }

    // -- gating ------------------------------------------------------------

    #[test]
    fn a_mayrequire_gated_list_item_is_not_dangling_when_gate_closed() {
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>Wall</defName>
                    <researchPrerequisites>
                      <li>Alpha</li>
                      <li>Beta</li>
                      <li>Gamma</li>
                      <li>Delta</li>
                      <li>Epsilon</li>
                      <li MayRequire="nonexistent.mod">GhostResearch</li>
                    </researchPrerequisites>
                  </ThingDef>
                </Defs>"#,
        );
        let (research, _) = defs_mod(
            "research",
            r#"<Defs>
                  <ResearchProjectDef><defName>Alpha</defName></ResearchProjectDef>
                  <ResearchProjectDef><defName>Beta</defName></ResearchProjectDef>
                  <ResearchProjectDef><defName>Gamma</defName></ResearchProjectDef>
                  <ResearchProjectDef><defName>Delta</defName></ResearchProjectDef>
                  <ResearchProjectDef><defName>Epsilon</defName></ResearchProjectDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(research, Vec::new()), (referrer, referrer_sites)]);
        // The gated `li`'s own value never even enters the vote, so it's
        // never reported dangling.
        assert!(find_dangling(&conflicts, "GhostResearch").is_none());
    }

    #[test]
    fn a_gate_closed_def_is_not_a_referrer() {
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef MayRequire="nonexistent.mod">
                    <defName>Wall</defName>
                    <descriptionHyperlinks><ThingDef>GhostDef</ThingDef></descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(referrer, referrer_sites)]);
        assert!(find_dangling(&conflicts, "GhostDef").is_none());
    }

    /// Eligibility is decided once per owner and reused for that owner's
    /// following sites: a gate-closed def between two open ones must not
    /// lend its verdict to either neighbour, nor borrow theirs.
    #[test]
    fn a_gate_closed_def_between_open_defs_excludes_only_its_own_references() {
        let (referrer, referrer_sites) = defs_mod(
            "referrer",
            r#"<Defs>
                  <ThingDef>
                    <defName>OpenBefore</defName>
                    <descriptionHyperlinks>
                      <ThingDef>GhostBeforeOne</ThingDef>
                      <ThingDef>GhostBeforeTwo</ThingDef>
                    </descriptionHyperlinks>
                  </ThingDef>
                  <ThingDef MayRequire="nonexistent.mod">
                    <defName>Gated</defName>
                    <descriptionHyperlinks>
                      <ThingDef>GhostGatedOne</ThingDef>
                      <ThingDef>GhostGatedTwo</ThingDef>
                    </descriptionHyperlinks>
                  </ThingDef>
                  <ThingDef>
                    <defName>OpenAfter</defName>
                    <descriptionHyperlinks>
                      <ThingDef>GhostAfterOne</ThingDef>
                      <ThingDef>GhostAfterTwo</ThingDef>
                    </descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(referrer, referrer_sites)]);
        for open in [
            "GhostBeforeOne",
            "GhostBeforeTwo",
            "GhostAfterOne",
            "GhostAfterTwo",
        ] {
            assert!(
                find_dangling(&conflicts, open).is_some(),
                "{open} must be reported"
            );
        }
        for gated in ["GhostGatedOne", "GhostGatedTwo"] {
            assert!(
                find_dangling(&conflicts, gated).is_none(),
                "{gated} must not be reported"
            );
        }
    }

    #[test]
    fn an_abstract_template_reference_counts_only_with_an_active_concrete_descendant() {
        let (orphan_template, orphan_sites) = defs_mod(
            "orphan",
            r#"<Defs>
                  <ThingDef Name="OrphanBase" Abstract="True">
                    <descriptionHyperlinks><ThingDef>GhostDef</ThingDef></descriptionHyperlinks>
                  </ThingDef>
                </Defs>"#,
        );
        let conflicts = run(vec![(orphan_template.clone(), orphan_sites.clone())]);
        assert!(
            find_dangling(&conflicts, "GhostDef").is_none(),
            "no active concrete descendant — the reference isn't counted"
        );

        let (child, _) = defs_mod(
            "child",
            r#"<Defs><ThingDef ParentName="OrphanBase"><defName>Child</defName></ThingDef></Defs>"#,
        );
        let conflicts_with_child = run(vec![(orphan_template, orphan_sites), (child, Vec::new())]);
        assert!(
            find_dangling(&conflicts_with_child, "GhostDef").is_some(),
            "an active concrete descendant makes the template's own reference count"
        );
    }

    // -- patch values --------------------------------------------------------

    #[test]
    fn a_reference_injected_by_a_patch_value_is_flagged_and_attributed_to_the_patch() {
        let (owner, _) = defs_mod(
            "owner",
            r#"<Defs><ThingDef><defName>Wall</defName></ThingDef></Defs>"#,
        );
        let (patcher, patcher_sites) = patches_mod(
            "patcher",
            r#"<Patch>
                  <Operation Class="PatchOperationAdd">
                    <xpath>Defs/ThingDef[defName="Wall"]</xpath>
                    <value>
                      <descriptionHyperlinks><ThingDef>GhostDef</ThingDef></descriptionHyperlinks>
                    </value>
                  </Operation>
                </Patch>"#,
        );
        let conflicts = run(vec![(owner, Vec::new()), (patcher, patcher_sites)]);
        let dangling = find_dangling(&conflicts, "GhostDef").expect("GhostDef is dangling");
        assert_eq!(dangling.referrers.len(), 1);
        assert!(matches!(
            dangling.referrers[0].referrer,
            RefSiteReferrer::ReferencedFromPatch { ref mod_id, ref def_name, .. }
                if mod_id.as_str() == "patcher" && def_name == "Wall"
        ));
    }

    // -- determinism ---------------------------------------------------------

    #[test]
    fn dangling_reference_output_is_independent_of_ref_site_order() {
        let (research, _) = defs_mod("research", NINE_RESEARCH_DEFS);
        let referrer_xml = format!(
            r#"<Defs>
                  <ThingDef>
                    <defName>Wall</defName>
                    <researchPrerequisites>
                      {NINE_RESEARCH_LIST_ITEMS}
                      <li>GhostResearch</li>
                    </researchPrerequisites>
                  </ThingDef>
                </Defs>"#
        );
        let (referrer, mut referrer_sites) = defs_mod("referrer", &referrer_xml);
        let forward = run(vec![
            (research.clone(), Vec::new()),
            (referrer.clone(), referrer_sites.clone()),
        ]);
        referrer_sites.reverse();
        let reversed = run(vec![(research, Vec::new()), (referrer, referrer_sites)]);

        let names = |conflicts: &[Conflict]| -> Vec<String> {
            conflicts
                .iter()
                .filter_map(|c| match c {
                    Conflict::DanglingDefReference(d) => Some(d.name.clone()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(names(&forward), names(&reversed));
    }
}
