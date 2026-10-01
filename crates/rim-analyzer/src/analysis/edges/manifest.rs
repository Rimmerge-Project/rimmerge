//! Manifest-order edges: resolving a RimSort-style manifest's names against the active set.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::domain::{Edge, EdgeKind, ModId, ScannedMod, Warning};

use super::patches::{SeenEdges, push_edge_once};
use crate::analysis::indices::{ActiveMods, DisplayNameIndex};

/// Strips a community-manager-style version-bound suffix (`"ExampleFramework
/// >= 5.5.0"` -> `"Example Framework"`) from a Manifest.xml entry before
/// resolution — real entries carry one, and the bound itself is not part of
/// the mod's name or packageId, so left in place it makes every such entry
/// unresolvable (and can lose a genuine `ModDependency` edge). Splits on the
/// first of `>`, `<`, `=` — the only three comparison operators that format
/// uses — and trims; a raw string
/// with none of the three passes through unchanged. Only used for matching:
/// `detail`/warning text still shows the caller's own original `raw`.
fn strip_version_bound(raw: &str) -> &str {
    raw.split(['>', '<', '=']).next().unwrap_or(raw).trim()
}

/// Normalizes a display name for [`ManifestFallbackIndex`]'s tolerant
/// lookup: lowercased, with every non-alphanumeric character dropped, so
/// `"ExampleExoGenes"`, `"Example Exo Genes"` and `"example-exo-genes"`
/// all share one key. Lowercases *before* filtering, so a combining mark
/// that only appears once a character is lowercased (Turkish `İ` ->
/// `i` + U+0307) is dropped like any other non-alphanumeric rather than
/// surviving into the key. A name with no alphanumeric character at all
/// normalizes to the empty string, which [`ManifestFallbackIndex::build`]
/// never stores — matching on "nothing in common" would be the opposite
/// of a unique match.
fn normalize_display_name(name: &str) -> String {
    name.chars()
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// How one Manifest.xml entry resolved.
#[derive(Debug)]
enum ManifestMatch {
    /// Exactly one active mod, by any of the four lookups.
    One(ModId),
    /// Two or more active mods claim the key a fallback matched on —
    /// deliberately *not* resolved, see [`ManifestFallbackIndex`].
    Ambiguous(Vec<ModId>),
    Unresolved,
}

/// The two tolerant fallbacks [`resolve_manifest_name`] tries only once
/// an exact packageId and an exact display-name lookup have both failed:
/// a [`normalize_display_name`]d display name, and a bare Steam Workshop
/// id matched against the mod folder's own name.
///
/// Both map a key to *every* active mod claiming it, not just the first.
/// RimWorld itself never reads `Manifest.xml` — these entries are author
/// statements this analyzer chooses to trust, emitted at `Declared`
/// strength — so resolving one wrongly fabricates an author declaration
/// nobody made. That is why a key claimed by two active mods resolves to
/// neither (warning instead, naming both), and why the normalization
/// only ever runs after the two exact lookups: an exact display-name hit
/// must keep winning over a normalized hit on a different mod.
///
/// Every id is mapped through [`ActiveMods::resolve`] before it is stored, so
/// a mod with both a local and a Workshop copy installed (scanned as two
/// `ScannedMod`s, `dup.mod` and `dup.mod_steam`, sharing one display name —
/// `tests/steam_suffix_scan.rs`) claims each key *once*, under the one id
/// `active` resolves its base to. Without that, the commonest duplicate on a
/// real install would read as an ambiguity about what is one mod.
///
/// Keyed on the mod folder's final path component rather than
/// [`Mod::workshop_id`](crate::domain::Mod::workshop_id): that field is
/// `None` for a *local* copy of a Workshop mod (see
/// `infra::discovery::workshop_id_of`), which keeps the numeric folder
/// name — the case this keying guards against, not one observed on this
/// install (every numeric-folder mod here is `Source::Workshop`).
#[derive(Debug, Default)]
struct ManifestFallbackIndex {
    by_normalized_name: BTreeMap<String, BTreeSet<ModId>>,
    by_workshop_folder: BTreeMap<String, BTreeSet<ModId>>,
}

impl ManifestFallbackIndex {
    /// Built once per [`manifest_order_edges`] run, over every scanned
    /// (therefore active) mod.
    fn build(scanned: &[ScannedMod], active: &ActiveMods) -> Self {
        let mut index = Self::default();
        for scanned_mod in scanned {
            let own_id = &scanned_mod.info.id;
            let id = active.resolve(own_id).unwrap_or(own_id);
            let normalized = normalize_display_name(&scanned_mod.info.name);
            if !normalized.is_empty() {
                index
                    .by_normalized_name
                    .entry(normalized)
                    .or_default()
                    .insert(id.clone());
            }
            let folder = scanned_mod
                .info
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            if is_workshop_id(folder) {
                index
                    .by_workshop_folder
                    .entry(folder.to_string())
                    .or_default()
                    .insert(id.clone());
            }
        }
        index
    }

    /// A normalized display name. An entry that normalizes to the empty
    /// string can only miss: [`Self::build`] stores no such key.
    fn by_name(&self, name: &str) -> ManifestMatch {
        unique_match(self.by_normalized_name.get(&normalize_display_name(name)))
    }

    fn by_folder(&self, entry: &str) -> ManifestMatch {
        if !is_workshop_id(entry) {
            return ManifestMatch::Unresolved;
        }
        unique_match(self.by_workshop_folder.get(entry))
    }
}

/// Whether a string is a bare Steam Workshop published-file id: ASCII
/// digits only, and at least one of them.
fn is_workshop_id(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// One candidate resolves; two or more are an ambiguity, never a guess.
fn unique_match(candidates: Option<&BTreeSet<ModId>>) -> ManifestMatch {
    let Some(candidates) = candidates else {
        return ManifestMatch::Unresolved;
    };
    let mut ids = candidates.iter();
    match (ids.next(), ids.next()) {
        (Some(only), None) => ManifestMatch::One(only.clone()),
        (Some(_), Some(_)) => ManifestMatch::Ambiguous(candidates.iter().cloned().collect()),
        (None, _) => ManifestMatch::Unresolved,
    }
}

/// Resolves one Manifest.xml entry (which may name another mod by its
/// display name or its packageId, optionally with a
/// [`strip_version_bound`]-stripped version bound — see
/// `extract::manifest_xml`'s own doc comment), in strictly decreasing
/// order of certainty:
///
/// 1. a packageId match (`_steam`-suffix- and case-aware, via
///    [`ActiveMods::resolve`]);
/// 2. an exact display-name match (lowercased, the same key `name_map`
///    itself uses — and so inheriting `build_name_map`'s "first mod to
///    claim a name wins" tie-break, which only the two fallbacks below
///    refuse to make);
/// 3. for an all-digit entry only, [`ManifestFallbackIndex`]'s
///    Workshop-id folder — tried before the normalized name because a
///    bare published-file id is never a display name, and because an
///    *ambiguous* normalized hit must not shadow a unique folder one;
/// 4. its normalized display name.
///
/// Only steps 1 and 2 are `_steam`-aware by construction; 3 and 4 get
/// there by indexing every mod under the id [`ActiveMods::resolve`] gives
/// its base (see [`ManifestFallbackIndex`]). Steps 3 and 4 resolve only
/// on a unique match.
fn resolve_manifest_name(
    raw: &str,
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
    fallback: &ManifestFallbackIndex,
) -> ManifestMatch {
    let name = strip_version_bound(raw);
    if let Some(id) = active.resolve(&ModId::new(name)) {
        return ManifestMatch::One(id.clone());
    }
    if let Some(id) = name_map.get(name) {
        return ManifestMatch::One(id.clone());
    }
    match fallback.by_folder(name) {
        ManifestMatch::Unresolved => fallback.by_name(name),
        resolved => resolved,
    }
}

/// Resolve-and-record for one [`manifest_order_edges`] run: resolves each
/// entry against the mods scanned this run and accumulates at most one
/// [`Warning`] per `(mod, raw)` entry that fails to. Deliberately fused
/// (a query that also records) rather than split into two passes — the
/// warning's wording depends on *how* the resolution failed, which only
/// the resolution itself knows; the method is named for both halves.
struct ManifestResolver<'a> {
    active: &'a ActiveMods,
    name_map: &'a DisplayNameIndex,
    fallback: ManifestFallbackIndex,
    warned: HashSet<(ModId, String)>,
    warnings: Vec<Warning>,
}

impl<'a> ManifestResolver<'a> {
    fn new(scanned: &[ScannedMod], active: &'a ActiveMods, name_map: &'a DisplayNameIndex) -> Self {
        Self {
            active,
            name_map,
            fallback: ManifestFallbackIndex::build(scanned, active),
            warned: HashSet::new(),
            warnings: Vec::new(),
        }
    }

    fn resolve_or_warn(&mut self, mod_id: &ModId, raw: &str) -> Option<ModId> {
        match resolve_manifest_name(raw, self.active, self.name_map, &self.fallback) {
            ManifestMatch::One(target) => Some(target),
            ManifestMatch::Ambiguous(candidates) => {
                self.warn(
                    mod_id,
                    raw,
                    ambiguous_manifest_warning(mod_id, raw, &candidates),
                );
                None
            }
            ManifestMatch::Unresolved => {
                self.warn(mod_id, raw, unresolved_manifest_warning(mod_id, raw));
                None
            }
        }
    }

    fn warn(&mut self, mod_id: &ModId, raw: &str, warning: Warning) {
        if self.warned.insert((mod_id.clone(), raw.to_string())) {
            self.warnings.push(warning);
        }
    }
}

/// `About/Manifest.xml`'s own `loadAfter`/`loadBefore`/`dependencies`
/// entries, resolved ([`resolve_manifest_name`]) and emitted as the same
/// `LoadAfter`/`LoadBefore`/`ModDependency` kinds
/// [`declared_edges`](super::declared::declared_edges) (from `About.xml`)
/// already produces — `detail` ends "(Manifest.xml)" so the two sources stay
/// distinguishable even once collapsed. A rule present in both files dedupes
/// to one edge via [`push_edge_once`], seeded from `existing_edges` (expected
/// to be `declared_edges`'s own output, the only other producer of these
/// three kinds — `report_builder::collect_edges` calls `declared_edges`
/// first, so the surviving edge is always `About.xml`'s own, the Manifest.xml
/// duplicate silently dropped, exactly the same-fact-twice case
/// `push_edge_once` exists for everywhere else in this module).
///
/// An entry that resolves to no active mod at all — or to two of them at once
/// via a [`ManifestFallbackIndex`] key they share, an ambiguity only the two
/// *fallback* tiers refuse to guess at (the exact display-name tier keeps
/// `build_name_map`'s own "first mod to claim the name wins" tie-break, which
/// already warned when it fired) — is reported as a [`Warning`] rather than
/// silently dropped — mirrors `find_mod_edges`'s own
/// `FindModResolution::unresolved`, though as a plain `Warning` here since no
/// dedicated report field like `unresolved_find_mod_names` exists for this
/// producer (a judgment call: `find_mod_edges`'s own structured field exists
/// specifically to keep `Report.warnings` about *scan* problems, not "an
/// optional compat mod isn't installed" — the same shape most unresolved
/// Manifest.xml entries turn out to be on a real install). Deduplicated per
/// `(mod, raw)`: the identical raw entry can appear in more than one of this
/// mod's own lists (e.g. both `loadAfter` and `dependencies` naming the same
/// optional mod), which must warn once, not once per list.
#[must_use]
pub fn manifest_order_edges(
    scanned: &[ScannedMod],
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
    existing_edges: &[Edge],
) -> (Vec<Edge>, Vec<Warning>) {
    let mut seen: SeenEdges = existing_edges
        .iter()
        .map(|e| (e.after.clone(), e.before.clone(), e.kind))
        .collect();
    let mut resolver = ManifestResolver::new(scanned, active, name_map);
    let mut edges = Vec::new();

    for scanned_mod in scanned {
        let id = &scanned_mod.info.id;
        let order = &scanned_mod.manifest_order;

        // `id` must load after each of these entries — the same
        // `(entry, kind, verb)` chaining `declared_edges` uses for
        // `About.xml`'s own after-shaped lists.
        let after_entries = order
            .load_after
            .iter()
            .map(|raw| (raw, EdgeKind::LoadAfter, "loadAfter"))
            .chain(
                order
                    .dependencies
                    .iter()
                    .map(|raw| (raw, EdgeKind::ModDependency, "a dependency on")),
            );
        for (raw, kind, verb) in after_entries {
            let Some(resolved) = resolver.resolve_or_warn(id, raw) else {
                continue;
            };
            // Compared on `ModId::base()`: a mod installed both locally and
            // from the Workshop names itself with neither copy's exact id,
            // and `foo_steam -> foo` is a self-edge, not a real constraint.
            if resolved.base() == id.base() {
                continue;
            }
            push_edge_once(
                &mut edges,
                &mut seen,
                id.clone(),
                resolved,
                kind,
                format!("{id} declares {verb} {raw} (Manifest.xml)"),
                None,
            );
        }
        for raw in &order.load_before {
            let Some(resolved) = resolver.resolve_or_warn(id, raw) else {
                continue;
            };
            if resolved.base() == id.base() {
                continue;
            }
            push_edge_once(
                &mut edges,
                &mut seen,
                resolved,
                id.clone(),
                EdgeKind::LoadBefore,
                format!("{id} declares loadBefore {raw} (Manifest.xml)"),
                None,
            );
        }
    }
    (edges, resolver.warnings)
}

fn unresolved_manifest_warning(mod_id: &ModId, raw: &str) -> Warning {
    Warning::new(
        Some(mod_id.clone()),
        format!(
            "Manifest.xml names '{raw}', which resolves to no active mod's packageId or display name"
        ),
    )
}

/// The ambiguous counterpart of [`unresolved_manifest_warning`]: the
/// entry matched a fallback key two or more active mods share, so it
/// resolves to none of them and names all of them instead — see
/// [`ManifestFallbackIndex`] for why guessing is not an option here.
fn ambiguous_manifest_warning(mod_id: &ModId, raw: &str, candidates: &[ModId]) -> Warning {
    let named = candidates
        .iter()
        .map(ModId::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    Warning::new(
        Some(mod_id.clone()),
        format!(
            "Manifest.xml names '{raw}', which matches more than one active mod ({named}) — left unresolved rather than guessing"
        ),
    )
}
