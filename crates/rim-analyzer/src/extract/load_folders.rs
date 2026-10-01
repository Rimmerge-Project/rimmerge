//! Resolves which folders under a mod root actually get loaded, per
//! `LoadFolders.xml` (when present) or RimWorld's default rule.
//!
//! Pure by construction: nothing here touches the filesystem. Candidate
//! folders are returned as relative path strings (`""` meaning the mod
//! root itself) — `infra::mod_scan` is the one that checks which of them
//! actually exist on disk.

use std::collections::HashSet;

use roxmltree::Node;
use thiserror::Error;

use crate::domain::{GameVersion, ModId};

use super::xml_util::{
    MAX_RAW_ELEMENT_DEPTH, decode_lossy, raw_element_nesting_exceeds, split_csv,
};

#[derive(Debug, Error)]
pub enum LoadFoldersError {
    #[error("failed to parse XML: {0}")]
    Xml(#[from] roxmltree::Error),
    #[error("root element is not <loadFolders>")]
    WrongRoot,
    #[error("raw element nesting exceeds {0} levels; refusing to parse")]
    TooDeep(usize),
}

/// One candidate folder from a resolved `LoadFolders.xml` entry, gated by
/// its own `<li>`'s `IfModActive`/`IfModActiveAll` — recorded per folder,
/// not only merged into [`VersionEntry::if_mod_active_targets`], so a later
/// consumer can ask "was *this* folder gated on mod X" for one specific
/// `<li>` rather than the whole version block (needed for a compat-folder
/// exclusion on `PatchRemovedNode`: a toucher living in a folder gated on
/// the remover's own mod was written for the remover's end state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionFolder {
    /// The relative path, already filtered by
    /// `IfModActive`/`IfModActiveAll`/`IfModNotActive`; `""` means the mod
    /// root itself. Existence on disk is not checked here.
    pub relative: String,
    /// Mod ids named on this one `<li>`'s own `IfModActive`/`IfModActiveAll`
    /// attributes (not `IfModNotActive` — "must be absent" isn't the
    /// "written for this mod's end state" relation the consumer above
    /// needs), collected only when the gate passed: a folder gated off
    /// never loads, so it has no gate to record.
    pub if_mod_active: Vec<ModId>,
}

/// One `LoadFolders.xml` version (or `<default>`) entry, resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionEntry {
    /// Candidate folders, already filtered by
    /// `IfModActive`/`IfModActiveAll`/`IfModNotActive`, in document order —
    /// see [`VersionFolder`]. `infra::mod_scan` reverses this into load
    /// priority order (last-listed `<li>` highest); this field keeps the
    /// document order, matching `<loadFolders>`'s own text.
    pub folders: Vec<VersionFolder>,
    /// Mod ids named in this entry's `IfModActive` and `IfModActiveAll`
    /// gates, collected regardless of whether the gate passed — source
    /// data for `IfModActive` awareness edges. An all-of gate is exactly as
    /// much "aware of, and assumed to load after" as an any-of one;
    /// `IfModNotActive` stays uncollected, since "must be absent" isn't
    /// that relation. Aggregated across every `<li>` in the entry — see
    /// `VersionFolder::if_mod_active` for the per-folder breakdown used by
    /// the compat-folder exclusion.
    pub if_mod_active_targets: Vec<ModId>,
}

/// Parses `LoadFolders.xml` once and resolves the entry for
/// `game_version`: the descending-version search first (`<vX.Y>` blocks
/// with a key `<=` `game_version`, highest key by string-descending order
/// wins), `<default>` only when nothing there is eligible — mirroring
/// `ModContentPack.InitLoadFolders`'s own fallthrough order, not the
/// reverse.
/// Returns `Ok(None)` when neither exists, so the caller falls back to
/// the default folder rule ([`default_folders`]); returns
/// `Ok(Some(entry))` with an empty `folders` when a block was selected
/// but every one of its entries gated off — the engine returns
/// having loaded nothing from this branch rather than falling through.
pub fn resolve_version_entry(
    bytes: &[u8],
    game_version: GameVersion,
    active_mods: &HashSet<ModId>,
) -> Result<Option<VersionEntry>, LoadFoldersError> {
    let text = decode_lossy(bytes);
    if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(LoadFoldersError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("loadFolders") {
        return Err(LoadFoldersError::WrongRoot);
    }

    let Some(version_blocks) = select_version_blocks(root, game_version) else {
        return Ok(None);
    };

    let mut folders = Vec::new();
    let mut if_mod_active_targets = Vec::new();
    let entries = version_blocks.into_iter().flat_map(|block| {
        block
            .children()
            .filter(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case("li"))
    });
    for li in entries {
        let mut li_targets = Vec::new();
        if let Some(raw) = li.attribute("IfModActive") {
            li_targets.extend(parse_ids(raw));
        }
        if let Some(raw) = li.attribute("IfModActiveAll") {
            li_targets.extend(parse_ids(raw));
        }
        if_mod_active_targets.extend(li_targets.iter().cloned());
        if gate_allows(li, active_mods) {
            let raw = li.text().unwrap_or("").trim();
            folders.push(VersionFolder {
                relative: normalize_relative(raw),
                if_mod_active: li_targets,
            });
        }
    }
    Ok(Some(VersionEntry {
        folders,
        if_mod_active_targets,
    }))
}

/// Whether `bytes` (a `LoadFolders.xml`) carries a `<vX.Y>` element for
/// exactly `game_version` — **independent of `<default>`**, a
/// deliberately stricter question than [`resolve_version_entry`]'s own
/// (which falls back to a lower eligible version block, and then to
/// `<default>`). The `load_folders_version_matched` guard needs to know
/// whether the scanned folder set came from the block for exactly this
/// game version: when a file has `<v1.4>`/`<v1.5>` but no `<v1.6>`,
/// `resolve_version_entry` correctly resolves `<v1.5>`, but that is still
/// not the *exact* block, so `Some(false)` is the right answer here.
///
/// **Trade-off, accepted deliberately**: a `<default>`-only file (no
/// version blocks at all — nothing else the fallback could ever prefer)
/// also reads `false` here, even though it isn't actually at risk the
/// way the mixed-block case is; this function answers "does the exact
/// block exist", not "is this file safe", trading that finer distinction
/// away for a simpler, strictly-conservative predicate.
pub fn has_exact_version_block(
    bytes: &[u8],
    game_version: GameVersion,
) -> Result<bool, LoadFoldersError> {
    let text = decode_lossy(bytes);
    if raw_element_nesting_exceeds(&text, MAX_RAW_ELEMENT_DEPTH) {
        return Err(LoadFoldersError::TooDeep(MAX_RAW_ELEMENT_DEPTH));
    }
    let doc = roxmltree::Document::parse(&text)?;
    let root = doc.root_element();
    if !root.tag_name().name().eq_ignore_ascii_case("loadFolders") {
        return Err(LoadFoldersError::WrongRoot);
    }
    let version_tag = game_version.tag_name();
    Ok(root
        .children()
        .any(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case(&version_tag)))
}

/// The blocks (in document order) RimWorld would concatenate for
/// `game_version`: the descending-version search first, then
/// `<default>` only when no version block is eligible — matching
/// `ModContentPack.InitLoadFolders`'s own fallthrough order. Every block
/// whose normalized key equals the winning key is returned, not just the
/// first (a repeated key accumulates). `None` means neither a version block nor `<default>`
/// exists, so the caller falls back to the default *directory* rule.
fn select_version_blocks<'a, 'input>(
    root: Node<'a, 'input>,
    game_version: GameVersion,
) -> Option<Vec<Node<'a, 'input>>> {
    let blocks: Vec<(String, Node<'a, 'input>)> = root
        .children()
        .filter(|c| c.is_element())
        .map(|node| (block_key(node), node))
        .collect();

    let selected_key = match best_eligible_key(&blocks, game_version) {
        Some(key) => key,
        None if blocks.iter().any(|(key, _)| key == "default") => "default".to_string(),
        None => return None,
    };

    Some(
        blocks
            .into_iter()
            .filter(|(key, _)| *key == selected_key)
            .map(|(_, node)| node)
            .collect(),
    )
}

/// Among block keys other than `default`, the highest one (by
/// string-descending order, pinned by a test naming why) that parses to
/// a version `<=` `game_version`. Mirrors the candidate filter in
/// `ModContentPack.InitLoadFolders`: `default`, empty keys, and keys with
/// no `.` are never candidates here, regardless of what the (more
/// permissive) key parser would otherwise accept.
fn best_eligible_key(blocks: &[(String, Node)], game_version: GameVersion) -> Option<String> {
    blocks
        .iter()
        .map(|(key, _)| key.as_str())
        .filter(|key| *key != "default" && !key.is_empty() && key.contains('.'))
        .filter(|key| parse_engine_version_key(key).is_some_and(|v| v <= game_version))
        .max()
        .map(str::to_string)
}

/// Normalizes a block element's tag into its lookup key: lowercased, with
/// one leading `v` stripped — `<v1.6>`, `<V1.6>` and `<1.6>` all key
/// `"1.6"`, `<default>` keys `"default"`
/// (`ModLoadFolders.LoadDataFromXmlCustom`).
fn block_key(node: Node) -> String {
    let name = node.tag_name().name().to_lowercase();
    match name.strip_prefix('v') {
        Some(rest) => rest.to_string(),
        None => name,
    }
}

/// Parses a block key the way the engine's own
/// `VersionControl.VersionFromString` does: one to three dot-separated
/// non-negative integers; anything else — too many components, a
/// non-numeric one — makes the key ineligible rather than a parse error,
/// mirroring the IL's own `catch { return false; }`.
///
/// Deliberately distinct from [`parse_version_folder_name`], which
/// parses on-disk *directory* names for the unrelated default-folder
/// rule and requires exactly two components — conflating the two would
/// make either rule silently wrong on the other's inputs.
///
/// Disclosed divergence: [`GameVersion`] is `(major, minor)` only,
/// so a three-component key's build number is parsed (to reject a
/// non-numeric one, matching the engine) but then discarded —
/// `<v1.6.9999>` compares as `1.6` here, while the real engine also
/// compares the build. This is **not** merely "eligible for us,
/// ineligible for the engine": because [`best_eligible_key`]'s tie-break
/// is lexicographic, a
/// three-component key sharing a prefix with a shorter, genuinely
/// eligible one sorts *above* it (`"1.6.0"` > `"1.6"` as strings) and so
/// **wins selection outright, displacing the correct block** — a
/// wrong-folder-set outcome, not a harmless extra candidate. Zero
/// three-component keys exist on the real install as of this writing;
/// revisit if that stops being true.
fn parse_engine_version_key(key: &str) -> Option<GameVersion> {
    let components: Vec<&str> = key.split('.').collect();
    if components.len() > 3 {
        return None;
    }
    let numbers = components
        .into_iter()
        .map(parse_non_negative_integer)
        .collect::<Option<Vec<u32>>>()?;
    let major = *numbers.first()?;
    let minor = numbers.get(1).copied().unwrap_or(0);
    Some(GameVersion::new(major, minor))
}

/// A component is eligible only if it is entirely ASCII digits — no
/// sign, no whitespace, no empty string — matching `VersionFromString`'s
/// own strict per-component `int.Parse`.
fn parse_non_negative_integer(component: &str) -> Option<u32> {
    if component.is_empty() || !component.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    component.parse().ok()
}

/// Real, published mods write `<li>\</li>` for "the mod root" — a lone
/// backslash rather than `/`. Left unrecognized, `"\\"` would fall
/// through as a literal relative folder name, and
/// `infra::mod_scan::relative_to_path`'s `mod_root.join("\\")` on Windows
/// silently collapses to the *drive root* (`Path::push`'s own documented
/// behavior: a pushed path with a root but no prefix replaces everything
/// after the prefix) — so the mod's root-level `Textures/`/`Sounds/`/
/// `Defs/`/`Patches/`/`Assemblies/` would never be scanned, with no
/// warning (the "folder" doesn't exist under the drive root, so
/// [`super::super::infra::mod_scan`]'s `existing_dirs` filter silently
/// drops it). Both separators — and any number of them, leading — mean
/// the same thing, matching RimWorld's own `Path.Combine`-based
/// resolution, which never distinguishes them.
fn normalize_relative(raw: &str) -> String {
    raw.trim_start_matches(['/', '\\']).to_string()
}

fn parse_ids(raw: &str) -> Vec<ModId> {
    split_csv(raw).into_iter().map(ModId::new).collect()
}

/// `active_mods` is compared by [`ModId::base`] on both sides:
/// `IfModActive`/`IfModActiveAll`/`IfModNotActive` name a mod's bare
/// packageId, but the caller's active set may include a `_steam`-suffixed
/// id for a mod that also has a local copy on disk (see [`ModId::base`]).
///
/// `IfModActiveAll` requires *every*
/// named id to be active (`ModLister.AllModsActiveNoSuffix`), unlike
/// `IfModActive`'s "any one of" — an empty attribute value parses to no
/// required ids, which is vacuously satisfied, matching the engine's own
/// `requiredAllOfPackageIds.NullOrEmpty()` short-circuit in `LoadFolder.
/// ShouldLoad`.
fn gate_allows(li: Node, active_mods: &HashSet<ModId>) -> bool {
    if let Some(raw) = li.attribute("IfModActive") {
        let required = parse_ids(raw);
        // The IL is symmetric —
        // `requiredAnyOfPackageIds.NullOrEmpty() || AnyModActiveNoSuffix(...)`
        // — so an empty attribute (or one that's only commas/whitespace;
        // `split_csv` drops empty entries) must short-circuit to allowed,
        // same as the `IfModActiveAll` arm below. Without this guard,
        // `.any()` over an empty `required` is `false` and the folder is
        // dropped while the real engine loads it — an under-scan.
        if !required.is_empty()
            && !required
                .iter()
                .any(|id| active_mods.iter().any(|a| a.base() == id.base()))
        {
            return false;
        }
    }
    if let Some(raw) = li.attribute("IfModActiveAll") {
        let required = parse_ids(raw);
        if !required
            .iter()
            .all(|id| active_mods.iter().any(|a| a.base() == id.base()))
        {
            return false;
        }
    }
    if let Some(raw) = li.attribute("IfModNotActive") {
        let forbidden = parse_ids(raw);
        if forbidden
            .iter()
            .any(|id| active_mods.iter().any(|a| a.base() == id.base()))
        {
            return false;
        }
    }
    true
}

/// The default folder-selection rule when no `LoadFolders.xml` entry
/// applies: the highest `<major>.<minor>`-named entry in
/// `existing_dir_names` that's `<= game_version`, then `Common` if
/// present, then the mod root — as relative path strings (`""` for the
/// mod root). `existing_dir_names` should be the real subdirectory names
/// of the mod root; this function does no filesystem I/O itself.
#[must_use]
pub fn default_folders(existing_dir_names: &[String], game_version: GameVersion) -> Vec<String> {
    let mut folders = Vec::new();
    if let Some(version_folder) = best_version_folder_name(existing_dir_names, game_version) {
        folders.push(version_folder);
    }
    if let Some(common) = existing_dir_names
        .iter()
        .find(|n| n.eq_ignore_ascii_case("Common"))
    {
        folders.push(common.clone());
    }
    folders.push(String::new());
    folders
}

fn best_version_folder_name(names: &[String], game_version: GameVersion) -> Option<String> {
    names
        .iter()
        .filter_map(|name| parse_version_folder_name(name).map(|v| (v, name.clone())))
        .filter(|(v, _)| *v <= game_version)
        .max_by_key(|(v, _)| *v)
        .map(|(_, name)| name)
}

fn parse_version_folder_name(name: &str) -> Option<GameVersion> {
    let (major, minor) = name.split_once('.')?;
    if !major.chars().all(|c| c.is_ascii_digit()) || major.is_empty() {
        return None;
    }
    if !minor.chars().all(|c| c.is_ascii_digit()) || minor.is_empty() {
        return None;
    }
    Some(GameVersion::new(major.parse().ok()?, minor.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v16() -> GameVersion {
        GameVersion::new(1, 6)
    }

    /// A [`VersionFolder`] with no `IfModActive`/`IfModActiveAll` gate —
    /// what most of this module's fixtures need.
    fn vf(relative: &str) -> VersionFolder {
        VersionFolder {
            relative: relative.to_string(),
            if_mod_active: Vec::new(),
        }
    }

    /// A gated [`VersionFolder`], for the tests that need the per-folder
    /// gate itself, not just the aggregate `if_mod_active_targets`.
    fn gated_vf(relative: &str, if_mod_active: &[&str]) -> VersionFolder {
        VersionFolder {
            relative: relative.to_string(),
            if_mod_active: if_mod_active.iter().map(|s| ModId::new(*s)).collect(),
        }
    }

    #[test]
    fn parses_version_folder_names() {
        assert_eq!(
            parse_version_folder_name("1.6"),
            Some(GameVersion::new(1, 6))
        );
        assert_eq!(parse_version_folder_name("Common"), None);
        assert_eq!(parse_version_folder_name("1.6.2"), None);
    }

    #[test]
    fn default_folders_picks_highest_version_le_game_version_plus_common_and_root() {
        let names: Vec<String> = ["1.4", "1.5", "1.7", "Common"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let folders = default_folders(&names, v16());
        assert_eq!(
            folders,
            vec!["1.5".to_string(), "Common".to_string(), String::new()]
        );
    }

    #[test]
    fn default_folders_is_just_root_when_nothing_else_exists() {
        let folders = default_folders(&[], v16());
        assert_eq!(folders, vec![String::new()]);
    }

    #[test]
    fn resolve_version_entry_selects_matching_version_and_applies_if_mod_active() {
        let xml = br#"<loadFolders>
              <v1.6>
                <li>/</li>
                <li IfModActive="active.mod">1.6/Compat</li>
                <li IfModActive="missing.mod">1.6/Compat</li>
              </v1.6>
            </loadFolders>"#;
        let mut active = HashSet::new();
        active.insert(ModId::new("active.mod"));

        let entry = resolve_version_entry(xml, v16(), &active).unwrap().unwrap();

        assert_eq!(
            entry.folders,
            vec![vf(""), gated_vf("1.6/Compat", &["active.mod"])]
        );
        assert_eq!(
            entry.if_mod_active_targets,
            vec![ModId::new("active.mod"), ModId::new("missing.mod")]
        );
    }

    /// Real-world case (see [`super::normalize_relative`]'s own doc
    /// comment): a real, published mod's own `LoadFolders.xml` uses `\`
    /// (a lone backslash), not `/`, for "the mod root" — both must
    /// normalize to the same empty-string sentinel `infra::mod_scan`
    /// resolves back to `mod_root` itself.
    #[test]
    fn resolve_version_entry_treats_a_lone_backslash_as_the_mod_root_too() {
        let xml = br#"<loadFolders>
              <v1.6>
                <li>1.6</li>
                <li>\</li>
              </v1.6>
            </loadFolders>"#;

        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();

        assert_eq!(
            entry.folders,
            vec![vf("1.6"), vf("")],
            "a lone backslash must normalize to the root sentinel, not a literal \"\\\\\" folder name"
        );
    }

    /// A file with `<v1.4>`/`<v1.5>` but no `<v1.6>` must resolve via the
    /// descending-version search, not fall straight through to the
    /// default *directory* rule.
    #[test]
    fn resolve_version_entry_falls_back_to_the_highest_lower_version_block_not_default() {
        let xml = br#"<loadFolders>
              <v1.4><li>only.for.1.4</li></v1.4>
              <v1.5><li>only.for.1.5</li></v1.5>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!(entry.folders, vec![vf("only.for.1.5")]);
    }

    /// No eligible version block and no `<default>` at all.
    #[test]
    fn resolve_version_entry_is_none_with_no_version_blocks_at_all() {
        let xml = br#"<loadFolders></loadFolders>"#;
        assert_eq!(
            resolve_version_entry(xml, v16(), &HashSet::new()).unwrap(),
            None
        );
    }

    /// A version block above `game_version` must never win selection, even
    /// with nothing else present for the fallback to prefer instead — it must
    /// not "leak in" as if it were eligible.
    #[test]
    fn resolve_version_entry_does_not_let_a_too_new_version_block_leak_in() {
        let xml = br#"<loadFolders><v1.7><li>too.new</li></v1.7></loadFolders>"#;
        assert_eq!(
            resolve_version_entry(xml, v16(), &HashSet::new()).unwrap(),
            None
        );
    }

    /// The engine's own selection is `orderby x
    /// descending` over the raw key *strings*
    /// (`ModContentPack.InitLoadFolders`), not a numeric comparison of
    /// parsed versions. `"1.9"` sorts above `"1.10"` lexicographically
    /// (`'9' > '1'` at the second character) even though 1.10 is the
    /// numerically greater version — mirror the engine's actual behaviour
    /// here rather than "fixing" it into numeric order; a future reader
    /// changing `.max()` to compare parsed versions would silently
    /// reintroduce a real divergence from RimWorld.
    #[test]
    fn resolve_version_entry_tie_breaks_by_string_descending_not_numeric_order() {
        let xml = br#"<loadFolders>
              <v1.9><li>from.1.9</li></v1.9>
              <v1.10><li>from.1.10</li></v1.10>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, GameVersion::new(1, 20), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!(entry.folders, vec![vf("from.1.9")]);
    }

    /// Malformed keys are skipped (the engine's own `catch`), never
    /// fatal — the well-formed `<v1.5>` block must still be found
    /// alongside them.
    ///
    /// The four-component key is `<v1.6.0.0>`, not something like
    /// `<v1.2.3.4>`: that key parses (major, minor) as `(1, 2)`, which
    /// loses the string tie-break to `"1.5"` regardless of whether
    /// `parse_engine_version_key`'s `components.len() > 3` guard exists,
    /// so it would never exercise that guard. `"1.6.0.0"` parses to
    /// `(1, 6)` (eligible against game 1.6) and sorts *above* `"1.5"`, so
    /// it would win selection — flipping this assertion, with
    /// `wins.by.mistake` — if the guard were removed.
    #[test]
    fn resolve_version_entry_skips_malformed_keys_without_erroring() {
        let xml = br#"<loadFolders>
              <vfoo><li>bad.alpha</li></vfoo>
              <v1.x><li>bad.mixed</li></v1.x>
              <v1.6.0.0><li>wins.by.mistake</li></v1.6.0.0>
              <v1.5><li>only.for.1.5</li></v1.5>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!(entry.folders, vec![vf("only.for.1.5")]);
    }

    /// An exact `<v1.6>` block wins over a lower one when both are present.
    #[test]
    fn resolve_version_entry_prefers_the_exact_version_block_over_a_lower_one() {
        let xml = br#"<loadFolders>
              <v1.5><li>from.1.5</li></v1.5>
              <v1.6><li>from.1.6</li></v1.6>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!(entry.folders, vec![vf("from.1.6")]);
    }

    /// `IfModActiveAll` requires every named id to be active (unlike
    /// `IfModActive`'s "any one of"), and combines correctly with
    /// `IfModActive`/`IfModNotActive` gating the same `<li>`.
    #[test]
    fn if_mod_active_all_requires_every_named_mod_to_be_active() {
        let xml = br#"<loadFolders>
              <v1.6>
                <li IfModActiveAll="a.mod,b.mod">needs.both</li>
                <li IfModActiveAll="a.mod,missing.mod">needs.missing</li>
                <li IfModActive="a.mod" IfModActiveAll="b.mod" IfModNotActive="c.mod">combined</li>
              </v1.6>
            </loadFolders>"#;
        let mut active = HashSet::new();
        active.insert(ModId::new("a.mod"));
        active.insert(ModId::new("b.mod"));

        let entry = resolve_version_entry(xml, v16(), &active).unwrap().unwrap();

        assert_eq!(
            entry.folders,
            vec![
                gated_vf("needs.both", &["a.mod", "b.mod"]),
                gated_vf("combined", &["a.mod", "b.mod"]),
            ]
        );
    }

    /// `IfModActiveAll` ids are collected as awareness targets
    /// regardless of whether the gate passed, matching `IfModActive`'s
    /// "collected regardless" contract.
    #[test]
    fn if_mod_active_all_ids_are_collected_as_awareness_targets_regardless_of_gate_result() {
        let xml = br#"<loadFolders>
              <v1.6><li IfModActiveAll="a.mod,missing.mod">gated.off</li></v1.6>
            </loadFolders>"#;
        let mut active = HashSet::new();
        active.insert(ModId::new("a.mod"));

        let entry = resolve_version_entry(xml, v16(), &active).unwrap().unwrap();

        assert!(entry.folders.is_empty());
        assert_eq!(
            entry.if_mod_active_targets,
            vec![ModId::new("a.mod"), ModId::new("missing.mod")]
        );
    }

    /// `IfModActive=""` allows the folder, matching the real engine's own
    /// `NullOrEmpty()` short-circuit. Without the `!required.is_empty()`
    /// guard, `.any()` over the empty `required` that `split_csv` produces
    /// is `false` and the folder would be denied.
    #[test]
    fn if_mod_active_with_no_real_ids_is_vacuously_satisfied() {
        let xml = br#"<loadFolders>
              <v1.6><li IfModActive="">always</li></v1.6>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!(entry.folders, vec![vf("always")]);
    }

    /// `IfModActiveAll=""` allows the folder (the engine's own
    /// `NullOrEmpty()` short-circuit, mirrored by `.all()` over an empty
    /// set being vacuously `true`); that correctness depends entirely on
    /// `split_csv` filtering the empty entry out, which this test pins.
    #[test]
    fn if_mod_active_all_with_no_real_ids_is_vacuously_satisfied() {
        let xml = br#"<loadFolders>
              <v1.6><li IfModActiveAll="">always</li></v1.6>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!(entry.folders, vec![vf("always")]);
    }

    /// Pins the disclosed divergence's real severity: because the
    /// tie-break is lexicographic, a three-component key sharing a prefix with a
    /// shorter, correct block doesn't just join the candidate set beside
    /// it — it **wins** and displaces the correct block outright
    /// (`"1.6.0"` > `"1.6"` as strings, both parsing to `(1, 6)`). The
    /// real engine would treat `"1.6.0"` as a *different*, generally
    /// ineligible version (it compares the build component too); this is
    /// a known, disclosed wrong-folder-set divergence, not a merely
    /// harmless extra candidate.
    #[test]
    fn resolve_version_entry_lets_a_three_component_key_win_over_the_correct_exact_block() {
        let xml = br#"<loadFolders>
              <v1.6><li>correct</li></v1.6>
              <v1.6.0><li>divergent</li></v1.6.0>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!(entry.folders, vec![vf("divergent")]);
    }

    /// A repeated block key accumulates entries from
    /// every matching block, in document order, rather than the later
    /// block replacing or shadowing the earlier one
    /// (`ModLoadFolders.LoadDataFromXmlCustom`'s own `ContainsKey`/`Add`).
    #[test]
    fn resolve_version_entry_accumulates_entries_from_every_block_sharing_a_key() {
        let xml = br#"<loadFolders>
              <v1.5><li>first</li></v1.5>
              <v1.5><li>second</li></v1.5>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!(entry.folders, vec![vf("first"), vf("second")]);
    }

    /// A selected block whose entries are all
    /// gated off returns `Some` with zero folders, never `None` — the
    /// engine returns having loaded nothing from this branch rather than
    /// falling through to the directory rule, and this is the one case
    /// where the distinction between the two is observable.
    #[test]
    fn resolve_version_entry_returns_some_empty_when_the_selected_block_gates_everything_off() {
        let xml = br#"<loadFolders>
              <v1.6><li IfModActive="missing.mod">never</li></v1.6>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert!(entry.folders.is_empty());
    }

    #[test]
    fn has_exact_version_block_is_true_for_a_real_vxy_element() {
        let xml = br#"<loadFolders><v1.6><li>/</li></v1.6></loadFolders>"#;
        assert!(has_exact_version_block(xml, v16()).unwrap());
    }

    /// `<v1.4>`/`<v1.5>`/`<default>`, no `<v1.6>` —
    /// `resolve_version_entry` correctly selects `<v1.5>` here rather
    /// than falling back to `<default>`, but this stricter query still
    /// must say `false`: it answers "does the *exact* block exist", and
    /// it doesn't.
    #[test]
    fn has_exact_version_block_is_false_when_only_default_and_other_versions_exist() {
        let xml = br#"<loadFolders>
              <v1.4><li>a</li></v1.4>
              <v1.5><li>b</li></v1.5>
              <default><li>c</li></default>
            </loadFolders>"#;
        assert!(!has_exact_version_block(xml, v16()).unwrap());
        // Confirms the two functions really do disagree on this exact
        // fixture, not just in theory.
        assert!(
            resolve_version_entry(xml, v16(), &HashSet::new())
                .unwrap()
                .is_some()
        );
    }

    /// The accepted trade-off, pinned: a `<default>`-only file (nothing
    /// else the fallback could ever prefer) also reads `false` here.
    #[test]
    fn has_exact_version_block_is_false_for_a_default_only_file() {
        let xml = br#"<loadFolders><default><li>/</li></default></loadFolders>"#;
        assert!(!has_exact_version_block(xml, v16()).unwrap());
    }

    #[test]
    fn has_exact_version_block_is_false_with_no_load_folders_content_at_all() {
        let xml = br#"<loadFolders></loadFolders>"#;
        assert!(!has_exact_version_block(xml, v16()).unwrap());
    }

    #[test]
    fn has_exact_version_block_wrong_root_is_an_error() {
        assert!(matches!(
            has_exact_version_block(b"<NotLoadFolders/>", v16()),
            Err(LoadFoldersError::WrongRoot)
        ));
    }

    /// `<default>` is consulted only *after* the descending-version search
    /// finds nothing eligible, never before it — proven by a file that has
    /// both, where the lower version block must win.
    #[test]
    fn resolve_version_entry_prefers_a_lower_version_block_over_default() {
        let xml = br#"<loadFolders>
              <default><li>/</li><li>Base</li></default>
              <v1.5><li>only.for.1.5</li></v1.5>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!(entry.folders, vec![vf("only.for.1.5")]);
    }

    /// When no version block is eligible, `<default>` is used.
    #[test]
    fn resolve_version_entry_falls_back_to_default_when_no_version_block_is_eligible() {
        let xml = br#"<loadFolders>
              <v1.7><li>too.new</li></v1.7>
              <default><li>/</li><li>Base</li></default>
            </loadFolders>"#;
        let entry = resolve_version_entry(xml, v16(), &HashSet::new())
            .unwrap()
            .unwrap();
        assert_eq!(entry.folders, vec![vf(""), vf("Base")]);
    }

    #[test]
    fn if_mod_active_matches_a_steam_suffixed_active_id() {
        let xml = br#"<loadFolders>
              <v1.6><li IfModActive="active.mod">Compat</li></v1.6>
            </loadFolders>"#;
        let mut active = HashSet::new();
        active.insert(ModId::new("active.mod_steam"));

        let entry = resolve_version_entry(xml, v16(), &active).unwrap().unwrap();

        assert_eq!(entry.folders, vec![gated_vf("Compat", &["active.mod"])]);
    }

    #[test]
    fn if_mod_not_active_excludes_the_folder_when_the_named_mod_is_active() {
        let xml = br#"<loadFolders>
              <v1.6><li IfModNotActive="conflicting.mod">Vanilla</li></v1.6>
            </loadFolders>"#;
        let mut active = HashSet::new();
        active.insert(ModId::new("conflicting.mod"));

        let entry = resolve_version_entry(xml, v16(), &active).unwrap().unwrap();

        assert!(entry.folders.is_empty());
    }

    #[test]
    fn wrong_root_is_an_error() {
        assert!(matches!(
            resolve_version_entry(b"<NotLoadFolders/>", v16(), &HashSet::new()),
            Err(LoadFoldersError::WrongRoot)
        ));
    }
}
