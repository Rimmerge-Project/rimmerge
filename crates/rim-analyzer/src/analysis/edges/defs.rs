//! Def-level edges: injected-def owners, parent/child name targets,
//! `usesType`, parent templates, and def-override-after-origin.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::domain::{DefTarget, Edge, EdgeKind, EdgeStrength, ModId, ScannedMod, Selector};
use crate::extract::xpath_expr;

use super::assembly::dll_owner_of;
use super::patches::{SeenEdges, push_edge_once};
use crate::analysis::indices::{ActiveMods, DefKey, DisplayNameIndex, Indices, patch_op_active};
use crate::analysis::inheritance::{TemplateRegistrations, parent_references};

/// Every mod that patch-injects a whole def, by [`DefKey`] — the
/// `<xpath>Defs</xpath>` shape of [`PatchOp::injected_paths`], parsed back
/// into a `DefKey` (unlike `injected_paths`' own flat
/// `"{def_type}/{def_name}"` string form).
///
/// A free function here, not a field on [`Indices`] — same rationale as
/// [`dll_owner_of`]: a caller needs exactly `scanned`/`active`/`name_map`,
/// not the whole `Indices`, and this needs `patch_op_active`'s *full* gating
/// (including a `PatchOperationFindMod` `<match>`/`<nomatch>` branch), which
/// `name_map` requires — `Indices::build` runs *before*
/// [`build_name_map`](super::names::build_name_map) in `report_builder.rs`,
/// so a field on `Indices` itself could only ever apply
/// `may_require_satisfied`. That narrower gating admits a large share of
/// phantom owners on a real install — defs injected only inside an
/// unsatisfied `FindMod` branch (the classic "compat patch only when the
/// other mod is present" shape, e.g. `CultureDef/EX_Draconic` claimed by a
/// content-pack mod even though its own injecting op never runs) — which the
/// second `patch_injected_node_edges` pass would turn into a wrongly-enforced
/// `Hard` edge, able to force a real cycle drop.
///
/// `.filter(|path| path.matches('/').count() == 1)` guards against a
/// malformed or future-mis-shaped `injected_paths` entry reaching the
/// `DefKey` split below with the wrong number of segments (the
/// whole-`<Defs>`-root shape this reads is always exactly
/// `"{element_tag}/{defName}"`, one `/`, by construction — this is
/// defense-in-depth, not a case expected to fire today).
#[must_use]
pub fn injected_def_owners(
    scanned: &[ScannedMod],
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
) -> BTreeMap<DefKey, Vec<ModId>> {
    let mut owners: BTreeMap<DefKey, Vec<ModId>> = BTreeMap::new();
    for scanned_mod in scanned {
        let id = &scanned_mod.info.id;
        for key in scanned_mod
            .patch_ops
            .iter()
            .filter(|op| op.is_mutating && op.target.is_none())
            .filter(|op| patch_op_active(op, active, name_map))
            .flat_map(|op| op.injected_paths.iter())
            .filter(|path| path.matches('/').count() == 1)
            .filter_map(|path| path.split_once('/'))
            .map(|(def_type, def_name)| (def_type.to_string(), def_name.to_string()))
            .collect::<HashSet<_>>()
        {
            owners.entry(key).or_default().push(id.clone());
        }
    }
    owners
}

/// Every def/template a `[@ParentName="X"]`-headed patch xpath actually
/// touches — the answer
/// [`crate::extract::xpath_target::parent_name_predicate`] alone can never
/// give (that module has no cross-mod index at all; see its own doc comment).
/// Without this, every head-position `[@ParentName="X"]` occurrence (over a
/// hundred on a 1000-mod install) would be indexed under no def key at all
/// and silently counted as `unscoped` instead.
///
/// One [`DefTarget`] per child `children_by_template` lists for
/// `(def_type, parent_name)`, every one sharing the head's own
/// `sub_path` — the identical "every matched def shares one sub_path"
/// convention [`crate::extract::xpath_target::parse_all`] already uses
/// for a disjunctive `defName="X" or defName="Y"` head. `Selector` is
/// picked the same way `rim-session`'s own `inspect_def::child_ref`
/// already does for exactly this question (`crates/rim-session/CLAUDE.md`):
/// `DefName` when `owners_by_def` carries the child's own key (some
/// active mod registers it as a concrete def), `NameAttr` otherwise (a
/// template-only child, no `defName` of its own).
///
/// Takes `owners_by_def`/`children_by_template` directly, not a whole
/// [`SourceIndex`](crate::analysis::source_index::SourceIndex) — this lives
/// in `rim-analyzer` (which has no `SourceIndex` type of its own; that's a
/// `rim-session`-facing view built *from* this crate's own per-mod scan, not
/// the other way around) and is called from `source_index::build`'s own
/// second pass, which already has both maps fully built by then (see that
/// function's own doc comment for why a *complete* `children_by_template` —
/// built from every mod, not just those loaded so far — is exactly what this
/// needs and a single interleaved pass could never supply).
#[must_use]
pub fn parent_name_targets(
    xpath: &str,
    owners_by_def: &BTreeMap<DefKey, Vec<ModId>>,
    children_by_template: &BTreeMap<(String, String), Vec<(ModId, DefKey)>>,
) -> Vec<DefTarget> {
    let Some((def_type, parent_name, sub_path)) =
        crate::extract::xpath_target::parent_name_predicate(xpath)
    else {
        return Vec::new();
    };
    let Some(children) = children_by_template.get(&(def_type, parent_name)) else {
        return Vec::new();
    };

    let mut targets: Vec<DefTarget> = Vec::new();
    for (_, (child_type, child_name)) in children {
        let selector = if owners_by_def.contains_key(&(child_type.clone(), child_name.clone())) {
            Selector::DefName
        } else {
            Selector::NameAttr
        };
        let target = DefTarget {
            def_type: child_type.clone(),
            def_name: child_name.clone(),
            selector,
            sub_path: sub_path.clone(),
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

/// Resolves a `[race/intelligence="Humanlike"]`-shaped head — a
/// child-element-value equality naming no def directly — against every
/// **concrete** def of the head's own type, using only the *winning* owner's
/// own scanned content — `owners_by_def`'s list is already in load order
/// (built by a single pass over `scan.load_order` in `source_index::build`),
/// so its own `.last()` is the winner: the same last-in-load-order-wins
/// convention `analysis::mod_cost`'s own `sorted_owners.last() !=
/// Some(this_id)` check already applies to a same-key ownership list
/// elsewhere in this crate, and the one `rim-merge`'s own "def override is
/// last copy wins wholesale" rule states from the replay side. Never a losing
/// owner's copy — the real engine never keeps that one loaded once patches
/// run at all (see `rim-merge`'s own "Losers' nodes are absent during replay"
/// note) — so testing every owner's content indiscriminately could resolve a
/// def the effective (winning) content no longer actually matches, exactly
/// the over-broad/wrong-def risk this feature must not reopen.
///
/// `xpath_target`/`xpath_expr` alone can never answer this at all (see
/// [`xpath_expr::head_content_predicate`]'s own doc comment for why — this
/// mirrors [`parent_name_targets`]'s split exactly): this module is where the
/// query is actually run, against
/// [`SourceIndex::child_value_hashes_by_mod`](crate::analysis::source_index::SourceIndex::child_value_hashes_by_mod)'s
/// real per-mod content. `None`/empty for anything
/// [`xpath_expr::head_content_predicate`] itself can't recognize, and for a
/// def whose winning owner's scan simply never recorded a matching leaf —
/// both stay an honest miss, never a guess.
#[must_use]
pub fn child_value_targets(
    xpath: &str,
    owners_by_def: &BTreeMap<DefKey, Vec<ModId>>,
    child_value_hashes_by_mod: &BTreeMap<ModId, HashSet<u64>>,
) -> Vec<DefTarget> {
    let Some(query) = xpath_expr::head_content_predicate(xpath) else {
        return Vec::new();
    };

    let mut targets: Vec<DefTarget> = Vec::new();
    for ((candidate_type, candidate_name), owners) in owners_by_def {
        if *candidate_type != query.def_type {
            continue;
        }
        let Some(winner) = owners.last() else {
            continue;
        };
        let Some(hashes) = child_value_hashes_by_mod.get(winner) else {
            continue;
        };
        let matches = query.alternatives.iter().any(|(path, value)| {
            let key = format!(
                "{candidate_type}/{candidate_name}/{}={value}",
                path.join("/")
            );
            hashes.contains(&crate::domain::hash_node_path(&key))
        });
        if !matches {
            continue;
        }
        let target = DefTarget {
            def_type: candidate_type.clone(),
            def_name: candidate_name.clone(),
            selector: Selector::DefName,
            sub_path: query.sub_path.clone(),
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

/// A def in one mod names a type (`Class="ns.X"`, or any other
/// fully-qualified type string written inline in `Defs/`) whose namespace is
/// owned by another active mod's shipped DLL (see [`dll_owner_of`]). Whether
/// the using mod *declares* a relation to the owner (the ledger's
/// `UndeclaredTypeDependency` finding) is left to the ledger: it's fully
/// derivable there from whether a `Declared`/`Hard` edge already exists
/// between the same two mods, so this edge carries no extra flag.
#[must_use]
pub fn uses_type_edges(scanned: &[ScannedMod], indices: &Indices) -> Vec<Edge> {
    let mut edges = Vec::new();
    for scanned_mod in scanned {
        let user_id = &scanned_mod.info.id;
        let mut seen = HashSet::new();
        for type_name in &scanned_mod.inline_types {
            let Some(owner) = dll_owner_of(type_name, &indices.assembly_owners) else {
                continue;
            };
            if owner == user_id || !seen.insert(owner.clone()) {
                continue;
            }
            edges.push(Edge {
                after: user_id.clone(),
                before: owner.clone(),
                kind: EdgeKind::UsesType,
                detail: format!("names type '{type_name}' from {owner}'s assembly"),
                load_time: true,
                subject: Some(type_name.clone()),
            });
        }
    }
    edges
}

/// A def's or template's `ParentName` names a template exactly one other
/// active mod registers, and nothing else registers it — so that mod must
/// load at or before this one, or `Verse.XmlInheritance` drops the def.
///
/// **Modeled on the decompiled `GetBestParentFor`**
/// ([`EdgeKind::ParentTemplate`]'s own doc comment summarizes the rule and
/// why the resulting edges are `EdgeStrength::Hard`). The
/// single-non-vanilla-owner shape of
/// [`patch_target_edges`](super::patches::patch_target_edges) is wrong for
/// inheritance in two directions at once:
/// - Filtering the child and the vanilla owners out of the candidate list
///   and then emitting an edge to whatever foreign owner remains is wrong:
///   a child's own registration, and a vanilla one, each satisfy the
///   at-or-before test unconditionally — the lookup cannot fail, so there
///   is no ordering fact at all. Such edges would order **Core after a
///   mod** for names Core itself defines (`Bomb`, `CutBase`, `Solid`, ...).
/// - Treating the surviving edges as mere `Awareness` evidence is wrong
///   too: inheritance does not resolve after every mod's defs load, and an
///   unresolved `ParentName` is an `XML error` and a dropped def chain.
///
/// [`TemplateRegistrations`] answers "who registers this name, and can
/// the lookup fail" — including the patch case (a patch-added node
/// registers with `mod == null`, `GetBestParentFor`'s own fallback) and
/// the `MayRequire` gate, neither of which
/// [`Indices::template_owners`](crate::analysis::indices::Indices::template_owners)
/// models.
///
/// **Two or more foreign owners produce no edge**, deliberately. The true
/// statement there is an any-of — at least one owner must load at or
/// before the child, and which one wins only changes *which* parent is
/// inherited, never whether the def survives — and
/// [`crate::domain::Constraint::AnyOf`] is the right shape for it. But
/// that variant's payload is an assembly name, and its consumers
/// (`rim_resolve::sort::any_of`, `rim_resolve::evaluate`, that crate's
/// `EdgeProvenance::AnyOf`, and the desktop's own rendering of it) would
/// all need a second variant threaded through them: new sorter work, not
/// a producer change. Emitting one edge per candidate instead is exactly
/// the contradictory-per-candidate modelling
/// [`crate::domain::Constraint`]'s own module doc says any-of exists to
/// avoid, and picking one candidate arbitrarily would assert an ordering
/// the engine does not require. So this case is skipped, and recorded as
/// forgone rather than silently absorbed.
#[must_use]
pub fn parent_template_edges(
    scanned: &[ScannedMod],
    active: &ActiveMods,
    registrations: &TemplateRegistrations,
) -> Vec<Edge> {
    let mut edges = Vec::new();
    for scanned_mod in scanned {
        // A **vanilla child** never produces an edge, whatever it inherits.
        // Core and the DLCs are first by tier, so the only edge this could
        // ever emit is "some mod before Core", which the sorter cannot
        // satisfy and must not be asked to: an `EdgeStrength::Hard` edge is
        // added at `Layer::Hard`, ahead of everything, and would drag the
        // owner in front of vanilla. The engine's own verdict is the same —
        // `GetBestParentFor` finds nothing at or before a vanilla child and
        // logs `Could not find parent node named`, which is the *mod's* bug
        // (it registered a name vanilla content inherits from, without
        // vanilla registering it) and is reported by
        // `checks::unresolved_parent_templates`, not by an ordering the
        // sorter has no way to express.
        if scanned_mod.info.source.is_vanilla() {
            continue;
        }
        let child_id = &scanned_mod.info.id;
        // `BTreeSet`, not `HashSet`: iterated below, and when two parent
        // names resolve to the same owner, only the first one processed
        // makes it into that edge's detail text (`seen` dedups on the
        // owner) — `HashSet`'s per-process random order would make that
        // text vary between two runs over the same install.
        let parent_names: BTreeSet<&str> = parent_references(scanned_mod, active)
            .map(|reference| reference.parent_name)
            .collect();

        let mut seen = HashSet::new();
        for name in parent_names {
            if registrations.resolves_in_any_order(name, child_id) {
                continue;
            }
            let foreign_owners = registrations.foreign_owners(name, child_id);
            let [owner] = foreign_owners.as_slice() else {
                continue;
            };
            if seen.insert((*owner).clone()) {
                edges.push(Edge {
                    after: child_id.clone(),
                    before: (*owner).clone(),
                    kind: EdgeKind::ParentTemplate,
                    detail: format!("inherits '{name}', a template only {owner} registers - loading it later drops the def"
                    ),
                    load_time: true,
                    subject: Some(name.to_string()),
                });
            }
        }
    }
    edges
}

/// `def_name`'s family prefix: the text up to and including the first `_`,
/// when that text is at least 3 characters — `None` for a def_name with no
/// `_` (or an underscore-only/too-short prefix, e.g. `"A_B"`'s `"A_"`), which
/// never qualifies for the prefix-sharing rule below. Named `_family_prefix`,
/// not `_prefix`: `rim_resolve::tags::evidence::prefix_of` is a
/// same-crate-family but *different* function with different semantics
/// (lowercased, splits on `_` or `.`, excludes the separator itself, no
/// minimum length) — sharing the shorter name invited assuming the two agree
/// when they don't.
pub(super) fn def_name_family_prefix(def_name: &str) -> Option<&str> {
    let idx = def_name.find('_')?;
    let prefix = &def_name[..=idx];
    (prefix.chars().count() >= 3).then_some(prefix)
}

/// For a `Defs/`-inline def with two or more active owners and no
/// `Declared`/`Hard` edge already connecting any two of them (in either
/// direction — such an edge already orders that pair without inference, so
/// the whole def is skipped rather than only the already-ordered pair), infer
/// an **origin** when exactly one owner qualifies under either rule below;
/// every other owner is ordered after it. Ambiguous (zero, or more than one,
/// qualifying owner) emits nothing — the ledger's existing def_override
/// handling stands unchanged. `Edge.subject` carries the def's own key,
/// `"{def_type}/{def_name}"`.
///
/// Deliberately scoped to `indices.def_owners` (`Defs/`-inline ownership
/// only), not `conflicts::merged_def_owners`'s injected-owner merge: both
/// origin rules below are inherently about `Defs/`-inline authorship (a
/// `ParentName`/prefix-sharing sibling def is only ever `Defs/`-inline in
/// this analyzer's model), so a def that exists *only* via patch
/// injection has no evidence either rule could use anyway.
///
/// Two origin rules, evaluated per candidate owner:
/// - **ParentName template ownership**: this owner's own copy of the def
///   carries a `ParentName` naming a `Name`-attributed template this same
///   owner also owns (`indices.template_owners`) — a mod that ships both
///   a def and its own immediate parent template together is very likely
///   the def's original author. Can independently match more than one
///   owner (two owners each shipping their own same-named template);
///   ambiguity from that is resolved by the "exactly one owner qualifies"
///   rule below, over the *union* of both rules' results, not by this
///   rule in isolation.
/// - **Def-name prefix** ([`def_name_family_prefix`]): this owner owns strictly
///   more `Defs/`-inline defs **of any def type** sharing the overridden
///   def's own name prefix than each of the def's *other* owners (a
///   deliberately narrow reading of "than every other owner": scoped to
///   the def's own owner set, not every mod on the install that happens to
///   share the prefix — the override in question is only ever between this
///   def's own owners). **Not** scoped to the def's own `def_type`: a shared
///   prefix across unrelated types might look like weaker evidence, but
///   scoping by type inverts real cases: a mod's own namespace
///   prefix is precisely a *cross-type* signal (one mod stamps `XV_` on
///   its `ThingDef`s, `FurDef`s, and `GeneDef`s alike), so restricting
///   the count to one type lets a small same-type compat pack outvote the
///   namespace's actual owner. Real example: `FurDef/XV_BarkSkin`,
///   owned by `examplevr.mossveil` (174 `XV_`-prefixed defs
///   across all types — the namespace's obvious owner) and
///   `example.apparelscale.extended` (5 across all types, but 4 of the
///   *narrower* `FurDef`-only count, inverting the origin to the wrong
///   mod). At most one owner can ever satisfy this rule (a strict
///   "more than" comparison has no ties by construction).
///
/// Vanilla owning the *origin* needs no special handling: vanilla already
/// loads first by tier regardless of any edge this producer emits. Vanilla as
/// a **losing** `other` owner does need explicit handling: unfiltered, this
/// producer would order Core/DLC *after* a mod whenever a mod's own
/// prefix-family or ParentName-template count happened to exceed vanilla's
/// (e.g. an exploration-loot mod shipping more `MapGen_`-prefixed
/// `ThingSetMakerDef` overrides than Core — `MapGen_` is a generic
/// English-word prefix, not a namespace, so the heuristic has no way to know
/// the def it's overriding is Core's own). RimWorld cannot load Core/DLC
/// after a mod, so such an edge is not merely weak evidence, it's provably
/// wrong — the analyzer already knows this from `indices.mod_source`, the
/// same way every other owner-set producer in this file filters vanilla out
/// of the *losing* side (`patch_target_edges`, `parent_template_edges`,
/// `assembly_version_precedence_edges`).
#[must_use]
pub fn def_override_after_origin_edges(
    scanned: &[ScannedMod],
    indices: &Indices,
    existing_edges: &[Edge],
) -> Vec<Edge> {
    let declared_or_hard_pairs: HashSet<(ModId, ModId)> = existing_edges
        .iter()
        .filter(|e| matches!(e.strength(), EdgeStrength::Declared | EdgeStrength::Hard))
        .flat_map(|e| {
            [
                (e.after.clone(), e.before.clone()),
                (e.before.clone(), e.after.clone()),
            ]
        })
        .collect();

    // Every distinct (owner, def) pair's own `ParentName` — built once over
    // `scanned` rather than re-scanned per multi-owner def. A mod listing the
    // identical def twice (e.g. two `MayRequire`-gated variant copies —
    // `def_owners` itself is `may_require_satisfied`-filtered, but this map
    // deliberately is not, since it only needs to answer "does *some* copy of
    // this owner's def carry this ParentName", never which copy specifically)
    // prefers a `Some` `ParentName` over a `None` on collision
    // (first-wins-regardless could read a gated-off copy's own missing
    // `ParentName` first and never see a later copy's real one).
    let mut parent_name_by_owner_def: HashMap<(ModId, String, String), Option<String>> =
        HashMap::new();
    for scanned_mod in scanned {
        let id = &scanned_mod.info.id;
        for def in &scanned_mod.defs {
            let key = (id.clone(), def.def_type.clone(), def.def_name.clone());
            match parent_name_by_owner_def.entry(key) {
                std::collections::hash_map::Entry::Vacant(slot) => {
                    slot.insert(def.parent_name.clone());
                }
                std::collections::hash_map::Entry::Occupied(mut slot) => {
                    if slot.get().is_none() && def.parent_name.is_some() {
                        slot.insert(def.parent_name.clone());
                    }
                }
            }
        }
    }

    // prefix -> owner -> how many distinct defs (any def type — see this
    // function's own doc comment for why scoping this to one def type is
    // wrong) sharing that prefix the owner owns — precomputed once over every
    // `def_owners` entry rather than rescanned per candidate.
    let mut prefix_owner_counts: BTreeMap<String, BTreeMap<ModId, usize>> = BTreeMap::new();
    for ((_def_type, def_name), owners) in &indices.def_owners {
        let Some(prefix) = def_name_family_prefix(def_name) else {
            continue;
        };
        let bucket = prefix_owner_counts.entry(prefix.to_string()).or_default();
        for owner in owners {
            *bucket.entry(owner.clone()).or_insert(0) += 1;
        }
    }

    let mut edges = Vec::new();
    let mut seen: SeenEdges = HashSet::new();
    for ((def_type, def_name), owners) in &indices.def_owners {
        let sorted_owners: Vec<ModId> = owners
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if sorted_owners.len() < 2 {
            continue;
        }
        let already_ordered = sorted_owners.iter().enumerate().any(|(i, a)| {
            sorted_owners[i + 1..]
                .iter()
                .any(|b| declared_or_hard_pairs.contains(&(a.clone(), b.clone())))
        });
        if already_ordered {
            continue;
        }

        let prefix = def_name_family_prefix(def_name);
        let prefix_counts = prefix.and_then(|p| prefix_owner_counts.get(p));

        let qualifies_via_parent_template = |owner: &ModId| -> bool {
            let Some(Some(parent_name)) =
                parent_name_by_owner_def.get(&(owner.clone(), def_type.clone(), def_name.clone()))
            else {
                return false;
            };
            indices
                .template_owners
                .get(parent_name)
                .is_some_and(|owners| owners.contains(owner))
        };
        let qualifies_via_prefix = |owner: &ModId| -> bool {
            let Some(counts) = prefix_counts else {
                return false;
            };
            let Some(&own_count) = counts.get(owner) else {
                return false;
            };
            sorted_owners
                .iter()
                .all(|other| other == owner || own_count > counts.get(other).copied().unwrap_or(0))
        };

        let qualifying: Vec<(&ModId, bool)> = sorted_owners
            .iter()
            .filter_map(|owner| {
                let via_parent = qualifies_via_parent_template(owner);
                let via_prefix = qualifies_via_prefix(owner);
                (via_parent || via_prefix).then_some((owner, via_parent))
            })
            .collect();
        let [(origin, via_parent)] = qualifying.as_slice() else {
            continue;
        };
        let origin = (*origin).clone();

        let reason = if *via_parent {
            let parent = parent_name_by_owner_def
                .get(&(origin.clone(), def_type.clone(), def_name.clone()))
                .cloned()
                .flatten()
                .unwrap_or_default();
            format!("owns the '{parent}' template its ParentName names")
        } else {
            format!(
                "owns more of the '{}' name family than its other owners",
                prefix.unwrap_or_default()
            )
        };

        let subject = format!("{def_type}/{def_name}");
        for other in &sorted_owners {
            if *other == origin {
                continue;
            }
            // Never order a vanilla owner *after* another mod — RimWorld
            // cannot load Core/DLC after a mod, and "vanilla is already first
            // by tier" only covers vanilla as the *origin*, not as a losing
            // `other` owner. Every other owner-set producer in this file
            // filters vanilla for the same reason (`patch_target_edges`,
            // `parent_template_edges`, `assembly_version_precedence_edges`).
            if indices
                .mod_source
                .get(other)
                .is_some_and(|s| s.is_vanilla())
            {
                continue;
            }
            push_edge_once(
                &mut edges,
                &mut seen,
                other.clone(),
                origin.clone(),
                EdgeKind::DefOverrideAfterOrigin,
                format!("'{subject}' also defined by {origin}, inferred as the origin ({reason})"),
                Some(subject.clone()),
            );
        }
    }
    edges
}
