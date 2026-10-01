//! Tests for the patch maker's domain.

use std::collections::HashMap;

use jiff::Timestamp;
use proptest::prelude::*;

use super::*;

fn ts(seconds: i64) -> Timestamp {
    Timestamp::from_second(seconds).unwrap_or(Timestamp::UNIX_EPOCH)
}

fn occurrence(cardinality: Cardinality, values: &[&str]) -> FieldOccurrence {
    FieldOccurrence {
        cardinality,
        values: values.iter().map(|v| v.to_string()).collect(),
    }
}

fn field(tag: &str) -> FieldPath {
    FieldPath::new(vec![PathSegment::Child(tag.to_string())])
}

/// `existing_def_type` for every test that isn't exercising
/// tag-reconstruction tie discriminator
/// itself: "no type exists", so [`tag_reconstructed_type`] always
/// returns `None` and every pre-existing test keeps classifying
/// exactly the way it did before that discriminator existed.
fn no_type_exists(_: &str) -> bool {
    false
}

// -- AssignmentSchema::infer_fields: the two real-install failure
// modes --------------------

/// Failure mode 1: hundreds of unresolvable values (races from
/// inactive mods) must never count as evidence *against* a field
/// being a reference field — only the resolvable ones matter to the
/// coverage ratio.
#[test]
fn unresolvable_values_are_unknown_not_evidence_against() {
    let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();

    // Six distinct values resolve to ThingDef, all owned outside R.
    // A hundred more distinct values resolve to nothing at all
    // (inactive-mod races) — the old, buggy share-of-all-values gate
    // would compute 6/106 and misclassify this as Opaque.
    let mut resolvable: HashMap<String, Vec<(String, ModId)>> = HashMap::new();
    for i in 0..6 {
        resolvable.insert(
            format!("Race{i}"),
            vec![("ThingDef".to_string(), ModId::new("some.other.mod"))],
        );
    }
    let mut race_names: Vec<String> = resolvable.keys().cloned().collect();
    for i in 0..100 {
        race_names.push(format!("InactiveRace{i}"));
    }

    let instances = vec![(
        ModId::new("example.framework"),
        InstanceValues::from([(
            field("speciesNames"),
            occurrence(
                Cardinality::List,
                &race_names.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
        )]),
    )];

    let resolve = |value: &str| resolvable.get(value).cloned().unwrap_or_default();
    let dll_owner = |_: &str| None;

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);

    assert_eq!(
        fields[&field("speciesNames")].role,
        FieldRole::TargetKey {
            def_type: "ThingDef".to_string()
        },
        "6 of 6 resolvable values agreeing must classify as a reference field \
             regardless of how many more values were simply unresolvable"
    );
}

/// Failure mode 2: a `defName` that exists under two types (some
/// values only resolve to the "wrong" one) must be classified by
/// which type explains the *whole field*, not by a per-value
/// majority.
#[test]
fn field_wide_type_voting_picks_the_type_that_explains_every_value() {
    let refs: BTreeSet<ModId> = [ModId::new("example.speciessupport")].into_iter().collect();

    // Every one of the 5 values resolves to `example.PartDef`; only 2
    // of them are *also* ambiguously a `ExampleApparelHediffDef` (the
    // real install's `Clamp`/`Satchel` case). A per-value majority
    // would misclassify those two by whatever tie-break it used;
    // the field-wide vote must pick `example.PartDef` (5 votes) over
    // `ExampleApparelHediffDef` (2 votes) regardless.
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        let owner = ModId::new("example.speciessupport");
        match value {
            "Clamp" | "Satchel" => vec![
                ("example.PartDef".to_string(), owner.clone()),
                ("ExampleApparelHediffDef".to_string(), owner),
            ],
            "PartA" | "PartB" | "PartC" => vec![("example.PartDef".to_string(), owner)],
            _ => vec![],
        }
    };
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("example.speciessupport"),
        InstanceValues::from([(
            field("partsA"),
            occurrence(
                Cardinality::List,
                &["Clamp", "Satchel", "PartA", "PartB", "PartC"],
            ),
        )]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);

    assert_eq!(
        fields[&field("partsA")].role,
        FieldRole::ItemSlot {
            def_type: "example.PartDef".to_string()
        },
        "the field-wide vote must pick example.PartDef (5 votes), not \
             ExampleApparelHediffDef (2 votes)"
    );
}

/// A field whose leaf tag is *not* `Def`/`Defs`-suffixed gets no
/// exemption from [`MIN_RESOLVED_DISTINCT`]: below the count floor,
/// it falls through to `Scalar` — see
/// [`a_def_suffixed_scalar_with_one_resolved_value_is_an_item_slot`]
/// for the same shape on a `Def`-suffixed field, which the exemption
/// rescues.
#[test]
fn a_reference_field_below_the_count_threshold_is_not_classified_as_one() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        if value.starts_with("Egg") {
            vec![("ThingDef".to_string(), ModId::new("owner"))]
        } else {
            vec![]
        }
    };
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(
            field("raceName"),
            occurrence(Cardinality::Scalar, &["EggA"]),
        )]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);

    // Fewer than MIN_RESOLVED_DISTINCT resolvable values, and the
    // leaf tag isn't Def/Defs-suffixed: falls through to Scalar,
    // never TargetKey/ItemSlot.
    assert!(matches!(
        fields[&field("raceName")].role,
        FieldRole::Scalar { .. }
    ));
}

/// The `defaultToolDef` real-install case itself: 23 distinct values,
/// only 1 of which resolves (the other 22 `defName`s belong to races the
/// user doesn't have active) — `resolvable = 1`, `resolved = 1`,
/// `coverage = 1.00`. Without the `Def`-suffix exemption this fails
/// [`MIN_RESOLVED_DISTINCT`] and falls through to
/// `Scalar { kind: ScalarKind::Text }` (23 distinct values is over
/// [`ENUM_MAX_DISTINCT`]) — confirmed below, not assumed. With the
/// exemption, a `resolved >= 1` floor applies instead, and — because
/// it is rescued *only* by the exemption (`resolved < MIN_RESOLVED_DISTINCT`)
/// — the inside/outside vote is skipped entirely in favour of an
/// unconditional `ItemSlot`, even though the one resolved value's
/// owner sits outside `refs` (which that vote alone would have scored
/// `TargetKey`): a low-evidence `<fooDef>` scalar is always a value
/// the user picks, never the row's identity.
#[test]
fn a_def_suffixed_scalar_with_one_resolved_value_is_an_item_slot() {
    let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        if value == "ThingA" {
            vec![("ThingDef".to_string(), ModId::new("some.other.mod"))]
        } else {
            vec![]
        }
    };
    let dll_owner = |_: &str| None;

    let unresolvable: Vec<String> = (0..22).map(|i| format!("Unresolvable{i}")).collect();
    let mut instances: Vec<(ModId, InstanceValues)> = vec![(
        ModId::new("mod0"),
        InstanceValues::from([(
            field("defaultToolDef"),
            occurrence(Cardinality::Scalar, &["ThingA"]),
        )]),
    )];
    instances.extend(unresolvable.iter().enumerate().map(|(i, value)| {
        (
            ModId::new(format!("mod{}", i + 1)),
            InstanceValues::from([(
                field("defaultToolDef"),
                occurrence(Cardinality::Scalar, &[value.as_str()]),
            )]),
        )
    }));

    // This test's single resolved value sits below the plain
    // MIN_RESOLVED_DISTINCT floor (5) — without the exemption below,
    // classify_reference returns None here and this field falls
    // through to Scalar{Text} (23 distinct values, over
    // ENUM_MAX_DISTINCT).
    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    let spec = &fields[&field("defaultToolDef")];

    assert_eq!(
        spec.role,
        FieldRole::ItemSlot {
            def_type: "ThingDef".to_string()
        },
        "a Def-suffixed field with one resolved value must become an ItemSlot, \
             not fall through to Scalar{{Text}}"
    );
    assert_eq!(spec.cardinality, Cardinality::Scalar);
}

/// [`AssignmentSchema::reference_field_diagnostics`] over the exact
/// same fixture as the test above must report the numbers
/// `classify_reference` itself decided on: rescued only by the
/// exemption (`is_def_suffixed`, `resolved_count` under
/// [`MIN_RESOLVED_DISTINCT`]), and the same `ItemSlot { ThingDef }`
/// role — the real-install measurement test
/// (`apps/cli/tests/real_install_def_suffix_exemption.rs`) trusts
/// this function to agree with `infer_fields` on exactly this shape.
#[test]
fn reference_field_diagnostics_reports_a_field_rescued_only_by_the_exemption() {
    let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        if value == "ThingA" {
            vec![("ThingDef".to_string(), ModId::new("some.other.mod"))]
        } else {
            vec![]
        }
    };
    let dll_owner = |_: &str| None;

    let unresolvable: Vec<String> = (0..22).map(|i| format!("Unresolvable{i}")).collect();
    let mut instances: Vec<(ModId, InstanceValues)> = vec![(
        ModId::new("mod0"),
        InstanceValues::from([(
            field("defaultToolDef"),
            occurrence(Cardinality::Scalar, &["ThingA"]),
        )]),
    )];
    instances.extend(unresolvable.iter().enumerate().map(|(i, value)| {
        (
            ModId::new(format!("mod{}", i + 1)),
            InstanceValues::from([(
                field("defaultToolDef"),
                occurrence(Cardinality::Scalar, &[value.as_str()]),
            )]),
        )
    }));

    let diagnostics = AssignmentSchema::reference_field_diagnostics(
        &instances,
        &refs,
        &resolve,
        &dll_owner,
        &no_type_exists,
    );
    let field_diagnostics = &diagnostics[&field("defaultToolDef")];

    assert!(field_diagnostics.is_def_suffixed);
    assert_eq!(field_diagnostics.resolved_count, 1);
    assert_eq!(field_diagnostics.cardinality, Cardinality::Scalar);
    assert_eq!(
        field_diagnostics.role,
        Some(FieldRole::ItemSlot {
            def_type: "ThingDef".to_string()
        })
    );
}

/// The regression guard against a future "just lower
/// `MIN_RESOLVED_DISTINCT`": the same one-resolved-of-many-distinct
/// shape as the test above, but on a field whose leaf tag does *not*
/// end in `Def`/`Defs` (the `HediffDef.description` real-install
/// case: many distinct free-text values, one of which happens to
/// coincide with a real `defName`). This must stay `Scalar { kind:
/// Text }` with or without the exemption — proving the exemption is
/// scoped to the field's own name, not to "any scalar with a thin
/// resolvable sample".
#[test]
fn a_non_def_suffixed_scalar_with_one_resolved_value_stays_scalar() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        if value == "0" {
            // The real coincidence this guards against: a free-text
            // description happens to equal a real defName ("0").
            vec![(
                "ExampleShips.ShipDef".to_string(),
                ModId::new("example.ships"),
            )]
        } else {
            vec![]
        }
    };
    let dll_owner = |_: &str| None;

    let mut values: Vec<String> = vec!["0".to_string()];
    values.extend((0..22).map(|i| format!("Some free text {i}")));
    let instances: Vec<(ModId, InstanceValues)> = values
        .iter()
        .enumerate()
        .map(|(i, value)| {
            (
                ModId::new(format!("mod{i}")),
                InstanceValues::from([(
                    field("description"),
                    occurrence(Cardinality::Scalar, &[value.as_str()]),
                )]),
            )
        })
        .collect();

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);

    assert!(
        matches!(
            fields[&field("description")].role,
            FieldRole::Scalar {
                kind: ScalarKind::Text,
                ..
            }
        ),
        "a coincidental single resolution on a non-Def-suffixed field must never \
             promote it to TargetKey/ItemSlot, regardless of MIN_RESOLVED_DISTINCT: got {:?}",
        fields[&field("description")].role
    );
}

/// A `Def`-suffixed field is still not exempt from
/// [`TYPE_COVERAGE_MIN`]: even with the count floor relaxed to
/// `resolved >= 1`, a winning type that only explains a minority of
/// the field's resolvable values must still fail the reference gate.
/// 5 resolvable distinct values spread across three types (2/2/1,
/// the winning type tied at 2 and won lexically) — the winning
/// type's own `resolved = 2` is the *lowest* the field-wide vote can
/// produce here: with only 3 types sharing 5 resolvable values, the
/// pigeonhole principle forces some type to reach at least
/// `ceil(5/3) = 2`, so `resolved = 1` is not reachable at
/// `resolvable = 5` over exactly three types — 2/5 = 0.4 still fails
/// `TYPE_COVERAGE_MIN` (0.8) just as cleanly.
#[test]
fn a_def_suffixed_field_still_needs_type_coverage() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        match value {
            "A0" | "A1" => vec![("AaaDef".to_string(), ModId::new("owner"))],
            "B0" | "B1" => vec![("BbbDef".to_string(), ModId::new("owner"))],
            "C0" => vec![("CccDef".to_string(), ModId::new("owner"))],
            _ => vec![],
        }
    };
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(
            field("partDef"),
            occurrence(Cardinality::List, &["A0", "A1", "B0", "B1", "C0"]),
        )]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);

    assert!(
        !matches!(
            fields[&field("partDef")].role,
            FieldRole::TargetKey { .. } | FieldRole::ItemSlot { .. }
        ),
        "a Def-suffixed field whose winning type covers only 2 of 5 resolvable \
             values (0.4 < TYPE_COVERAGE_MIN) must still fall through to Scalar/Opaque"
    );
}

/// The exemption must not hijack a `Def`-suffixed field that already
/// clears [`MIN_RESOLVED_DISTINCT`] on its own — rule 3's inside/
/// outside vote still runs normally, exactly as for a non-Def field.
#[test]
fn a_def_suffixed_field_above_the_count_gate_still_uses_rule_3() {
    let refs: BTreeSet<ModId> = [ModId::new("example.framework")].into_iter().collect();
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        if value.starts_with('V') {
            vec![("ThingDef".to_string(), ModId::new("outside.owner"))]
        } else {
            vec![]
        }
    };
    let dll_owner = |_: &str| None;

    let values = ["V0", "V1", "V2", "V3", "V4"];
    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(field("raceDef"), occurrence(Cardinality::List, &values))]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);

    assert_eq!(
        fields[&field("raceDef")].role,
        FieldRole::TargetKey {
            def_type: "ThingDef".to_string()
        },
        "a Def-suffixed field that clears MIN_RESOLVED_DISTINCT normally must still \
             go through rule 3 — its outside-R owner makes it a TargetKey, not an \
             unconditional ItemSlot"
    );
}

/// Pins [`MIN_RESOLVED_DISTINCT`]'s own boundary: exactly the
/// threshold's worth of distinct resolved values passes, one fewer
/// falls through — both with `resolvable == resolved` (a ratio of
/// 1.0), so [`TYPE_COVERAGE_MIN`] can never be what's deciding here.
#[test]
fn min_resolved_distinct_boundary_five_passes_four_fails() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let values_of = |count: usize| -> Vec<String> { (0..count).map(|i| format!("V{i}")).collect() };
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        if value.starts_with('V') {
            vec![("ThingDef".to_string(), ModId::new("outside"))]
        } else {
            vec![]
        }
    };
    let dll_owner = |_: &str| None;

    let five = values_of(MIN_RESOLVED_DISTINCT);
    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(
            field("f"),
            occurrence(
                Cardinality::List,
                &five.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
        )]),
    )];
    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(
        fields[&field("f")].role,
        FieldRole::TargetKey {
            def_type: "ThingDef".to_string()
        },
        "exactly MIN_RESOLVED_DISTINCT resolved distinct values must pass"
    );

    let four = values_of(MIN_RESOLVED_DISTINCT - 1);
    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(
            field("f"),
            occurrence(
                Cardinality::List,
                &four.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
        )]),
    )];
    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert!(
        !matches!(
            fields[&field("f")].role,
            FieldRole::TargetKey { .. } | FieldRole::ItemSlot { .. }
        ),
        "one fewer than MIN_RESOLVED_DISTINCT must not pass"
    );
}

/// Pins [`TYPE_COVERAGE_MIN`]'s own boundary, independent of
/// [`MIN_RESOLVED_DISTINCT`] (both cases hold `resolved == 8 >= 5`):
/// a ratio of exactly `0.8` passes (the operator is `>=`), a ratio
/// below it does not.
#[test]
fn type_coverage_min_boundary_at_the_ratio_passes_just_under_fails() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        if value.starts_with("R") {
            vec![("ThingDef".to_string(), ModId::new("outside"))]
        } else {
            vec![]
        }
    };
    let dll_owner = |_: &str| None;

    // 8 resolved of 10 resolvable (2 resolve to a different type,
    // still "resolvable"): 0.8 exactly.
    let values: Vec<&str> = vec![
        "R0", "R1", "R2", "R3", "R4", "R5", "R6", "R7", "Other0", "Other1",
    ];
    let resolve_at_boundary = |value: &str| -> Vec<(String, ModId)> {
        if value.starts_with("Other") {
            vec![("PawnKindDef".to_string(), ModId::new("outside"))]
        } else {
            resolve(value)
        }
    };
    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(field("f"), occurrence(Cardinality::List, &values))]),
    )];
    let fields = AssignmentSchema::infer_fields(
        &instances,
        &refs,
        &resolve_at_boundary,
        &dll_owner,
        &no_type_exists,
    );
    assert_eq!(
        fields[&field("f")].role,
        FieldRole::TargetKey {
            def_type: "ThingDef".to_string()
        },
        "a ratio of exactly 0.8 must pass"
    );

    // 8 resolved of 11 resolvable: ~0.727, just under 0.8.
    let values: Vec<&str> = vec![
        "R0", "R1", "R2", "R3", "R4", "R5", "R6", "R7", "Other0", "Other1", "Other2",
    ];
    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(field("f"), occurrence(Cardinality::List, &values))]),
    )];
    let fields = AssignmentSchema::infer_fields(
        &instances,
        &refs,
        &resolve_at_boundary,
        &dll_owner,
        &no_type_exists,
    );
    assert!(
        !matches!(
            fields[&field("f")].role,
            FieldRole::TargetKey { .. } | FieldRole::ItemSlot { .. }
        ),
        "a ratio below 0.8 must not pass"
    );
}

/// Pins the field-wide type vote's own lexical tie-break: two types with
/// exactly equal vote counts across the whole field must resolve to
/// the lexically smaller type name.
#[test]
fn field_wide_vote_ties_are_broken_lexically() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    // Every value resolves to both types equally: a perfect tie.
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        if value.starts_with('V') {
            vec![
                ("Bbb".to_string(), ModId::new("outside")),
                ("Aaa".to_string(), ModId::new("outside")),
            ]
        } else {
            vec![]
        }
    };
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(
            field("f"),
            occurrence(Cardinality::List, &["V0", "V1", "V2", "V3", "V4"]),
        )]),
    )];
    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(
        fields[&field("f")].role,
        FieldRole::TargetKey {
            def_type: "Aaa".to_string()
        },
        "a perfectly tied vote must pick the lexically smaller type name"
    );
}

// -- "Def-suffix tie discriminator: a
//    positive rule, not a fall-through" ------------------------------

/// A tie the tag resolves: "BarDef" (lexically smaller, so plain
/// `argmax` would pick it — see `field_wide_vote_ties_are_broken_lexically`
/// just above) ties 1-1 against "FooDef", the leaf tag "fooDef"'s own
/// reconstruction. `existing_def_type` recognizes "FooDef" as real, so
/// it must win instead of the alphabetical default.
#[test]
fn a_tied_def_suffixed_vote_prefers_the_tag_reconstructed_type_when_it_exists() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |_: &str| -> Vec<(String, ModId)> {
        vec![
            ("BarDef".to_string(), ModId::new("outside")),
            ("FooDef".to_string(), ModId::new("outside")),
        ]
    };
    let dll_owner = |_: &str| None;
    let existing_def_type = |t: &str| t.eq_ignore_ascii_case("FooDef");

    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(field("fooDef"), occurrence(Cardinality::Scalar, &["V0"]))]),
    )];
    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &existing_def_type);

    assert_eq!(
        fields[&field("fooDef")].role,
        FieldRole::ItemSlot {
            def_type: "FooDef".to_string()
        },
        "a tie whose leaf tag reconstructs a real corpus type must pick that type, not the \
             lexically-smaller argmax default"
    );
}

/// A tie the tag does not resolve: "filthDef" (role name, not a type
/// name) reconstructs to "FilthDef", which `existing_def_type` does
/// not recognize as real — the vote's own alphabetical tie-break
/// (`AaaDef` over `ZzzDef`) must survive unchanged, exactly as it did
/// before this discriminator existed.
#[test]
fn a_tied_def_suffixed_vote_is_unchanged_when_the_tag_reconstructs_no_real_type() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |_: &str| -> Vec<(String, ModId)> {
        vec![
            ("ZzzDef".to_string(), ModId::new("outside")),
            ("AaaDef".to_string(), ModId::new("outside")),
        ]
    };
    let dll_owner = |_: &str| None;
    let existing_def_type = |_: &str| false;

    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(field("filthDef"), occurrence(Cardinality::Scalar, &["V0"]))]),
    )];
    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &existing_def_type);

    assert_eq!(
        fields[&field("filthDef")].role,
        FieldRole::ItemSlot {
            def_type: "AaaDef".to_string()
        },
        "a tie whose tag names no real corpus type must fall through to the vote's own \
             lexical tie-break unchanged, never a fall-through to Scalar"
    );
}

/// A non-tie: "BarDef" wins 4-1 over "FooDef", a real decisive margin
/// — the discriminator must never override it even though the tag
/// ("fooDef") reconstructs "FooDef" and `existing_def_type` says it's
/// real. `is_tied` (`resolved_count >= runner_up`) is the guard this
/// pins: a non-strict `>=` would wrongly treat this margin as tied.
#[test]
fn a_non_tied_def_suffixed_vote_never_applies_the_tag_reconstruction_override() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |value: &str| -> Vec<(String, ModId)> {
        match value {
            "V0" | "V1" | "V2" | "V3" => vec![("BarDef".to_string(), ModId::new("outside"))],
            "V4" => vec![("FooDef".to_string(), ModId::new("outside"))],
            _ => vec![],
        }
    };
    let dll_owner = |_: &str| None;
    let existing_def_type = |t: &str| t.eq_ignore_ascii_case("FooDef");

    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(
            field("fooDef"),
            occurrence(Cardinality::Scalar, &["V0", "V1", "V2", "V3", "V4"]),
        )]),
    )];
    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &existing_def_type);

    assert_eq!(
        fields[&field("fooDef")].role,
        FieldRole::ItemSlot {
            def_type: "BarDef".to_string()
        },
        "a decisive (non-tied) vote must never be overridden by the tag-reconstruction \
             discriminator, even when the tag names a real, different corpus type"
    );
}

/// A `List` field whose tie is resolved by the tag reconstruction must
/// keep `Cardinality::List` — the whole reason the shipped rule is a
/// positive override rather than a fall-through to `Scalar`:
/// `render_leaf` (`crates/rim-merge/src/assign.rs`) emits a `Scalar`
/// role as a bare text node regardless of cardinality, so demoting a
/// `List` field there would emit structurally invalid XML.
#[test]
fn a_tied_list_field_keeps_its_cardinality_when_the_tag_reconstruction_fires() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |_: &str| -> Vec<(String, ModId)> {
        vec![
            ("StuffCategoryDef".to_string(), ModId::new("outside")),
            ("ThingDef".to_string(), ModId::new("outside")),
        ]
    };
    let dll_owner = |_: &str| None;
    let existing_def_type = |t: &str| t.eq_ignore_ascii_case("ThingDef");

    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(field("thingDefs"), occurrence(Cardinality::List, &["V0"]))]),
    )];
    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &existing_def_type);

    let spec = &fields[&field("thingDefs")];
    assert_eq!(
        spec.role,
        FieldRole::ItemSlot {
            def_type: "ThingDef".to_string()
        },
        "the psychic-ritual-shaped tie must resolve to the tag's own reconstructed type"
    );
    assert_eq!(
        spec.cardinality,
        Cardinality::List,
        "overriding def_type on a tie must never demote a List field's own cardinality"
    );
}

/// Pins [`Cardinality`]'s own schema-wide aggregation rule: RimWorld's
/// `List<T>` XML deserializer accepts a bare, unwrapped scalar element
/// as a one-element list shorthand, so a field leans `List` once
/// instances show it `li`-wrapped — even when most instances write the
/// unwrapped shorthand instead. Only a field *never once* observed
/// `li`-wrapped is `Scalar`. A plain majority vote is wrong here: on a
/// real install, Example's own sparse `chance...` fields are almost
/// entirely unwrapped-shorthand occurrences, so a majority vote would
/// classify them `Scalar` instead of `List`.
///
/// **The minimum-evidence guard**: a single `li`-wrapped
/// occurrence among several unwrapped ones is not enough on its
/// own (see [`cardinality_stays_scalar_for_a_single_li_wrapped_outlier`]
/// for that case) — this test's own "1 List, 2 Scalar" case below
/// asserts `Scalar`; see `schema.rs`'s own `observe` doc comment for the
/// full rationale, including why a `MIN_RESOLVED_DISTINCT`-sized floor
/// is not used in favour of a lower "at least two" one.
#[test]
fn cardinality_is_list_when_multiple_instances_show_it_li_wrapped() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |_: &str| -> Vec<(String, ModId)> { vec![] };
    let dll_owner = |_: &str| None;

    let cardinality_over = |occurrences: Vec<(ModId, InstanceValues)>| -> Cardinality {
        AssignmentSchema::infer_fields(&occurrences, &refs, &resolve, &dll_owner, &no_type_exists)
            [&field("f")]
            .cardinality
    };

    // 2 List, 1 Scalar: two corroborating List occurrences, so List.
    let instances = vec![
        (
            ModId::new("a"),
            InstanceValues::from([(field("f"), occurrence(Cardinality::List, &["x"]))]),
        ),
        (
            ModId::new("b"),
            InstanceValues::from([(field("f"), occurrence(Cardinality::List, &["x"]))]),
        ),
        (
            ModId::new("c"),
            InstanceValues::from([(field("f"), occurrence(Cardinality::Scalar, &["x"]))]),
        ),
    ];
    assert_eq!(cardinality_over(instances), Cardinality::List);

    // 1 List, 2 Scalar: a *lone* `li`-wrapped occurrence among several
    // unwrapped ones no longer flips the field — the minimum-evidence
    // guard's whole point.
    let instances = vec![
        (
            ModId::new("a"),
            InstanceValues::from([(field("f"), occurrence(Cardinality::List, &["x"]))]),
        ),
        (
            ModId::new("b"),
            InstanceValues::from([(field("f"), occurrence(Cardinality::Scalar, &["x"]))]),
        ),
        (
            ModId::new("c"),
            InstanceValues::from([(field("f"), occurrence(Cardinality::Scalar, &["x"]))]),
        ),
    ];
    assert_eq!(cardinality_over(instances), Cardinality::Scalar);

    // 0 List, 2 Scalar: never `li`-wrapped, so Scalar.
    let instances = vec![
        (
            ModId::new("a"),
            InstanceValues::from([(field("f"), occurrence(Cardinality::Scalar, &["x"]))]),
        ),
        (
            ModId::new("b"),
            InstanceValues::from([(field("f"), occurrence(Cardinality::Scalar, &["x"]))]),
        ),
    ];
    assert_eq!(cardinality_over(instances), Cardinality::Scalar);
}

/// One odd,
/// `li`-wrapped instance must not flip an otherwise always-scalar
/// field (e.g. `label`) to `List` on its own, even across a much
/// larger instance count than the test above uses — the guard is
/// count-relative (a lone outlier among *any* number of unwrapped
/// peers), not a fixed small sample.
#[test]
fn cardinality_stays_scalar_for_a_single_li_wrapped_outlier() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |_: &str| -> Vec<(String, ModId)> { vec![] };
    let dll_owner = |_: &str| None;

    let mut instances: Vec<(ModId, InstanceValues)> = (0..20)
        .map(|i| {
            (
                ModId::new(format!("scalar.{i}")),
                InstanceValues::from([(field("label"), occurrence(Cardinality::Scalar, &["x"]))]),
            )
        })
        .collect();
    instances.push((
        ModId::new("odd.one.out"),
        InstanceValues::from([(field("label"), occurrence(Cardinality::List, &["x"]))]),
    ));

    let cardinality =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists)
            [&field("label")]
            .cardinality;
    assert_eq!(
        cardinality,
        Cardinality::Scalar,
        "one malformed instance among 20 clean ones must not flip the field to List"
    );
}

/// The guard's other side: a genuinely sparse but *plural* wrapped
/// minority (the real-install `chance...` fields rule 5 was written
/// for) must still classify `List` — two corroborating occurrences is
/// enough even against a large unwrapped majority.
#[test]
fn cardinality_is_list_for_a_sparse_but_plural_li_wrapped_minority() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |_: &str| -> Vec<(String, ModId)> { vec![] };
    let dll_owner = |_: &str| None;

    let mut instances: Vec<(ModId, InstanceValues)> = (0..50)
        .map(|i| {
            (
                ModId::new(format!("scalar.{i}")),
                InstanceValues::from([(
                    field("chanceprimaryTool"),
                    occurrence(Cardinality::Scalar, &["0.5"]),
                )]),
            )
        })
        .collect();
    for i in 0..2 {
        instances.push((
            ModId::new(format!("wrapped.{i}")),
            InstanceValues::from([(
                field("chanceprimaryTool"),
                occurrence(Cardinality::List, &["0.5"]),
            )]),
        ));
    }

    let cardinality =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists)
            [&field("chanceprimaryTool")]
            .cardinality;
    assert_eq!(cardinality, Cardinality::List);
}

/// Pins the public [`majority_owner`] function's own tie-break
/// directly — `rim-merge`'s emitter calls this to pick one
/// value's dependency owner among several, so its two-owner tie
/// behaviour is worth pinning on its own, not just through
/// `infer_fields`'s inside-share computation.
#[test]
fn majority_owner_breaks_a_two_owner_tie_lexically() {
    let owners = vec![ModId::new("b"), ModId::new("a")];
    assert_eq!(majority_owner(&owners), Some(ModId::new("a")));
}

#[test]
fn majority_owner_picks_the_true_majority_over_a_minority() {
    let owners = vec![ModId::new("a"), ModId::new("b"), ModId::new("a")];
    assert_eq!(majority_owner(&owners), Some(ModId::new("a")));
}

#[test]
fn enum_max_distinct_boundary_eight_is_enum_nine_is_text() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |_: &str| -> Vec<(String, ModId)> { vec![] };
    let dll_owner = |_: &str| None;

    let eight = ["v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7"];
    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(field("f"), occurrence(Cardinality::Scalar, &eight))]),
    )];
    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert!(matches!(
        fields[&field("f")].role,
        FieldRole::Scalar {
            kind: ScalarKind::Enum { .. },
            ..
        }
    ));

    let nine = ["v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8"];
    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(field("f"), occurrence(Cardinality::Scalar, &nine))]),
    )];
    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(
        fields[&field("f")].role,
        FieldRole::Scalar {
            kind: ScalarKind::Text,
            default: Some("v0".to_string())
        }
    );
}

fn owner_only_resolver(
    type_name: &'static str,
    owner: ModId,
    values: &'static [&'static str],
) -> impl Fn(&str) -> Vec<(String, ModId)> {
    move |value: &str| {
        if values.contains(&value) {
            vec![(type_name.to_string(), owner.clone())]
        } else {
            vec![]
        }
    }
}

fn five_values() -> &'static [&'static str] {
    &["V0", "V1", "V2", "V3", "V4"]
}

#[test]
fn an_inside_share_at_or_above_the_threshold_is_an_item_slot() {
    let refs: BTreeSet<ModId> = [ModId::new("framework")].into_iter().collect();
    let resolve = owner_only_resolver("SlotType", ModId::new("framework"), five_values());
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("framework"),
        InstanceValues::from([(field("slot"), occurrence(Cardinality::List, five_values()))]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(
        fields[&field("slot")].role,
        FieldRole::ItemSlot {
            def_type: "SlotType".to_string()
        }
    );
}

#[test]
fn an_inside_share_below_the_threshold_and_no_dll_signal_is_a_target_key() {
    let refs: BTreeSet<ModId> = [ModId::new("framework")].into_iter().collect();
    let resolve = owner_only_resolver("KeyType", ModId::new("outsider"), five_values());
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("framework"),
        InstanceValues::from([(field("key"), occurrence(Cardinality::List, five_values()))]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(
        fields[&field("key")].role,
        FieldRole::TargetKey {
            def_type: "KeyType".to_string()
        }
    );
}

/// The namespace-DLL signal alone can promote a field to `ItemSlot`
/// even when most of its values are owned outside R (Example Race
/// Support's own addons-define-most-parts case).
#[test]
fn the_dll_namespace_signal_alone_makes_a_field_an_item_slot() {
    let refs: BTreeSet<ModId> = [ModId::new("framework")].into_iter().collect();
    let resolve = owner_only_resolver("example.PartDef", ModId::new("addon"), five_values());
    let dll_owner =
        |type_name: &str| (type_name == "example.PartDef").then(|| ModId::new("framework"));

    let instances = vec![(
        ModId::new("framework"),
        InstanceValues::from([(field("slot"), occurrence(Cardinality::List, five_values()))]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(
        fields[&field("slot")].role,
        FieldRole::ItemSlot {
            def_type: "example.PartDef".to_string()
        }
    );
}

/// A `_steam`-suffixed ref
/// must classify its own owned types as `ItemSlot`, exactly like its
/// base-id counterpart would — a one-sided comparison
/// (`owner.base()` against a raw, possibly-unnormalised `refs`)
/// would silently misclassify this as `TargetKey` instead.
#[test]
fn a_steam_suffixed_ref_still_classifies_its_own_types_as_an_item_slot() {
    let refs: BTreeSet<ModId> = [ModId::new("framework_steam")].into_iter().collect();
    let resolve = owner_only_resolver("SlotType", ModId::new("framework"), five_values());
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("framework"),
        InstanceValues::from([(field("slot"), occurrence(Cardinality::List, five_values()))]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(
        fields[&field("slot")].role,
        FieldRole::ItemSlot {
            def_type: "SlotType".to_string()
        },
        "a _steam-suffixed ref must classify the same as its base id would"
    );
}

#[test]
fn a_values_split_evenly_inside_and_outside_r_is_an_item_slot_on_the_boundary() {
    // INSIDE_MIN is `>=`, so an exact 50/50 split (3 inside, 3
    // outside, all 6 clearing MIN_RESOLVED_DISTINCT) counts as
    // inside.
    let refs: BTreeSet<ModId> = [ModId::new("inside")].into_iter().collect();
    let owners = [
        ModId::new("inside"),
        ModId::new("inside"),
        ModId::new("inside"),
        ModId::new("outside"),
        ModId::new("outside"),
        ModId::new("outside"),
    ];
    let values = ["V0", "V1", "V2", "V3", "V4", "V5"];
    let resolve = move |value: &str| -> Vec<(String, ModId)> {
        values
            .iter()
            .position(|v| *v == value)
            .map(|i| vec![("SlotType".to_string(), owners[i].clone())])
            .unwrap_or_default()
    };
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(field("slot"), occurrence(Cardinality::List, &values))]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(
        fields[&field("slot")].role,
        FieldRole::ItemSlot {
            def_type: "SlotType".to_string()
        }
    );
}

#[test]
fn a_field_whose_values_are_all_from_inactive_mods_is_opaque_not_scalar() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |_: &str| -> Vec<(String, ModId)> { vec![] };
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([(
            field("kindNames"),
            occurrence(Cardinality::List, &["Ghost1", "Ghost2", "Ghost3"]),
        )]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(fields[&field("kindNames")].role, FieldRole::Opaque);
}

#[test]
fn a_chances_list_pairs_to_its_sibling_item_slot_case_insensitively() {
    let refs: BTreeSet<ModId> = [ModId::new("framework")].into_iter().collect();
    let resolve = owner_only_resolver("example.PartDef", ModId::new("framework"), five_values());
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("framework"),
        InstanceValues::from([
            (
                field("primaryTool"),
                occurrence(Cardinality::List, five_values()),
            ),
            (
                field("chanceprimaryTool"),
                occurrence(Cardinality::List, &["0.5", "0.2", "0.1", "0.1", "0.1"]),
            ),
        ]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(
        fields[&field("chanceprimaryTool")].role,
        FieldRole::Chances {
            for_slot: field("primaryTool")
        }
    );
}

/// `Chances` pairing takes a
/// *float* list by name — a non-numeric list that merely happens to
/// be named `chance...` must fall to `Opaque`, never `Chances`.
#[test]
fn a_chance_named_list_that_is_not_all_floats_is_opaque_not_chances() {
    let refs: BTreeSet<ModId> = [ModId::new("framework")].into_iter().collect();
    let resolve = owner_only_resolver("example.PartDef", ModId::new("framework"), five_values());
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("framework"),
        InstanceValues::from([
            (
                field("primaryTool"),
                occurrence(Cardinality::List, five_values()),
            ),
            (
                field("chanceprimaryTool"),
                occurrence(Cardinality::List, &["0.5", "not-a-number", "0.1"]),
            ),
        ]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(fields[&field("chanceprimaryTool")].role, FieldRole::Opaque);
}

/// The real-install list-cardinality shape: Example's own `chance...`
/// fields are observed on only a handful of instances, and almost all of
/// *those* occurrences write RimWorld's unwrapped `List<T>` shorthand
/// rather than an `li`-wrapped list — a field-wide majority vote would
/// classify this `Scalar(Number)`, never reaching `Chances` pairing at
/// all. The field is `List` even though only two of five instances
/// wrap it, so it reaches (and passes) `Chances` pairing.
///
/// **The minimum-evidence guard**: with the guard's "at least two"
/// floor a single wrapped occurrence is not enough on its own (see
/// `cardinality_stays_scalar_for_a_single_li_wrapped_outlier`), so this
/// fixture wraps two of the five instances, still a small minority
/// against the other three unwrapped ones.
#[test]
fn a_mostly_unwrapped_chance_field_still_classifies_as_chances() {
    let refs: BTreeSet<ModId> = [ModId::new("framework")].into_iter().collect();
    let resolve = owner_only_resolver("example.PartDef", ModId::new("framework"), five_values());
    let dll_owner = |_: &str| None;

    let instances: Vec<(ModId, InstanceValues)> = five_values()
        .iter()
        .enumerate()
        .map(|(i, part)| {
            let mut values = InstanceValues::from([(
                field("primaryTool"),
                occurrence(Cardinality::List, &[*part]),
            )]);
            // Two of the five instances write the `li`-wrapped shape;
            // the rest use the bare-scalar shorthand.
            let chance_occurrence = if i < 2 {
                occurrence(Cardinality::List, &["0.8"])
            } else {
                occurrence(Cardinality::Scalar, &["0.8"])
            };
            values.insert(field("chanceprimaryTool"), chance_occurrence);
            (ModId::new("framework"), values)
        })
        .collect();

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(
        fields[&field("chanceprimaryTool")].role,
        FieldRole::Chances {
            for_slot: field("primaryTool")
        }
    );
}

#[test]
fn scalar_typing_picks_bool_number_enum_and_text_correctly() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |_: &str| -> Vec<(String, ModId)> { vec![] };
    let dll_owner = |_: &str| None;

    let instances = vec![(
        ModId::new("mod"),
        InstanceValues::from([
            (
                field("hasSingleGender"),
                occurrence(Cardinality::Scalar, &["false"]),
            ),
            (
                field("raceSexDrive"),
                occurrence(Cardinality::Scalar, &["1.5"]),
            ),
            (
                field("mode"),
                occurrence(Cardinality::Scalar, &["A", "A", "B"]),
            ),
            (
                field("freeform"),
                occurrence(
                    Cardinality::Scalar,
                    &["v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8"],
                ),
            ),
        ]),
    )];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);

    assert_eq!(
        fields[&field("hasSingleGender")].role,
        FieldRole::Scalar {
            kind: ScalarKind::Bool,
            default: Some("false".to_string())
        }
    );
    assert_eq!(
        fields[&field("raceSexDrive")].role,
        FieldRole::Scalar {
            kind: ScalarKind::Number,
            default: Some("1.5".to_string())
        }
    );
    assert_eq!(
        fields[&field("mode")].role,
        FieldRole::Scalar {
            kind: ScalarKind::Enum {
                values: ["A", "B"].into_iter().map(str::to_string).collect()
            },
            default: Some("A".to_string())
        }
    );
    assert_eq!(
        fields[&field("freeform")].role,
        FieldRole::Scalar {
            kind: ScalarKind::Text,
            default: Some("v0".to_string())
        }
    );
}

#[test]
fn observed_counts_how_many_instances_carried_the_field() {
    let refs: BTreeSet<ModId> = BTreeSet::new();
    let resolve = |_: &str| -> Vec<(String, ModId)> { vec![] };
    let dll_owner = |_: &str| None;

    let instances = vec![
        (
            ModId::new("a"),
            InstanceValues::from([(field("note"), occurrence(Cardinality::Scalar, &["x"]))]),
        ),
        (ModId::new("b"), InstanceValues::new()),
    ];

    let fields =
        AssignmentSchema::infer_fields(&instances, &refs, &resolve, &dll_owner, &no_type_exists);
    assert_eq!(fields[&field("note")].observed, (1, 2));
}

// -- Determinism ------------------------------------------------

fn permutation_fixture() -> (Vec<(ModId, InstanceValues)>, BTreeSet<ModId>) {
    let refs: BTreeSet<ModId> = [ModId::new("framework")].into_iter().collect();
    let mut instances = Vec::new();
    for i in 0..6u32 {
        let race_name = format!("Race{i}");
        instances.push((
            ModId::new(format!("mod{i}")),
            InstanceValues::from([
                (
                    field("speciesNames"),
                    // `occurrence` clones its values into owned
                    // `String`s immediately, so a plain borrow of
                    // `race_name` (scoped to this loop body) is
                    // enough — no need to leak it to get a
                    // `'static` reference.
                    occurrence(Cardinality::List, &[race_name.as_str()]),
                ),
                (
                    field("hasSingleGender"),
                    occurrence(Cardinality::Scalar, &["true"]),
                ),
            ]),
        ));
    }
    (instances, refs)
}

fn permutation_resolver(value: &str) -> Vec<(String, ModId)> {
    if let Some(rest) = value.strip_prefix("Race") {
        vec![("ThingDef".to_string(), ModId::new(format!("owner{rest}")))]
    } else {
        vec![]
    }
}

proptest! {
    #[test]
    fn infer_fields_is_invariant_to_instance_order(seed in proptest::collection::vec(any::<u8>(), 6)) {
        let (instances, refs) = permutation_fixture();
        let dll_owner = |_: &str| None;

        let baseline = AssignmentSchema::infer_fields(&instances,
            &refs,
            &permutation_resolver,
            &dll_owner,
            &no_type_exists);

        let mut shuffled: Vec<_> = instances.into_iter().zip(seed).collect();
        shuffled.sort_by_key(|(_, key)| *key);
        let shuffled: Vec<_> = shuffled.into_iter().map(|(inst, _)| inst).collect();
        let permuted = AssignmentSchema::infer_fields(&shuffled,
            &refs,
            &permutation_resolver,
            &dll_owner,
            &no_type_exists);

        prop_assert_eq!(baseline, permuted);
    }
}

// -- FieldRole confirmation transitions --------------------------

fn schema_with_one_field(role: FieldRole) -> AssignmentSchema {
    let mut fields = BTreeMap::new();
    fields.insert(
        field("speciesNames"),
        FieldSpec {
            role,
            cardinality: Cardinality::List,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    AssignmentSchema {
        def_type: "example.PartAssignmentDef".to_string(),
        refs: BTreeSet::new(),
        fields,
        target_shapes: BTreeMap::new(),
    }
}

#[test]
fn confirm_role_keeps_the_original_inferred_role_across_repeated_reclassification() {
    let original = FieldRole::TargetKey {
        def_type: "ThingDef".to_string(),
    };
    let mut schema = schema_with_one_field(original.clone());

    schema
        .confirm_role(
            &field("speciesNames"),
            FieldRole::ItemSlot {
                def_type: "ThingDef".to_string(),
            },
        )
        .expect("field exists");
    assert_eq!(
        schema.fields[&field("speciesNames")].inferred_role,
        Some(original.clone())
    );

    schema
        .confirm_role(&field("speciesNames"), FieldRole::Opaque)
        .expect("field exists");
    assert_eq!(
        schema.fields[&field("speciesNames")].role,
        FieldRole::Opaque,
        "the role itself must update on every reclassification"
    );
    assert_eq!(
        schema.fields[&field("speciesNames")].inferred_role,
        Some(original),
        "the *original* inferred role must survive a second reclassification"
    );
}

#[test]
fn confirm_role_on_an_unknown_field_is_an_error() {
    let mut schema = schema_with_one_field(FieldRole::Opaque);
    let result = schema.confirm_role(&field("nope"), FieldRole::Opaque);
    assert_eq!(result, Err(UnknownFieldError(field("nope"))));
}

#[test]
fn add_field_has_no_inferred_role() {
    let mut schema = schema_with_one_field(FieldRole::Opaque);
    schema
        .add_field(
            field("custom"),
            FieldRole::Scalar {
                kind: ScalarKind::Text,
                default: None,
            },
            Cardinality::Scalar,
        )
        .expect("a fresh field path must be accepted");
    assert_eq!(schema.fields[&field("custom")].inferred_role, None);
}

#[test]
fn add_field_rejects_a_path_the_schema_already_has() {
    let mut schema = schema_with_one_field(FieldRole::Opaque);
    let result = schema.add_field(
        field("speciesNames"),
        FieldRole::Opaque,
        Cardinality::Scalar,
    );
    assert_eq!(result, Err(FieldAlreadyExistsError(field("speciesNames"))));
}

#[test]
fn remove_field_hides_it() {
    let mut schema = schema_with_one_field(FieldRole::Opaque);
    assert!(schema.remove_field(&field("speciesNames")).is_some());
    assert!(!schema.fields.contains_key(&field("speciesNames")));
}

#[test]
fn has_target_key_is_true_only_when_a_field_is_one() {
    let with_key = schema_with_one_field(FieldRole::TargetKey {
        def_type: "ThingDef".to_string(),
    });
    assert!(with_key.has_target_key());

    let without_key = schema_with_one_field(FieldRole::Opaque);
    assert!(!without_key.has_target_key());
}

// -- TargetShape candidates ---------------------------------------

#[test]
fn infer_learns_a_share_not_a_strict_intersection() {
    let sets: Vec<BTreeSet<String>> = (0..100)
        .map(|i| {
            let mut set: BTreeSet<String> = ["defName", "label", "race"]
                .into_iter()
                .map(str::to_string)
                .collect();
            // One outlier target lacks `statBases` — present on 99/100
            // (>= 95%), so it still ends up required; a strict
            // intersection would have dropped it.
            if i != 0 {
                set.insert("statBases".to_string());
            }
            set
        })
        .collect();

    let shape = TargetShape::infer("ThingDef", &sets);

    assert!(shape.required_children.contains("race"));
    assert!(shape.required_children.contains("statBases"));
    assert!(!shape.required_children.contains("defName"));
    assert!(!shape.required_children.contains("label"));
}

#[test]
fn infer_excludes_a_child_below_the_share_threshold() {
    let sets: Vec<BTreeSet<String>> = (0..100)
        .map(|i| {
            let mut set: BTreeSet<String> = ["defName", "race"]
                .into_iter()
                .map(str::to_string)
                .collect();
            // Present on only 90/100 (< 95%): excluded.
            if i < 90 {
                set.insert("rare".to_string());
            }
            set
        })
        .collect();

    let shape = TargetShape::infer("ThingDef", &sets);
    assert!(!shape.required_children.contains("rare"));
}

/// Pins [`SHAPE_MIN`]'s own boundary precisely: present on exactly
/// 95/100 passes (the operator is `>=`), 94/100 does not.
#[test]
fn shape_min_boundary_ninety_five_of_a_hundred_passes_ninety_four_fails() {
    let sets_with_share = |present: usize| -> Vec<BTreeSet<String>> {
        (0..100)
            .map(|i| {
                let mut set: BTreeSet<String> = ["defName", "race"]
                    .into_iter()
                    .map(str::to_string)
                    .collect();
                if i < present {
                    set.insert("borderline".to_string());
                }
                set
            })
            .collect()
    };

    let passes = TargetShape::infer("ThingDef", &sets_with_share(95));
    assert!(
        passes.required_children.contains("borderline"),
        "95/100 (exactly SHAPE_MIN) must pass"
    );

    let fails = TargetShape::infer("ThingDef", &sets_with_share(94));
    assert!(
        !fails.required_children.contains("borderline"),
        "94/100 (just under SHAPE_MIN) must not pass"
    );
}

#[test]
fn infer_with_no_referenced_targets_is_unfiltered() {
    let shape = TargetShape::infer("ThingDef", &[]);
    assert!(shape.is_unfiltered());
}

#[test]
fn matches_requires_every_required_child() {
    let shape = TargetShape {
        def_type: "ThingDef".to_string(),
        required_children: ["race", "statBases"]
            .into_iter()
            .map(str::to_string)
            .collect(),
    };
    let full: BTreeSet<String> = ["race", "statBases", "label"]
        .into_iter()
        .map(str::to_string)
        .collect();
    let partial: BTreeSet<String> = ["race"].into_iter().map(str::to_string).collect();

    assert!(shape.matches(&full));
    assert!(!shape.matches(&partial));
}

// -- AssignmentId ---------------------------------------------------

#[test]
fn assignment_id_derive_is_stable_and_well_formed() {
    let id = AssignmentId::derive("profile", &ModId::new("mypatch.parts"), ts(1));
    assert_eq!(id.as_str().len(), 12);
    assert!(
        id.as_str()
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    );
    assert_eq!(
        id,
        AssignmentId::derive("profile", &ModId::new("mypatch.parts"), ts(1))
    );
}

#[test]
fn assignment_id_derive_differs_when_any_input_differs() {
    let base = AssignmentId::derive("abc123", &ModId::new("mypatch.parts"), ts(1000));
    assert_ne!(
        base,
        AssignmentId::derive("def456", &ModId::new("mypatch.parts"), ts(1000))
    );
    assert_ne!(
        base,
        AssignmentId::derive("abc123", &ModId::new("sample.other"), ts(1000))
    );
    assert_ne!(
        base,
        AssignmentId::derive("abc123", &ModId::new("mypatch.parts"), ts(2000))
    );
}

#[test]
fn assignment_id_from_str_accepts_exactly_twelve_lowercase_hex_chars() {
    let id: AssignmentId = "3f9a1c02be77".parse().expect("valid id");
    assert_eq!(id.to_string(), "3f9a1c02be77");
}

#[test]
fn assignment_id_from_str_rejects_wrong_length() {
    assert!("3f9a1c".parse::<AssignmentId>().is_err());
}

#[test]
fn assignment_id_from_str_rejects_uppercase() {
    assert!("3F9A1C02BE77".parse::<AssignmentId>().is_err());
}

#[test]
fn assignment_id_from_str_rejects_non_hex_characters() {
    assert!("3f9a1c02be7z".parse::<AssignmentId>().is_err());
}

// -- AssignmentProject: serde round trip + old-file tolerance -------

fn project() -> AssignmentProject {
    AssignmentProject::new(
        AssignmentId::derive("profile", &ModId::new("mypatch.parts"), ts(1)),
        "Example race patch".to_string(),
        PatchModIdentity::new("mypatch.parts", "Sample Part Patch").expect("valid identity"),
        [ModId::new("example.framework")].into_iter().collect(),
        [ModId::new("some.race.mod")].into_iter().collect(),
        schema_with_one_field(FieldRole::TargetKey {
            def_type: "ThingDef".to_string(),
        }),
        ts(1),
    )
}

/// A [`KnownDefs`] fake that answers `false` for everything —
/// "nothing is already active", the right answer whenever a test
/// doesn't care about item-slot or def-name-collision validation.
struct NoneKnown;
impl KnownDefs for NoneKnown {
    fn contains(&self, _def_type: &str, _name: &str) -> bool {
        false
    }
}

/// A [`KnownDefs`] fake that answers `true` for any name of exactly
/// one def type — used by tests that need an item slot's names to
/// validate without asserting on which specific names exist. Never
/// answers `true` for a schema's own def type (`"example.PartAssignmentDef"`
/// in every fixture here), so it never spuriously collides with
/// [`AssignmentRowError::DefNameExistsInActiveList`].
struct KnownItemType(&'static str);
impl KnownDefs for KnownItemType {
    fn contains(&self, def_type: &str, _name: &str) -> bool {
        def_type == self.0
    }
}

#[test]
fn to_stored_and_from_stored_round_trip() {
    let mut original = project();
    original
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(TargetRef {
                key_field: field("speciesNames"),
                def: DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Human".to_string(),
                },
            }),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "mypatch_parts_Human".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("valid row");

    let stored = original.to_stored();
    let restored = AssignmentProject::from_stored(stored);

    assert_eq!(restored, original);
}

/// [`FieldSpec::inferred_role`] is `Option`, so a JSON object saved
/// before a schema had ever been reclassified (no `inferred_role`
/// key at all) must still deserialize, defaulting to `None` — the
/// same tolerance a real `rimmerge.json`/store file needs across a
/// schema evolution.
#[test]
fn field_spec_deserializes_when_inferred_role_is_missing() {
    let json = r#"{"role":{"type":"opaque"},"cardinality":"scalar","observed":[1,1]}"#;
    let spec: FieldSpec =
        serde_json::from_str(json).expect("must tolerate a missing optional field");
    assert_eq!(spec.inferred_role, None);
}

#[test]
fn assignment_schema_json_round_trips() {
    let schema = schema_with_one_field(FieldRole::ItemSlot {
        def_type: "example.PartDef".to_string(),
    });
    let json = serde_json::to_string(&schema).expect("serializable");
    let restored: AssignmentSchema = serde_json::from_str(&json).expect("deserializable");
    assert_eq!(restored, schema);
}

#[test]
fn content_sha256_is_stable_for_the_same_project() {
    let project = project();
    assert_eq!(project.content_sha256(), project.content_sha256());
}

#[test]
fn content_sha256_changes_when_a_row_is_added() {
    let mut project = project();
    let before = project.content_sha256();
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(TargetRef {
                key_field: field("speciesNames"),
                def: DefKey {
                    def_type: "ThingDef".to_string(),
                    def_name: "Human".to_string(),
                },
            }),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "mypatch_parts_Human".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("valid row");
    assert_ne!(before, project.content_sha256());
}

// -- AssignmentProject::set_row invariants --------------------------

#[test]
fn set_row_rejects_a_target_whose_key_field_is_not_a_target_key() {
    // A schema that *does* have a real `TargetKey` field ("realKey")
    // so the section is target-keyed overall — the point of this test
    // is a `TargetRef` naming a *different*, non-`TargetKey` field
    // ("speciesNames", `Opaque` here), not a free-standing section (that
    // shape is `set_row_rejects_an_own_key_on_a_target_keyed_section`'s
    // mirror-image sibling).
    let mut fields = BTreeMap::new();
    fields.insert(
        field("realKey"),
        FieldSpec {
            role: FieldRole::TargetKey {
                def_type: "ThingDef".to_string(),
            },
            cardinality: Cardinality::List,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    fields.insert(
        field("speciesNames"),
        FieldSpec {
            role: FieldRole::Opaque,
            cardinality: Cardinality::List,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    let schema = AssignmentSchema {
        def_type: "example.PartAssignmentDef".to_string(),
        refs: BTreeSet::new(),
        fields,
        target_shapes: BTreeMap::new(),
    };
    let mut project = AssignmentProject::new(
        AssignmentId::derive("profile", &ModId::new("mypatch.parts"), ts(1)),
        "name".to_string(),
        PatchModIdentity::new("mypatch.parts", "name").expect("valid identity"),
        BTreeSet::new(),
        BTreeSet::new(),
        schema,
        ts(1),
    );
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(TargetRef {
            key_field: field("speciesNames"),
            def: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Human".to_string(),
            },
        }),
        AssignmentRow {
            values: BTreeMap::new(),
            def_name: "x".to_string(),
            note: None,
        },
        &NoneKnown,
    );
    assert_eq!(
        result,
        Err(AssignmentRowError::NotATargetKey(field("speciesNames")))
    );
}

#[test]
fn set_row_rejects_an_empty_def_name() {
    let mut project = project_with_slot();
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values: BTreeMap::new(),
            def_name: "   ".to_string(),
            note: None,
        },
        &NoneKnown,
    );
    assert_eq!(result, Err(AssignmentRowError::EmptyDefName));
}

/// A `def_name` that
/// already names an active instance of the schema's own def type
/// must be refused, not silently allowed to collide on export.
#[test]
fn set_row_rejects_a_def_name_that_already_names_an_active_instance() {
    let mut project = project_with_slot();
    struct KnownDup;
    impl KnownDefs for KnownDup {
        fn contains(&self, def_type: &str, name: &str) -> bool {
            def_type == "example.PartAssignmentDef" && name == "dup"
        }
    }
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values: BTreeMap::new(),
            def_name: "dup".to_string(),
            note: None,
        },
        &KnownDup,
    );
    assert_eq!(
        result,
        Err(AssignmentRowError::DefNameExistsInActiveList(
            "dup".to_string()
        ))
    );
}

#[test]
fn set_row_rejects_a_value_for_an_unknown_field() {
    let mut project = project_with_slot();
    let mut values = BTreeMap::new();
    values.insert(field("nope"), RowValue::Text("x".to_string()));
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "x".to_string(),
            note: None,
        },
        &NoneKnown,
    );
    assert_eq!(result, Err(AssignmentRowError::UnknownField(field("nope"))));
}

#[test]
fn set_row_rejects_a_text_value_on_an_item_slot_field() {
    let mut project = project_with_slot();
    let mut values = BTreeMap::new();
    values.insert(
        field("primaryTool"),
        RowValue::Text("not a name list".to_string()),
    );
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "x".to_string(),
            note: None,
        },
        &NoneKnown,
    );
    assert_eq!(
        result,
        Err(AssignmentRowError::WrongValueShape {
            path: field("primaryTool"),
            role: FieldRole::ItemSlot {
                def_type: "example.PartDef".to_string()
            },
        })
    );
}

#[test]
fn set_row_rejects_any_value_at_all_on_a_target_key_field() {
    let mut project = project_with_slot();
    let mut values = BTreeMap::new();
    values.insert(field("speciesNames"), RowValue::Omit);
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "x".to_string(),
            note: None,
        },
        &NoneKnown,
    );
    assert_eq!(
        result,
        Err(AssignmentRowError::WrongValueShape {
            path: field("speciesNames"),
            role: FieldRole::TargetKey {
                def_type: "ThingDef".to_string()
            },
        }),
        "even `Omit` is a shape mismatch on a TargetKey field — it's addressed \
             through TargetRef::key_field, never given a row value of its own"
    );
}

#[test]
fn set_row_rejects_a_non_finite_chance_value() {
    let mut project = project_with_slot();
    let mut values = BTreeMap::new();
    values.insert(
        field("primaryTool"),
        RowValue::Names(vec!["PartA".to_string()]),
    );
    values.insert(
        field("chanceprimaryTool"),
        RowValue::Numbers(vec![f64::NAN]),
    );
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "x".to_string(),
            note: None,
        },
        &KnownItemType("example.PartDef"),
    );
    assert_eq!(
        result,
        Err(AssignmentRowError::NonFiniteChance(field(
            "chanceprimaryTool"
        )))
    );
}

/// A `Cardinality::Scalar` `ItemSlot` must name exactly one item —
/// `rim-merge`'s `render_leaf` already refuses anything else (a
/// `SkippedField`, not an error), so the domain check here closes the
/// gap that let the CLI (with no picker of its own) accept a value
/// only the export step would ever refuse.
#[test]
fn set_row_rejects_a_scalar_item_slot_with_more_than_one_name() {
    let mut project = project_with_scalar_slot();
    let mut values = BTreeMap::new();
    values.insert(
        field("defaultToolDef"),
        RowValue::Names(vec!["Chicken".to_string(), "Duck".to_string()]),
    );
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "x".to_string(),
            note: None,
        },
        &NoneKnown,
    );
    assert_eq!(
        result,
        Err(AssignmentRowError::ScalarItemSlotWrongCount {
            path: field("defaultToolDef"),
            actual: 2,
        })
    );
}

#[test]
fn set_row_rejects_a_scalar_item_slot_with_no_names() {
    let mut project = project_with_scalar_slot();
    let mut values = BTreeMap::new();
    values.insert(field("defaultToolDef"), RowValue::Names(Vec::new()));
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "x".to_string(),
            note: None,
        },
        &NoneKnown,
    );
    assert_eq!(
        result,
        Err(AssignmentRowError::ScalarItemSlotWrongCount {
            path: field("defaultToolDef"),
            actual: 0,
        })
    );
}

#[test]
fn set_row_accepts_a_scalar_item_slot_with_exactly_one_known_name() {
    let mut project = project_with_scalar_slot();
    let mut values = BTreeMap::new();
    values.insert(
        field("defaultToolDef"),
        RowValue::Names(vec!["Chicken".to_string()]),
    );
    struct KnownChicken;
    impl KnownDefs for KnownChicken {
        fn contains(&self, def_type: &str, name: &str) -> bool {
            def_type == "ThingDef" && name == "Chicken"
        }
    }
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "x".to_string(),
            note: None,
        },
        &KnownChicken,
    );
    assert!(result.is_ok(), "{result:?}");
}

/// [`project_with_slot`]'s sibling, but its one item-slot field
/// (`defaultToolDef`) is `Cardinality::Scalar` — the shape the
/// `Def`/`Defs`-suffix exemption makes common (`rim-resolve/CLAUDE.md`).
fn project_with_scalar_slot() -> AssignmentProject {
    let mut fields = BTreeMap::new();
    fields.insert(
        field("speciesNames"),
        FieldSpec {
            role: FieldRole::TargetKey {
                def_type: "ThingDef".to_string(),
            },
            cardinality: Cardinality::List,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    fields.insert(
        field("defaultToolDef"),
        FieldSpec {
            role: FieldRole::ItemSlot {
                def_type: "ThingDef".to_string(),
            },
            cardinality: Cardinality::Scalar,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    let schema = AssignmentSchema {
        def_type: "example.PartAssignmentDef".to_string(),
        refs: BTreeSet::new(),
        fields,
        target_shapes: BTreeMap::new(),
    };
    AssignmentProject::new(
        AssignmentId::derive("profile", &ModId::new("mypatch.parts"), ts(1)),
        "name".to_string(),
        PatchModIdentity::new("mypatch.parts", "name").expect("valid identity"),
        BTreeSet::new(),
        BTreeSet::new(),
        schema,
        ts(1),
    )
}

fn project_with_slot() -> AssignmentProject {
    let mut fields = BTreeMap::new();
    fields.insert(
        field("speciesNames"),
        FieldSpec {
            role: FieldRole::TargetKey {
                def_type: "ThingDef".to_string(),
            },
            cardinality: Cardinality::List,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    fields.insert(
        field("primaryTool"),
        FieldSpec {
            role: FieldRole::ItemSlot {
                def_type: "example.PartDef".to_string(),
            },
            cardinality: Cardinality::List,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    fields.insert(
        field("chanceprimaryTool"),
        FieldSpec {
            role: FieldRole::Chances {
                for_slot: field("primaryTool"),
            },
            cardinality: Cardinality::List,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    let schema = AssignmentSchema {
        def_type: "example.PartAssignmentDef".to_string(),
        refs: BTreeSet::new(),
        fields,
        target_shapes: BTreeMap::new(),
    };
    AssignmentProject::new(
        AssignmentId::derive("profile", &ModId::new("mypatch.parts"), ts(1)),
        "name".to_string(),
        PatchModIdentity::new("mypatch.parts", "name").expect("valid identity"),
        BTreeSet::new(),
        BTreeSet::new(),
        schema,
        ts(1),
    )
}

fn human_target() -> TargetRef {
    TargetRef {
        key_field: field("speciesNames"),
        def: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Human".to_string(),
        },
    }
}

#[test]
fn set_row_rejects_an_item_slot_value_naming_an_unknown_def() {
    let mut project = project_with_slot();
    let mut values = BTreeMap::new();
    values.insert(
        field("primaryTool"),
        RowValue::Names(vec!["Ghost".to_string()]),
    );
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "x".to_string(),
            note: None,
        },
        &NoneKnown,
    );
    assert_eq!(
        result,
        Err(AssignmentRowError::UnknownItem {
            path: field("primaryTool"),
            def_type: "example.PartDef".to_string(),
            name: "Ghost".to_string(),
        })
    );
}

#[test]
fn set_row_rejects_a_chances_length_mismatch() {
    let mut project = project_with_slot();
    let mut values = BTreeMap::new();
    values.insert(
        field("primaryTool"),
        RowValue::Names(vec!["PartA".to_string(), "PartB".to_string()]),
    );
    values.insert(field("chanceprimaryTool"), RowValue::Numbers(vec![1.0]));
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "x".to_string(),
            note: None,
        },
        &KnownItemType("example.PartDef"),
    );
    assert_eq!(
        result,
        Err(AssignmentRowError::ChanceLengthMismatch {
            path: field("chanceprimaryTool"),
            expected: 2,
            actual: 1,
        })
    );
}

#[test]
fn set_row_accepts_a_valid_row() {
    let mut project = project_with_slot();
    let mut values = BTreeMap::new();
    values.insert(
        field("primaryTool"),
        RowValue::Names(vec!["PartA".to_string(), "PartB".to_string()]),
    );
    values.insert(
        field("chanceprimaryTool"),
        RowValue::Numbers(vec![0.5, 0.5]),
    );
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "mypatch_parts_Human".to_string(),
            note: None,
        },
        &KnownItemType("example.PartDef"),
    );
    assert!(result.is_ok());
}

#[test]
fn set_row_rejects_a_duplicate_def_name_across_rows() {
    let mut project = project_with_slot();
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(human_target()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "dup".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("first row valid");

    let other_target = TargetRef {
        key_field: field("speciesNames"),
        def: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Elf".to_string(),
        },
    };
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(other_target),
        AssignmentRow {
            values: BTreeMap::new(),
            def_name: "dup".to_string(),
            note: None,
        },
        &NoneKnown,
    );
    assert_eq!(
        result,
        Err(AssignmentRowError::DuplicateDefName("dup".to_string()))
    );
}

#[test]
fn set_row_allows_replacing_the_same_target_with_the_same_def_name() {
    let mut project = project_with_slot();
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(human_target()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "same".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("first write valid");
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values: BTreeMap::new(),
            def_name: "same".to_string(),
            note: Some("updated".to_string()),
        },
        &NoneKnown,
    );
    assert!(result.is_ok());
}

// -- AssignmentProject: standalone ("new def") projects, no TargetKey
//    field at all (a candidate with no target key is only offered when T is
//    empty, and its rows are free-standing instances addressed by
//    their own defName rather than a TargetRef — see
//    `Self::is_standalone`'s own doc comment) ------------------------

/// A schema shaped like a fresh content type (e.g. a new
/// `example.PartDef`), with an `ItemSlot` field so both item and
/// def-name validation are exercised — no `TargetKey` field anywhere.
fn standalone_schema() -> AssignmentSchema {
    let mut fields = BTreeMap::new();
    fields.insert(
        field("hediffName"),
        FieldSpec {
            role: FieldRole::Scalar {
                kind: ScalarKind::Text,
                default: None,
            },
            cardinality: Cardinality::Scalar,
            observed: (1, 1),
            inferred_role: None,
        },
    );
    AssignmentSchema {
        def_type: "example.PartDef".to_string(),
        refs: BTreeSet::new(),
        fields,
        target_shapes: BTreeMap::new(),
    }
}

fn standalone_project() -> AssignmentProject {
    AssignmentProject::new(
        AssignmentId::derive("profile", &ModId::new("sample.newpart"), ts(1)),
        "New Part".to_string(),
        PatchModIdentity::new("sample.newpart", "Sample's New Part").expect("valid identity"),
        [ModId::new("example.framework")].into_iter().collect(),
        BTreeSet::new(),
        standalone_schema(),
        ts(1),
    )
}

#[test]
fn is_standalone_is_true_for_a_schema_with_no_target_key_field() {
    assert!(
        standalone_project()
            .section("example.PartDef")
            .expect("section present")
            .is_standalone()
    );
    // Sanity check on the other side: `project()`'s own schema has a
    // `TargetKey` field, so it is never standalone.
    assert!(
        !project()
            .section("example.PartAssignmentDef")
            .expect("section present")
            .is_standalone()
    );
}

#[test]
fn set_standalone_row_accepts_and_stores_a_valid_row() {
    let mut project = standalone_project();

    let replaced = project
        .set_row(
            "example.PartDef",
            RowKey::Own("mypatch_newpart_Tail".to_string()),
            AssignmentRow {
                values: BTreeMap::from([(
                    field("hediffName"),
                    RowValue::Text("NewPart_Hediff".to_string()),
                )]),
                def_name: "mypatch_newpart_Tail".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("a valid standalone row must be accepted");

    assert!(replaced.is_none());
    assert_eq!(
        project
            .section("example.PartDef")
            .expect("section present")
            .rows
            .len(),
        1
    );
}

#[test]
fn set_row_rejects_an_own_key_on_a_target_keyed_section() {
    // `project()`'s schema has a TargetKey field — an own-keyed row
    // never applies to it.
    let mut project = project();

    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Own("x".to_string()),
        AssignmentRow {
            values: BTreeMap::new(),
            def_name: "x".to_string(),
            note: None,
        },
        &NoneKnown,
    );

    assert_eq!(
        result,
        Err(AssignmentRowError::RowKeyMismatch(
            "example.PartAssignmentDef".to_string()
        ))
    );
}

/// A `RowKey::Own(name)` whose own
/// string doesn't match `row.def_name` must be refused — otherwise
/// `Section::own_instance_names()` (what an item picker and
/// `remove_section`'s in-use scan see) would answer `name`, while the
/// row actually renders under `row.def_name`, a dangling-reference gap
/// no later validation ever catches.
#[test]
fn set_row_rejects_an_own_key_that_does_not_match_the_rows_own_def_name() {
    let mut project = standalone_project();

    let result = project.set_row(
        "example.PartDef",
        RowKey::Own("keyX".to_string()),
        AssignmentRow {
            values: BTreeMap::new(),
            def_name: "Actual".to_string(),
            note: None,
        },
        &NoneKnown,
    );

    assert_eq!(
        result,
        Err(AssignmentRowError::OwnKeyDefNameMismatch {
            key: "keyX".to_string(),
            def_name: "Actual".to_string(),
        })
    );
}

#[test]
fn set_row_rejects_an_empty_def_name_on_a_standalone_row() {
    let mut project = standalone_project();

    let result = project.set_row(
        "example.PartDef",
        RowKey::Own("  ".to_string()),
        AssignmentRow {
            values: BTreeMap::new(),
            def_name: "  ".to_string(),
            note: None,
        },
        &NoneKnown,
    );

    assert_eq!(result, Err(AssignmentRowError::EmptyDefName));
}

#[test]
fn clear_row_removes_a_standalone_row_by_its_own_key() {
    let mut project = standalone_project();
    project
        .set_row(
            "example.PartDef",
            RowKey::Own("mypatch_newpart_Tail".to_string()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "mypatch_newpart_Tail".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("valid row");

    let cleared = project.clear_row(
        "example.PartDef",
        &RowKey::Own("mypatch_newpart_Tail".to_string()),
    );

    assert!(cleared.is_some());
    assert!(
        project
            .section("example.PartDef")
            .expect("section present")
            .rows
            .is_empty()
    );
}

#[test]
fn standalone_project_round_trips_through_stored() {
    let mut original = standalone_project();
    original
        .set_row(
            "example.PartDef",
            RowKey::Own("mypatch_newpart_Tail".to_string()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "mypatch_newpart_Tail".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("valid row");

    let restored = AssignmentProject::from_stored(original.to_stored());

    assert_eq!(restored, original);
}

// -- PrecedenceRule: the data loader's own entry point, and Unverified

/// `top_level_field` is what the data loader builds a `key_priority`
/// entry with from a plain tag name — one `Child` segment, never a
/// parsed path.
#[test]
fn top_level_field_builds_a_single_child_segment_path() {
    let path = top_level_field("kindNames");
    assert_eq!(
        path.segments(),
        &[PathSegment::Child("kindNames".to_string())]
    );
}

#[test]
fn unverified_never_names_a_winner() {
    let order = LoadOrder::new(vec![ModId::new("a")]);
    let matches = vec![ExistingMatch {
        owner: ModId::new("a"),
        instance_def_name: "SomeGroup".to_string(),
        key_field: field("speciesNames"),
    }];
    let winner = PrecedenceRule::Unverified.winner(&matches, None, &order);
    assert_eq!(winner, None);
}

/// The Example rule's own four-way tie-break, in priority order:
/// framework-vs-not beats key priority beats load order. Each
/// candidate differs from the previous one along exactly one
/// dimension, so this fails if any dimension is checked out of
/// order.
#[test]
fn prefer_outside_framework_ranks_by_framework_then_key_priority_then_load_order() {
    let rule = PrecedenceRule::PreferOutsideFramework {
        framework: ModId::new("example.framework"),
        key_priority: vec![field("kindNames"), field("speciesNames")],
    };
    let order = LoadOrder::new(vec![
        ModId::new("example.framework"),
        ModId::new("addon.a"),
        ModId::new("addon.b"),
    ]);

    // 1. An Example-owned match loses to any non-Example match outright,
    //    even though Example loads first.
    let matches = vec![
        ExistingMatch {
            owner: ModId::new("example.framework"),
            instance_def_name: "Builtin".to_string(),
            key_field: field("speciesNames"),
        },
        ExistingMatch {
            owner: ModId::new("addon.b"),
            instance_def_name: "AddonGroup".to_string(),
            key_field: field("speciesNames"),
        },
    ];
    assert_eq!(
        rule.winner(&matches, None, &order),
        Some(Winner::Existing(matches[1].clone()))
    );

    // 2. This project's own (not-yet-exported) row is never "owned
    //    by the framework", so it beats an Example-owned match outright.
    let this_project_key = field("speciesNames");
    assert_eq!(
        rule.winner(
            &matches[..1],
            Some((&ModId::new("mypatch.parts"), &this_project_key)),
            &order
        ),
        Some(Winner::ThisProject)
    );

    // 3. Two non-framework matches, tied on framework: the one
    //    matched through the higher-priority key field wins,
    //    regardless of load order (addon.b loads after addon.a).
    let matches = vec![
        ExistingMatch {
            owner: ModId::new("addon.a"),
            instance_def_name: "ByRace".to_string(),
            key_field: field("speciesNames"),
        },
        ExistingMatch {
            owner: ModId::new("addon.b"),
            instance_def_name: "ByKind".to_string(),
            key_field: field("kindNames"),
        },
    ];
    assert_eq!(
        rule.winner(&matches, None, &order),
        Some(Winner::Existing(matches[1].clone())),
        "kindNames outranks speciesNames regardless of load order"
    );

    // 4. Two non-framework matches, tied on framework and matched
    //    through the same key field: the earlier in the selected
    //    load order wins.
    let matches = vec![
        ExistingMatch {
            owner: ModId::new("addon.b"),
            instance_def_name: "Second".to_string(),
            key_field: field("speciesNames"),
        },
        ExistingMatch {
            owner: ModId::new("addon.a"),
            instance_def_name: "First".to_string(),
            key_field: field("speciesNames"),
        },
    ];
    assert_eq!(
        rule.winner(&matches, None, &order),
        Some(Winner::Existing(matches[1].clone()))
    );
}

#[test]
fn winner_is_none_when_there_is_nothing_to_rank() {
    let rule = PrecedenceRule::PreferOutsideFramework {
        framework: ModId::new("example.framework"),
        key_priority: vec![],
    };
    let order = LoadOrder::new(vec![]);
    assert_eq!(rule.winner(&[], None, &order), None);
}

// -- coverage: a small fixture with exact numbers -------------------

fn coverage_fixture() -> (
    AssignmentProject,
    BTreeMap<TargetRef, ModId>,
    Vec<ExistingInstance>,
    LoadOrder,
) {
    let human = TargetRef {
        key_field: field("speciesNames"),
        def: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Human".to_string(),
        },
    };
    let elf = TargetRef {
        key_field: field("speciesNames"),
        def: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Elf".to_string(),
        },
    };
    let orc = TargetRef {
        key_field: field("speciesNames"),
        def: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Orc".to_string(),
        },
    };
    let mut candidates = BTreeMap::new();
    candidates.insert(human.clone(), ModId::new("ludeon.rimworld"));
    candidates.insert(elf.clone(), ModId::new("race.elves"));
    candidates.insert(orc.clone(), ModId::new("race.orcs"));

    let existing = vec![
        ExistingInstance {
            owner: ModId::new("example.framework"),
            instance_def_name: "Human_Group".to_string(),
            key_field: field("speciesNames"),
            target: human.def.clone(),
        },
        ExistingInstance {
            owner: ModId::new("addon.elves"),
            instance_def_name: "Elf_Group".to_string(),
            key_field: field("speciesNames"),
            target: elf.def.clone(),
        },
    ];

    let order = LoadOrder::new(vec![
        ModId::new("example.framework"),
        ModId::new("addon.elves"),
        ModId::new("mypatch.parts"),
    ]);

    let mut project = project();
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(elf),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "mypatch_parts_Elf".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("elf row valid");

    (project, candidates, existing, order)
}

#[test]
fn coverage_lists_every_candidate_uncovered_first_with_exact_counts() {
    let (project, candidates, existing, order) = coverage_fixture();
    let rule = PrecedenceRule::Unverified;

    let result = coverage(
        &project,
        "example.PartAssignmentDef",
        &candidates,
        &existing,
        &order,
        &rule,
    );

    assert_eq!(result.rows.len(), 3);
    // Uncovered (Orc) sorts before both overrides.
    assert_eq!(result.rows[0].target.def.def_name, "Orc");
    assert!(matches!(result.rows[0].intent, RowIntent::Cover));
    assert!(result.rows[0].matches.is_empty());
    assert!(!result.rows[0].has_row);

    // Both covered rows sort after the uncovered one, then by owner:
    // Human's owner (`ludeon.rimworld`) precedes Elf's
    // (`race.elves`) lexically.
    let covered: Vec<&str> = result.rows[1..]
        .iter()
        .map(|row| row.target.def.def_name.as_str())
        .collect();
    assert_eq!(covered, vec!["Human", "Elf"]);
    assert!(matches!(result.rows[1].intent, RowIntent::Override));
    assert_eq!(result.rows[1].matches.len(), 1);
    assert!(
        !result.rows[1].has_row,
        "no row was set for Human in the fixture"
    );
    assert!(result.rows[2].has_row, "the Elf row was set in the fixture");

    // Unverified names no winner anywhere.
    assert!(result.rows.iter().all(|row| row.winner.is_none()));
}

#[test]
fn coverage_winner_prefers_the_earlier_load_order_existing_match_over_this_projects_own_row() {
    let (project, candidates, existing, order) = coverage_fixture();
    let rule = PrecedenceRule::PreferOutsideFramework {
        framework: ModId::new("example.framework"),
        key_priority: vec![field("kindNames"), field("speciesNames")],
    };

    let result = coverage(
        &project,
        "example.PartAssignmentDef",
        &candidates,
        &existing,
        &order,
        &rule,
    );
    let elf_row = result
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Elf")
        .expect("Elf row present");

    // The existing Elf_Group match is owned by `addon.elves`, not the
    // framework, and this project's own row is likewise not
    // framework-owned — both tie on the framework dimension, and on
    // key priority (both matched via speciesNames), so the load order
    // decides: `addon.elves` sits at position 1 in this fixture's
    // `order`, ahead of this project's own package id
    // (`mypatch.parts`, position 2), so the existing match wins.
    assert_eq!(
        elf_row.winner,
        Some(Winner::Existing(ExistingMatch {
            owner: ModId::new("addon.elves"),
            instance_def_name: "Elf_Group".to_string(),
            key_field: field("speciesNames"),
        }))
    );

    let human_row = result
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Human")
        .expect("Human row present");
    // The Human match is Example's own built-in: loses outright to
    // nothing else being offered, so it's still the only candidate
    // and therefore still the winner (no row exists for Human here).
    assert_eq!(
        human_row.winner,
        Some(Winner::Existing(ExistingMatch {
            owner: ModId::new("example.framework"),
            instance_def_name: "Human_Group".to_string(),
            key_field: field("speciesNames"),
        }))
    );
}

/// Once this project's own package id
/// is genuinely active and loads *earlier* than a competing
/// non-framework match, its own row must win on load order — not
/// always lose by being treated as unplaced. Uses a different
/// `LoadOrder` than [`coverage_fixture`]'s own (there, this
/// project's package id sits *after* `addon.elves`, so replacing
/// `winner`'s `order.position(owner)` lookup with a hardcoded `None`
/// would still lose to `addon.elves` either way and
/// this test would not catch it).
#[test]
fn coverage_names_this_project_as_winner_when_it_loads_before_the_competing_match() {
    let (project, candidates, existing, _default_order) = coverage_fixture();
    let order = LoadOrder::new(vec![
        ModId::new("example.framework"),
        ModId::new("mypatch.parts"),
        ModId::new("addon.elves"),
    ]);
    let rule = PrecedenceRule::PreferOutsideFramework {
        framework: ModId::new("example.framework"),
        key_priority: vec![field("kindNames"), field("speciesNames")],
    };

    let result = coverage(
        &project,
        "example.PartAssignmentDef",
        &candidates,
        &existing,
        &order,
        &rule,
    );
    let elf_row = result
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Elf")
        .expect("Elf row present");

    // Both candidates are owned outside the framework and matched
    // via the same key field — tied on both dimensions — so the
    // load order decides: this project's own package id
    // (`mypatch.parts`, position 1) now loads before the existing
    // `addon.elves` match (position 2).
    assert_eq!(elf_row.winner, Some(Winner::ThisProject));
}

/// A genuine `ThisProject` win through [`coverage`] itself (not just
/// [`PrecedenceRule::winner`] in isolation): a target with no
/// existing match at all, but this project has a row for it — the
/// row is the only candidate, so it wins uncontested.
#[test]
fn coverage_names_this_project_as_the_winner_when_it_has_no_competition() {
    let (mut project, candidates, existing, order) = coverage_fixture();
    let orc = TargetRef {
        key_field: field("speciesNames"),
        def: DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Orc".to_string(),
        },
    };
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(orc),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "mypatch_parts_Orc".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("orc row valid");
    let rule = PrecedenceRule::PreferOutsideFramework {
        framework: ModId::new("example.framework"),
        key_priority: vec![field("kindNames"), field("speciesNames")],
    };

    let result = coverage(
        &project,
        "example.PartAssignmentDef",
        &candidates,
        &existing,
        &order,
        &rule,
    );
    let orc_row = result
        .rows
        .iter()
        .find(|row| row.target.def.def_name == "Orc")
        .expect("Orc row present");

    assert_eq!(orc_row.winner, Some(Winner::ThisProject));
}

// -- AssignmentProject: multi-section ----------------

/// A [`KnownDefs`] fake answering "nothing is active, but `own` names
/// an own instance of `def_type`" — exercises the
/// [`KnownDefs::own_instances`] half of an [`FieldRole::ItemSlot`]
/// check independent of [`KnownDefs::contains`].
struct OwnOnly {
    def_type: &'static str,
    own: BTreeSet<String>,
}
impl KnownDefs for OwnOnly {
    fn contains(&self, _def_type: &str, _name: &str) -> bool {
        false
    }
    fn own_instances(&self, def_type: &str) -> BTreeSet<String> {
        if def_type == self.def_type {
            self.own.clone()
        } else {
            BTreeSet::new()
        }
    }
}

/// `project_with_slot()` plus a second, free-standing
/// `"example.PartDef"` section — the two-section fixture every test
/// below builds on.
fn two_section_project() -> AssignmentProject {
    let mut project = project_with_slot();
    project
        .add_section(standalone_schema())
        .expect("a fresh def type must add cleanly");
    project
}

#[test]
fn add_section_rejects_a_duplicate_def_type() {
    let mut project = two_section_project();
    let result = project.add_section(standalone_schema());
    assert_eq!(
        result,
        Err(SectionError::AlreadyExists("example.PartDef".to_string()))
    );
}

#[test]
fn remove_section_is_idempotent_for_a_missing_def_type() {
    let mut project = two_section_project();
    assert_eq!(project.remove_section("no.such.type", false), Ok(None));
}

#[test]
fn remove_section_drops_an_unreferenced_section() {
    let mut project = two_section_project();
    let removed = project
        .remove_section("example.PartDef", false)
        .expect("no reference, must succeed");
    assert!(removed.is_some());
    assert!(project.section("example.PartDef").is_none());
}

#[test]
fn set_row_accepts_an_own_instance_name_via_known_defs() {
    let mut project = two_section_project();
    project
        .set_row(
            "example.PartDef",
            RowKey::Own("PartA".to_string()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "PartA".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("valid own row");

    let mut values = BTreeMap::new();
    values.insert(
        field("primaryTool"),
        RowValue::Names(vec!["PartA".to_string()]),
    );
    let known = OwnOnly {
        def_type: "example.PartDef",
        own: ["PartA".to_string()].into_iter().collect(),
    };
    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values,
            def_name: "mypatch_parts_Human".to_string(),
            note: None,
        },
        &known,
    );
    assert!(
        result.is_ok(),
        "an own instance of the slot's item type must satisfy it: {result:?}"
    );
}

#[test]
fn remove_section_refuses_when_referenced_by_another_sections_item_slot() {
    let mut project = two_section_project();
    project
        .set_row(
            "example.PartDef",
            RowKey::Own("PartA".to_string()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "PartA".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("valid own row");
    let known = OwnOnly {
        def_type: "example.PartDef",
        own: ["PartA".to_string()].into_iter().collect(),
    };
    let mut values = BTreeMap::new();
    values.insert(
        field("primaryTool"),
        RowValue::Names(vec!["PartA".to_string()]),
    );
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(human_target()),
            AssignmentRow {
                values,
                def_name: "mypatch_parts_Human".to_string(),
                note: None,
            },
            &known,
        )
        .expect("valid referencing row");

    let result = project.remove_section("example.PartDef", false);

    assert_eq!(
        result,
        Err(SectionError::SectionInUse {
            referenced_by: vec![(
                "example.PartAssignmentDef".to_string(),
                RowKey::Target(human_target()),
                field("primaryTool")
            )],
        })
    );
    assert!(
        project.section("example.PartDef").is_some(),
        "a refused removal must leave the section in place"
    );
}

#[test]
fn remove_section_with_force_removes_despite_references() {
    let mut project = two_section_project();
    project
        .set_row(
            "example.PartDef",
            RowKey::Own("PartA".to_string()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "PartA".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("valid own row");
    let known = OwnOnly {
        def_type: "example.PartDef",
        own: ["PartA".to_string()].into_iter().collect(),
    };
    let mut values = BTreeMap::new();
    values.insert(
        field("primaryTool"),
        RowValue::Names(vec!["PartA".to_string()]),
    );
    project
        .set_row(
            "example.PartAssignmentDef",
            RowKey::Target(human_target()),
            AssignmentRow {
                values,
                def_name: "mypatch_parts_Human".to_string(),
                note: None,
            },
            &known,
        )
        .expect("valid referencing row");

    let result = project.remove_section("example.PartDef", true);

    assert!(result.is_ok(), "force must remove despite the reference");
    assert!(project.section("example.PartDef").is_none());
    // The referencing row's own dangling name is left in place — a
    // later export's own concern, never silently scrubbed here.
    let remaining = project
        .section("example.PartAssignmentDef")
        .expect("section present")
        .rows
        .get(&RowKey::Target(human_target()))
        .expect("row still present");
    assert_eq!(
        remaining.values.get(&field("primaryTool")),
        Some(&RowValue::Names(vec!["PartA".to_string()]))
    );
}

#[test]
fn set_row_rejects_a_def_name_colliding_with_another_sections_own_row() {
    let mut project = two_section_project();
    project
        .set_row(
            "example.PartDef",
            RowKey::Own("dup".to_string()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "dup".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("valid own row");

    let result = project.set_row(
        "example.PartAssignmentDef",
        RowKey::Target(human_target()),
        AssignmentRow {
            values: BTreeMap::new(),
            def_name: "dup".to_string(),
            note: None,
        },
        &NoneKnown,
    );

    assert_eq!(
        result,
        Err(AssignmentRowError::DuplicateDefName("dup".to_string()))
    );
}

#[test]
fn content_sha256_covers_every_section() {
    let mut project = project_with_slot();
    let before = project.content_sha256();

    project
        .add_section(standalone_schema())
        .expect("a fresh def type must add cleanly");
    let after_add = project.content_sha256();
    assert_ne!(before, after_add, "adding a section must change the hash");

    project
        .set_row(
            "example.PartDef",
            RowKey::Own("PartA".to_string()),
            AssignmentRow {
                values: BTreeMap::new(),
                def_name: "PartA".to_string(),
                note: None,
            },
            &NoneKnown,
        )
        .expect("valid own row");
    let after_row = project.content_sha256();
    assert_ne!(
        after_add, after_row,
        "a row change in a second section must change the hash too"
    );
}

proptest! {
    /// [`RowKey`] round-trips through `serde_json` unchanged, for both
    /// variants.
    #[test]
    fn row_key_serde_round_trips(name in "[a-zA-Z0-9_]{1,16}", def_name in "[a-zA-Z0-9_]{1,16}") {
        let own = RowKey::Own(name.clone());
        let restored: RowKey = serde_json::from_str(&serde_json::to_string(&own).expect("RowKey serializes"))
        .expect("RowKey deserializes");
        prop_assert_eq!(restored, own);

        let target = RowKey::Target(TargetRef {
            key_field: field("speciesNames"),
            def: DefKey {
                def_type: "ThingDef".to_string(),
                def_name,
            },
        });
        let restored: RowKey = serde_json::from_str(&serde_json::to_string(&target).expect("RowKey serializes"))
        .expect("RowKey deserializes");
        prop_assert_eq!(restored, target);
    }

    /// Every [`RowKey::Target`] sorts before every [`RowKey::Own`]
    /// (declaration order), and two [`RowKey::Own`]s sort by their own
    /// string — the ordering [`std::collections::BTreeMap`] relies on
    /// to keep a section's rows deterministic.
    #[test]
    fn row_key_ordering_is_target_first_then_own_lexical(a in "[a-z]{1,8}", b in "[a-z]{1,8}") {
        let target_key = RowKey::Target(TargetRef {
            key_field: field("speciesNames"),
            def: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: a.clone(),
            },
        });
        let own_key = RowKey::Own(b.clone());
        prop_assert!(target_key < own_key);
        prop_assert_eq!(RowKey::Own(a.clone()).cmp(&RowKey::Own(b.clone())),
            a.cmp(&b)
        );
    }
}

// -- ScalarKind::Display -------------------------------------------

#[test]
fn scalar_kind_display_is_stable_and_bounded() {
    assert_eq!(ScalarKind::Bool.to_string(), "Bool");
    assert_eq!(ScalarKind::Number.to_string(), "Number");
    assert_eq!(ScalarKind::Text.to_string(), "Text");
    assert_eq!(
        ScalarKind::Enum {
            values: BTreeSet::from(["b".to_string(), "a".to_string(), "c".to_string()])
        }
        .to_string(),
        "Enum(a|b|c)",
        "a BTreeSet already sorts its members, so the joined text is deterministic"
    );
}
