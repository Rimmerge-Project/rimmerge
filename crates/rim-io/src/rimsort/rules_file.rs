//! Shared parsing for RimSort's `userRules.json`/`communityRules.json`
//! shape: `{ "rules": { "<packageId>": { loadAfter, loadBefore, loadTop,
//! loadBottom, incompatibleWith } } }`. Both files use this shape; only
//! the [`RuleOrigin`] tagging the result differs.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{IncompatibleRule, PairRule, Placement, PlacementRule, Rule, RuleOrigin};
use serde::{Deserialize, Deserializer};

/// A comment RimSort stores as either a bare string or a list of strings
/// (both shapes appear in the real `communityRules.json`) — joined with
/// `"; "` when it's a list.
fn deserialize_flexible_comment<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flexible {
        One(String),
        Many(Vec<String>),
    }
    Ok(
        Option::<Flexible>::deserialize(deserializer)?.map(|value| match value {
            Flexible::One(text) => text,
            Flexible::Many(parts) => parts.join("; "),
        }),
    )
}

#[derive(Debug, Default, Deserialize)]
struct RelatedRule {
    #[serde(default, deserialize_with = "deserialize_flexible_comment")]
    comment: Option<String>,
}

/// `loadTop`/`loadBottom` without a `value` field means `false` — RimSort
/// commonly writes `{"comment": ""}` with no `value` at all.
#[derive(Debug, Default, Deserialize)]
struct LoadFlag {
    #[serde(default)]
    value: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModRuleEntry {
    #[serde(default)]
    load_after: BTreeMap<String, RelatedRule>,
    #[serde(default)]
    load_before: BTreeMap<String, RelatedRule>,
    #[serde(default)]
    incompatible_with: BTreeMap<String, RelatedRule>,
    #[serde(default)]
    load_top: Option<LoadFlag>,
    #[serde(default)]
    load_bottom: Option<LoadFlag>,
}

#[derive(Debug, Default, Deserialize)]
struct RulesFile {
    #[serde(default)]
    rules: BTreeMap<String, ModRuleEntry>,
}

/// `id` must load after every active key of `related`. Pushes one
/// [`Rule::Pair`] per active relation into `rules`, and counts the rest
/// in `skipped`.
fn push_load_after(
    id: &ModId,
    related: &BTreeMap<String, RelatedRule>,
    origin: RuleOrigin,
    active: &BTreeSet<ModId>,
    rules: &mut Vec<Rule>,
    skipped: &mut usize,
) {
    for (other_raw, relation) in related {
        let other = ModId::new(other_raw).base();
        if active.contains(&other) {
            rules.push(Rule::Pair(PairRule {
                after: id.clone(),
                before: other,
                origin,
                comment: relation.comment.clone(),
                overrides_declared: false,
            }));
        } else {
            *skipped += 1;
        }
    }
}

/// `id` must load before every active key of `related` — the same fact
/// as "each of them loads after `id`", flipped into [`PairRule`]'s one
/// direction.
fn push_load_before(
    id: &ModId,
    related: &BTreeMap<String, RelatedRule>,
    origin: RuleOrigin,
    active: &BTreeSet<ModId>,
    rules: &mut Vec<Rule>,
    skipped: &mut usize,
) {
    for (other_raw, relation) in related {
        let other = ModId::new(other_raw).base();
        if active.contains(&other) {
            rules.push(Rule::Pair(PairRule {
                after: other,
                before: id.clone(),
                origin,
                comment: relation.comment.clone(),
                overrides_declared: false,
            }));
        } else {
            *skipped += 1;
        }
    }
}

/// `id` is incompatible with every active key of `related`.
fn push_incompatible_with(
    id: &ModId,
    related: &BTreeMap<String, RelatedRule>,
    origin: RuleOrigin,
    active: &BTreeSet<ModId>,
    rules: &mut Vec<Rule>,
    skipped: &mut usize,
) {
    for other_raw in related.keys() {
        let other = ModId::new(other_raw).base();
        if active.contains(&other) {
            rules.push(Rule::Incompatible(IncompatibleRule {
                a: id.clone(),
                b: other,
                origin,
            }));
        } else {
            *skipped += 1;
        }
    }
}

/// Pushes a [`Rule::Placement`] for `id` when `flag` is set to `true`
/// (see [`LoadFlag`]'s doc comment for why a missing `value` means
/// `false`, not "unset").
fn push_placement(
    id: &ModId,
    flag: Option<&LoadFlag>,
    placement: Placement,
    origin: RuleOrigin,
    rules: &mut Vec<Rule>,
) {
    if flag.is_some_and(|flag| flag.value) {
        rules.push(Rule::Placement(PlacementRule {
            mod_id: id.clone(),
            placement,
            origin,
            comment: None,
        }));
    }
}

/// How many relations `entry` names in total — used to count every one
/// of them skipped at once when `entry`'s own mod isn't active, without
/// checking each active mod's own relations individually.
fn relation_count(entry: &ModRuleEntry) -> usize {
    entry.load_after.len()
        + entry.load_before.len()
        + entry.incompatible_with.len()
        + usize::from(entry.load_top.as_ref().is_some_and(|flag| flag.value))
        + usize::from(entry.load_bottom.as_ref().is_some_and(|flag| flag.value))
}

/// Parses `bytes` (a RimSort `userRules.json`/`communityRules.json`) into
/// [`Rule`]s tagged `origin`, keeping only rules where every named mod's
/// base id (see [`ModId::base`]) is in `active`. Returns the rules plus a
/// count of relations skipped because at least one side wasn't active.
pub(crate) fn parse(
    bytes: &[u8],
    origin: RuleOrigin,
    active: &BTreeSet<ModId>,
) -> Result<(Vec<Rule>, usize), serde_json::Error> {
    let file: RulesFile = serde_json::from_slice(bytes)?;
    let mut rules = Vec::new();
    let mut skipped = 0usize;

    for (raw_id, entry) in &file.rules {
        let id = ModId::new(raw_id).base();
        if !active.contains(&id) {
            skipped += relation_count(entry);
            continue;
        }

        push_load_after(
            &id,
            &entry.load_after,
            origin,
            active,
            &mut rules,
            &mut skipped,
        );
        push_load_before(
            &id,
            &entry.load_before,
            origin,
            active,
            &mut rules,
            &mut skipped,
        );
        push_incompatible_with(
            &id,
            &entry.incompatible_with,
            origin,
            active,
            &mut rules,
            &mut skipped,
        );
        push_placement(
            &id,
            entry.load_top.as_ref(),
            Placement::Top,
            origin,
            &mut rules,
        );
        push_placement(
            &id,
            entry.load_bottom.as_ref(),
            Placement::Bottom,
            origin,
            &mut rules,
        );
    }

    Ok((rules, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active(ids: &[&str]) -> BTreeSet<ModId> {
        ids.iter().map(|id| ModId::new(*id)).collect()
    }

    #[test]
    fn load_top_and_bottom_without_a_value_field_mean_false() {
        let json = br#"{"rules":{"a":{"loadBottom":{"comment":""},"loadTop":{"comment":""}}}}"#;
        let (rules, _) = parse(json, RuleOrigin::RimSortUser, &active(&["a"])).expect("parse");
        assert!(rules.is_empty());
    }

    #[test]
    fn load_bottom_with_value_true_becomes_a_placement_rule() {
        let json = br#"{"rules":{"a":{"loadBottom":{"value":true}}}}"#;
        let (rules, _) = parse(json, RuleOrigin::RimSortCommunity, &active(&["a"])).expect("parse");
        assert_eq!(rules.len(), 1);
        assert!(matches!(
            &rules[0],
            Rule::Placement(PlacementRule {
                placement: Placement::Bottom,
                ..
            })
        ));
    }

    #[test]
    fn load_after_becomes_a_pair_rule_when_both_sides_are_active() {
        let json = br#"{"rules":{"a":{"loadAfter":{"b":{"comment":"note"}}}}}"#;
        let (rules, skipped) =
            parse(json, RuleOrigin::RimSortUser, &active(&["a", "b"])).expect("parse");
        assert_eq!(skipped, 0);
        match &rules[0] {
            Rule::Pair(pair) => {
                assert_eq!(pair.after, ModId::new("a"));
                assert_eq!(pair.before, ModId::new("b"));
                assert_eq!(pair.comment.as_deref(), Some("note"));
            }
            other => panic!("expected a pair rule, got {other:?}"),
        }
    }

    #[test]
    fn load_before_is_flipped_into_the_after_direction() {
        let json = br#"{"rules":{"a":{"loadBefore":{"b":{}}}}}"#;
        let (rules, _) = parse(json, RuleOrigin::RimSortUser, &active(&["a", "b"])).expect("parse");
        match &rules[0] {
            Rule::Pair(pair) => {
                assert_eq!(pair.after, ModId::new("b"));
                assert_eq!(pair.before, ModId::new("a"));
            }
            other => panic!("expected a pair rule, got {other:?}"),
        }
    }

    #[test]
    fn incompatible_with_becomes_an_incompatible_rule() {
        let json = br#"{"rules":{"a":{"incompatibleWith":{"b":{}}}}}"#;
        let (rules, _) =
            parse(json, RuleOrigin::RimSortCommunity, &active(&["a", "b"])).expect("parse");
        assert!(matches!(&rules[0], Rule::Incompatible(_)));
    }

    #[test]
    fn a_rule_naming_an_inactive_mod_is_skipped() {
        let json = br#"{"rules":{"a":{"loadAfter":{"ghost":{}}}}}"#;
        let (rules, skipped) =
            parse(json, RuleOrigin::RimSortUser, &active(&["a"])).expect("parse");
        assert!(rules.is_empty());
        assert_eq!(skipped, 1);
    }

    #[test]
    fn the_top_level_mod_being_inactive_skips_every_relation() {
        let json = br#"{"rules":{"ghost":{"loadAfter":{"a":{}},"loadBefore":{"a":{}}}}}"#;
        let (rules, skipped) =
            parse(json, RuleOrigin::RimSortUser, &active(&["a"])).expect("parse");
        assert!(rules.is_empty());
        assert_eq!(skipped, 2);
    }

    #[test]
    fn comment_as_a_list_is_joined() {
        let json = br#"{"rules":{"a":{"loadAfter":{"b":{"comment":["first","second"]}}}}}"#;
        let (rules, _) =
            parse(json, RuleOrigin::RimSortCommunity, &active(&["a", "b"])).expect("parse");
        match &rules[0] {
            Rule::Pair(pair) => assert_eq!(pair.comment.as_deref(), Some("first; second")),
            other => panic!("expected a pair rule, got {other:?}"),
        }
    }

    #[test]
    fn matching_ignores_the_steam_suffix() {
        let json = br#"{"rules":{"a_steam":{"loadAfter":{"b":{}}}}}"#;
        let (rules, _) = parse(json, RuleOrigin::RimSortUser, &active(&["a", "b"])).expect("parse");
        assert_eq!(rules.len(), 1);
    }
}
