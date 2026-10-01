//! Tests for the port types.

use rim_resolve::domain::{Placement, RuleOrigin};

use super::*;
use crate::app_settings::StaleAfterDays;
use crate::settings::Settings;
use rim_analyzer::domain::ModId;
use rim_resolve::domain::{PairRule, PlacementRule, Rule};
use std::path::PathBuf;

#[test]
fn sniffs_png_by_its_magic_bytes() {
    assert_eq!(
        TextureFormat::sniff(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A]),
        Some(TextureFormat::Png)
    );
}

#[test]
fn sniffs_jpeg_by_its_magic_bytes() {
    assert_eq!(
        TextureFormat::sniff(&[0xFF, 0xD8, 0xFF, 0xE0]),
        Some(TextureFormat::Jpeg)
    );
}

/// The CLI prints these words on its scan progress line; they are output
/// other tooling may read, so they are pinned.
#[test]
fn every_scan_stage_keeps_its_english_progress_label() {
    let labels = [
        (ScanProgressStage::Discovering, "discovering mods"),
        (ScanProgressStage::Scanning, "scanning mods"),
        (ScanProgressStage::Analyzing, "analyzing"),
        (
            ScanProgressStage::CollectingTagEvidence,
            "collecting tag evidence",
        ),
        (ScanProgressStage::Done, "done"),
    ];

    for (stage, expected) in labels {
        assert_eq!(stage.english_label(), expected);
    }
}

fn status_with(cached: Option<CachedDatabase>) -> DatabaseStatus {
    DatabaseStatus {
        database: RuleDatabase::CommunityRules,
        enabled: true,
        path: PathBuf::from("communityRules.json"),
        cached,
        last_failure: None,
        last_attempt_at: None,
    }
}

#[test]
fn never_fetched_is_never_stale() {
    let status = status_with(None);
    assert!(!status.is_stale(jiff::Timestamp::now(), StaleAfterDays::default()));
}

#[test]
fn fetched_29_days_ago_is_not_stale() {
    let fetched_at = jiff::Timestamp::UNIX_EPOCH;
    let now = fetched_at + jiff::Span::new().hours(29 * 24);
    let status = status_with(Some(CachedDatabase {
        sha256: "abc".to_string(),
        bytes: 1,
        fetched_at,
    }));
    assert!(!status.is_stale(now, StaleAfterDays::default()));
}

#[test]
fn fetched_31_days_ago_is_stale() {
    let fetched_at = jiff::Timestamp::UNIX_EPOCH;
    let now = fetched_at + jiff::Span::new().hours(31 * 24);
    let status = status_with(Some(CachedDatabase {
        sha256: "abc".to_string(),
        bytes: 1,
        fetched_at,
    }));
    assert!(status.is_stale(now, StaleAfterDays::default()));
}

/// The exact boundary: `threshold` days is not stale, `threshold` days
/// plus one second is — `>`, never `>=`.
#[test]
fn the_exact_threshold_boundary() {
    let fetched_at = jiff::Timestamp::UNIX_EPOCH;
    let status = status_with(Some(CachedDatabase {
        sha256: "abc".to_string(),
        bytes: 1,
        fetched_at,
    }));
    let threshold = StaleAfterDays::new(30).unwrap();

    let exactly_30_days = fetched_at + jiff::Span::new().hours(30 * 24);
    assert!(!status.is_stale(exactly_30_days, threshold));

    let one_second_past = exactly_30_days + jiff::Span::new().seconds(1);
    assert!(status.is_stale(one_second_past, threshold));
}

/// A non-default threshold is actually honoured, not just the constant
/// it replaced.
#[test]
fn a_custom_threshold_is_honoured() {
    let fetched_at = jiff::Timestamp::UNIX_EPOCH;
    let now = fetched_at + jiff::Span::new().hours(8 * 24);
    let status = status_with(Some(CachedDatabase {
        sha256: "abc".to_string(),
        bytes: 1,
        fetched_at,
    }));
    assert!(status.is_stale(now, StaleAfterDays::new(7).unwrap()));
    assert!(!status.is_stale(now, StaleAfterDays::new(9).unwrap()));
}

/// A `fetched_at` after `now` (the clock moved back since the fetch) can never
/// read as fresh: it is stale, like every other stored-timestamp check.
#[test]
fn a_fetch_stamped_in_the_future_is_stale() {
    let now = jiff::Timestamp::UNIX_EPOCH;
    let status = status_with(Some(CachedDatabase {
        sha256: "abc".to_string(),
        bytes: 1,
        fetched_at: now + jiff::Span::new().seconds(1),
    }));
    assert!(status.is_stale(now, StaleAfterDays::default()));
}

#[test]
fn fetched_just_now_is_not_stale() {
    let now = jiff::Timestamp::now();
    let status = status_with(Some(CachedDatabase {
        sha256: "abc".to_string(),
        bytes: 1,
        fetched_at: now,
    }));
    assert!(!status.is_stale(now, StaleAfterDays::default()));
}

#[test]
fn rejects_anything_else() {
    assert_eq!(TextureFormat::sniff(b"GIF89a"), None);
    assert_eq!(TextureFormat::sniff(&[]), None);
}

#[test]
fn rule_set_merges_every_rule_shape_in_precedence_order() {
    let stored = StoredRules {
        pairs: vec![PairRule {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::RimSortCommunity,
            comment: None,
            overrides_declared: false,
        }],
        placements: vec![PlacementRule {
            mod_id: ModId::new("c"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
            comment: None,
        }],
        incompatibles: Vec::new(),
        tag_rules: Vec::new(),
        manual_tags: Vec::new(),
        settings: Settings::default(),
    };

    let rule_set = stored.rule_set();

    let origins: Vec<RuleOrigin> = rule_set.iter().map(Rule::origin).collect();
    assert_eq!(
        origins,
        vec![RuleOrigin::UserDecision, RuleOrigin::RimSortCommunity],
        "UserDecision (the placement rule) must precede RimSortCommunity (the pair rule)"
    );
}

#[test]
fn default_stored_rules_has_no_rules_and_default_settings() {
    let stored = StoredRules::default();
    assert!(stored.rule_set().is_empty());
    assert_eq!(stored.settings.threshold.percent(), 80);
}
