//! Tests for findings and finding keys.

use super::*;
use crate::domain::def_ref::DefRef;
use proptest::prelude::*;
use std::collections::BTreeSet;

// -- `normalize_log_text` --------------------------------------------

#[test]
fn normalize_log_text_collapses_a_multi_line_xpath_to_one_line() {
    let multi_line = "Defs/ThingDef[\n\t\t\t@Name=\"BaseMechanoid\" or\n\t\t\t@Name=\"Base_X2_AIRobot\"\n\t\t]/comps";
    let one_line = "Defs/ThingDef[ @Name=\"BaseMechanoid\" or @Name=\"Base_X2_AIRobot\" ]/comps";
    assert_eq!(normalize_log_text(multi_line), one_line);
}

#[test]
fn normalize_log_text_is_idempotent_on_already_single_line_text() {
    let text = "Verse.PatchOperationFindMod(Example Temperature Expanded)";
    assert_eq!(normalize_log_text(text), text);
}

// -- `FindingKey::def_ref` ------------------------------------------
//
// One assertion per variant, in declaration order, so a new variant
// added to `FindingKey` without a matching line here is easy to spot
// even though the `match` itself already forces a compile error.

fn def_key(def_type: &str, def_name: &str) -> DefKey {
    DefKey {
        def_type: def_type.to_string(),
        def_name: def_name.to_string(),
    }
}

fn ids(values: &[&str]) -> BTreeSet<ModId> {
    values.iter().map(|v| ModId::new(*v)).collect()
}

#[test]
fn def_ref_for_def_override_uses_def_name_selector() {
    let key = FindingKey::DefOverride {
        key: def_key("ThingDef", "Wall"),
        owners: ids(&["a"]),
    };
    assert_eq!(
        key.def_ref(),
        Some(DefRef::new(def_key("ThingDef", "Wall"), Selector::DefName))
    );
}

#[test]
fn def_ref_for_patch_collision_carries_its_own_selector() {
    let key = FindingKey::PatchCollision {
        key: def_key("ThingDef", "WallBase"),
        selector: Selector::NameAttr,
        sub_path: None,
        mods: ids(&["a"]),
    };
    assert_eq!(
        key.def_ref(),
        Some(DefRef::new(
            def_key("ThingDef", "WallBase"),
            Selector::NameAttr
        ))
    );
}

#[test]
fn def_ref_for_patch_will_fail_carries_its_own_selector() {
    let key = FindingKey::PatchWillFail {
        mod_id: ModId::new("a"),
        def_key: def_key("ThingDef", "WallBase"),
        selector: Selector::NameAttr,
        operation: r#"Verse.PatchOperationReplace(Defs/ThingDef[@Name="WallBase"]/statBases)"#
            .to_string(),
    };
    assert_eq!(
        key.def_ref(),
        Some(DefRef::new(
            def_key("ThingDef", "WallBase"),
            Selector::NameAttr
        ))
    );
}

/// `operation`'s own text embeds a mod's own display
/// name verbatim (via `PatchOperationFindMod`'s rendering), and real mods
/// commonly have a literal `:` in their own name (`Example Splice: Core`).
/// A parser that naively splits on every colon in the whole string fails
/// to round-trip such a key (`FromStr` returns `WrongFieldCount`) — the
/// `arb_path()` proptest generator covering this same field never
/// catches this, since its own regex never produces a `:`, so this is a
/// dedicated, explicit case using a realistic value, not left to the
/// property test alone.
#[test]
fn finding_key_display_from_str_roundtrips_an_operation_whose_mod_name_has_a_colon() {
    let key = FindingKey::PatchWillFail {
        mod_id: ModId::new("example.genesplice"),
        def_key: def_key("ThingDef", "Wall"),
        selector: Selector::DefName,
        operation: "Verse.PatchOperationFindMod(Example Splice: Core)".to_string(),
    };
    let text = key.to_string();
    let parsed: FindingKey = text.parse().expect("must round-trip, not WrongFieldCount");
    assert_eq!(parsed, key);
}

#[test]
fn def_ref_for_duplicate_template_name_uses_a_name_only_ref() {
    let key = FindingKey::DuplicateTemplateName {
        name: "WallBase".to_string(),
        owners: ids(&["a", "b"]),
    };
    let def_ref = key
        .def_ref()
        .expect("DuplicateTemplateName names a def_ref");
    assert_eq!(def_ref, DefRef::name_only("WallBase"));
    assert!(def_ref.is_name_only());
    assert_eq!(def_ref.to_string(), "@WallBase");
}

#[test]
fn def_ref_is_none_for_every_non_def_finding_kind() {
    let non_def_keys = vec![
        FindingKey::EdgeDropped {
            after: ModId::new("a"),
            before: ModId::new("b"),
            kind: EdgeKind::LoadAfter,
        },
        FindingKey::DeclarationQuestioned {
            declared_after: ModId::new("a"),
            declared_before: ModId::new("b"),
            relation_kind: EdgeKind::UsesType,
        },
        FindingKey::AnyOfChoice {
            after: ModId::new("a"),
            assembly: "Some.dll".to_string(),
        },
        FindingKey::TextureOverride {
            texture_path: "things/wall".to_string(),
            owners: ids(&["a", "b"]),
        },
        FindingKey::DuplicateAssembly {
            assembly_name: "Some.dll".to_string(),
            owners: ids(&["a", "b"]),
        },
        FindingKey::KeyedTranslationCollision {
            pair: (ModId::new("a"), ModId::new("b")),
        },
        FindingKey::SoundOverride {
            path: "shot_fire".to_string(),
            owners: ids(&["a", "b"]),
        },
        FindingKey::UndeclaredTypeDependency {
            user: ModId::new("a"),
            provider: ModId::new("b"),
            type_name: "Foo.Bar".to_string(),
        },
        FindingKey::RuntimePatchCollision {
            target_type: "Verse.Pawn".to_string(),
            target_method: "Kill".to_string(),
            owners: ids(&["a", "b"]),
        },
        FindingKey::RuleOverruled {
            after: ModId::new("a"),
            before: ModId::new("b"),
            origin: RuleOrigin::UserDecision,
        },
        FindingKey::PlacementOverruled {
            mod_id: ModId::new("a"),
            placement: Placement::Top,
            origin: RuleOrigin::UserDecision,
        },
        FindingKey::PlacementQuestioned {
            mod_id: ModId::new("a"),
            placement: Placement::Bottom,
            relation: EdgeKind::UsesType,
        },
        FindingKey::PlacementOrderingOverridden {
            mod_id: ModId::new("a"),
            pinned: ModId::new("b"),
            placement: Placement::Bottom,
        },
        FindingKey::PlacementPromotesDependents {
            mod_id: ModId::new("a"),
            placement: Placement::Bottom,
        },
        FindingKey::LikelyDuplicateMod {
            pair: (ModId::new("a"), ModId::new("b")),
        },
        FindingKey::MissingMod {
            mod_id: ModId::new("a"),
        },
        FindingKey::MissingDependency {
            mod_id: ModId::new("a"),
            dependency: ModId::new("b"),
        },
        FindingKey::IncompatiblePair {
            pair: (ModId::new("a"), ModId::new("b")),
        },
        FindingKey::UnsupportedVersion {
            mod_id: ModId::new("a"),
        },
        FindingKey::UndeclaredHardDependency {
            after: ModId::new("a"),
            before: ModId::new("b"),
        },
        FindingKey::LazyReferenceViolated {
            after: ModId::new("a"),
            before: ModId::new("b"),
        },
        FindingKey::TagInferred {
            mod_id: ModId::new("a"),
            tag: Tag::new("some_tag").unwrap(),
        },
        FindingKey::ContributesNothing {
            mod_id: ModId::new("a"),
        },
    ];
    for key in non_def_keys {
        assert_eq!(key.def_ref(), None, "{key:?}");
    }
}

#[test]
fn def_override_key_renders_canonical_text() {
    let key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: [
            ModId::new("a.mod"),
            ModId::new("b.mod"),
            ModId::new("c.mod"),
        ]
        .into_iter()
        .collect(),
    };
    assert_eq!(
        key.to_string(),
        "def_override:ThingDef/Wall:[a.mod,b.mod,c.mod]"
    );
}

#[test]
fn empty_owner_set_renders_empty_brackets_and_parses_back() {
    let key = FindingKey::DefOverride {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        owners: BTreeSet::new(),
    };
    let text = key.to_string();
    assert_eq!(text, "def_override:ThingDef/Wall:[]");
    assert_eq!(text.parse::<FindingKey>().unwrap(), key);
}

#[test]
fn patch_collision_with_no_sub_path_uses_the_none_token() {
    let key = FindingKey::PatchCollision {
        key: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        },
        selector: Selector::DefName,
        sub_path: None,
        mods: [ModId::new("a")].into_iter().collect(),
    };
    let text = key.to_string();
    assert_eq!(text, "patch_collision:ThingDef/Wall:def_name:-:[a]");
    assert_eq!(text.parse::<FindingKey>().unwrap(), key);
}

/// The single-field key round-trips through its
/// canonical text form exactly like `MissingMod`/`UnsupportedVersion`.
#[test]
fn contributes_nothing_key_renders_canonical_text_and_round_trips() {
    let key = FindingKey::ContributesNothing {
        mod_id: ModId::new("inert.mod"),
    };
    let text = key.to_string();
    assert_eq!(text, "contributes_nothing:inert.mod");
    assert_eq!(text.parse::<FindingKey>().unwrap(), key);
}

#[test]
fn unknown_kind_is_rejected() {
    let result = "not_a_real_kind:a:b".parse::<FindingKey>();
    assert!(matches!(result, Err(FindingKeyParseError::UnknownKind(_))));
}

#[test]
fn wrong_field_count_is_rejected() {
    let result = "missing_mod:a:b".parse::<FindingKey>();
    match result {
        Err(FindingKeyParseError::WrongFieldCount {
            kind,
            expected,
            found,
        }) => {
            assert_eq!(kind, "missing_mod");
            assert_eq!(expected, 1);
            assert_eq!(found, 2);
        }
        other => panic!("expected WrongFieldCount, got {other:?}"),
    }
}

#[test]
fn malformed_list_field_is_rejected() {
    // `owners` needs `[a,b]` brackets; a bare comma-separated value
    // without them must be rejected, not silently split anyway.
    let result = "def_override:ThingDef/Wall:a,b".parse::<FindingKey>();
    assert!(matches!(
        result,
        Err(FindingKeyParseError::MalformedList(_))
    ));
}

#[test]
fn malformed_def_key_is_rejected() {
    // A def key needs a `<def_type>/<def_name>` separator.
    let result = "def_override:NoSlashHere:[a]".parse::<FindingKey>();
    assert!(matches!(
        result,
        Err(FindingKeyParseError::MalformedDefKey(_))
    ));
}

#[test]
fn unknown_edge_kind_is_rejected() {
    let result = "edge_dropped:a:b:not_a_real_edge_kind".parse::<FindingKey>();
    assert!(matches!(
        result,
        Err(FindingKeyParseError::UnknownEdgeKind(_))
    ));
}

#[test]
fn unknown_selector_is_rejected() {
    let result = "patch_collision:ThingDef/Wall:not_a_real_selector:-:[a]".parse::<FindingKey>();
    assert!(matches!(
        result,
        Err(FindingKeyParseError::UnknownSelector(_))
    ));
}

#[test]
fn invalid_field_is_rejected_for_a_malformed_tag() {
    let result = "tag_inferred:a:NOT A VALID SLUG".parse::<FindingKey>();
    assert!(matches!(result, Err(FindingKeyParseError::InvalidField(_))));
}

// -- proptest roundtrip -------------------------------------------
//
// Generated text avoids the grammar's own delimiters (`:`, `[`, `]`,
// `,`) and the sub-path "none" sentinel (`-`) — the constraint every
// *verified* real-world identifier this crate has actually measured
// satisfies (mod ids, def types/names, texture/sound paths). Not a
// universal claim: `DefKey::synthesize_for_runtime_target`'s own
// `def_name` deliberately embeds a literal `::` (see its doc comment)
// — but that synthesized key only ever reaches `Action::PreferWinner`,
// persisted through `Action`'s serde JSON form (which escapes `:`
// fine), never through `FindingKey`'s own colon-delimited `Display`/
// `FromStr` this roundtrip exercises, so the two never collide.

fn arb_mod_id() -> impl Strategy<Value = ModId> {
    "[a-z][a-z0-9]{0,6}(\\.[a-z0-9]{1,6}){0,2}(-[a-z0-9]{1,4})?(_steam)?".prop_map(ModId::new)
}

fn arb_text() -> impl Strategy<Value = String> {
    "[\\p{L}\\p{N}_.]{1,12}"
}

fn arb_path() -> impl Strategy<Value = String> {
    "(/[\\p{L}\\p{N}_-]{1,8}){1,3}"
}

fn arb_def_key() -> impl Strategy<Value = DefKey> {
    (arb_text(), arb_text()).prop_map(|(def_type, def_name)| DefKey { def_type, def_name })
}

fn arb_id_set() -> impl Strategy<Value = BTreeSet<ModId>> {
    proptest::collection::vec(arb_mod_id(), 1..4).prop_map(|ids| ids.into_iter().collect())
}

fn arb_sorted_pair() -> impl Strategy<Value = (ModId, ModId)> {
    (arb_mod_id(), arb_mod_id()).prop_map(|(a, b)| if a <= b { (a, b) } else { (b, a) })
}

/// A hand-listed `prop_oneof![Just(..)]`, not derived from `EdgeKind`
/// itself (there is no `EdgeKind::ALL`/`strum`-style enumeration to
/// derive from) — deliberately literal so that adding a new
/// `EdgeKind` variant without adding it here leaves this generator
/// silently unable to produce it. That matters because this is the
/// only positive guard on `edge_kind_str`/`parse_edge_kind`'s round
/// trip (`finding_key_display_from_str_roundtrips`, below): a typo in
/// either table for a kind this generator never emits would ship
/// green. Update this list with every new `EdgeKind`, the same
/// discipline `apps/desktop/src/utils/edgeKind.test.ts`'s
/// `ALL_EDGE_KINDS` already documents on the TypeScript side.
fn arb_edge_kind() -> impl Strategy<Value = EdgeKind> {
    prop_oneof![
        Just(EdgeKind::AssemblyRef),
        Just(EdgeKind::ForceLoadAfter),
        Just(EdgeKind::ForceLoadBefore),
        Just(EdgeKind::LoadAfter),
        Just(EdgeKind::LoadBefore),
        Just(EdgeKind::ModDependency),
        Just(EdgeKind::FindMod),
        Just(EdgeKind::IfModActive),
        Just(EdgeKind::PatchTargetsDef),
        Just(EdgeKind::MayRequire),
        Just(EdgeKind::PatchInjectedNode),
        Just(EdgeKind::AssemblyVersionPrecedence),
        Just(EdgeKind::UsesType),
        Just(EdgeKind::ParentTemplate),
        Just(EdgeKind::PatchRemovedNode),
        Just(EdgeKind::RetextureAfterOwner),
        Just(EdgeKind::DefOverrideAfterOrigin),
        Just(EdgeKind::PatchSelectsInjectedNode),
        Just(EdgeKind::PatchInvalidatesPredicate),
        Just(EdgeKind::PatchRemovedNodeCosmetic),
        Just(EdgeKind::ReplaceDiscardsAddition),
    ]
}

fn arb_selector() -> impl Strategy<Value = Selector> {
    prop_oneof![Just(Selector::DefName), Just(Selector::NameAttr)]
}

fn arb_tag() -> impl Strategy<Value = Tag> {
    "[a-z][a-z0-9_-]{0,10}".prop_map(|s| Tag::new(s).unwrap())
}

fn arb_rule_origin() -> impl Strategy<Value = RuleOrigin> {
    prop_oneof![
        Just(RuleOrigin::UserDecision),
        Just(RuleOrigin::RimSortUser),
        Just(RuleOrigin::RimSortCommunity),
        Just(RuleOrigin::SteamDb),
    ]
}

fn arb_placement() -> impl Strategy<Value = Placement> {
    prop_oneof![Just(Placement::Top), Just(Placement::Bottom)]
}

fn arb_finding_key() -> impl Strategy<Value = FindingKey> {
    prop_oneof![
        (arb_mod_id(), arb_mod_id(), arb_edge_kind()).prop_map(|(after, before, kind)| {
            FindingKey::EdgeDropped {
                after,
                before,
                kind,
            }
        }),
        (arb_mod_id(), arb_mod_id(), arb_edge_kind()).prop_map(
            |(declared_after, declared_before, relation_kind)| {
                FindingKey::DeclarationQuestioned {
                    declared_after,
                    declared_before,
                    relation_kind,
                }
            }
        ),
        (arb_mod_id(), arb_mod_id(), arb_edge_kind()).prop_map(
            |(declared_after, declared_before, kind)| {
                FindingKey::DeclarationOverridden {
                    declared_after,
                    declared_before,
                    kind,
                }
            }
        ),
        (arb_mod_id(), arb_text())
            .prop_map(|(after, assembly)| FindingKey::AnyOfChoice { after, assembly }),
        (arb_def_key(), arb_id_set())
            .prop_map(|(key, owners)| FindingKey::DefOverride { key, owners }),
        (
            arb_def_key(),
            arb_selector(),
            proptest::option::of(arb_path()),
            arb_id_set()
        )
            .prop_map(|(key, selector, sub_path, mods)| {
                FindingKey::PatchCollision {
                    key,
                    selector,
                    sub_path,
                    mods,
                }
            }),
        (arb_path(), arb_id_set()).prop_map(|(texture_path, owners)| {
            FindingKey::TextureOverride {
                texture_path,
                owners,
            }
        }),
        (arb_text(), arb_id_set()).prop_map(|(assembly_name, owners)| {
            FindingKey::DuplicateAssembly {
                assembly_name,
                owners,
            }
        }),
        (arb_text(), arb_id_set())
            .prop_map(|(name, owners)| { FindingKey::DuplicateTemplateName { name, owners } }),
        arb_sorted_pair().prop_map(|pair| FindingKey::KeyedTranslationCollision { pair }),
        (arb_path(), arb_id_set())
            .prop_map(|(path, owners)| FindingKey::SoundOverride { path, owners }),
        (arb_mod_id(), arb_mod_id(), arb_text()).prop_map(|(user, provider, type_name)| {
            FindingKey::UndeclaredTypeDependency {
                user,
                provider,
                type_name,
            }
        }),
        (arb_text(), arb_text(), arb_id_set()).prop_map(|(target_type, target_method, owners)| {
            FindingKey::RuntimePatchCollision {
                target_type,
                target_method,
                owners,
            }
        }),
        (arb_text(), arb_text(), arb_id_set()).prop_map(|(target_type, target_method, owners)| {
            FindingKey::TranspilerCollision {
                target_type,
                target_method,
                owners,
            }
        }),
        (arb_mod_id(), arb_mod_id(), arb_rule_origin()).prop_map(|(after, before, origin)| {
            FindingKey::RuleOverruled {
                after,
                before,
                origin,
            }
        }),
        (arb_mod_id(), arb_placement(), arb_rule_origin()).prop_map(
            |(mod_id, placement, origin)| FindingKey::PlacementOverruled {
                mod_id,
                placement,
                origin,
            }
        ),
        (arb_mod_id(), arb_placement(), arb_edge_kind()).prop_map(
            |(mod_id, placement, relation)| FindingKey::PlacementQuestioned {
                mod_id,
                placement,
                relation,
            }
        ),
        arb_sorted_pair().prop_map(|pair| FindingKey::LikelyDuplicateMod { pair }),
        arb_mod_id().prop_map(|mod_id| FindingKey::MissingMod { mod_id }),
        (arb_mod_id(), arb_mod_id()).prop_map(|(mod_id, dependency)| {
            FindingKey::MissingDependency { mod_id, dependency }
        }),
        arb_sorted_pair().prop_map(|pair| FindingKey::IncompatiblePair { pair }),
        arb_mod_id().prop_map(|mod_id| FindingKey::UnsupportedVersion { mod_id }),
        (arb_mod_id(), arb_mod_id())
            .prop_map(|(after, before)| { FindingKey::UndeclaredHardDependency { after, before } }),
        (arb_mod_id(), arb_mod_id())
            .prop_map(|(after, before)| { FindingKey::LazyReferenceViolated { after, before } }),
        (arb_mod_id(), arb_tag()).prop_map(|(mod_id, tag)| FindingKey::TagInferred { mod_id, tag }),
        (arb_mod_id(), arb_mod_id(), arb_placement()).prop_map(|(mod_id, pinned, placement)| {
            FindingKey::PlacementOrderingOverridden {
                mod_id,
                pinned,
                placement,
            }
        }),
        (arb_mod_id(), arb_placement()).prop_map(|(mod_id, placement)| {
            FindingKey::PlacementPromotesDependents { mod_id, placement }
        }),
        (arb_mod_id(), arb_def_key(), arb_text(), arb_text()).prop_map(
            |(referrer, def, field, path)| FindingKey::MissingTexturePath {
                referrer,
                def,
                field,
                path,
            }
        ),
        (arb_mod_id(), arb_def_key(), arb_selector(), arb_path()).prop_map(
            |(mod_id, def_key, selector, operation)| FindingKey::PatchWillFail {
                mod_id,
                def_key,
                selector,
                operation,
            }
        ),
        arb_mod_id().prop_map(|mod_id| FindingKey::ContributesNothing { mod_id }),
        (arb_mod_id(), arb_text())
            .prop_map(|(mod_id, path)| FindingKey::UndecodableTexture { mod_id, path }),
        arb_text().prop_map(|name| FindingKey::DanglingDefReference { name }),
    ]
}

proptest! {
    #[test]
    fn finding_key_display_from_str_roundtrips(key in arb_finding_key()) {
        let text = key.to_string();
        let parsed: FindingKey = text.parse().unwrap();
        prop_assert_eq!(parsed, key);
    }
}
