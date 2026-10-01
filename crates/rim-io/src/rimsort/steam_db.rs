//! Parses RimSort's `steamDB.json`: a field-minimal [`serde::Deserialize`]
//! target keeping only `packageId` and `dependencies` — the real file is
//! ~50 MB and mostly fields this importer never needs (names, urls,
//! authors, game-version tags, ...).

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{PairRule, Rule, RuleOrigin};
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
struct SteamDbEntry {
    #[serde(default, rename = "packageId")]
    package_id: Option<String>,
    #[serde(default)]
    dependencies: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
struct SteamDbFile {
    #[serde(default)]
    database: BTreeMap<String, SteamDbEntry>,
}

/// Resolves one database entry (keyed by `workshop_key`, its own database
/// key parsed as a workshop id) to an active mod's base id — workshop id
/// first, `packageId` only as a fallback for a mod with none. Matching by
/// `packageId` alone
/// would conflate two different uploads that happen to share one: this
/// entry is only the fallback match when the active mod carrying that
/// `packageId` has no workshop id of its own to have matched by instead —
/// an active mod that *does* have one is only ever resolved by its own
/// correct entry, never by a different upload's.
fn resolve_active_id(
    workshop_key: Option<u64>,
    package_id: Option<&str>,
    by_workshop_id: &BTreeMap<u64, ModId>,
    active_without_workshop_id: &BTreeSet<ModId>,
) -> Option<ModId> {
    if let Some(key) = workshop_key
        && let Some(id) = by_workshop_id.get(&key)
    {
        return Some(id.clone());
    }
    let candidate = ModId::new(package_id?).base();
    active_without_workshop_id
        .contains(&candidate)
        .then_some(candidate)
}

/// Parses `bytes` (RimSort's `steamDB.json`) into `after`-loads-after-
/// `before` [`PairRule`]s: for every entry whose own upload resolves to an
/// active mod (see [`resolve_active_id`]), for every dependency workshop
/// id that itself resolves (within the same database) to an active mod.
/// Returns the rules plus a count of *skipped* entries — a genuinely
/// inactive `packageId` that actually had dependencies to report. An
/// entry that fails to resolve only because it's a *different* upload of
/// an otherwise-active `packageId` (matched by its own, different entry)
/// is not counted: it was never the mod's own data, so counting it would
/// misreport a correctly-ignored duplicate as a meaningful skip. An
/// inactive entry with no dependencies at all (the overwhelming majority of
/// the real tens-of-thousands-entry database — mods no active mod could
/// ever depend on) isn't counted either, for the same "never a candidate
/// relation" reason the original rule already gave.
pub(crate) fn parse(
    bytes: &[u8],
    active: &BTreeMap<ModId, Option<u64>>,
) -> Result<(Vec<Rule>, usize), serde_json::Error> {
    let file: SteamDbFile = serde_json::from_slice(bytes)?;
    let mut rules = Vec::new();
    let mut skipped = 0usize;

    let by_workshop_id: BTreeMap<u64, ModId> = active
        .iter()
        .filter_map(|(id, workshop_id)| workshop_id.map(|w| (w, id.clone())))
        .collect();
    let active_without_workshop_id: BTreeSet<ModId> = active
        .iter()
        .filter(|(_, workshop_id)| workshop_id.is_none())
        .map(|(id, _)| id.clone())
        .collect();

    for (key, entry) in &file.database {
        let workshop_key = key.parse::<u64>().ok();
        let Some(dependent) = resolve_active_id(
            workshop_key,
            entry.package_id.as_deref(),
            &by_workshop_id,
            &active_without_workshop_id,
        ) else {
            let genuinely_inactive = entry
                .package_id
                .as_deref()
                .map(|package_id| !active.contains_key(&ModId::new(package_id).base()))
                .unwrap_or(true);
            if genuinely_inactive && !entry.dependencies.is_empty() {
                skipped += 1;
            }
            continue;
        };
        for dependency_key in entry.dependencies.keys() {
            let Some(dependency_entry) = file.database.get(dependency_key) else {
                continue;
            };
            let dependency_workshop_key = dependency_key.parse::<u64>().ok();
            let Some(dependency) = resolve_active_id(
                dependency_workshop_key,
                dependency_entry.package_id.as_deref(),
                &by_workshop_id,
                &active_without_workshop_id,
            ) else {
                continue;
            };
            rules.push(Rule::Pair(PairRule {
                after: dependent.clone(),
                before: dependency,
                origin: RuleOrigin::SteamDb,
                comment: None,
                overrides_declared: false,
            }));
        }
    }

    Ok((rules, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(ids: &[&str]) -> BTreeMap<ModId, Option<u64>> {
        ids.iter().map(|id| (ModId::new(*id), None)).collect()
    }

    #[test]
    fn a_dependency_becomes_a_pair_rule_when_both_sides_resolve_and_are_active() {
        let json = br#"{
            "database": {
                "100": { "packageId": "addon.mod", "dependencies": { "200": ["Framework", "url"] } },
                "200": { "packageId": "framework.mod" }
            }
        }"#;

        let (rules, skipped) =
            parse(json, &active(&["addon.mod", "framework.mod"])).expect("parse");

        assert_eq!(skipped, 0);
        match &rules[0] {
            Rule::Pair(pair) => {
                assert_eq!(pair.after, ModId::new("addon.mod"));
                assert_eq!(pair.before, ModId::new("framework.mod"));
                assert_eq!(pair.origin, RuleOrigin::SteamDb);
            }
            other => panic!("expected a pair rule, got {other:?}"),
        }
    }

    #[test]
    fn an_inactive_entry_with_dependencies_is_skipped_and_counted() {
        let json = br#"{
            "database": {
                "100": { "packageId": "inactive.mod", "dependencies": { "200": ["Framework", "url"] } }
            }
        }"#;
        let (rules, skipped) = parse(json, &active(&["something.else"])).expect("parse");
        assert!(rules.is_empty());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn an_inactive_entry_with_no_dependencies_is_not_counted() {
        let json = br#"{"database": {"100": {"packageId": "inactive.mod"}}}"#;
        let (rules, skipped) = parse(json, &active(&["something.else"])).expect("parse");
        assert!(rules.is_empty());
        assert_eq!(
            skipped, 0,
            "an inactive entry with nothing to depend on is not a meaningful skip"
        );
    }

    #[test]
    fn an_entry_with_no_package_id_at_all_is_ignored_without_counting() {
        let json = br#"{"database": {"100": {"steamName": "not a mod, e.g. a DLC placeholder"}}}"#;
        let (rules, skipped) = parse(json, &active(&["a"])).expect("parse");
        assert!(rules.is_empty());
        assert_eq!(skipped, 0);
    }

    #[test]
    fn a_dependency_that_does_not_resolve_to_an_active_mod_produces_no_rule() {
        let json = br#"{
            "database": {
                "100": { "packageId": "addon.mod", "dependencies": { "200": ["Framework", "url"] } },
                "200": { "packageId": "framework.mod" }
            }
        }"#;
        // "framework.mod" isn't active this time.
        let (rules, _) = parse(json, &active(&["addon.mod"])).expect("parse");
        assert!(rules.is_empty());
    }

    /// The Example Security shape: two
    /// uploads of the same `packageId` in the database, only one of which
    /// (identified by its own workshop id — the JSON key) is the actually
    /// installed copy. Matching by `packageId` alone would merge both
    /// uploads' dependencies onto the installed one; instead only the
    /// matching-workshop-id upload's own dependencies (here:
    /// none) are used, and the other upload's dependency is never
    /// produced as a rule.
    #[test]
    fn only_the_matching_workshop_id_uploads_dependencies_are_used() {
        let json = br#"{
            "database": {
                "3000000011": { "packageId": "example.vecontent.security" },
                "9999999999": {
                    "packageId": "example.vecontent.security",
                    "dependencies": { "100": ["Unrelated framework", "url"] }
                },
                "100": { "packageId": "some.framework" }
            }
        }"#;
        let mut active = active(&["example.vecontent.security", "some.framework"]);
        active.insert(
            ModId::new("example.vecontent.security"),
            Some(3_000_000_011),
        );

        let (rules, _) = parse(json, &active).expect("parse");

        assert!(
            rules.is_empty(),
            "the second, non-matching upload's dependencies must never be merged onto the installed copy: {rules:?}"
        );
    }

    #[test]
    fn a_mod_with_no_workshop_id_still_matches_by_package_id() {
        let json = br#"{
            "database": {
                "100": { "packageId": "local.mod", "dependencies": { "200": ["Framework", "url"] } },
                "200": { "packageId": "framework.mod" }
            }
        }"#;
        // Neither active mod carries a workshop id (e.g. both are local).
        let (rules, _) = parse(json, &active(&["local.mod", "framework.mod"])).expect("parse");

        assert_eq!(rules.len(), 1);
    }
}
