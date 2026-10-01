//! Texture overrides and texture paths no active mod ships.

use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{
    Conflict, LoadOrder, MissingTexturePath, ModId, ScannedMod, TextureOverride, UndecodableTexture,
};
use crate::extract::graphics::{MASK_SUFFIX, names_collection_graphic, names_rotation_face};

use super::duplicate_mods::active_def_keys_of;
use super::sorted_by_load_order;
use crate::analysis::indices::{ActiveMods, Indices, MIN_CORE_RESOURCE_TEXTURES};

/// The same normalized texture path shipped by more than one active mod.
#[must_use]
pub fn texture_overrides(indices: &Indices, load_order: &LoadOrder) -> Vec<Conflict> {
    indices
        .texture_owners
        .iter()
        .filter(|(_, owners)| owners.len() > 1)
        .map(|(path, owners)| {
            Conflict::TextureOverride(TextureOverride {
                texture_path: path.clone(),
                same_author: indices.shared_author(owners),
                owners: sorted_by_load_order(owners, load_order),
            })
        })
        .collect()
}

/// Normalizes a raw `texPath`-family field's text value the same way
/// [`crate::extract::textures::normalize`] normalizes a shipped file's
/// own relative path (lowercased, `/`-separated) — there is no extension
/// to strip here, a `texPath` value never carries one.
fn normalize_texture_path_text(path: &str) -> String {
    path.replace('\\', "/").to_lowercase()
}

/// Whether `normalized_path` (already run through
/// [`normalize_texture_path_text`]) names a texture some active mod ships.
/// Three shapes all count as "ships", each measured against a real install:
/// - **Exact**: the plain file case.
/// - **`path_A`/`path_east`-style variant**: RimWorld resolves a
///   `Graphic_Multi`/`Graphic_Random`/rotation-aware graphic's base
///   `texPath` against a *family* of files suffixed `_north`/`_south`/
///   `_east`/`_west`/`_a`/`_b`/... — the exact base path itself is never
///   shipped in that case.
/// - **Folder container**: a `texPath` naming a directory (not a file
///   stem) whose own contents RimWorld loads regardless of their own
///   file name — a real, common authoring convention (one texture per
///   uniquely-named subfolder, e.g. `.../Aetheophyllum/Aetheophyllum.dds`
///   for a `texPath` of `.../Aetheophyllum`, as in a real biomes mod's own
///   plant/animal textures). **Only a
///   *direct*, non-mask child counts** — see below.
///
/// Each check is a `BTreeMap` range/prefix lookup against
/// `Indices.texture_owners`' own sorted keys, not a substring search —
/// cheap even at the real install's tens of thousands of distinct
/// texture paths.
///
/// **Not used for a `Graphic_Collection` `texPath` at all**: that case is
/// decided by [`collection_graphic_rows`], which models
/// `Verse.Graphic_Collection.Init` directly instead of asking a yes/no "does
/// it resolve" question. `Init` only ever enumerates
/// `GetAllUnderPath("{path}/")`, so for a collection graphic neither an exact
/// file at the path nor a `path_A`-style sibling is even looked at — the two
/// short-circuits above would wrongly rescue both.
fn texture_path_resolves(indices: &Indices, normalized_path: &str) -> bool {
    resolves_in_map(&indices.texture_owners, normalized_path)
        || resolves_in_set(&indices.non_loose_textures, normalized_path)
}

/// The exact/variant/folder resolution rule [`texture_path_resolves`]' own
/// doc comment describes, over `Indices.texture_owners`' own key space.
fn resolves_in_map(map: &BTreeMap<String, Vec<ModId>>, normalized_path: &str) -> bool {
    if map.contains_key(normalized_path) {
        return true;
    }
    let variant_prefix = format!("{normalized_path}_");
    let has_variant = map
        .range(variant_prefix.clone()..)
        .next()
        .is_some_and(|(key, _)| key.starts_with(&variant_prefix));
    if has_variant {
        return true;
    }
    let folder_prefix = format!("{normalized_path}/");
    map.range(folder_prefix.clone()..)
        .next()
        .is_some_and(|(key, _)| key.starts_with(&folder_prefix))
}

/// [`resolves_in_map`]'s identical rule, over `Indices.non_loose_textures`'
/// own key space (bundle and Core-resource keys, which carry no owner
/// list) — kept as a second small function rather than a shared generic
/// over both container types, since `BTreeMap::range`/`BTreeSet::range`
/// don't share a common trait to abstract over here.
fn resolves_in_set(set: &BTreeSet<String>, normalized_path: &str) -> bool {
    if set.contains(normalized_path) {
        return true;
    }
    let variant_prefix = format!("{normalized_path}_");
    let has_variant = set
        .range(variant_prefix.clone()..)
        .next()
        .is_some_and(|key| key.starts_with(&variant_prefix));
    if has_variant {
        return true;
    }
    let folder_prefix = format!("{normalized_path}/");
    set.range(folder_prefix.clone()..)
        .next()
        .is_some_and(|key| key.starts_with(&folder_prefix))
}

/// The `<graphicClass>` every `Name`-attributed template declares, so a
/// candidate that declares only its own `texPath` can be resolved through
/// its `ParentName` chain.
///
/// **Why this exists**: reading only the `graphicClass` written literally
/// beside the `texPath` misses the dominant authoring shape. `XmlInheritance`
/// merges a template's `<graphicData>` into the child's, so vanilla's own
/// `PlantBase`/`StoneBase`-family templates declare
/// `<graphicClass>Graphic_Random</graphicClass>` once and hundreds of
/// concrete defs — vanilla's and every mod's — inherit it while writing only
/// a `texPath`. Without the chain walk the strict collection rule would
/// silently skip all of them.
///
/// An ambiguous name (two active mods registering the same `Name` with
/// *different* classes) resolves to `None` rather than a guess: the
/// collection rule produces findings, so an unresolved class must fall
/// back to the lenient path, never to a fabricated one.
struct GraphicClassIndex<'a> {
    /// `Name` -> the class that name resolves to, and the name it in turn
    /// inherits from. `None` class means "declared nothing, or two
    /// registrants disagree".
    by_name: BTreeMap<&'a str, (Option<&'a str>, Option<&'a str>)>,
}

/// How far [`GraphicClassIndex::resolve`] walks a `ParentName` chain
/// before giving up. RimWorld's own chains are a handful of links deep at
/// most; this is a cycle guard (`XmlInheritance` logs `Cyclic inheritance
/// hierarchy detected` for a real cycle and drops the def, so a cycle
/// here means the def never loads anyway), not a real limit.
const MAX_TEMPLATE_CHAIN_DEPTH: usize = 32;

impl<'a> GraphicClassIndex<'a> {
    fn build(scanned: &'a [ScannedMod]) -> Self {
        let mut by_name: BTreeMap<&str, (Option<&str>, Option<&str>)> = BTreeMap::new();
        for scanned_mod in scanned {
            for template in &scanned_mod.templates {
                let class = template.graphic_class.as_deref();
                let parent = template.parent_name.as_deref();
                by_name
                    .entry(&template.name)
                    .and_modify(|existing| {
                        if existing.0 != class {
                            existing.0 = None;
                        }
                        if existing.1 != parent {
                            existing.1 = None;
                        }
                    })
                    .or_insert((class, parent));
            }
        }
        Self { by_name }
    }

    /// Whether `candidate`'s own graphic is a [`crate::extract::graphics::COLLECTION_GRAPHIC_CLASSES`]
    /// one — declared beside the field, or inherited through the owning
    /// def's `ParentName` chain.
    ///
    /// Inheritance is only consulted for a field that actually sits in a
    /// `<graphicData>` block ([`TexturePathCandidate::container_tag`]):
    /// a `texPath` a mod's own def type reads, or one nested in a comp
    /// list, shares nothing with the def's graphic and must not pick up a
    /// template's class.
    fn is_collection_graphic(
        &self,
        scanned_mod: &ScannedMod,
        candidate: &crate::domain::TexturePathCandidate,
    ) -> bool {
        if let Some(class) = candidate.graphic_class.as_deref() {
            return names_collection_graphic(class);
        }
        if candidate.container_tag.as_deref() != Some("graphicData") {
            return false;
        }
        let parent = scanned_mod
            .defs
            .iter()
            .find(|def| def.def_type == candidate.def_type && def.def_name == candidate.def_name)
            .and_then(|def| def.parent_name.as_deref());
        self.resolve(parent).is_some_and(names_collection_graphic)
    }

    /// The `graphicClass` the chain starting at `name` declares, nearest
    /// link first.
    fn resolve(&self, name: Option<&str>) -> Option<&'a str> {
        let mut current = name?;
        for _ in 0..MAX_TEMPLATE_CHAIN_DEPTH {
            let (class, parent) = self.by_name.get(current)?;
            if let Some(class) = class {
                return Some(class);
            }
            current = (*parent)?;
        }
        None
    }
}

/// Every `texPath`/`texPathFemale`/`iconPath`/`uiIconPath` field value naming
/// a texture no active mod ships — see [`texture_path_resolves`] for what
/// counts as "ships". Diagnostic only, never an ordering fact.
///
/// **Vanilla (Core/DLC) referrers are excluded**: a real-install run surfaces
/// ~2,200 vanilla-owned candidates this way, almost entirely RimWorld
/// 1.5/1.6's own procedurally-composited content (book/document covers, gene
/// and `DrawStyleDef` UI icons) whose `texPath` is a shader/material
/// parameter, never a literal file — genuinely unresolvable by a static
/// existence check, and not the kind of gap this diagnostic is for (vanilla
/// ships and QAs its own content; the real cases this is for — a biomes mod,
/// a body-sizes mod — are third-party mods). The same exclusion
/// [`likely_duplicate_mods`](crate::analysis::conflicts::duplicate_mods::likely_duplicate_mods)
/// already applies, for the same underlying reason: "RimWorld's own content
/// isn't a [gap] of anything."
#[must_use]
pub fn missing_texture_paths(
    scanned: &[ScannedMod],
    indices: &Indices,
    active: &ActiveMods,
) -> Vec<Conflict> {
    // The fail-safe: without a usable Core resource index, this check
    // runs against a near-empty universe and is ~99.6% false positives,
    // so it disables itself entirely rather than
    // running degraded. `analysis::checks::core_resource_index_warning`
    // is the sibling function that surfaces this as a [`Warning`], from the
    // same count this reads.
    if indices.core_resource_texture_count < MIN_CORE_RESOURCE_TEXTURES {
        return Vec::new();
    }

    let graphic_classes = GraphicClassIndex::build(scanned);
    let mut conflicts = Vec::new();
    for scanned_mod in scanned {
        if scanned_mod.info.source.is_vanilla() {
            continue;
        }
        let active_def_keys = active_def_keys_of(scanned_mod, active);
        for candidate in &scanned_mod.texture_path_candidates {
            if !active_def_keys
                .contains(&(candidate.def_type.as_str(), candidate.def_name.as_str()))
            {
                continue;
            }
            // A `{`/`[`-bearing value is a format string or encoded data,
            // not a literal path — never a candidate to flag.
            if candidate.path.contains('{') || candidate.path.contains('[') {
                continue;
            }
            let normalized = normalize_texture_path_text(&candidate.path);
            let mut row = |path: String| {
                Conflict::MissingTexturePath(MissingTexturePath {
                    referrer: scanned_mod.info.id.clone(),
                    def_type: candidate.def_type.clone(),
                    def_name: candidate.def_name.clone(),
                    field: candidate.field.clone(),
                    path,
                })
            };
            // A collection graphic's own folder model comes first: for
            // it, neither an exact file at the path nor a `path_A`
            // sibling is part of `GetAllUnderPath`'s enumeration, so
            // `texture_path_resolves`' short-circuits must not rescue it.
            // `None` means the enumeration came back empty, which this
            // index cannot distinguish from vanilla-provided content —
            // see `collection_graphic_rows`' own doc comment — so that
            // one case falls through to the ordinary check.
            if graphic_classes.is_collection_graphic(scanned_mod, candidate)
                && let Some(rows) = collection_graphic_rows(indices, &normalized)
            {
                conflicts.extend(rows.into_iter().map(&mut row));
                continue;
            }
            if !texture_path_resolves(indices, &normalized) {
                conflicts.push(row(normalized));
            }
        }
    }
    conflicts
}

/// Every texture path a `Graphic_Collection` `texPath` will fail on,
/// modelled straight off `Verse.Graphic_Collection.Init` — one entry per
/// path the engine actually attempts, so each row matches one game-log
/// line. Empty when the graphic initializes cleanly.
///
/// `Init` enumerates `ContentFinder<Texture2D>.GetAllInFolder(req.path)`,
/// which is `ModContentHolder.GetAllUnderPath` — a **recursive** trie
/// scan of the `"{path}/"` prefix, and *only* that prefix: an exact file
/// at `path` itself, or a `path_A` sibling, is never part of a
/// collection's own enumeration. Masks (`Graphic_Single.MaskSuffix`,
/// `_m`) are filtered out before anything else. Then:
/// - **nothing left** -> the engine would log
///   `Collection cannot init: No textures found at path`, but this
///   returns `None` rather than a row. See the blind spot below.
/// - otherwise, each remaining texture becomes
///   `req.path + "/" + texture.name` — the file **name** only, any
///   subfolder segment dropped — and is loaded through
///   `GraphicDatabase.Get`, which logs
///   `Could not load Texture2D at '<that path>' for def '<def>'` when it
///   does not exist. One row per distinct such path.
///
/// **Real-install validation**: five defs have a collection-graphic folder
/// holding subfolder textures, all in `examplebiomes.biomes`, and in four of
/// them every subfolder file's name is *shadowed* by a same-named direct
/// sibling (`Crinoid/Dry/Crinoid_A` beside `Crinoid/Crinoid_A`), so the
/// flattened lookup finds the sibling and succeeds. Only `Sigillaria`'s
/// subfolder names have no direct counterpart, and this predicts exactly its
/// 7 attempted paths (`SigillariaGrown_A`..`_E`, `SigillariaImmature_A`/`_B`)
/// — bit for bit the 7 lines the game log carries, and no others. That
/// shadowing case is why this asks whether the *flattened* path exists rather
/// than merely whether a key is nested.
///
/// **The blind spot, and why an empty enumeration is not reported**: **Core
/// and the DLCs ship no loose texture files at all** — `RimWorld/Data/Core`
/// has only `About`/`Defs`/`Languages`, and vanilla art is reached through
/// `ContentFinder`'s own `Resources.LoadAll` fallback, which this analyzer
/// cannot see. `Indices.texture_owners` therefore holds mod files only, and
/// *every* vanilla-provided collection folder looks empty here. Reporting
/// that emptiness produces exactly two rows on a real install — a
/// mod's `Example_HiveA` and another mod's `Example_HiveB`, both pointing at
/// vanilla's own
/// `Things/Building/Natural/Hive` `Graphic_Random` folder — and the game log
/// contains neither, nor any other `Collection cannot init` line. So an empty
/// enumeration returns `None` and the caller falls back to the ordinary
/// [`texture_path_resolves`] check, which reports only when nothing matches
/// anywhere. The per-file predictions below are unaffected: they need a real
/// enumerated mod file to fire at all, so the blind spot cannot fabricate
/// one.
///
/// One deliberate narrowing, from `Init` itself: a name carrying a
/// rotation suffix (`_north`/`_south`/`_east`/`_west`) is pulled out of
/// the per-file loop into a single `Graphic_Multi` keyed on the group
/// name, which resolves its own `_north`-style variants — a different
/// lookup this does not model, so such names count towards the folder
/// being non-empty but are never predicted as a failing path.
fn collection_graphic_rows(indices: &Indices, folder_path: &str) -> Option<BTreeSet<String>> {
    let folder_prefix = format!("{folder_path}/");
    let mut enumerated: Vec<&str> = indices
        .texture_owners
        .range(folder_prefix.clone()..)
        .take_while(|(key, _)| key.starts_with(&folder_prefix))
        .filter_map(|(key, _)| key[folder_prefix.len()..].rsplit('/').next())
        .filter(|file_name| !file_name.ends_with(MASK_SUFFIX))
        .collect();
    // The non-loose universe (asset bundles, Core resources) enumerates
    // under the same folder prefix — a subfolder file served only from a
    // bundle still counts towards the folder being non-empty, and the same
    // file-name flattening applies to it (see this function's own doc
    // comment).
    enumerated.extend(
        indices
            .non_loose_textures
            .range(folder_prefix.clone()..)
            .take_while(|key| key.starts_with(&folder_prefix))
            .filter_map(|key| key[folder_prefix.len()..].rsplit('/').next())
            .filter(|file_name| !file_name.ends_with(MASK_SUFFIX)),
    );
    if enumerated.is_empty() {
        return None;
    }

    let mut rows = BTreeSet::new();
    for file_name in enumerated {
        if names_rotation_face(file_name) {
            continue;
        }
        let attempted = format!("{folder_prefix}{file_name}");
        if !indices.texture_owners.contains_key(&attempted)
            && !indices.non_loose_textures.contains(&attempted)
        {
            rows.insert(attempted);
        }
    }
    Some(rows)
}

/// Every `.dds` file an active mod actually loads that RimWorld's own DDS
/// loader cannot decode — see [`crate::extract::textures::classify_dds`].
/// A flat re-export of each scanned mod's own
/// [`ScannedMod::undecodable_textures`](crate::domain::ScannedMod::undecodable_textures),
/// already computed during the scan (the header read happens once, in
/// `infra::mod_scan`, not here), sorted for a deterministic report.
#[must_use]
pub fn undecodable_textures(scanned: &[ScannedMod]) -> Vec<Conflict> {
    let mut rows: Vec<UndecodableTexture> = scanned
        .iter()
        .flat_map(|m| m.undecodable_textures.iter().cloned())
        .collect();
    rows.sort_by(|a, b| (&a.mod_id, &a.path).cmp(&(&b.mod_id, &b.path)));
    rows.into_iter().map(Conflict::UndecodableTexture).collect()
}
