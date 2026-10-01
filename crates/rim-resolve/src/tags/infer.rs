//! [`infer_tags`]: turns [`TagEvidence`] plus [`TagRule`]s into a
//! [`Tagging`], then layers manual overrides on top.
//!
//! There is no shipped default tag rule (see `crate::domain::TagSignal`'s
//! own doc comment). Every tag comes from a user-authored [`TagRule`] or a
//! [`ManualTag`].

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;

use crate::domain::{
    Confidence, GeneratedModIdentity, ManualTag, Tag, TagAssignment, TagEvidence, TagMode,
    TagProvenance, TagRule, TagSignal, Tagging,
};

/// Runs every rule against every mod's evidence, then applies manual
/// overrides: a [`TagMode::Remove`] always wins over an inferred
/// assignment, and a [`TagMode::Add`] always applies regardless of
/// anything else.
///
/// The generated merge mod is excluded from the final output entirely,
/// regardless of what matched or what a manual override asked for —
/// [`super::evidence::collect_evidence`]/[`super::evidence::evidence_from_report`]
/// already never produce evidence for it, so no rule can match it in
/// practice, but this is the belt to that suspenders (see
/// `ledger::findings`'s own
/// second-line-of-defense filter for the same concern at the findings
/// layer).
#[must_use]
pub fn infer_tags(evidence: &[TagEvidence], rules: &[TagRule], manual: &[ManualTag]) -> Tagging {
    let mut by_pair: BTreeMap<(ModId, Tag), TagAssignment> = BTreeMap::new();

    for rule in rules {
        for one in evidence {
            let matched = matching_signals(one, &rule.any_of);
            if matched.is_empty() {
                continue;
            }
            let confidence = combined_confidence(&matched);
            let key = (one.mod_id.clone(), rule.tag.clone());
            by_pair.insert(
                key,
                TagAssignment {
                    mod_id: one.mod_id.clone(),
                    tag: rule.tag.clone(),
                    provenance: TagProvenance::Inferred {
                        matched,
                        confidence,
                    },
                },
            );
        }
    }

    for manual_tag in manual.iter().filter(|m| m.mode == TagMode::Remove) {
        by_pair.remove(&(manual_tag.mod_id.clone(), manual_tag.tag.clone()));
    }

    for manual_tag in manual.iter().filter(|m| m.mode == TagMode::Add) {
        let key = (manual_tag.mod_id.clone(), manual_tag.tag.clone());
        by_pair.insert(
            key,
            TagAssignment {
                mod_id: manual_tag.mod_id.clone(),
                tag: manual_tag.tag.clone(),
                provenance: TagProvenance::Manual,
            },
        );
    }

    Tagging::new(
        by_pair
            .into_values()
            .filter(|assignment| !GeneratedModIdentity::is_generated(&assignment.mod_id))
            .collect(),
    )
}

fn matching_signals(evidence: &TagEvidence, signals: &[TagSignal]) -> Vec<TagSignal> {
    signals
        .iter()
        .filter(|signal| signal_matches(evidence, signal))
        .cloned()
        .collect()
}

fn signal_matches(evidence: &TagEvidence, signal: &TagSignal) -> bool {
    match signal {
        TagSignal::UrlContains(pattern) => evidence
            .url
            .as_deref()
            .is_some_and(|url| url.to_lowercase().contains(&pattern.to_lowercase())),
        TagSignal::AssemblyRefTo(target) => evidence.assembly_refs_to.contains(&target.base()),
        TagSignal::DependsOn(target) => evidence.declares_after.contains(&target.base()),
    }
}

/// A signal's base weight: a DLL reference is the strongest (an
/// author-verifiable fact), a URL substring next, then a declared
/// dependency.
fn signal_weight(signal: &TagSignal) -> u8 {
    match signal {
        TagSignal::AssemblyRefTo(_) => 90,
        TagSignal::UrlContains(_) => 85,
        TagSignal::DependsOn(_) => 75,
    }
}

/// The highest matched signal's weight, plus 5 for every additional
/// matched signal, capped at 99 (100 is reserved for a manual
/// assignment's implicit certainty).
pub(crate) fn combined_confidence(matched: &[TagSignal]) -> Confidence {
    let mut weights: Vec<u8> = matched.iter().map(signal_weight).collect();
    weights.sort_unstable_by(|left, right| right.cmp(left));
    let base = weights.first().copied().unwrap_or(0);
    let extra_signals = u8::try_from(weights.len().saturating_sub(1)).unwrap_or(u8::MAX);
    let percent = base.saturating_add(extra_signals.saturating_mul(5)).min(99);
    match Confidence::new(percent) {
        Ok(confidence) => confidence,
        Err(_) => {
            unreachable!("percent is clamped to <=99, always within Confidence's 0..=100 range")
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::Source;

    use super::*;

    fn evidence(mod_id: &str) -> TagEvidence {
        TagEvidence {
            mod_id: ModId::new(mod_id),
            url: None,
            source: Source::Workshop,
            def_type_namespaces: [].into_iter().collect(),
            def_name_prefixes: [].into_iter().collect(),
            assembly_refs_to: [].into_iter().collect(),
            declares_after: [].into_iter().collect(),
        }
    }

    fn tag(name: &str) -> Tag {
        Tag::new(name).unwrap()
    }

    #[test]
    fn url_contains_signal_matches_case_insensitively() {
        let mut one = evidence("addon");
        one.url = Some("https://www.MODS.Example/files/x".to_string());
        let rule = TagRule {
            tag: tag("framework"),
            any_of: vec![TagSignal::UrlContains("mods.example".to_string())],
        };

        let tagging = infer_tags(&[one], &[rule], &[]);

        assert!(
            tagging
                .members(&tag("framework"))
                .any(|id| *id == ModId::new("addon"))
        );
    }

    #[test]
    fn assembly_ref_to_signal_matches_by_base_id() {
        let mut one = evidence("addon");
        one.assembly_refs_to.insert(ModId::new("example.framework"));
        let rule = TagRule {
            tag: tag("framework"),
            any_of: vec![TagSignal::AssemblyRefTo(ModId::new(
                "example.framework_steam",
            ))],
        };

        let tagging = infer_tags(&[one], &[rule], &[]);

        assert!(
            tagging
                .members(&tag("framework"))
                .any(|id| *id == ModId::new("addon"))
        );
    }

    #[test]
    fn depends_on_signal_matches_a_declared_relation() {
        let mut one = evidence("addon");
        one.declares_after.insert(ModId::new("core.framework"));
        let rule = TagRule {
            tag: tag("framework-addon"),
            any_of: vec![TagSignal::DependsOn(ModId::new("core.framework"))],
        };

        let tagging = infer_tags(&[one], &[rule], &[]);

        assert!(
            tagging
                .members(&tag("framework-addon"))
                .any(|id| *id == ModId::new("addon"))
        );
    }

    #[test]
    fn non_matching_evidence_earns_no_tag() {
        let one = evidence("vanilla.like");
        let rule = TagRule {
            tag: tag("framework"),
            any_of: vec![TagSignal::UrlContains("mods.example".to_string())],
        };

        let tagging = infer_tags(&[one], &[rule], &[]);

        assert!(tagging.tags_of(&ModId::new("vanilla.like")).is_empty());
    }

    #[test]
    fn confidence_is_the_strongest_signals_weight_when_only_one_matches() {
        let mut one = evidence("addon");
        one.assembly_refs_to.insert(ModId::new("example.framework"));
        let rule = TagRule {
            tag: tag("framework"),
            any_of: vec![TagSignal::AssemblyRefTo(ModId::new("example.framework"))],
        };

        let tagging = infer_tags(&[one], &[rule], &[]);

        let assignment = tagging
            .assignments()
            .iter()
            .find(|a| a.mod_id == ModId::new("addon"))
            .unwrap();
        match &assignment.provenance {
            TagProvenance::Inferred { confidence, .. } => assert_eq!(confidence.percent(), 90),
            TagProvenance::Manual => panic!("expected an inferred assignment"),
        }
    }

    #[test]
    fn combination_bonus_adds_5_per_extra_signal_capped_at_99() {
        let mut one = evidence("addon");
        one.assembly_refs_to.insert(ModId::new("example.framework")); // 90
        one.url = Some("https://mods.example/x".to_string()); // 85
        one.declares_after.insert(ModId::new("example.framework")); // 75
        let rule = TagRule {
            tag: tag("framework"),
            any_of: vec![
                TagSignal::UrlContains("mods.example".to_string()),
                TagSignal::AssemblyRefTo(ModId::new("example.framework")),
                TagSignal::DependsOn(ModId::new("example.framework")),
            ],
        };

        let tagging = infer_tags(&[one], &[rule], &[]);

        let assignment = tagging
            .assignments()
            .iter()
            .find(|a| a.mod_id == ModId::new("addon"))
            .unwrap();
        match &assignment.provenance {
            // base 90 + 2 extra signals * 5 = 100, capped to 99.
            TagProvenance::Inferred {
                confidence,
                matched,
            } => {
                assert_eq!(matched.len(), 3);
                assert_eq!(confidence.percent(), 99);
            }
            TagProvenance::Manual => panic!("expected an inferred assignment"),
        }
    }

    #[test]
    fn manual_remove_beats_an_inferred_assignment() {
        let mut one = evidence("addon");
        one.assembly_refs_to.insert(ModId::new("example.framework"));
        let rule = TagRule {
            tag: tag("framework"),
            any_of: vec![TagSignal::AssemblyRefTo(ModId::new("example.framework"))],
        };
        let manual = vec![ManualTag {
            mod_id: ModId::new("addon"),
            tag: tag("framework"),
            mode: TagMode::Remove,
        }];

        let tagging = infer_tags(&[one], &[rule], &manual);

        assert!(tagging.tags_of(&ModId::new("addon")).is_empty());
    }

    #[test]
    fn manual_add_always_applies_even_with_no_matching_evidence() {
        let one = evidence("plain.mod");
        let manual = vec![ManualTag {
            mod_id: ModId::new("plain.mod"),
            tag: tag("framework"),
            mode: TagMode::Add,
        }];

        let tagging = infer_tags(&[one], &[], &manual);

        assert!(
            tagging
                .members(&tag("framework"))
                .any(|id| *id == ModId::new("plain.mod"))
        );
        let assignment = tagging
            .assignments()
            .iter()
            .find(|a| a.mod_id == ModId::new("plain.mod"))
            .unwrap();
        assert_eq!(assignment.provenance, TagProvenance::Manual);
    }

    #[test]
    fn manual_add_wins_even_when_a_remove_for_the_same_pair_is_also_present() {
        let one = evidence("addon");
        let manual = vec![
            ManualTag {
                mod_id: ModId::new("addon"),
                tag: tag("framework"),
                mode: TagMode::Remove,
            },
            ManualTag {
                mod_id: ModId::new("addon"),
                tag: tag("framework"),
                mode: TagMode::Add,
            },
        ];

        let tagging = infer_tags(&[one], &[], &manual);

        assert!(
            tagging
                .members(&tag("framework"))
                .any(|id| *id == ModId::new("addon"))
        );
    }

    /// The generated merge mod must earn no tag at all, even from a
    /// signal it would otherwise match — a defense-in-depth exclusion, so
    /// Rimmerge's own output never feeds back into tag inference.
    #[test]
    fn a_generated_mod_gets_no_tag_even_when_it_would_otherwise_match() {
        let mut generated = evidence("rimmerge.merge.abc123456789");
        generated.url = Some("https://mods.example/x".to_string());
        let real = {
            let mut e = evidence("addon");
            e.url = Some("https://mods.example/x".to_string());
            e
        };
        let rule = TagRule {
            tag: tag("framework"),
            any_of: vec![TagSignal::UrlContains("mods.example".to_string())],
        };

        let tagging = infer_tags(&[generated, real], &[rule], &[]);

        assert!(
            tagging
                .tags_of(&ModId::new("rimmerge.merge.abc123456789"))
                .is_empty(),
            "the generated merge mod must never carry an inferred tag"
        );
        assert!(
            tagging
                .members(&tag("framework"))
                .any(|id| *id == ModId::new("addon")),
            "a real matching mod must still be tagged normally"
        );
    }

    /// Even a manual `Add` override — which normally wins over everything
    /// else, see [`manual_add_always_applies_even_with_no_matching_evidence`]
    /// — must not be able to tag the generated merge mod.
    #[test]
    fn a_manual_add_cannot_tag_the_generated_merge_mod_either() {
        let manual = vec![ManualTag {
            mod_id: ModId::new("rimmerge.merge.abc123456789"),
            tag: tag("framework"),
            mode: TagMode::Add,
        }];

        let tagging = infer_tags(&[], &[], &manual);

        assert!(
            tagging
                .tags_of(&ModId::new("rimmerge.merge.abc123456789"))
                .is_empty()
        );
    }
}
