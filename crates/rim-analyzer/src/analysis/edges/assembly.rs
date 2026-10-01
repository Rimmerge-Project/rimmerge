//! Hard assembly-reference edges and constraints, DLL ownership, and the
//! assembly-version precedence edge.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::domain::{Constraint, ConstraintStatus, Edge, EdgeKind, LoadOrder, ModId, ScannedMod};

use crate::analysis::indices::Indices;

/// Assembly names that are RimWorld's own managed runtime or the base
/// class library, never a mod's — kept as an explicit, install-independent
/// list rather than inferred, because an owner-count measurement could
/// never tell "the engine" apart from "a mod that happens to be the only
/// one shipping a DLL with this exact name" (a real install's own copies
/// already reach [`is_ignored_assembly`] through `vanilla_names`; this
/// covers the .NET runtime names that never appear there since they're
/// never mod-shipped either). `unityengine`/`system` are handled as
/// prefix checks below instead of list entries, since the game ships many
/// differently-suffixed assemblies under each (`UnityEngine.CoreModule`,
/// `System.Xml`, ...).
const ENGINE_ASSEMBLY_NAMES: &[&str] = &["assembly-csharp", "mscorlib", "netstandard"];

/// Distinct active mods shipping their own copy of an assembly name, at or
/// above which [`is_ignored_assembly`] treats it as a shared library
/// rather than cross-mod ordering evidence — a **backstop** for a name
/// [`designated_provider`] can't resolve (no owner is declared-by-majority
/// as a dependency by the others), measured on a real, ~1,000-active-mod
/// install.
///
/// The measurement: grouping every active mod's shipped assembly names by
/// distinct owner count on that install found a clean gap. Every name
/// behind a real, still-wanted provider/consumer relationship (a
/// referencing mod earning an [`crate::domain::Constraint::AnyOf`] against
/// the actual owners) topped out at 5 owners — a framework mod plus a
/// handful of compatibility patches from other authors that bundle their
/// own copy instead of depending on it (the typical shape: 1 real
/// upstream owner, several downstream mods vendoring their own copy,
/// several more genuinely referencing it without shipping their own).
/// Every name at 8 or more owners was an install-wide-adopted modding
/// library referenced by dozens of otherwise-unrelated authors (the
/// install-wide patching-library case the type-level doc comment on
/// [`assembly_version_precedence_edges`] describes). `6` — one past the
/// highest measured "still preserve this" case (5) — is the smallest
/// value this backstop can take without swallowing that real evidence;
/// it is not a claim of a natural midpoint between 5 and 8, just the
/// tightest safe bound given what was measured.
///
/// An absolute owner count, not a fraction of the install's mod count, on
/// purpose: a name independently shipped by six or more distinct mods is
/// unusual regardless of whether the install has 50 mods or 1,000, so the
/// rule doesn't get looser just because a smaller install has fewer mods
/// to spread ownership across.
///
/// One known, accepted gap from this measurement: a name shipped by
/// exactly one author's own handful of mods (e.g. a small shared
/// dependency across three mods from the same author) sits below this
/// threshold and is therefore *not* excluded. On the measured install
/// that produces no edge (no other active mod references such a name
/// without shipping its own copy), and evidence confined to one author's
/// own portfolio is not "shared by unrelated mods" anyway.
pub(super) const SHARED_LIBRARY_OWNER_THRESHOLD: usize = 6;

/// True when `name` is shipped by [`SHARED_LIBRARY_OWNER_THRESHOLD`] or
/// more distinct active mods — see that constant's own doc comment for
/// the measurement behind the number, and [`designated_provider`]'s own
/// doc comment for why this is only the fallback signal, not the primary
/// one. Used by [`is_ignored_assembly`] only — [`dll_owner_of`] needs no
/// owner-count check of its own: its `let [owner] = owners.as_slice()`
/// match already requires exactly one owner, so a many-owner name simply
/// never matches it, structurally, with nothing extra to check for.
fn is_shared_library(name: &str, assembly_owners: &BTreeMap<String, Vec<ModId>>) -> bool {
    assembly_owners
        .get(name)
        .is_some_and(|owners| owners.len() >= SHARED_LIBRARY_OWNER_THRESHOLD)
}

/// Among `owners` of one shared assembly name, the owner a strict
/// majority of the *other* owners declare a dependency on
/// (`modDependencies`, `loadAfter`, or `forceLoadAfter` — see
/// [`crate::domain::DeclaredOrder::declares_after_or_dependency`]): the
/// install's own observable evidence for which owner is the real,
/// designated upstream source, as opposed to a mod that merely bundles
/// its own copy with no declared relation to anyone. This is the
/// *preferred* signal over [`SHARED_LIBRARY_OWNER_THRESHOLD`]'s owner-count
/// backstop, because owner count alone cannot tell a genuine shared
/// library apart from a small install's own copy of one: a designated
/// provider's own loader mod legitimately ships an *older* copy than mods
/// that bundle a newer, auto-updated build, so "highest version wins"
/// ordering (or an any-of that treats every owner as an interchangeable
/// candidate) is wrong for it regardless of how many mods currently ship
/// it — the failure this function exists to prevent
/// (see [`assembly_version_precedence_edges`] and
/// [`assembly_ref_constraints`]).
///
/// **Measured on the real install**: among names with 6+ owners, the most
/// widely-adopted library there has evidence exactly this strong (34 of
/// 52 other owners declare it — a clear majority, no other candidate
/// above 1); the two next-largest libraries had no such majority (at
/// most 1 of 17, and a tie at 1 of 7) — most consumers of those two never
/// declare a formal dependency on their own provider at all, so
/// [`SHARED_LIBRARY_OWNER_THRESHOLD`]'s owner-count backstop is still
/// doing real work for them, not merely a safety net for a case that
/// never happens.
///
/// A **strict majority**, not merely "the most declared-by count",
/// because a plurality of 1 (the common case when almost nobody declares
/// anything) is not real evidence of a design fact — see the measurement
/// above. Ties at the top (including a tie at zero) return `None` rather
/// than guessing; `owners.len() < 2` also returns `None` (nothing to
/// prefer among).
fn designated_provider<'a>(
    owners: &'a [ModId],
    mod_declared: &HashMap<ModId, crate::domain::DeclaredOrder>,
) -> Option<&'a ModId> {
    if owners.len() < 2 {
        return None;
    }
    let counts: Vec<(&ModId, usize)> = owners
        .iter()
        .map(|candidate| {
            let count = owners
                .iter()
                .filter(|other| *other != candidate)
                .filter(|other| {
                    mod_declared
                        .get(other)
                        .is_some_and(|declared| declared.declares_after_or_dependency(candidate))
                })
                .count();
            (candidate, count)
        })
        .collect();
    let max_count = counts.iter().map(|(_, count)| *count).max()?;
    if max_count == 0 {
        return None;
    }
    let mut leaders = counts.iter().filter(|(_, count)| *count == max_count);
    let (leader, _) = leaders.next()?;
    if leaders.next().is_some() {
        return None; // A tie at the top is ambiguous, not evidence.
    }
    if max_count * 2 > owners.len() - 1 {
        Some(leader)
    } else {
        None
    }
}

pub(super) fn is_ignored_assembly(
    name: &str,
    vanilla_names: &HashSet<String>,
    assembly_owners: &BTreeMap<String, Vec<ModId>>,
) -> bool {
    ENGINE_ASSEMBLY_NAMES.contains(&name)
        || name.starts_with("unityengine")
        || name == "system"
        || name.starts_with("system.")
        || vanilla_names.contains(name)
        || is_shared_library(name, assembly_owners)
}

/// Every distinct assembly name `scanned_mod` references across all its
/// shipped DLLs, mapped to whether *any* of those references is a
/// load-time one. `BTreeMap`, not `HashMap`: iterated by both
/// [`assembly_ref_edges`] and [`assembly_ref_constraints`] in the order
/// that determines this mod's edge/constraint order in the final report,
/// and `HashMap`'s per-process random iteration order would make two runs
/// over the same install produce a differently-ordered report.
fn referenced_assemblies(scanned_mod: &ScannedMod) -> BTreeMap<&str, bool> {
    let mut referenced: BTreeMap<&str, bool> = BTreeMap::new();
    for assembly in &scanned_mod.assemblies {
        for reference in &assembly.references {
            let load_time = referenced.entry(reference.name.as_str()).or_insert(false);
            *load_time |= reference.load_time;
        }
    }
    referenced
}

/// `after` ships a DLL referencing an assembly owned by exactly one other
/// active mod, `before`. An assembly name shipped by *more than one*
/// active mod is never resolved to an edge here — see
/// [`assembly_ref_constraints`], which models that as an any-of
/// constraint instead of a contradictory per-candidate edge.
#[must_use]
pub fn assembly_ref_edges(scanned: &[ScannedMod], indices: &Indices) -> Vec<Edge> {
    let mut edges = Vec::new();
    for scanned_mod in scanned {
        let shipped: HashSet<&str> = scanned_mod
            .assemblies
            .iter()
            .map(|a| a.name.as_str())
            .collect();

        for (reference, load_time) in referenced_assemblies(scanned_mod) {
            if shipped.contains(reference)
                || is_ignored_assembly(
                    reference,
                    &indices.vanilla_assembly_names,
                    &indices.assembly_owners,
                )
            {
                continue;
            }
            let Some(owners) = indices.assembly_owners.get(reference) else {
                continue;
            };
            if owners.len() != 1 {
                continue;
            }
            let owner = &owners[0];
            if *owner == scanned_mod.info.id {
                continue;
            }
            edges.push(Edge {
                after: scanned_mod.info.id.clone(),
                before: owner.clone(),
                kind: EdgeKind::AssemblyRef,
                detail: format!("references assembly '{reference}'"),
                load_time,
                subject: None,
            });
        }
    }
    edges
}

/// `after` ships a DLL referencing an assembly name shipped by more than
/// one active mod — one [`Constraint::AnyOf`] per (referencing mod,
/// ambiguous assembly) pair, satisfied when any one of the candidates
/// loads before `after`. When `designated_provider` can name a single
/// real upstream owner among them, `candidates` holds just that one —
/// satisfied only by the actual provider, not any interchangeable
/// bundler.
#[must_use]
pub fn assembly_ref_constraints(
    scanned: &[ScannedMod],
    indices: &Indices,
    load_order: &LoadOrder,
) -> Vec<Constraint> {
    let mut constraints = Vec::new();
    for scanned_mod in scanned {
        let shipped: HashSet<&str> = scanned_mod
            .assemblies
            .iter()
            .map(|a| a.name.as_str())
            .collect();

        for (reference, load_time) in referenced_assemblies(scanned_mod) {
            if shipped.contains(reference)
                || is_ignored_assembly(
                    reference,
                    &indices.vanilla_assembly_names,
                    &indices.assembly_owners,
                )
            {
                continue;
            }
            let Some(owners) = indices.assembly_owners.get(reference) else {
                continue;
            };
            // Reachable only when `owners.len() < SHARED_LIBRARY_OWNER_THRESHOLD`
            // (`is_ignored_assembly` above already skips the large-shared-
            // library case): when the install's own declarations pick out a
            // designated provider among these owners, point straight at it
            // instead of an any-of across every owner — most of the others
            // just bundle their own copy, with no real bearing on this
            // reference.
            if let Some(provider) = designated_provider(owners, &indices.mod_declared) {
                constraints.push(Constraint::AnyOf {
                    after: scanned_mod.info.id.clone(),
                    assembly: reference.to_string(),
                    candidates: vec![provider.clone()],
                    load_time,
                    status: if load_order.is_before(provider, &scanned_mod.info.id) == Some(true) {
                        ConstraintStatus::Satisfied
                    } else {
                        ConstraintStatus::Violated
                    },
                });
                continue;
            }
            let candidates: Vec<ModId> = owners
                .iter()
                .filter(|id| **id != scanned_mod.info.id)
                .cloned()
                .collect();
            if candidates.len() < 2 {
                continue;
            }
            let satisfied = candidates
                .iter()
                .any(|c| load_order.is_before(c, &scanned_mod.info.id) == Some(true));
            constraints.push(Constraint::AnyOf {
                after: scanned_mod.info.id.clone(),
                assembly: reference.to_string(),
                candidates,
                load_time,
                status: if satisfied {
                    ConstraintStatus::Satisfied
                } else {
                    ConstraintStatus::Violated
                },
            });
        }
    }
    constraints
}

/// Namespace roots whose first `.`-segment is never a real mod's own
/// assembly: the engine's own namespaces, never the "owner" of a type
/// merely because some mod's shipped assembly happens to be named after
/// one of these. The namespace-segment counterpart of
/// [`ENGINE_ASSEMBLY_NAMES`], for the same reason: this is an
/// install-independent, engine-level fact no owner-count measurement
/// could discover.
///
/// Third-party library namespaces are deliberately not listed:
/// [`dll_owner_of`]'s own match already requires *exactly one* mod to
/// ship the candidate assembly name (`let [owner] = owners.as_slice()`)
/// — a name shipped by many mods simply never matches that pattern in
/// the first place, with nothing extra needed to catch it — so no
/// mod-specific name belongs here.
pub const ENGINE_NAMESPACE_ROOTS: &[&str] = &["verse", "rimworld", "unityengine", "system"];

/// The active mod owning `type_name`'s namespace, by shipped DLL name: the
/// longest dot-separated prefix of `type_name` that exactly matches
/// (case-insensitively) an assembly name some active mod ships, when exactly
/// one mod ships it. Never namespace-usage matching — a mod merely
/// *referencing* a namespace in its own defs is not evidence of ownership,
/// only actually shipping the assembly is.
///
/// `pub` because the patch maker's field-inference engine needs this exact
/// rule to classify a reference field as an
/// [`crate::domain::FieldRole`]-shaped item slot even when most of its values
/// are owned outside the reference set, the same way
/// [`uses_type_edges`](super::defs::uses_type_edges) uses it to build
/// `UsesType` edges. Takes `assembly_owners` directly (not the whole
/// [`Indices`]) so a caller that only has that one map —
/// [`SourceIndex::dll_owner_of`](crate::analysis::source_index::SourceIndex::dll_owner_of)
/// is exactly that caller — never needs to construct an [`Indices`] just to
/// ask this one question.
#[must_use]
pub fn dll_owner_of<'a>(
    type_name: &str,
    assembly_owners: &'a BTreeMap<String, Vec<ModId>>,
) -> Option<&'a ModId> {
    let segments: Vec<&str> = type_name.split('.').collect();
    if segments.len() < 2 {
        return None;
    }
    if ENGINE_NAMESPACE_ROOTS.contains(&segments[0].to_lowercase().as_str()) {
        return None;
    }
    for split in (1..segments.len()).rev() {
        let candidate = segments[..split].join(".").to_lowercase();
        if let Some(owners) = assembly_owners.get(&candidate)
            && let [owner] = owners.as_slice()
        {
            return Some(owner);
        }
    }
    None
}

/// Who ships an assembly, as [`assembly_ownership`] and
/// [`namespace_ownership`] find it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssemblyOwnership<'a> {
    /// No active mod ships the assembly (or the type is in an engine namespace
    /// or has no namespace to look up).
    Unowned,
    /// Exactly one active mod ships it.
    Sole(&'a ModId),
    /// Several active mods ship it (at least two).
    Shared(&'a [ModId]),
}

/// Who ships the assembly named exactly `assembly_name` (case-insensitive).
#[must_use]
pub fn assembly_ownership<'a>(
    assembly_name: &str,
    assembly_owners: &'a BTreeMap<String, Vec<ModId>>,
) -> AssemblyOwnership<'a> {
    match assembly_owners
        .get(&assembly_name.to_lowercase())
        .map(Vec::as_slice)
    {
        None | Some([]) => AssemblyOwnership::Unowned,
        Some([owner]) => AssemblyOwnership::Sole(owner),
        Some(owners) => AssemblyOwnership::Shared(owners),
    }
}

/// The active mods shipping the assembly at the *longest* dot-separated prefix
/// of `type_name` that matches (case-insensitively) an assembly name some
/// active mod ships: none, one, or several.
///
/// The strict counterpart of [`dll_owner_of`], for a caller that must tell
/// "no owner" from "ambiguous owner" (a log's stack frame): where
/// `dll_owner_of` falls through an ambiguous longer prefix to a shorter unique
/// one (right for a `UsesType` edge, which only needs some ordering hint), this
/// stops at the longest prefix that names any assembly and reports everyone
/// who ships it. Engine namespace roots and a name with no namespace are
/// [`AssemblyOwnership::Unowned`], as in `dll_owner_of`.
#[must_use]
pub fn namespace_ownership<'a>(
    type_name: &str,
    assembly_owners: &'a BTreeMap<String, Vec<ModId>>,
) -> AssemblyOwnership<'a> {
    let segments: Vec<&str> = type_name.split('.').collect();
    if segments.len() < 2 || ENGINE_NAMESPACE_ROOTS.contains(&segments[0].to_lowercase().as_str()) {
        return AssemblyOwnership::Unowned;
    }
    for split in (1..segments.len()).rev() {
        let ownership = assembly_ownership(&segments[..split].join("."), assembly_owners);
        if ownership != AssemblyOwnership::Unowned {
            return ownership;
        }
    }
    AssemblyOwnership::Unowned
}

/// For each duplicate assembly name, every owner shipping the single highest
/// version among the active owners must load before every owner shipping a
/// strictly lower one — first loaded wins at runtime, so the newest copy must
/// be the one that actually loads first (the relative order among the
/// *other*, non-highest owners is never claimed: none of them can be "the"
/// copy that wins, so there's no evidence about their order relative to each
/// other). Owners tied at the highest version produce no edge between
/// themselves — the analyzer has no basis to prefer one identical-version
/// copy over another. A vanilla (Core/DLC) copy is excluded from
/// consideration entirely — same "duplicate" definition as
/// `super::conflicts::duplicate_assemblies`: a mod's own copy of a
/// vanilla-shipped assembly isn't a duplicate-shipping conflict between two
/// *mods*, and Core/DLC always load first regardless. An owner whose copy's
/// version metadata failed to parse carries no evidence about which copy is
/// newer, so it participates neither as the highest-version copy nor as a
/// lower one ordered before it — unlike
/// `super::conflicts::duplicate_assemblies`'s own `versions` field
/// (informational, so an unparsed version there safely falls back to
/// [`crate::domain::AssemblyVersion::default`]), a missing version here would
/// silently make every real, parsed version "newer" than it, asserting a
/// precedence the scan has no basis for. A version that parses successfully
/// as `0.0.0.0` gets the same exclusion, for a distinct reason: that's
/// `.NET`'s own default `AssemblyVersion` for a DLL built with no explicit
/// `[AssemblyVersion]` attribute (not a parse failure, and coincidentally the
/// same bit pattern [`crate::domain::AssemblyVersion::default`] itself
/// produces), so it carries no more real version evidence than an unparsed
/// one does — treating it as a genuine "oldest possible" version would
/// fabricate a precedence claim (e.g. a mod's `0ExampleColorTool` build without
/// the attribute ordered relative to two other mods' real `1.0.0.0` copy).
///
/// Deliberately narrower than "every pair with differing versions": an
/// install-wide-adopted shared library (see `SHARED_LIBRARY_OWNER_THRESHOLD`
/// — the real install this was measured on has one at 53 owners) would
/// otherwise become a near-total ordering over unrelated mods that merely
/// happen to bundle different builds of the same dependency, which is
/// exactly the over-eager inference the "ownership by shipping, never by
/// use" rule for DLL namespaces cautions against. For the same reason,
/// this excludes every [`is_ignored_assembly`] name — the same check
/// [`assembly_ref_edges`] excludes from cross-mod dependency edges: on a
/// real install nearly every edge and violation would otherwise trace to
/// just the two or three most widely-adopted libraries.
///
/// **Never subordinates a `designated_provider`**, even for a name
/// `is_ignored_assembly` doesn't already exclude outright (a small
/// install with too few owners yet to trip
/// `SHARED_LIBRARY_OWNER_THRESHOLD`): "highest version wins" is wrong by
/// convention for a library with a designated single provider, since that
/// provider's own loader mod can legitimately ship an *older* copy than
/// mods that bundle a newer, auto-updated build — asserting the naive
/// direction would order the provider after a mod that merely happens to
/// auto-update its bundled reference build. No edge is ever emitted with
/// the provider on the losing (`after`) side; edges among the *other*
/// owners of the same name are untouched, since nothing here claims
/// anything about their relative order.
#[must_use]
pub fn assembly_version_precedence_edges(indices: &Indices) -> Vec<Edge> {
    let mut edges = Vec::new();
    for (name, owners) in &indices.assembly_owners {
        if is_ignored_assembly(
            name,
            &indices.vanilla_assembly_names,
            &indices.assembly_owners,
        ) {
            continue;
        }
        let non_vanilla_owners: Vec<&ModId> = owners
            .iter()
            .filter(|id| !indices.mod_source.get(*id).is_some_and(|s| s.is_vanilla()))
            .collect();
        if non_vanilla_owners.len() < 2 {
            continue;
        }
        // Computed from the raw, pre-vanilla-filtered `owners` (matching
        // `assembly_ref_constraints`'s own call), since a designated
        // provider is never vanilla anyway and this keeps both call sites
        // asking the identical question of the identical owner list.
        let provider = designated_provider(owners, &indices.mod_declared);
        // `filter_map`, not `map` with `unwrap_or_default`: an owner with
        // no parsed version is dropped from consideration entirely, per
        // this function's own doc comment. `0.0.0.0` gets the same
        // treatment as `None`: it's the CLR's default `AssemblyVersion` for
        // a DLL built with no explicit `[AssemblyVersion]` attribute, not a
        // real, strictly-lower version — treating it as one fabricates a
        // precedence claim the metadata never actually made (e.g.
        // `example.zonegate`'s `0ExampleColorTool` build vs. two other mods'
        // real `1.0.0.0` copy). `DuplicateAssembly::versions` still reports
        // the raw `0.0.0.0` value untouched; only edge inference ignores it.
        let versioned: Vec<(ModId, crate::domain::AssemblyVersion)> = non_vanilla_owners
            .iter()
            .filter_map(|id| {
                let version = *indices
                    .assembly_versions
                    .get(&((**id).clone(), name.clone()))?;
                if version == crate::domain::AssemblyVersion::default() {
                    return None;
                }
                Some(((*id).clone(), version))
            })
            .collect();
        let Some(highest_version) = versioned.iter().map(|(_, version)| *version).max() else {
            continue;
        };

        for (highest_id, _) in versioned.iter().filter(|(_, v)| *v == highest_version) {
            for (lower_id, lower_version) in &versioned {
                if *lower_version >= highest_version {
                    continue;
                }
                if provider == Some(lower_id) {
                    // Never subordinate the designated provider to a
                    // bundler on version grounds — see this function's
                    // own doc comment.
                    continue;
                }
                edges.push(Edge {
                    after: lower_id.clone(),
                    before: highest_id.clone(),
                    kind: EdgeKind::AssemblyVersionPrecedence,
                    detail: format!(
                        "ships an older copy of '{name}' than {highest_id} (first loaded wins)"
                    ),
                    load_time: true,
                    subject: None,
                });
            }
        }
    }
    edges
}
