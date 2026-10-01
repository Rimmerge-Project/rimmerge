//! Whole-run sanity checks that aren't edges or conflicts: missing
//! dependencies, incompatible pairs both active, version mismatches, and
//! near-miss `FindMod`/`MayRequire` references. `ParentName` problems are
//! `inheritance::broken_inheritance`'s own ledger finding, not a
//! scan-note warning — see that module.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::domain::{
    Conflict, GameVersion, InactiveMod, IncompatiblePair, MissingDependency, ModId,
    ModReferenceKind, NearMissModReference, NearMissRule, ScannedMod, Warning, XmlLocator,
};

use super::indices::{ActiveMods, MIN_CORE_RESOURCE_TEXTURES};
use super::name_similarity::{self, PreparedName};

/// Declared `modDependencies` entries whose target isn't active.
/// `dep.id` never carries a `_steam` suffix (see [`ModId::base`](crate::domain::ModId::base)),
/// so membership is checked through `active` rather than a raw set lookup.
#[must_use]
pub fn missing_dependencies(scanned: &[ScannedMod], active: &ActiveMods) -> Vec<MissingDependency> {
    scanned
        .iter()
        .flat_map(|sm| {
            sm.info
                .declared
                .dependencies
                .iter()
                .filter(|dep| !active.contains(&dep.id))
                .map(|dep| MissingDependency {
                    mod_id: sm.info.id.clone(),
                    dependency: dep.clone(),
                })
        })
        .collect()
}

/// A [`Warning`] naming the fail-safe in
/// `conflicts::textures::missing_texture_paths`: when `core_resource_texture_count`
/// (Core's own scanned resource-texture index — see
/// [`crate::domain::ScanOutput::core_resource_textures`]) hasn't cleared
/// [`MIN_CORE_RESOURCE_TEXTURES`], that check disables itself for the whole
/// scan rather than run against a near-empty (or wrong-file) index. `None`
/// once the count is high enough to trust.
#[must_use]
pub fn core_resource_index_warning(core_resource_texture_count: usize) -> Option<Warning> {
    if core_resource_texture_count >= MIN_CORE_RESOURCE_TEXTURES {
        return None;
    }
    Some(Warning::new(
        None,
        format!(
            "Core resource texture index has only {core_resource_texture_count} entries \
             (need at least {MIN_CORE_RESOURCE_TEXTURES}); missing_texture_path is disabled \
             for this scan"
        ),
    ))
}

/// Unordered pairs of active mods where at least one declares the other
/// `incompatibleWith`, deduplicated (`a`, `b`) == (`b`, `a`). Reports the
/// exact active id (resolved through `active`) on both sides, matching
/// what every other active-mod-facing id in the report uses.
#[must_use]
pub fn incompatible_active_pairs(
    scanned: &[ScannedMod],
    active: &ActiveMods,
) -> Vec<IncompatiblePair> {
    let mut seen = HashSet::new();
    let mut pairs = Vec::new();
    for scanned_mod in scanned {
        for other in &scanned_mod.info.declared.incompatible_with {
            let Some(resolved) = active.resolve(other) else {
                continue;
            };
            if *resolved == scanned_mod.info.id {
                continue;
            }
            let key = if scanned_mod.info.id < *resolved {
                (scanned_mod.info.id.clone(), resolved.clone())
            } else {
                (resolved.clone(), scanned_mod.info.id.clone())
            };
            if seen.insert(key.clone()) {
                pairs.push(IncompatiblePair { a: key.0, b: key.1 });
            }
        }
    }
    pairs
}

/// Active mods whose `supportedVersions` don't list the current game
/// version. A mod with no declared `supportedVersions` at all makes no
/// claim either way and is never flagged.
#[must_use]
pub fn unsupported_version_mods(scanned: &[ScannedMod], game_version: GameVersion) -> Vec<ModId> {
    let current = game_version.folder_name();
    scanned
        .iter()
        .filter(|sm| !sm.info.supported_versions.is_empty())
        .filter(|sm| {
            !sm.info
                .supported_versions
                .iter()
                .any(|v| v.trim() == current)
        })
        .map(|sm| sm.info.id.clone())
        .collect()
}

/// One candidate active mod, scored against a written value —
/// [`best_match`]'s own accumulator.
#[derive(Clone, Copy)]
struct CandidateMatch<'a> {
    rule: NearMissRule,
    similarity: f64,
    id: &'a ModId,
    display_name: &'a str,
}

/// `candidate` outranks `current` when it's a stronger rule (letter
/// order — see [`NearMissRule`]'s own derived `Ord`), else a closer
/// similarity, else the lexicographically smaller id — matching the
/// plan's own "keep only the best by (rule letter, then similarity, then
/// id)" tie-break, so the winner never depends on scan order.
fn is_better(candidate: &CandidateMatch, current: &CandidateMatch) -> bool {
    if candidate.rule != current.rule {
        return candidate.rule < current.rule;
    }
    if candidate.similarity != current.similarity {
        return candidate.similarity > current.similarity;
    }
    candidate.id < current.id
}

/// The single best-matching candidate among `candidates` — each a
/// `(id, comparison text, display name)` triple, since `MayRequire`
/// compares against a mod's own base package id while its evidence still
/// names the mod by its display name.
fn best_match<'a>(
    written: &str,
    candidates: impl Iterator<Item = (&'a ModId, &'a PreparedName, &'a str)>,
    allow_case_only: bool,
) -> Option<CandidateMatch<'a>> {
    let written = PreparedName::new(written);
    let mut best: Option<CandidateMatch<'a>> = None;
    for (id, compare_text, display_name) in candidates {
        let Some(m) = name_similarity::classify(&written, compare_text, allow_case_only) else {
            continue;
        };
        let candidate = CandidateMatch {
            rule: m.rule,
            similarity: m.similarity,
            id,
            display_name,
        };
        best = Some(match best {
            Some(current) if !is_better(&candidate, &current) => current,
            _ => candidate,
        });
    }
    best
}

/// Every `PatchOperationFindMod` name in `scanned_mod`, deduplicated
/// (first locator wins — `scanned_mod.patch_ops` is in scan order), that
/// resolves to no active mod's display name at all and to no installed
/// inactive mod's name either.
fn find_mod_candidates(scanned_mod: &ScannedMod) -> BTreeMap<String, XmlLocator> {
    let mut written = BTreeMap::new();
    for op in &scanned_mod.patch_ops {
        for name in &op.find_mod_names {
            written
                .entry(name.clone())
                .or_insert_with(|| op.locator.clone());
        }
    }
    written
}

/// Every `MayRequire`/`MayRequireAnyOf` id read from a site the game
/// actually honours (`LoadedModManager.ParseAndProcessXML`,
/// `DirectXmlToObject.ListFromXml`): a def or template node directly
/// under `<Defs>`, and a `<li>` list item (`PatchOp::is_list_item`). A
/// top-level `<Operation>`'s own attribute and a `<match>`/`<nomatch>`
/// node's are never read by the game, so their `may_require` is skipped
/// here even though the field is still populated on the `PatchOp`
/// itself. Deliberately does **not** cover a def-typed scalar
/// cross-reference field (a third honoured shape, via
/// `WantedRefForObject.BadCrossRefAllowed`) — this crate has no generic
/// cross-reference field extractor to draw it from, and every real
/// near-miss this feature measured came from a def/template root or a
/// patch list item.
fn may_require_candidates(scanned_mod: &ScannedMod) -> BTreeMap<String, XmlLocator> {
    let mut written = BTreeMap::new();
    for def in &scanned_mod.defs {
        for id in def.may_require.iter().chain(&def.may_require_any_of) {
            written
                .entry(id.clone())
                .or_insert_with(|| def.locator.clone());
        }
    }
    for template in &scanned_mod.templates {
        for id in &template.may_require {
            written
                .entry(id.clone())
                .or_insert_with(|| template.locator.clone());
        }
    }
    for op in scanned_mod.patch_ops.iter().filter(|op| op.is_list_item) {
        for id in op.may_require.iter().chain(&op.may_require_any_of) {
            written
                .entry(id.clone())
                .or_insert_with(|| op.locator.clone());
        }
    }
    // A keyed list-item element nested inside a def or template's own
    // field tree (e.g. `<need MayRequire="...">Bladder</need>`), honoured
    // the same as a plain `<li>` — see `DefsFile::nested_may_require`'s
    // own doc comment for the exact extraction shape.
    for (id, locator) in &scanned_mod.nested_may_require {
        written.entry(id.clone()).or_insert_with(|| locator.clone());
    }
    written
}

/// A written `PatchOperationFindMod` name or `MayRequire`/
/// `MayRequireAnyOf` id that resolves to no active mod, and to no
/// installed-but-inactive mod either, but closely resembles exactly one
/// active mod under [`super::name_similarity`]'s rules — a likely typo or
/// drifted reference. See [`crate::domain::NearMissModReference`].
///
/// **`FindMod` names** are matched exactly and case-sensitively
/// (`PatchOperationFindMod.ApplyWorker`): a name matching some active
/// mod only case-insensitively is [`NearMissRule::CaseOnly`] — a genuine
/// silent miss in-game, which `rim_analyzer`'s own `DisplayNameIndex`
/// resolves leniently by design.
///
/// **`MayRequire` ids** are matched case-insensitively and suffix-free
/// already (`ModLister.GetActiveModWithIdentifier`,
/// [`ActiveMods::contains`]), so [`NearMissRule::CaseOnly`] never
/// applies to them — a case difference there isn't a miss at all.
#[must_use]
pub fn near_miss_mod_references(
    scanned: &[ScannedMod],
    active: &ActiveMods,
    inactive: &[InactiveMod],
) -> Vec<Conflict> {
    let active_names = ActiveModNames::build(scanned);
    // A verdict depends only on the written text (never on which mod or
    // site wrote it), and the same missing id is typically written by many
    // mods — so each distinct text is classified against every active mod
    // once, not once per referrer.
    let mut find_mod_verdicts: HashMap<String, Option<CandidateMatch<'_>>> = HashMap::new();
    let mut may_require_verdicts: HashMap<String, Option<CandidateMatch<'_>>> = HashMap::new();

    let mut findings = Vec::new();
    for scanned_mod in scanned {
        for (written, locator) in find_mod_candidates(scanned_mod) {
            let verdict = *find_mod_verdicts
                .entry(written.clone())
                .or_insert_with(|| active_names.find_mod_near_miss(&written, inactive));
            let Some(best) = verdict else {
                continue;
            };
            findings.push(near_miss_finding(
                scanned_mod,
                ModReferenceKind::FindModName,
                written,
                best,
                locator,
            ));
        }

        for (written, locator) in may_require_candidates(scanned_mod) {
            let verdict = *may_require_verdicts
                .entry(written.clone())
                .or_insert_with(|| active_names.may_require_near_miss(&written, active, inactive));
            let Some(best) = verdict else {
                continue;
            };
            findings.push(near_miss_finding(
                scanned_mod,
                ModReferenceKind::MayRequireId,
                written,
                best,
                locator,
            ));
        }
    }
    findings
}

/// Every active mod's own id, display name, and base-id text — built once,
/// not per written value or per referrer, and each comparison text already
/// [`PreparedName`]d for [`best_match`].
struct ActiveModNames<'a> {
    names: Vec<(&'a ModId, &'a str)>,
    prepared_names: Vec<PreparedName>,
    prepared_base_ids: Vec<PreparedName>,
}

impl<'a> ActiveModNames<'a> {
    fn build(scanned: &'a [ScannedMod]) -> Self {
        Self {
            names: scanned
                .iter()
                .map(|sm| (&sm.info.id, sm.info.name.as_str()))
                .collect(),
            prepared_names: scanned
                .iter()
                .map(|sm| PreparedName::new(&sm.info.name))
                .collect(),
            prepared_base_ids: scanned
                .iter()
                .map(|sm| PreparedName::new(sm.info.id.base().as_str()))
                .collect(),
        }
    }

    /// The near-miss verdict for one written `PatchOperationFindMod` name.
    fn find_mod_near_miss(
        &self,
        written: &str,
        inactive: &[InactiveMod],
    ) -> Option<CandidateMatch<'_>> {
        if self.names.iter().any(|(_, name)| *name == written) {
            return None;
        }
        // A written value that already IS some active mod's own raw
        // package id (case-insensitive) — a real-install case: a
        // mod's own `FindMod` check writing a literal dotted package
        // id where the game expects a display name.
        // `PatchOperationFindMod` genuinely never matches this
        // in-game (it compares against `ModContentPack.Name`, never
        // the id), but it isn't a typo *of a display name* either:
        // `written` unambiguously already names one specific active
        // mod, so there is no "closer name" for this feature to
        // suggest. Checked by id, not display name — `best_match`
        // below would otherwise resolve it to that very same mod
        // under rule (b) once `normalize` strips the id's own dots,
        // reporting a "near miss" whose candidate is the mod the id
        // already, correctly identifies.
        if self
            .names
            .iter()
            .any(|(id, _)| id.as_str().eq_ignore_ascii_case(written))
        {
            return None;
        }
        if inactive.iter().any(|m| m.name == written) {
            return None;
        }
        let candidates = self
            .names
            .iter()
            .zip(&self.prepared_names)
            .map(|((id, name), prepared)| (*id, prepared, *name));
        best_match(written, candidates, true)
    }

    /// The near-miss verdict for one written `MayRequire`/`MayRequireAnyOf` id.
    fn may_require_near_miss(
        &self,
        written: &str,
        active: &ActiveMods,
        inactive: &[InactiveMod],
    ) -> Option<CandidateMatch<'_>> {
        if active.contains(&ModId::new(written)) {
            return None;
        }
        if inactive
            .iter()
            .any(|m| m.id.base() == ModId::new(written).base())
        {
            return None;
        }
        let candidates = self
            .prepared_base_ids
            .iter()
            .zip(&self.names)
            .map(|(base_id, (id, name))| (*id, base_id, *name));
        best_match(written, candidates, false)
    }
}

fn near_miss_finding(
    referrer: &ScannedMod,
    reference_kind: ModReferenceKind,
    written: String,
    best: CandidateMatch<'_>,
    locator: XmlLocator,
) -> Conflict {
    Conflict::NearMissModReference(NearMissModReference {
        referrer: referrer.info.id.clone(),
        reference_kind,
        written,
        candidate: best.id.clone(),
        candidate_name: best.display_name.to_string(),
        rule: best.rule,
        locator: Some(locator),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{DeclaredOrder, Mod, ModDependency, Source};
    use std::path::PathBuf;

    #[test]
    fn core_resource_index_warning_is_none_at_or_above_the_floor() {
        assert!(core_resource_index_warning(MIN_CORE_RESOURCE_TEXTURES).is_none());
        assert!(core_resource_index_warning(MIN_CORE_RESOURCE_TEXTURES + 1).is_none());
    }

    #[test]
    fn core_resource_index_warning_fires_below_the_floor() {
        let warning = core_resource_index_warning(0).expect("must warn below the floor");
        assert!(warning.message.contains("0 entries"));
    }

    fn mod_with(id: &str, declared: DeclaredOrder, supported_versions: Vec<String>) -> ScannedMod {
        ScannedMod {
            info: Mod {
                id: ModId::new(id),
                name: id.to_string(),
                authors: Vec::new(),
                url: None,
                path: PathBuf::from(id),
                source: Source::Local,
                supported_versions,
                declared,
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
            textures: std::collections::BTreeMap::new(),
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

    #[test]
    fn flags_dependency_on_an_inactive_mod() {
        let declared = DeclaredOrder {
            dependencies: vec![ModDependency {
                id: ModId::new("missing"),
                display_name: Some("Missing Mod".into()),
            }],
            ..DeclaredOrder::default()
        };
        let scanned = vec![mod_with("a", declared, vec![])];
        let active = ActiveMods::build(&scanned);

        let missing = missing_dependencies(&scanned, &active);

        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].mod_id, ModId::new("a"));
        assert_eq!(missing[0].dependency.id, ModId::new("missing"));
    }

    #[test]
    fn incompatible_pair_deduplicated_regardless_of_which_mod_declares_it() {
        let declared_a = DeclaredOrder {
            incompatible_with: vec![ModId::new("b")],
            ..DeclaredOrder::default()
        };
        let declared_b = DeclaredOrder {
            incompatible_with: vec![ModId::new("a")],
            ..DeclaredOrder::default()
        };
        let scanned = vec![
            mod_with("a", declared_a, vec![]),
            mod_with("b", declared_b, vec![]),
        ];
        let active = ActiveMods::build(&scanned);

        let pairs = incompatible_active_pairs(&scanned, &active);

        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn missing_dependency_matches_a_steam_suffixed_active_id() {
        let declared = DeclaredOrder {
            dependencies: vec![ModDependency {
                id: ModId::new("dep.mod"),
                display_name: None,
            }],
            ..DeclaredOrder::default()
        };
        let scanned = vec![
            mod_with("a", declared, vec![]),
            mod_with("dep.mod_steam", DeclaredOrder::default(), vec![]),
        ];
        let active = ActiveMods::build(&scanned);

        assert!(missing_dependencies(&scanned, &active).is_empty());
    }

    #[test]
    fn unsupported_version_flags_mods_missing_current_version() {
        let scanned = vec![
            mod_with("a", DeclaredOrder::default(), vec!["1.5".to_string()]),
            mod_with("b", DeclaredOrder::default(), vec!["1.6".to_string()]),
            mod_with("c", DeclaredOrder::default(), vec![]),
        ];
        let flagged = unsupported_version_mods(&scanned, GameVersion::new(1, 6));
        assert_eq!(flagged, vec![ModId::new("a")]);
    }

    // -- near_miss_mod_references ---------------------------------------

    fn mod_named(id: &str, name: &str) -> ScannedMod {
        let mut m = mod_with(id, DeclaredOrder::default(), vec![]);
        m.info.name = name.to_string();
        m
    }

    fn locator_for(ordinal: u32) -> XmlLocator {
        XmlLocator::new(
            std::sync::Arc::from(std::path::Path::new("test.xml")),
            vec![ordinal],
        )
    }

    fn find_mod_op(names: &[&str], ordinal: u32) -> crate::domain::PatchOp {
        crate::domain::PatchOp {
            class: "PatchOperationFindMod".to_string(),
            xpath: None,
            target: None,
            find_mod_context: Vec::new(),
            conditional_xpath: None,
            find_mod_names: names.iter().map(|s| (*s).to_string()).collect(),
            may_require: Vec::new(),
            may_require_any_of: Vec::new(),
            is_mutating: false,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
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
            locator: locator_for(ordinal),
        }
    }

    fn may_require_op(ids: &[&str], is_list_item: bool, ordinal: u32) -> crate::domain::PatchOp {
        crate::domain::PatchOp {
            class: "PatchOperationAdd".to_string(),
            xpath: Some("Defs".to_string()),
            target: None,
            find_mod_context: Vec::new(),
            conditional_xpath: None,
            find_mod_names: Vec::new(),
            may_require: ids.iter().map(|s| (*s).to_string()).collect(),
            may_require_any_of: Vec::new(),
            is_mutating: true,
            injected_types: std::collections::BTreeSet::new(),
            injected_paths: std::collections::BTreeSet::new(),
            injected_template_names: std::collections::BTreeSet::new(),
            is_list_item,
            load_folder_gate: Vec::new(),
            sequence_tail: true,
            conditional_branch: None,
            conditional_nomatch_creates: false,
            names_single_def: true,
            value_child_names: std::collections::BTreeSet::new(),
            toggle_active: true,
            value_root_names: Vec::new(),
            value_digest: None,
            locator: locator_for(ordinal),
        }
    }

    fn inactive_mod_named(id: &str, name: &str) -> InactiveMod {
        InactiveMod {
            id: ModId::new(id),
            name: name.to_string(),
            authors: Vec::new(),
            path: PathBuf::from(id),
            source: Source::Local,
            supported_versions: Vec::new(),
            workshop_id: None,
            generated: None,
            declared: DeclaredOrder::default(),
        }
    }

    fn only_near_miss(conflicts: Vec<Conflict>) -> Vec<NearMissModReference> {
        conflicts
            .into_iter()
            .map(|c| match c {
                Conflict::NearMissModReference(n) => n,
                other => panic!("expected only NearMissModReference, got {other:?}"),
            })
            .collect()
    }

    #[test]
    fn an_exact_inactive_mod_name_is_not_a_typo() {
        let mut referrer = mod_named("referrer", "Referrer");
        referrer.patch_ops = vec![find_mod_op(&["Old Mod"], 0)];
        let scanned = vec![referrer];
        let active = ActiveMods::build(&scanned);
        let inactive = vec![inactive_mod_named("old.mod", "Old Mod")];

        let found = near_miss_mod_references(&scanned, &active, &inactive);

        assert!(found.is_empty());
    }

    #[test]
    fn mayrequire_on_a_top_level_operation_is_ignored() {
        let mut referrer = mod_named("referrer", "Referrer");
        referrer.patch_ops = vec![may_require_op(&["typo.mod"], false, 0)];
        let scanned = vec![referrer, mod_named("typo.mod2", "Typo Mod 2")];
        let active = ActiveMods::build(&scanned);

        let found = near_miss_mod_references(&scanned, &active, &[]);

        assert!(
            found.is_empty(),
            "a top-level Operation's own MayRequire is never read by the game: {found:?}"
        );
    }

    #[test]
    fn mayrequire_on_a_sequence_li_is_checked() {
        let mut referrer = mod_named("referrer", "Referrer");
        referrer.patch_ops = vec![may_require_op(&["exampel.mod.long.enough"], true, 0)];
        let scanned = vec![
            referrer,
            mod_named("example.mod.long.enough", "Example Mod Long Enough"),
        ];
        let active = ActiveMods::build(&scanned);

        let found = only_near_miss(near_miss_mod_references(&scanned, &active, &[]));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reference_kind, ModReferenceKind::MayRequireId);
        assert_eq!(found[0].candidate, ModId::new("example.mod.long.enough"));
    }

    #[test]
    fn mayrequire_nested_inside_a_def_field_tree_is_checked() {
        // A keyed list-item element nested deep inside a def's own field
        // tree (e.g. `<need MayRequire="...">Bladder</need>` inside a
        // GeneDef), not on the def root and not a PatchOp list item — the
        // real-world gap `DefsFile::collect_nested_may_require` closes.
        let mut referrer = mod_named("referrer", "Referrer");
        referrer.nested_may_require = vec![("exampel.mod.long.enough".to_string(), locator_for(0))];
        let scanned = vec![
            referrer,
            mod_named("example.mod.long.enough", "Example Mod Long Enough"),
        ];
        let active = ActiveMods::build(&scanned);

        let found = only_near_miss(near_miss_mod_references(&scanned, &active, &[]));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reference_kind, ModReferenceKind::MayRequireId);
        assert_eq!(found[0].candidate, ModId::new("example.mod.long.enough"));
    }

    #[test]
    fn mayrequire_is_compared_case_insensitively_and_suffix_free() {
        let mut referrer = mod_named("referrer", "Referrer");
        referrer.patch_ops = vec![may_require_op(&["A.B_STEAM"], true, 0)];
        let scanned = vec![referrer, mod_named("a.b", "A B")];
        let active = ActiveMods::build(&scanned);

        let found = near_miss_mod_references(&scanned, &active, &[]);

        assert!(
            found.is_empty(),
            "already resolves under the case-insensitive, suffix-free MayRequire rule: {found:?}"
        );
    }

    #[test]
    fn one_row_per_written_value_per_referrer() {
        let mut referrer = mod_named("referrer", "Referrer");
        referrer.patch_ops = vec![
            may_require_op(&["exampel.mod.long.enough"], true, 0),
            may_require_op(&["exampel.mod.long.enough"], true, 1),
        ];
        let scanned = vec![
            referrer,
            mod_named("example.mod.long.enough", "Example Mod Long Enough"),
        ];
        let active = ActiveMods::build(&scanned);

        let found = near_miss_mod_references(&scanned, &active, &[]);

        assert_eq!(found.len(), 1);
    }

    #[test]
    fn a_case_only_find_mod_difference_is_rule_a() {
        let mut referrer = mod_named("referrer", "Referrer");
        referrer.patch_ops = vec![find_mod_op(&["Example Sidearms"], 0)];
        let scanned = vec![referrer, mod_named("example.sidearms", "example sidearms")];
        let active = ActiveMods::build(&scanned);

        let found = only_near_miss(near_miss_mod_references(&scanned, &active, &[]));

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].reference_kind, ModReferenceKind::FindModName);
        assert_eq!(found[0].rule, NearMissRule::CaseOnly);
    }

    /// Regression: a `FindMod` value that already IS some active mod's
    /// own raw package id (not its display name) must never surface as a
    /// "near miss" whose candidate is that very same mod — a real
    /// confirmed case where one mod's own `FindMod` check wrote another
    /// mod's literal dotted package id where a display name was
    /// expected; `normalize` strips the id's own dots and resolved it to
    /// a self-referential "Normalized" match before this fix.
    #[test]
    fn a_find_mod_value_matching_an_active_ids_own_text_is_not_a_near_miss() {
        let mut referrer = mod_named("referrer", "Referrer");
        referrer.patch_ops = vec![find_mod_op(&["dotted.mod.id"], 0)];
        let scanned = vec![referrer, mod_named("dotted.mod.id", "DottedModId")];
        let active = ActiveMods::build(&scanned);

        let found = near_miss_mod_references(&scanned, &active, &[]);

        assert!(
            found.is_empty(),
            "the written value already names an active mod by id: {found:?}"
        );
    }

    /// The same set of mods, scanned in two different orders, must
    /// produce the same set of findings — determinism under input
    /// reordering, a scoped-down, non-property-test check of the same
    /// invariant a full proptest would exercise more exhaustively.
    #[test]
    fn output_is_deterministic_under_a_referrer_order_shuffle() {
        let mut a = mod_named("a", "A");
        a.patch_ops = vec![may_require_op(&["exampel.mod.long.enough"], true, 0)];
        let mut b = mod_named("b", "B");
        b.patch_ops = vec![find_mod_op(&["Example Sidearms"], 0)];
        let c = mod_named("example.mod.long.enough", "Example Mod Long Enough");
        let d = mod_named("example.sidearms", "example sidearms");

        let forward = vec![a.clone(), b.clone(), c.clone(), d.clone()];
        let reversed = vec![d, c, b, a];

        let summarize = |mods: &[ScannedMod]| {
            let active = ActiveMods::build(mods);
            let mut rows: Vec<_> = near_miss_mod_references(mods, &active, &[])
                .into_iter()
                .map(|c| match c {
                    Conflict::NearMissModReference(n) => {
                        (n.referrer, n.reference_kind, n.written, n.candidate, n.rule)
                    }
                    other => panic!("expected only NearMissModReference, got {other:?}"),
                })
                .collect();
            rows.sort_by(|x, y| format!("{x:?}").cmp(&format!("{y:?}")));
            rows
        };

        assert_eq!(summarize(&forward), summarize(&reversed));
    }
}
