//! Tests for the compatibility patch domain.

use std::collections::BTreeMap;

use jiff::Timestamp;

use super::*;
use crate::domain::DefKey;
use crate::domain::decision::Decision;
use crate::domain::finding::FindingKey;
use crate::domain::merge::{FieldPath, MergeChoice};
use crate::domain::resolution::Action;
use rim_analyzer::domain::ModId;
use std::collections::BTreeSet;

fn ts(seconds: i64) -> Timestamp {
    Timestamp::from_second(seconds).unwrap_or(Timestamp::UNIX_EPOCH)
}

// -- PatchId ---------------------------------------------------------

#[test]
fn derive_is_stable_for_the_same_inputs() {
    let a = PatchId::derive("abc123", &ModId::new("sample.abcompat"), ts(1000));
    let b = PatchId::derive("abc123", &ModId::new("sample.abcompat"), ts(1000));
    assert_eq!(a, b);
}

#[test]
fn derive_differs_when_any_input_differs() {
    let base = PatchId::derive("abc123", &ModId::new("sample.abcompat"), ts(1000));
    assert_ne!(
        base,
        PatchId::derive("def456", &ModId::new("sample.abcompat"), ts(1000))
    );
    assert_ne!(
        base,
        PatchId::derive("abc123", &ModId::new("sample.other"), ts(1000))
    );
    assert_ne!(
        base,
        PatchId::derive("abc123", &ModId::new("sample.abcompat"), ts(2000))
    );
}

#[test]
fn derive_produces_exactly_twelve_lowercase_hex_chars() {
    let id = PatchId::derive("abc123", &ModId::new("sample.abcompat"), ts(1000));
    assert_eq!(id.as_str().len(), 12);
    assert!(
        id.as_str()
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    );
}

#[test]
fn from_str_accepts_exactly_twelve_lowercase_hex_chars() {
    assert!("3f9a1c02be77".parse::<PatchId>().is_ok());
}

#[test]
fn from_str_rejects_wrong_length() {
    assert!("3f9a1c".parse::<PatchId>().is_err());
}

#[test]
fn from_str_rejects_uppercase_hex() {
    assert!("3F9A1C02BE77".parse::<PatchId>().is_err());
}

#[test]
fn from_str_rejects_non_hex_characters() {
    assert!("3f9a1c02be7z".parse::<PatchId>().is_err());
}

// -- PatchModIdentity --------------------------------------------------

#[test]
fn new_accepts_a_valid_identity() {
    let identity = PatchModIdentity::new("sample.abcompat", "A + B Compatibility").unwrap();
    assert_eq!(identity.package_id(), &ModId::new("sample.abcompat"));
    assert_eq!(identity.folder_name(), "sample_abcompat");
    assert_eq!(identity.display_name(), "A + B Compatibility");
}

#[test]
fn new_lowercases_the_package_id() {
    let identity = PatchModIdentity::new("Sample.ABCompat", "Name").unwrap();
    assert_eq!(identity.package_id(), &ModId::new("sample.abcompat"));
    assert_eq!(identity.folder_name(), "sample_abcompat");
}

#[test]
fn new_derives_the_folder_name_by_replacing_dots_with_underscores() {
    let identity = PatchModIdentity::new("a.b.c", "Name").unwrap();
    assert_eq!(identity.folder_name(), "a_b_c");
}

#[test]
fn new_rejects_a_package_id_over_the_length_limit() {
    let too_long = format!("a.{}", "b".repeat(60));
    let result = PatchModIdentity::new(&too_long, "Name");
    assert!(matches!(
        result,
        Err(PatchIdentityError::PackageIdFormat(_, _))
    ));
}

#[test]
fn new_rejects_an_illegal_character() {
    let result = PatchModIdentity::new("sample.ab-compat", "Name");
    assert!(matches!(
        result,
        Err(PatchIdentityError::PackageIdFormat(_, _))
    ));
}

#[test]
fn new_rejects_a_leading_dot() {
    assert!(matches!(
        PatchModIdentity::new(".sample", "Name"),
        Err(PatchIdentityError::PackageIdFormat(_, _))
    ));
}

#[test]
fn new_rejects_a_trailing_dot() {
    assert!(matches!(
        PatchModIdentity::new("sample.", "Name"),
        Err(PatchIdentityError::PackageIdFormat(_, _))
    ));
}

#[test]
fn new_rejects_a_doubled_dot() {
    assert!(matches!(
        PatchModIdentity::new("sample..abcompat", "Name"),
        Err(PatchIdentityError::PackageIdFormat(_, _))
    ));
}

#[test]
fn new_rejects_a_package_id_with_no_author_segment() {
    assert!(matches!(
        PatchModIdentity::new("abcompat", "Name"),
        Err(PatchIdentityError::MissingAuthorSegment(_))
    ));
}

#[test]
fn new_rejects_the_reserved_merge_mod_prefix() {
    assert!(matches!(
        PatchModIdentity::new("rimmerge.merge.abc123abc123", "Name"),
        Err(PatchIdentityError::ReservedPrefix(_))
    ));
}

#[test]
fn new_rejects_an_empty_display_name() {
    assert!(matches!(
        PatchModIdentity::new("sample.abcompat", "   "),
        Err(PatchIdentityError::EmptyDisplayName)
    ));
}

#[test]
fn new_rejects_a_display_name_over_the_length_limit() {
    let too_long = "a".repeat(101);
    assert!(matches!(
        PatchModIdentity::new("sample.abcompat", &too_long),
        Err(PatchIdentityError::DisplayNameTooLong)
    ));
}

#[test]
fn as_generated_carries_every_field_through() {
    let identity = PatchModIdentity::new("sample.abcompat", "A + B Compatibility").unwrap();
    let generated = identity.as_generated();
    assert_eq!(generated.package_id, ModId::new("sample.abcompat"));
    assert_eq!(generated.folder_name, "sample_abcompat");
    assert_eq!(generated.display_name, "A + B Compatibility");
}

// -- PatchScope --------------------------------------------------------

#[test]
fn new_rejects_fewer_than_two_members() {
    let result = PatchScope::new([ModId::new("a")]);
    assert!(matches!(result, Err(PatchScopeError(1))));
}

#[test]
fn new_rejects_an_empty_scope() {
    let result = PatchScope::new(std::iter::empty());
    assert!(matches!(result, Err(PatchScopeError(0))));
}

#[test]
fn new_dedups_a_steam_copy_against_its_local_counterpart() {
    let result = PatchScope::new([ModId::new("a"), ModId::new("a_steam")]);
    assert!(matches!(result, Err(PatchScopeError(1))));
}

#[test]
fn contains_matches_a_steam_suffixed_id_against_a_base_member() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    assert!(scope.contains(&ModId::new("a_steam")));
}

fn def_key() -> DefKey {
    DefKey {
        def_type: "ThingDef".to_string(),
        def_name: "Wall".to_string(),
    }
}

fn owners(ids: &[&str]) -> BTreeSet<ModId> {
    ids.iter().map(|id| ModId::new(*id)).collect()
}

#[test]
fn membership_def_override_two_members_no_others_is_full() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    assert_eq!(scope.membership(&key), ScopeMembership::Full);
    assert!(scope.admits(&key));
}

#[test]
fn membership_def_override_with_an_extra_owner_is_partial() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b", "c"]),
    };
    assert_eq!(
        scope.membership(&key),
        ScopeMembership::Partial {
            outside: owners(&["c"])
        }
    );
    assert!(scope.admits(&key));
}

#[test]
fn membership_def_override_with_fewer_than_two_scope_members_is_outside() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "c", "d"]),
    };
    assert_eq!(scope.membership(&key), ScopeMembership::Outside);
    assert!(!scope.admits(&key));
}

/// Core is never counted as "outside": even though `ludeon.rimworld`
/// isn't a scope member, a `DefOverride` it co-owns with two real scope
/// members is `Full`, not `Partial`.
#[test]
fn membership_core_is_never_outside() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["ludeon.rimworld", "a", "b"]),
    };
    assert_eq!(scope.membership(&key), ScopeMembership::Full);
}

/// A DLC id is an ordinary mod as far as scope membership is
/// concerned — it counts as "outside" just like any other mod the user
/// didn't put in scope, unlike Core.
#[test]
fn membership_a_dlc_counts_as_outside_unless_in_scope() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["ludeon.rimworld.royalty", "a", "b"]),
    };
    assert_eq!(
        scope.membership(&key),
        ScopeMembership::Partial {
            outside: owners(&["ludeon.rimworld.royalty"])
        }
    );
}

#[test]
fn membership_patch_collision_follows_the_same_owner_set_rule() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    let key = FindingKey::PatchCollision {
        key: def_key(),
        selector: rim_analyzer::domain::Selector::DefName,
        sub_path: None,
        mods: owners(&["a", "b"]),
    };
    assert_eq!(scope.membership(&key), ScopeMembership::Full);
}

#[test]
fn membership_texture_override_follows_the_same_owner_set_rule() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    let key = FindingKey::TextureOverride {
        texture_path: "Things/Wall.png".to_string(),
        owners: owners(&["a", "b", "c"]),
    };
    assert_eq!(
        scope.membership(&key),
        ScopeMembership::Partial {
            outside: owners(&["c"])
        }
    );
}

#[test]
fn membership_duplicate_assembly_follows_the_same_owner_set_rule() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    let key = FindingKey::DuplicateAssembly {
        assembly_name: "Foo".to_string(),
        owners: owners(&["a", "b"]),
    };
    assert_eq!(scope.membership(&key), ScopeMembership::Full);
}

#[test]
fn membership_likely_duplicate_mod_is_full_only_when_both_are_in_scope() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    assert_eq!(
        scope.membership(&FindingKey::LikelyDuplicateMod {
            pair: (ModId::new("a"), ModId::new("b"))
        }),
        ScopeMembership::Full
    );
    assert_eq!(
        scope.membership(&FindingKey::LikelyDuplicateMod {
            pair: (ModId::new("a"), ModId::new("c"))
        }),
        ScopeMembership::Outside
    );
}

#[test]
fn membership_incompatible_pair_is_full_only_when_both_are_in_scope() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    assert_eq!(
        scope.membership(&FindingKey::IncompatiblePair {
            pair: (ModId::new("a"), ModId::new("b"))
        }),
        ScopeMembership::Full
    );
}

#[test]
fn membership_undeclared_hard_dependency_is_full_only_when_both_are_in_scope() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    assert_eq!(
        scope.membership(&FindingKey::UndeclaredHardDependency {
            after: ModId::new("a"),
            before: ModId::new("b"),
        }),
        ScopeMembership::Full
    );
    assert_eq!(
        scope.membership(&FindingKey::UndeclaredHardDependency {
            after: ModId::new("a"),
            before: ModId::new("c"),
        }),
        ScopeMembership::Outside
    );
}

#[test]
fn membership_lazy_reference_violated_is_full_only_when_both_are_in_scope() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    assert_eq!(
        scope.membership(&FindingKey::LazyReferenceViolated {
            after: ModId::new("a"),
            before: ModId::new("b"),
        }),
        ScopeMembership::Full
    );
}

#[test]
fn membership_edge_dropped_is_full_only_when_both_are_in_scope() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    assert_eq!(
        scope.membership(&FindingKey::EdgeDropped {
            after: ModId::new("a"),
            before: ModId::new("b"),
            kind: rim_analyzer::domain::EdgeKind::MayRequire,
        }),
        ScopeMembership::Full
    );
}

#[test]
fn membership_declaration_questioned_is_full_only_when_both_are_in_scope() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    assert_eq!(
        scope.membership(&FindingKey::DeclarationQuestioned {
            declared_after: ModId::new("a"),
            declared_before: ModId::new("b"),
            relation_kind: rim_analyzer::domain::EdgeKind::FindMod,
        }),
        ScopeMembership::Full
    );
    assert_eq!(
        scope.membership(&FindingKey::DeclarationQuestioned {
            declared_after: ModId::new("a"),
            declared_before: ModId::new("c"),
            relation_kind: rim_analyzer::domain::EdgeKind::FindMod,
        }),
        ScopeMembership::Outside
    );
}

#[test]
fn membership_missing_dependency_is_outside_in_practice() {
    // The dependency is by definition inactive, so it's never a scope
    // member — this is the pair rule producing `Outside` in the only
    // shape that ever actually occurs.
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    assert_eq!(
        scope.membership(&FindingKey::MissingDependency {
            mod_id: ModId::new("a"),
            dependency: ModId::new("gone"),
        }),
        ScopeMembership::Outside
    );
}

#[test]
fn membership_single_mod_kinds_are_always_outside() {
    let scope = PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap();
    assert_eq!(
        scope.membership(&FindingKey::AnyOfChoice {
            after: ModId::new("a"),
            assembly: "Foo".to_string(),
        }),
        ScopeMembership::Outside
    );
    assert_eq!(
        scope.membership(&FindingKey::MissingMod {
            mod_id: ModId::new("a"),
        }),
        ScopeMembership::Outside
    );
    assert_eq!(
        scope.membership(&FindingKey::UnsupportedVersion {
            mod_id: ModId::new("a"),
        }),
        ScopeMembership::Outside
    );
    assert_eq!(
        scope.membership(&FindingKey::TagInferred {
            mod_id: ModId::new("a"),
            tag: crate::domain::Tag::new("t").unwrap(),
        }),
        ScopeMembership::Outside
    );
}

// -- PatchProject::decide ----------------------------------------------

fn scope_ab() -> PatchScope {
    PatchScope::new([ModId::new("a"), ModId::new("b")]).unwrap()
}

fn project() -> PatchProject {
    PatchProject::new(
        PatchId::derive("profile", &ModId::new("sample.abcompat"), ts(1)),
        "AB compat".to_string(),
        PatchModIdentity::new("sample.abcompat", "A + B Compatibility").unwrap(),
        scope_ab(),
        ts(1),
    )
}

fn decision(key: FindingKey, action: Action) -> Decision {
    Decision {
        key,
        action,
        note: None,
        decided_at: ts(2),
    }
}

#[test]
fn decide_accepts_ignore_on_an_admitted_key() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    assert!(patch.decide(decision(key.clone(), Action::Ignore)).is_ok());
    assert!(patch.decisions().get(&key).is_some());
}

#[test]
fn decide_accepts_merge_with_choices_naming_scope_members() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    let mut choices = BTreeMap::new();
    choices.insert(
        "statBases/MaxHitPoints".parse().unwrap(),
        MergeChoice::From {
            mod_id: ModId::new("a"),
        },
    );
    let action = Action::Merge {
        key: def_key(),
        choices,
    };
    assert!(patch.decide(decision(key, action)).is_ok());
}

/// Core is always an implicit diff
/// participant for an admitted `DefOverride`, and the
/// emitter already drops it from `depends_on` — so a `Merge` choice
/// naming it must be accepted even though Core is never a declared
/// scope member (`PatchScope::contains` alone would reject it).
#[test]
fn decide_accepts_a_merge_choice_naming_core() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["ludeon.rimworld", "a", "b"]),
    };
    let mut choices = BTreeMap::new();
    choices.insert(
        "statBases/MaxHitPoints".parse().unwrap(),
        MergeChoice::From {
            mod_id: ModId::new("ludeon.rimworld"),
        },
    );
    let action = Action::Merge {
        key: def_key(),
        choices,
    };
    assert!(patch.decide(decision(key, action)).is_ok());
}

/// Unlike a `Merge` choice, a `ShipAsset` naming Core must still be
/// rejected: Core ships no loose texture a patch could copy, so
/// `admits_owner`'s Core exception deliberately does not extend to
/// [`PatchDecisionError::AssetOutsideScope`].
#[test]
fn decide_still_rejects_a_ship_asset_from_core() {
    let mut patch = project();
    let key = FindingKey::TextureOverride {
        texture_path: "Things/Wall.png".to_string(),
        owners: owners(&["ludeon.rimworld", "a", "b"]),
    };
    let action = Action::ShipAsset {
        texture_path: "Things/Wall.png".to_string(),
        from: ModId::new("ludeon.rimworld"),
    };
    let result = patch.decide(decision(key, action));
    assert_eq!(
        result,
        Err(PatchDecisionError::AssetOutsideScope(ModId::new(
            "ludeon.rimworld"
        )))
    );
}

#[test]
fn decide_rejects_reorder() {
    let mut patch = project();
    let key = FindingKey::UndeclaredHardDependency {
        after: ModId::new("a"),
        before: ModId::new("b"),
    };
    let result = patch.decide(decision(
        key,
        Action::Reorder {
            after: ModId::new("a"),
            before: ModId::new("b"),
        },
    ));
    assert!(matches!(
        result,
        Err(PatchDecisionError::NotPatchable(UnpatchableAction::Reorder))
    ));
}

#[test]
fn decide_rejects_accept() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    let result = patch.decide(decision(key, Action::Accept));
    assert!(matches!(
        result,
        Err(PatchDecisionError::NotPatchable(UnpatchableAction::Accept))
    ));
}

#[test]
fn decide_rejects_an_out_of_scope_key() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["c", "d"]),
    };
    let result = patch.decide(decision(key.clone(), Action::Ignore));
    assert_eq!(result, Err(PatchDecisionError::OutOfScope(Box::new(key))));
}

#[test]
fn decide_rejects_a_merge_choice_naming_an_outside_owner() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    let mut choices = BTreeMap::new();
    let path: FieldPath = "statBases/MaxHitPoints".parse().unwrap();
    choices.insert(
        path.clone(),
        MergeChoice::From {
            mod_id: ModId::new("c"),
        },
    );
    let action = Action::Merge {
        key: def_key(),
        choices,
    };
    let result = patch.decide(decision(key, action));
    assert_eq!(
        result,
        Err(PatchDecisionError::ChoiceOutsideScope {
            path,
            mod_id: ModId::new("c"),
        })
    );
}

#[test]
fn decide_rejects_a_ship_asset_from_outside_scope() {
    let mut patch = project();
    let key = FindingKey::TextureOverride {
        texture_path: "Things/Wall.png".to_string(),
        owners: owners(&["a", "b"]),
    };
    let action = Action::ShipAsset {
        texture_path: "Things/Wall.png".to_string(),
        from: ModId::new("c"),
    };
    let result = patch.decide(decision(key, action));
    assert_eq!(
        result,
        Err(PatchDecisionError::AssetOutsideScope(ModId::new("c")))
    );
}

#[test]
fn decide_replaces_an_earlier_decision_on_the_same_key() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    patch.decide(decision(key.clone(), Action::Ignore)).unwrap();
    let previous = patch
        .decide(decision(
            key.clone(),
            Action::Merge {
                key: def_key(),
                choices: BTreeMap::new(),
            },
        ))
        .unwrap();
    assert_eq!(previous.map(|d| d.action), Some(Action::Ignore));
}

#[test]
fn revert_removes_a_stored_decision() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    patch.decide(decision(key.clone(), Action::Ignore)).unwrap();
    assert!(patch.revert(&key).is_some());
    assert!(patch.decisions().get(&key).is_none());
}

// -- PatchProject::set_scope --------------------------------------------

#[test]
fn set_scope_orphans_a_decision_the_new_scope_no_longer_admits() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    patch.decide(decision(key.clone(), Action::Ignore)).unwrap();

    let change = patch.set_scope(PatchScope::new([ModId::new("a"), ModId::new("c")]).unwrap());

    assert_eq!(change.now_orphaned, vec![key.clone()]);
    assert!(change.choices_naming_removed.is_empty());
    // Never deleted:
    assert!(patch.decisions().get(&key).is_some());
}

#[test]
fn set_scope_reports_a_merge_choice_naming_a_removed_member_without_orphaning_the_key() {
    let mut patch = PatchProject::new(
        PatchId::derive("profile", &ModId::new("sample.abcompat"), ts(1)),
        "AB compat".to_string(),
        PatchModIdentity::new("sample.abcompat", "A + B Compatibility").unwrap(),
        PatchScope::new([ModId::new("a"), ModId::new("b"), ModId::new("c")]).unwrap(),
        ts(1),
    );
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b", "c"]),
    };
    let path: FieldPath = "statBases/MaxHitPoints".parse().unwrap();
    let mut choices = BTreeMap::new();
    choices.insert(
        path.clone(),
        MergeChoice::From {
            mod_id: ModId::new("b"),
        },
    );
    patch
        .decide(decision(
            key.clone(),
            Action::Merge {
                key: def_key(),
                choices,
            },
        ))
        .unwrap();

    // Shrinking to {a, c} still admits the key (a and c are both still
    // scope members), but the stored choice named `b`, which just left.
    let change = patch.set_scope(PatchScope::new([ModId::new("a"), ModId::new("c")]).unwrap());

    assert!(change.now_orphaned.is_empty());
    assert_eq!(
        change.choices_naming_removed,
        vec![(key.clone(), Some(path))]
    );
    assert!(patch.decisions().get(&key).is_some());
}

/// Likewise, `set_scope` must never treat Core as a
/// removed owner just because it shrinks the declared scope — Core was
/// never a declared member in the first place, so it can't be "removed"
/// by `admits_owner`'s reckoning.
#[test]
fn set_scope_never_reports_core_as_removed() {
    let mut patch = PatchProject::new(
        PatchId::derive("profile", &ModId::new("sample.abcompat"), ts(1)),
        "AB compat".to_string(),
        PatchModIdentity::new("sample.abcompat", "A + B Compatibility").unwrap(),
        PatchScope::new([ModId::new("a"), ModId::new("b"), ModId::new("c")]).unwrap(),
        ts(1),
    );
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["ludeon.rimworld", "a", "b", "c"]),
    };
    let mut choices = BTreeMap::new();
    choices.insert(
        "statBases/MaxHitPoints".parse().unwrap(),
        MergeChoice::From {
            mod_id: ModId::new("ludeon.rimworld"),
        },
    );
    patch
        .decide(decision(
            key.clone(),
            Action::Merge {
                key: def_key(),
                choices,
            },
        ))
        .unwrap();

    // Shrinking to {a, c}: the key stays admitted (a and c remain, b
    // left — Partial), and the stored choice still names Core, which
    // `admits_owner` accepts regardless of scope membership.
    let change = patch.set_scope(PatchScope::new([ModId::new("a"), ModId::new("c")]).unwrap());

    assert!(change.now_orphaned.is_empty());
    assert!(
        change.choices_naming_removed.is_empty(),
        "Core must never be reported as a removed owner: {:?}",
        change.choices_naming_removed
    );
}

#[test]
fn set_scope_reports_a_ship_asset_naming_a_removed_member() {
    let mut patch = PatchProject::new(
        PatchId::derive("profile", &ModId::new("sample.abcompat"), ts(1)),
        "AB compat".to_string(),
        PatchModIdentity::new("sample.abcompat", "A + B Compatibility").unwrap(),
        PatchScope::new([ModId::new("a"), ModId::new("b"), ModId::new("c")]).unwrap(),
        ts(1),
    );
    let key = FindingKey::TextureOverride {
        texture_path: "Things/Wall.png".to_string(),
        owners: owners(&["a", "b", "c"]),
    };
    patch
        .decide(decision(
            key.clone(),
            Action::ShipAsset {
                texture_path: "Things/Wall.png".to_string(),
                from: ModId::new("b"),
            },
        ))
        .unwrap();

    let change = patch.set_scope(PatchScope::new([ModId::new("a"), ModId::new("c")]).unwrap());

    assert!(change.now_orphaned.is_empty());
    assert_eq!(change.choices_naming_removed, vec![(key, None)]);
}

// -- PatchProject::{orphaned,prune_orphaned} ----------------------------

#[test]
fn orphaned_reports_a_decision_whose_key_is_no_longer_live() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    patch.decide(decision(key.clone(), Action::Ignore)).unwrap();

    let live: BTreeSet<FindingKey> = BTreeSet::new();
    let orphaned: Vec<&FindingKey> = patch.orphaned(&live).map(|d| &d.key).collect();
    assert_eq!(orphaned, vec![&key]);
}

#[test]
fn orphaned_reports_a_decision_the_current_scope_no_longer_admits() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    patch.decide(decision(key.clone(), Action::Ignore)).unwrap();
    patch.set_scope(PatchScope::new([ModId::new("a"), ModId::new("c")]).unwrap());

    let live: BTreeSet<FindingKey> = [key.clone()].into_iter().collect();
    let orphaned: Vec<&FindingKey> = patch.orphaned(&live).map(|d| &d.key).collect();
    assert_eq!(orphaned, vec![&key]);
}

#[test]
fn prune_orphaned_removes_and_returns_orphaned_decisions_only() {
    let mut patch = project();
    let live_key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    let gone_key = FindingKey::TextureOverride {
        texture_path: "Things/Wall.png".to_string(),
        owners: owners(&["a", "b"]),
    };
    patch
        .decide(decision(live_key.clone(), Action::Ignore))
        .unwrap();
    patch
        .decide(decision(gone_key.clone(), Action::Ignore))
        .unwrap();

    let live: BTreeSet<FindingKey> = [live_key.clone()].into_iter().collect();
    let pruned = patch.prune_orphaned(&live);

    assert_eq!(pruned.len(), 1);
    assert_eq!(pruned[0].key, gone_key);
    assert!(patch.decisions().get(&live_key).is_some());
    assert!(patch.decisions().get(&gone_key).is_none());
}

// -- PatchProject::decisions_sha256 -------------------------------------

#[test]
fn decisions_sha256_is_stable_for_the_same_decisions() {
    let mut a = project();
    let mut b = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    a.decide(decision(key.clone(), Action::Ignore)).unwrap();
    b.decide(decision(key, Action::Ignore)).unwrap();

    assert_eq!(a.decisions_sha256(), b.decisions_sha256());
}

#[test]
fn decisions_sha256_ignores_note_and_decided_at() {
    let mut a = project();
    let mut b = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    a.decide(Decision {
        key: key.clone(),
        action: Action::Ignore,
        note: Some("keep the winner".to_string()),
        decided_at: ts(2),
    })
    .unwrap();
    b.decide(Decision {
        key,
        action: Action::Ignore,
        note: None,
        decided_at: ts(999),
    })
    .unwrap();

    assert_eq!(a.decisions_sha256(), b.decisions_sha256());
}

#[test]
fn decisions_sha256_changes_when_a_choice_changes() {
    let mut patch = project();
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "b"]),
    };
    patch
        .decide(decision(
            key.clone(),
            Action::Merge {
                key: def_key(),
                choices: BTreeMap::new(),
            },
        ))
        .unwrap();
    let before = patch.decisions_sha256();

    let mut choices = BTreeMap::new();
    choices.insert(
        "statBases/MaxHitPoints".parse().unwrap(),
        MergeChoice::From {
            mod_id: ModId::new("a"),
        },
    );
    patch
        .decide(decision(
            key,
            Action::Merge {
                key: def_key(),
                choices,
            },
        ))
        .unwrap();
    let after = patch.decisions_sha256();

    assert_ne!(before, after);
}

// -- PatchProject::from_stored -------------------------------------

fn stored(scope: PatchScope, decisions: Vec<Decision>) -> StoredPatchProject {
    StoredPatchProject {
        id: PatchId::derive("profile", &ModId::new("sample.abcompat"), ts(1)),
        name: "AB compat".to_string(),
        identity: PatchModIdentity::new("sample.abcompat", "A + B Compatibility").unwrap(),
        author: "sample".to_string(),
        description: "a description".to_string(),
        scope,
        decisions,
        export_dir: None,
        created_at: ts(1),
        updated_at: ts(2),
    }
}

#[test]
fn from_stored_restores_every_field_including_updated_at() {
    let restored =
        PatchProject::from_stored(stored(scope_ab(), Vec::new())).expect("no decisions to reject");

    assert_eq!(restored.created_at(), ts(1));
    assert_eq!(
        restored.updated_at(),
        ts(2),
        "updated_at must round-trip independently of created_at"
    );
    assert_eq!(restored.author(), "sample");
    assert_eq!(restored.description(), "a description");
}

/// A decision whose key the *current*
/// scope no longer admits — legitimate after `set_scope` shrank the
/// scope, per `ScopeChange`'s own "never deletes" contract — must
/// still load, unlike replaying it through `decide`.
#[test]
fn from_stored_keeps_a_decision_the_current_scope_no_longer_admits() {
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "c"]),
    };
    let orphaned_decision = decision(key.clone(), Action::Ignore);

    let restored = PatchProject::from_stored(stored(scope_ab(), vec![orphaned_decision.clone()]))
        .expect("an orphaned decision must not be rejected");

    assert_eq!(restored.decisions().get(&key), Some(&orphaned_decision));
    assert!(
        !restored.scope().admits(&key),
        "sanity: the scope really doesn't admit this key"
    );
}

#[test]
fn from_stored_still_rejects_an_unpatchable_action() {
    let key = FindingKey::UndeclaredHardDependency {
        after: ModId::new("a"),
        before: ModId::new("b"),
    };
    let unpatchable = decision(
        key,
        Action::Reorder {
            after: ModId::new("a"),
            before: ModId::new("b"),
        },
    );

    let result = PatchProject::from_stored(stored(scope_ab(), vec![unpatchable]));

    assert_eq!(
        result,
        Err(PatchDecisionError::NotPatchable(UnpatchableAction::Reorder))
    );
}

#[test]
fn from_stored_still_lets_prune_orphaned_remove_the_orphan() {
    let key = FindingKey::DefOverride {
        key: def_key(),
        owners: owners(&["a", "c"]),
    };
    let orphaned_decision = decision(key.clone(), Action::Ignore);
    let mut restored = PatchProject::from_stored(stored(scope_ab(), vec![orphaned_decision]))
        .expect("an orphaned decision must not be rejected");

    let pruned = restored.prune_orphaned(&BTreeSet::new());

    assert_eq!(pruned.len(), 1);
    assert!(restored.decisions().get(&key).is_none());
}

#[test]
fn decisions_sha256_of_an_empty_project_is_stable() {
    assert_eq!(project().decisions_sha256(), project().decisions_sha256());
}
