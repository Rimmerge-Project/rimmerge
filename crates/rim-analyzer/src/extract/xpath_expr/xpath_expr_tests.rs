//! Tests for the xpath-subset parser.

use super::*;
use crate::domain::Selector;

struct Parsed {
    targets: Vec<DefTarget>,
    root_predicates: Vec<Predicate>,
    steps: Vec<Step>,
    selects_text: bool,
}

fn parsed(xpath: &str) -> Parsed {
    match parse(xpath) {
        XPathExpr::Supported {
            targets,
            root_predicates,
            steps,
            selects_text,
        } => Parsed {
            targets,
            root_predicates,
            steps,
            selects_text,
        },
        XPathExpr::DocumentRoot => {
            panic!("expected '{xpath}' to be supported, got: DocumentRoot")
        }
        XPathExpr::Unsupported { reason } => {
            panic!("expected '{xpath}' to be supported, got: {reason}")
        }
    }
}

/// The single-target shorthand most cases below want.
fn supported(xpath: &str) -> (DefTarget, Vec<Step>) {
    let mut result = parsed(xpath);
    assert_eq!(
        result.targets.len(),
        1,
        "'{xpath}' was expected to name exactly one def"
    );
    assert!(
        result.root_predicates.is_empty(),
        "'{xpath}' was expected to have no root predicates"
    );
    assert!(
        !result.selects_text,
        "'{xpath}' was expected not to select text()"
    );
    (result.targets.remove(0), result.steps)
}

fn unsupported_reason(xpath: &str) -> String {
    match parse(xpath) {
        XPathExpr::Unsupported { reason } => reason,
        XPathExpr::Supported { .. } => panic!("expected '{xpath}' to be unsupported"),
        XPathExpr::DocumentRoot => {
            panic!("expected '{xpath}' to be unsupported, got: DocumentRoot")
        }
    }
}

fn step(name: &str, predicates: Vec<Predicate>) -> Step {
    Step {
        name: name.to_string(),
        predicates,
    }
}

#[test]
fn head_only_has_no_steps() {
    let (target, steps) = supported(r#"Defs/ThingDef[defName="Wall"]"#);
    assert_eq!(target.def_name, "Wall");
    assert!(steps.is_empty());
}

#[test]
fn plain_child_step_has_no_predicates() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="Wall"]/statBases"#);
    assert_eq!(steps, vec![step("statBases", vec![])]);
}

#[test]
fn attribute_equality_predicate() {
    let (_, steps) = supported(
        r#"Defs/HediffDef[defName="X"]/comps/li[@Class="ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust"]"#,
    );
    assert_eq!(
        steps,
        vec![
            step("comps", vec![]),
            step(
                "li",
                vec![Predicate::Attr(
                    "Class".to_string(),
                    "ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust".to_string()
                )]
            ),
        ]
    );
}

#[test]
fn child_text_equality_predicate() {
    let (_, steps) =
        supported(r#"Defs/BiomeDef[defName="X"]/wildPlants/li[plant="Plant_Grass"]/commonality"#);
    assert_eq!(
        steps,
        vec![
            step("wildPlants", vec![]),
            step(
                "li",
                vec![Predicate::ChildText(
                    "plant".to_string(),
                    "Plant_Grass".to_string()
                )]
            ),
            step("commonality", vec![]),
        ]
    );
}

#[test]
fn text_predicate() {
    let (_, steps) =
        supported(r#"Defs/ThingDef[defName="X"]/tradeTags/li[text()="ImplantEmpireCommon"]"#);
    assert_eq!(
        steps,
        vec![
            step("tradeTags", vec![]),
            step(
                "li",
                vec![Predicate::Text("ImplantEmpireCommon".to_string())]
            ),
        ]
    );
}

/// The single-step child-text form (`[thingDef="Column"]`, Example
/// Core's own real op) parses to `Predicate::ChildText`.
#[test]
fn single_step_child_text_predicate_was_already_supported() {
    let (_, steps) = supported(r#"Defs/RoomRequirementDef[defName="X"]/li[thingDef="Column"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::ChildText(
                "thingDef".to_string(),
                "Column".to_string()
            )]
        )]
    );
}

/// A two-step relative path predicate testing a
/// grandchild's text — RimWorld's own list-membership idiom
/// (`things/li="Column"`, Example Doors' real op against Example Core's
/// injected `RoomRequirement_ExampleAnyOfCount`).
#[test]
fn nested_child_text_equality_predicate() {
    let (_, steps) = supported(
        r#"Defs/PreceptDef[defName="X"]/li[@Class="RoomRequirement_ExampleAnyOfCount"][things/li="Column"]/things"#,
    );
    assert_eq!(
        steps,
        vec![
            step(
                "li",
                vec![
                    Predicate::Attr(
                        "Class".to_string(),
                        "RoomRequirement_ExampleAnyOfCount".to_string()
                    ),
                    Predicate::NestedChildText(
                        "things".to_string(),
                        "li".to_string(),
                        "Column".to_string()
                    ),
                ]
            ),
            step("things", vec![]),
        ]
    );
}

/// A relative path deeper than two steps is modeled exactly by
/// `parse_relative_path`, as nested [`Predicate::Child`]s. `NestedChildText`'s
/// own two-segment shape is untouched (its arm runs first) — see
/// [`a_two_step_relative_path_predicate_is_still_nested_child_text`].
#[test]
fn a_three_step_relative_path_predicate_is_nested_child_filters() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[a/b/c="Column"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::Child(
                "a".to_string(),
                Box::new(Predicate::Child(
                    "b".to_string(),
                    Box::new(Predicate::ChildText("c".to_string(), "Column".to_string()))
                ))
            )]
        )]
    );
}

/// The exact two-segment shape `analysis::edges::child_value_targets`
/// and `extract::defs` pattern-match still parses to
/// [`Predicate::NestedChildText`], not to the more general
/// nested-`Child` form (re-shaping a shipped, indexed fact for tidiness
/// is churn, not progress).
#[test]
fn a_two_step_relative_path_predicate_is_still_nested_child_text() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[things/li="Column"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::NestedChildText(
                "things".to_string(),
                "li".to_string(),
                "Column".to_string()
            )]
        )]
    );
}

#[test]
fn not_predicate() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[not(comps)]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::Not(Box::new(Predicate::Has(
                "comps".to_string()
            )))]
        )]
    );
}

#[test]
fn position_predicate_is_one_based() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[2]"#);
    assert_eq!(steps, vec![step("li", vec![Predicate::Position(2)])]);
}

#[test]
fn position_with_leading_zeros_still_parses() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[007]"#);
    assert_eq!(steps, vec![step("li", vec![Predicate::Position(7)])]);
}

#[test]
fn position_zero_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]/li[0]"#);
    assert!(reason.contains('0'), "{reason}");
    assert!(reason.contains("1-based"), "{reason}");
}

/// The explicit `position()=N` form maps onto the same
/// [`Predicate::Position`] variant the bare-integer form does.
#[test]
fn explicit_position_function_forms_are_accepted() {
    let cases = [
        (r#"Defs/ThingDef[defName="X"]/li[position()=1]"#, 1),
        (r#"Defs/ThingDef[defName="X"]/li[position() = 3]"#, 3),
        (r#"Defs/ThingDef[defName="X"]/li[position()= 3]"#, 3),
        (r#"Defs/ThingDef[defName="X"]/li[position() =3]"#, 3),
    ];
    for (xpath, expected) in cases {
        let (_, steps) = supported(xpath);
        assert_eq!(
            steps,
            vec![step("li", vec![Predicate::Position(expected)])],
            "{xpath}"
        );
    }
}

/// `position()` in every other shape stays `Unsupported`, naming the
/// offending token — including `position()=0`, which is an error like
/// `li[0]` rather than parsing to `Predicate::Position(0)`.
#[test]
fn position_function_other_forms_are_unsupported() {
    struct Case {
        xpath: &'static str,
        names: &'static str,
    }
    let cases = [
        Case {
            xpath: r#"Defs/ThingDef[defName="X"]/li[position()=0]"#,
            names: "position()=0",
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="X"]/li[position()>1]"#,
            names: "position()>1",
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="X"]/li[position()!=2]"#,
            names: "position()!=2",
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="X"]/li[position()=last()]"#,
            names: "position()=last()",
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="X"]/li[last()]"#,
            names: "last(",
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="X"]/li[last()-1]"#,
            names: "last(",
        },
    ];
    for case in cases {
        let reason = unsupported_reason(case.xpath);
        assert!(
            reason.contains(case.names),
            "'{}' -> reason '{reason}' does not name '{}'",
            case.xpath,
            case.names
        );
    }
}

#[test]
fn and_predicate() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[@Class="A" and stat="Mass"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::And(
                Box::new(Predicate::Attr("Class".to_string(), "A".to_string())),
                Box::new(Predicate::ChildText("stat".to_string(), "Mass".to_string()))
            )]
        )]
    );
}

#[test]
fn or_predicate() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[@Class="A" or @Class="B"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::Or(
                Box::new(Predicate::Attr("Class".to_string(), "A".to_string())),
                Box::new(Predicate::Attr("Class".to_string(), "B".to_string()))
            )]
        )]
    );
}

/// `and` binds tighter than `or`: `A or B and C` == `Or(A, And(B, C))`,
/// not `And(Or(A, B), C)`.
#[test]
fn and_binds_tighter_than_or_left_leaning() {
    let (_, steps) =
        supported(r#"Defs/ThingDef[defName="X"]/li[@Class="A" or @Class="B" and stat="Mass"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::Or(
                Box::new(Predicate::Attr("Class".to_string(), "A".to_string())),
                Box::new(Predicate::And(
                    Box::new(Predicate::Attr("Class".to_string(), "B".to_string())),
                    Box::new(Predicate::ChildText("stat".to_string(), "Mass".to_string()))
                ))
            )]
        )]
    );
}

/// Same precedence rule, other side: `A and B or C` == `Or(And(A, B), C)`.
#[test]
fn and_binds_tighter_than_or_right_leaning() {
    let (_, steps) =
        supported(r#"Defs/ThingDef[defName="X"]/li[@Class="A" and stat="Mass" or @Class="B"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::Or(
                Box::new(Predicate::And(
                    Box::new(Predicate::Attr("Class".to_string(), "A".to_string())),
                    Box::new(Predicate::ChildText("stat".to_string(), "Mass".to_string()))
                )),
                Box::new(Predicate::Attr("Class".to_string(), "B".to_string()))
            )]
        )]
    );
}

#[test]
fn parenthesized_and_or_composition() {
    let (_, steps) =
        supported(r#"Defs/ThingDef[defName="X"]/li[(@Class="A" or @Class="B") and not(comps)]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::And(
                Box::new(Predicate::Or(
                    Box::new(Predicate::Attr("Class".to_string(), "A".to_string())),
                    Box::new(Predicate::Attr("Class".to_string(), "B".to_string()))
                )),
                Box::new(Predicate::Not(Box::new(Predicate::Has(
                    "comps".to_string()
                ))))
            )]
        )]
    );
}

#[test]
fn multiple_predicates_on_one_step() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[@Class="A"][2]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![
                Predicate::Attr("Class".to_string(), "A".to_string()),
                Predicate::Position(2),
            ]
        )]
    );
}

#[test]
fn single_quoted_values_are_accepted_throughout() {
    let (_, steps) = supported(r"Defs/ThingDef[defName='X']/li[@Class='A']");
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::Attr("Class".to_string(), "A".to_string())]
        )]
    );
}

// --- Multi-def heads -----------------------------------------------

/// Every accepted head shape, and what defs it expands to. `parse`
/// must produce one target per named def, in source order, with the
/// same `sub_path` on each.
#[test]
fn multi_def_heads_expand_to_one_target_per_named_def() {
    struct Case {
        xpath: &'static str,
        expected: &'static [(&'static str, Selector)],
    }
    let cases = [
        Case {
            xpath: r#"Defs/ThingDef[defName="A"]/statBases"#,
            expected: &[("A", Selector::DefName)],
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="A" or defName="B"]/statBases"#,
            expected: &[("A", Selector::DefName), ("B", Selector::DefName)],
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="A" or defName="B" or @Name="C"]"#,
            expected: &[
                ("A", Selector::DefName),
                ("B", Selector::DefName),
                ("C", Selector::NameAttr),
            ],
        },
        Case {
            xpath: r"*/ThingDef[defName='A' or defName='B']",
            expected: &[("A", Selector::DefName), ("B", Selector::DefName)],
        },
        Case {
            xpath: r#"//ThingDef[ defName = "A"  or  defName = 'B' ]"#,
            expected: &[("A", Selector::DefName), ("B", Selector::DefName)],
        },
        Case {
            xpath: r#"Defs/ThingDef[(defName="A" or defName="B") or defName="C"]"#,
            expected: &[
                ("A", Selector::DefName),
                ("B", Selector::DefName),
                ("C", Selector::DefName),
            ],
        },
        Case {
            // Repeats collapse — the same def is only replayed once.
            xpath: r#"Defs/ThingDef[defName="A" or defName="A"]"#,
            expected: &[("A", Selector::DefName)],
        },
    ];

    for case in cases {
        let result = parsed(case.xpath);
        let actual: Vec<(&str, Selector)> = result
            .targets
            .iter()
            .map(|target| (target.def_name.as_str(), target.selector))
            .collect();
        assert_eq!(actual, case.expected.to_vec(), "{}", case.xpath);
        assert!(
            result.targets.iter().all(|t| t.def_type == "ThingDef"),
            "{}",
            case.xpath
        );
    }
}

/// Real head predicates wrap across lines and indent with tabs; the
/// `or` between two names is still an operator there.
#[test]
fn a_multi_def_head_split_across_lines_still_expands() {
    let xpath = "Defs/RecipeDef[defName=\"A\" or defName=\"B\"
		or defName=\"C\"]/recipeUsers";

    let result = parsed(xpath);

    assert_eq!(
        result
            .targets
            .iter()
            .map(|t| t.def_name.as_str())
            .collect::<Vec<_>>(),
        vec!["A", "B", "C"]
    );
}

/// A name that merely *ends* in an operator must not split.
#[test]
fn a_predicate_name_ending_in_an_operator_is_not_split() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[brand="Acme" and nor="Zero"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::And(
                Box::new(Predicate::ChildText(
                    "brand".to_string(),
                    "Acme".to_string()
                )),
                Box::new(Predicate::ChildText("nor".to_string(), "Zero".to_string()))
            )]
        )]
    );
}

#[test]
fn a_multi_def_head_keeps_the_same_sub_path_on_every_target() {
    let result = parsed(r#"Defs/ThingDef[defName="A" or defName="B"]/statBases/li[2]"#);
    assert!(
        result
            .targets
            .iter()
            .all(|t| t.sub_path.as_deref() == Some("statBases/li[2]"))
    );
    assert_eq!(
        result.steps,
        vec![
            step("statBases", vec![]),
            step("li", vec![Predicate::Position(2)]),
        ]
    );
}

// --- Root predicates -----------------------------------------------

#[test]
fn root_predicates_after_the_head_are_captured_separately() {
    struct Case {
        xpath: &'static str,
        expected: Vec<Predicate>,
    }
    let cases = [
        Case {
            xpath: r#"Defs/ThingDef[defName="X"][not(comps)]"#,
            expected: vec![Predicate::Not(Box::new(Predicate::Has(
                "comps".to_string(),
            )))],
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="X"][not(tags)]/comps"#,
            expected: vec![Predicate::Not(Box::new(Predicate::Has("tags".to_string())))],
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="X"][comps]/comps/li"#,
            expected: vec![Predicate::Has("comps".to_string())],
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="X"][@Class="Foo"]"#,
            expected: vec![Predicate::Attr("Class".to_string(), "Foo".to_string())],
        },
        Case {
            xpath: r#"Defs/ThingDef[defName="X"][not(comps)][@Class="Foo"]"#,
            expected: vec![
                Predicate::Not(Box::new(Predicate::Has("comps".to_string()))),
                Predicate::Attr("Class".to_string(), "Foo".to_string()),
            ],
        },
    ];

    for case in cases {
        let result = parsed(case.xpath);
        assert_eq!(result.root_predicates, case.expected, "{}", case.xpath);
    }
}

/// A root predicate is not part of the step path — the steps must
/// start at whatever follows the last bracket.
#[test]
fn root_predicates_do_not_become_steps() {
    let result = parsed(r#"Defs/ThingDef[defName="X"][not(comps)]/statBases/x"#);
    assert_eq!(
        result.steps,
        vec![step("statBases", vec![]), step("x", vec![])]
    );
    assert_eq!(
        result.targets[0].sub_path.as_deref(),
        Some("statBases/x"),
        "the root predicate is excluded from the collision sub_path too"
    );
}

#[test]
fn a_child_existence_predicate_is_supported_on_a_step_too() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/comps/li[props]"#);
    assert_eq!(
        steps,
        vec![
            step("comps", vec![]),
            step("li", vec![Predicate::Has("props".to_string())]),
        ]
    );
}

// --- text() ---------------------------------------------------------

#[test]
fn a_trailing_text_step_selects_the_elements_text() {
    let result = parsed(r#"Defs/ThingDef[defName="X"]/graphicData/texPath/text()"#);
    assert!(result.selects_text);
    assert_eq!(
        result.steps,
        vec![step("graphicData", vec![]), step("texPath", vec![])]
    );
}

#[test]
fn text_before_the_final_step_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]/text()/x"#);
    assert!(reason.contains("text()"), "{reason}");
}

// --- Quoted literals must never trip structural token checks -------

#[test]
fn a_literal_containing_the_word_and_is_supported() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[label="Fish and Chips"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::ChildText(
                "label".to_string(),
                "Fish and Chips".to_string()
            )]
        )]
    );
}

#[test]
fn a_literal_containing_a_pipe_is_supported() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[label="a|b"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::ChildText("label".to_string(), "a|b".to_string())]
        )]
    );
}

#[test]
fn a_literal_containing_a_double_slash_is_supported() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[label="http://x"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::ChildText(
                "label".to_string(),
                "http://x".to_string()
            )]
        )]
    );
}

#[test]
fn a_literal_containing_a_double_dot_is_supported() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[@Class="A..B"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::Attr("Class".to_string(), "A..B".to_string())]
        )]
    );
}

// --- One test per real skip shape (see `REAL_SKIP_SHAPES`) ------------
//
// The exact `Predicate` tree is asserted for each, not merely "it
// parses": a widened grammar that builds the *wrong* tree is the
// failure mode these tests exist to catch.

/// S1, 734 rows: the largest skip shape on the install, and the one
/// `parse` can never answer (only a scan-wide index knows which defs
/// carry a given `ParentName`). `head_filter_predicate` reports the
/// query instead, for a replay to evaluate against one def's tree.
#[test]
fn a_bare_parent_name_head_is_a_filter_query_with_its_steps() {
    let query = head_filter_predicate(r#"Defs/HediffDef[@ParentName="EBS_DefaultRaceTracker"]/modExtensions/li[@Class="ExampleBodySizes.PawnExtension"]/apparelRestrictions"#)
        .expect("a @ParentName head is a filter query");
    assert_eq!(query.def_type, "HediffDef");
    assert_eq!(
        query.predicate,
        Some(Predicate::Attr(
            "ParentName".to_string(),
            "EBS_DefaultRaceTracker".to_string()
        ))
    );
    assert_eq!(
        query.steps,
        vec![
            step("modExtensions", vec![]),
            step(
                "li",
                vec![Predicate::Attr(
                    "Class".to_string(),
                    "ExampleBodySizes.PawnExtension".to_string()
                )]
            ),
            step("apparelRestrictions", vec![]),
        ]
    );
    assert!(!query.selects_text);
}

/// S2, 142 rows: a child-value head `and`-composed with a negation —
/// too rich for `collect_head_names` *and* for `head_content_predicate`
/// (which is `or`-only), so this is the shape that motivated a third,
/// general head resolver rather than widening either existing one.
#[test]
fn an_and_composed_content_head_with_a_negation_is_a_filter_query() {
    let query =
        head_filter_predicate(r#"Defs/ThingDef[inspectorTabs/li="ITab_Bills" and not(comps)]"#)
            .expect("an and-composed head is a filter query");
    assert_eq!(
        query.predicate,
        Some(Predicate::And(
            Box::new(Predicate::NestedChildText(
                "inspectorTabs".to_string(),
                "li".to_string(),
                "ITab_Bills".to_string()
            )),
            Box::new(Predicate::Not(Box::new(Predicate::Has(
                "comps".to_string()
            ))))
        ))
    );
    assert!(query.steps.is_empty());
}

/// S3, 102 rows: a **bare-type** head — no `[...]` at all, so
/// `xpath_target::locate_head` cannot find it and `parse` reports "no
/// recognized head". Also the whitespace case: the step name is
/// written `mechEnabledWorkTypes [li [...] ]`, spaces and all.
#[test]
fn a_bare_type_head_with_author_whitespace_is_a_filter_query_with_no_predicate() {
    let query = head_filter_predicate(
        r#"/Defs/ThingDef/race/mechEnabledWorkTypes [li [text()="Hauling"] ]"#,
    )
    .expect("a bare-type head is a filter query");
    assert_eq!(query.def_type, "ThingDef");
    assert_eq!(query.predicate, None);
    assert_eq!(
        query.steps,
        vec![
            step("race", vec![]),
            step(
                "mechEnabledWorkTypes",
                vec![Predicate::Child(
                    "li".to_string(),
                    Box::new(Predicate::Text("Hauling".to_string()))
                )]
            ),
        ]
    );
}

/// S4, 88 rows: the one `contains` form this grammar admits, inside a
/// child filter, in head position.
#[test]
fn a_contains_text_child_filter_head_is_a_filter_query() {
    let query = head_filter_predicate(r#"/Defs/ThingDef [thingClass [contains(text(), "Storage")] ]/building/defaultStorageSettings"#)
        .expect("a contains(text(), ..) head is a filter query");
    assert_eq!(
        query.predicate,
        Some(Predicate::Child(
            "thingClass".to_string(),
            Box::new(Predicate::Contains("Storage".to_string()))
        ))
    );
    assert_eq!(
        query.steps,
        vec![
            step("building", vec![]),
            step("defaultStorageSettings", vec![]),
        ]
    );
}

/// Every *other* `contains` form stays `Unsupported` — the narrowness
/// is the point, so this is the guard that keeps a later,
/// casual widening honest.
#[test]
fn contains_on_anything_but_text_stays_unsupported() {
    for xpath in [
        r#"Defs/ThingDef[defName="W"]/li[contains(@Class, "Comp")]"#,
        r#"Defs/ThingDef[defName="W"]/li[contains(label, "Comp")]"#,
        r#"Defs/ThingDef[defName="W"]/li[starts-with(text(), "Comp")]"#,
    ] {
        let reason = unsupported_reason(xpath);
        assert!(
            reason.contains("unsupported function") || reason.contains("unrecognized predicate"),
            "{xpath}: {reason}"
        );
    }
}

/// S5, 174 rows: the family that looks like "whitespace around `=`"
/// but is nothing of the sort — the real defect is a *missing* space
/// before `or`. Unlike every other shape here this one is answered by
/// the strict grammar itself, so it yields real [`DefTarget`]s.
#[test]
fn an_or_list_head_with_no_space_before_or_names_every_def() {
    let names: Vec<String> =
        parsed(r#"Defs/HediffDef[defName="A" or defName="B"or defName = "C"or defName="D"]"#)
            .targets
            .into_iter()
            .map(|target| target.def_name)
            .collect();
    assert_eq!(names, vec!["A", "B", "C", "D"]);
}

/// S6, 32 rows: `@ParentName` `and`-composed with a bare *relative
/// path* existence test, which XPath reads as "has a `costList` child
/// that has a `ComponentIndustrial` child".
#[test]
fn a_parent_name_head_and_composed_with_a_relative_path_test_is_a_filter_query() {
    let query = head_filter_predicate(
        r#"Defs/ThingDef[@ParentName="BodyPartProstheticBase" and costList/ComponentIndustrial]"#,
    )
    .expect("an and-composed @ParentName head is a filter query");
    assert_eq!(
        query.predicate,
        Some(Predicate::And(
            Box::new(Predicate::Attr(
                "ParentName".to_string(),
                "BodyPartProstheticBase".to_string()
            )),
            Box::new(Predicate::Child(
                "costList".to_string(),
                Box::new(Predicate::Has("ComponentIndustrial".to_string()))
            ))
        ))
    );
}

/// A `./`-prefixed relative path whose middle segment carries its own filter
/// and whose last carries a value test — a real device-standby mod's head,
/// with over a hundred real rows behind it.
#[test]
fn a_self_axis_relative_path_with_a_mid_segment_filter_is_modelled_exactly() {
    let query = head_filter_predicate(r#"/Defs/ThingDef[@ParentName="BenchBase" and ./comps/li[@Class="CompProperties_Power"]/compClass="CompPowerTrader"]"#)
        .expect("a self-axis relative path is within the grammar");
    assert_eq!(
        query.predicate,
        Some(Predicate::And(
            Box::new(Predicate::Attr(
                "ParentName".to_string(),
                "BenchBase".to_string()
            )),
            Box::new(Predicate::Child(
                "comps".to_string(),
                Box::new(Predicate::Child(
                    "li".to_string(),
                    Box::new(Predicate::And(
                        Box::new(Predicate::Attr(
                            "Class".to_string(),
                            "CompProperties_Power".to_string()
                        )),
                        Box::new(Predicate::ChildText(
                            "compClass".to_string(),
                            "CompPowerTrader".to_string()
                        ))
                    ))
                ))
            ))
        ))
    );
}

/// A relative path is still only ever an *equality* against a literal:
/// an inequality anywhere in it refuses, rather than being read as a
/// plain `=`.
#[test]
fn a_relative_path_with_an_inequality_stays_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="W"]/li[a/b/c!="x"]"#);
    assert!(reason.contains("unrecognized predicate"), "{reason}");
}

/// S7, 31 rows: `not(...)` over a child filter, in a *step* predicate,
/// which the strict grammar parses outright. This is the operation
/// standing between `example.progression.kitchen` and
/// `example.progression.production` on a real install.
#[test]
fn a_step_predicate_negating_a_child_filter_parses_to_not_of_child() {
    let (_, steps) = supported(
        r#"Defs/ThingDef[defName="EX_ChemfuelStoveLarge"]/comps[not(li[@Class="CompProperties_AffectedByFacilities"])]"#,
    );
    assert_eq!(
        steps,
        vec![step(
            "comps",
            vec![Predicate::Not(Box::new(Predicate::Child(
                "li".to_string(),
                Box::new(Predicate::Attr(
                    "Class".to_string(),
                    "CompProperties_AffectedByFacilities".to_string()
                ))
            )))]
        )]
    );
}

/// S8, 23 rows: nested child filters, two deep, in head position.
#[test]
fn a_head_with_a_nested_child_filter_is_a_filter_query() {
    let query = head_filter_predicate(r#"/Defs/ThingDef[comps and comps[li[@Class="ExampleFurniture.CompProperties_Mountable"]]]"#)
        .expect("a nested child-filter head is a filter query");
    assert_eq!(
        query.predicate,
        Some(Predicate::And(
            Box::new(Predicate::Has("comps".to_string())),
            Box::new(Predicate::Child(
                "comps".to_string(),
                Box::new(Predicate::Child(
                    "li".to_string(),
                    Box::new(Predicate::Attr(
                        "Class".to_string(),
                        "ExampleFurniture.CompProperties_Mountable".to_string()
                    ))
                ))
            ))
        ))
    );
}

/// S9: a mod author's malformed predicate. `not(petness)="petness"` is
/// not XPath at all, and guessing at what its author meant is the one
/// thing this replay must never do.
#[test]
fn the_malformed_not_equality_predicate_is_never_parsed_by_either_entry_point() {
    let xpath = r#"Defs/ThingDef[defName="Cow"]/race[not(petness)="petness"]"#;
    let reason = unsupported_reason(xpath);
    assert!(reason.contains(r#"not(petness)="petness""#), "{reason}");
    assert_eq!(head_filter_predicate(xpath), None);
}

/// `not(comps)` keeps its exact child-absent meaning even though the argument
/// can be a whole predicate, not only a bare name.
#[test]
fn not_of_a_bare_name_still_means_child_absent() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="W"]/li[not(comps)]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::Not(Box::new(Predicate::Has(
                "comps".to_string()
            )))]
        )]
    );
}

/// A `li[2]` nested *inside* a child filter keeps XPath's own 1-based
/// "second `li`" meaning — the position restarts among the same-tag
/// siblings, which is exactly what `rim_merge::patch_eval`'s own
/// `Predicate::Child` evaluation implements.
#[test]
fn a_position_inside_a_child_filter_parses_as_a_position_predicate() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="W"]/comps[li[2]]"#);
    assert_eq!(
        steps,
        vec![step(
            "comps",
            vec![Predicate::Child(
                "li".to_string(),
                Box::new(Predicate::Position(2))
            )]
        )]
    );
}

/// A head this module *can* enumerate is still `head_targets`' job:
/// `head_filter_predicate` reports it too (it is strictly more
/// general), but `resolve_xpath`'s chain order means the strict parse
/// always wins — the guard here is that the strict parse keeps
/// succeeding, which is what makes that order meaningful.
#[test]
fn an_ordinary_def_name_head_still_parses_strictly() {
    let (target, _) = supported(r#"Defs/ThingDef[defName="W"]/comps"#);
    assert_eq!(target.def_name, "W");
}

/// The document root is never a bare-type head, however tempting the shape
/// looks: `/Defs` has its own [`XPathExpr::DocumentRoot`] meaning that a
/// filter query must not shadow.
#[test]
fn the_document_root_is_not_a_bare_type_head() {
    assert_eq!(head_filter_predicate("/Defs"), None);
    assert_eq!(head_filter_predicate("Defs"), None);
}

// --- Malformed quoting ----------------------------------------------

/// `split_boolean` tracks brackets as well as parens and quotes, like
/// both its siblings ([`split_top_level`], [`strip_matching_parens`]) —
/// otherwise an `or` *inside* a child filter would split the enclosing
/// predicate at the wrong place. With brackets counted the split
/// happens where XPath says it does and the filter's own `or` is parsed
/// by the call that owns it.
#[test]
fn an_or_inside_a_child_filter_does_not_split_the_enclosing_predicate() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="W"]/comps[li[a="1" or b="2"] and c]"#);
    assert_eq!(
        steps,
        vec![step(
            "comps",
            // The `and` is the only top-level operator here; without
            // bracket tracking the `or` inside `li[...]` split first
            // and left two halves that each fail to parse.
            vec![Predicate::And(
                Box::new(Predicate::Child(
                    "li".to_string(),
                    Box::new(Predicate::Or(
                        Box::new(Predicate::ChildText("a".to_string(), "1".to_string())),
                        Box::new(Predicate::ChildText("b".to_string(), "2".to_string()))
                    ))
                )),
                Box::new(Predicate::Has("c".to_string()))
            )]
        )]
    );
}

/// A run-on `defName = "A"or defName = "B"` parses: XPath 1.0 allows
/// it (a string literal is self-delimiting, so the following `or` is
/// unambiguously an operator), and real installs carry it.
/// [`split_boolean`] splits there, *before* [`strip_quotes`] ever sees
/// the run-on, so the value is read correctly rather than as a garbage
/// "first quote to last quote" `ChildText` value.
#[test]
fn an_operator_directly_after_a_closing_quote_splits_instead_of_misparsing() {
    let (_, steps) = supported(r#"Defs/ThingDef[defName="X"]/li[a="x"and b="y"]"#);
    assert_eq!(
        steps,
        vec![step(
            "li",
            vec![Predicate::And(
                Box::new(Predicate::ChildText("a".to_string(), "x".to_string())),
                Box::new(Predicate::ChildText("b".to_string(), "y".to_string()))
            )]
        )]
    );
}

/// The guard [`strip_quotes`] exists for is still standing: a run-on
/// with **no operator at all** between the two equalities has nothing
/// for [`split_boolean`] to split on, and must stay `Unsupported`
/// rather than reading `x"b="y` as a value.
#[test]
fn a_run_on_with_no_operator_at_all_is_still_rejected_not_misparsed() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]/li[a="x"b="y"]"#);
    assert!(reason.contains(r#"a="x"b="y""#), "{reason}");
}

// --- Depth/term cap ---------------------------------------------------

/// The deepest real head on this project's own 1002-mod install — a
/// 103-term `or` chain — parses, and one term past the cap still
/// fails cleanly. Pinned as a pair so a future reduction of
/// [`MAX_PREDICATE_DEPTH`] cannot silently break the real
/// ship-building mod family that needs it.
#[test]
fn a_real_hundred_term_or_chain_parses_and_the_cap_still_bites_beyond_it() {
    let head = |terms: usize| {
        let names = (0..terms)
            .map(|index| format!(r#"defName = "D{index}""#))
            .collect::<Vec<_>>()
            .join(" or ");
        format!("Defs/ThingDef[{names}]")
    };

    let parsed = parsed(&head(103));
    assert_eq!(parsed.targets.len(), 103);

    let reason = unsupported_reason(&head(usize::try_from(MAX_PREDICATE_DEPTH).unwrap() + 8));
    assert!(reason.contains("too deep"), "{reason}");
}

/// Every pathological shape must fail cleanly, not overflow the
/// stack — **and the proof runs on a 1 MiB stack**, which is the
/// pessimistic configuration [`MAX_PREDICATE_DEPTH`] and its
/// per-construct costs were calibrated against. Asserting the
/// arithmetic instead would not catch a miscalibrated cost, and every
/// shape is covered, not only the `and` chain: that is the *cheapest*
/// shape per level, so on its own it would pass while nested brackets
/// overflow well below the cap.
///
/// Each input is far past what the budget admits, so the expected
/// outcome is a clean `Unsupported` naming the depth — never a
/// process abort, and never a silent partial parse. The tree each
/// successful prefix built is dropped on this same small stack too,
/// since `Drop` for a `Box<Predicate>` chain recurses just like the
/// parser does.
#[test]
fn a_pathological_predicate_of_every_shape_fails_cleanly_on_a_small_stack() {
    fn nested(wrap: impl Fn(&str) -> String, levels: usize) -> String {
        let mut inner = "leaf".to_string();
        for _ in 0..levels {
            inner = wrap(&inner);
        }
        inner
    }

    let cases: Vec<(&str, String)> = vec![
        (
            "and chain",
            (0..10_000)
                .map(|i| format!(r#"a{i}="v""#))
                .collect::<Vec<_>>()
                .join(" and "),
        ),
        (
            "or chain",
            (0..10_000)
                .map(|i| format!(r#"a{i}="v""#))
                .collect::<Vec<_>>()
                .join(" or "),
        ),
        (
            "nested parens",
            nested(|inner| format!("({inner})"), 10_000),
        ),
        (
            "nested not()",
            nested(|inner| format!("not({inner})"), 2_000),
        ),
        (
            "nested child filters",
            nested(|inner| format!("a[{inner}]"), 2_000),
        ),
        (
            "long relative path",
            (0..20_000).map(|_| "a").collect::<Vec<_>>().join("/"),
        ),
    ];

    std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || {
            for (label, predicate) in cases {
                let xpath = format!(r#"Defs/ThingDef[defName="X"]/li[{predicate}]"#);
                let reason = match parse(&xpath) {
                    XPathExpr::Unsupported { reason } => reason,
                    other => panic!("{label}: expected Unsupported, got {other:?}"),
                };
                assert!(reason.contains("too deep"), "{label}: {reason}");
            }
        })
        .expect("spawning the probe thread")
        .join()
        .expect("a pathological predicate overflowed a 1 MiB stack");
}

// --- Document root -------------------------------------------------

/// The bare document root — no def name at all — is its own outcome,
/// distinct from "no recognized head": callers (`rim-merge::patch_eval`)
/// need to tell "this op adds a whole new top-level def" apart from
/// "this op's head genuinely doesn't parse".
#[test]
fn bare_document_root_forms_are_recognized() {
    for xpath in [
        "/Defs",
        "Defs",
        "/Defs/",
        "  /Defs  ",
        "\tDefs\n",
        " /Defs/ ",
    ] {
        assert_eq!(parse(xpath), XPathExpr::DocumentRoot, "{xpath:?}");
    }
}

/// A def-typed head still wins over the document-root check even
/// though it also starts with `Defs`.
#[test]
fn a_real_head_is_not_mistaken_for_the_document_root() {
    assert_ne!(
        parse(r#"Defs/ThingDef[defName="X"]"#),
        XPathExpr::DocumentRoot
    );
}

/// Anything merely *containing* `Defs` — a sub-path, a double slash —
/// is not the bare root and must fall through to the ordinary
/// unrecognized-head handling.
#[test]
fn document_root_with_a_sub_path_is_not_the_document_root_case() {
    let reason = unsupported_reason("Defs/ThingDef");
    assert!(reason.contains("Defs/ThingDef"), "{reason}");
    assert!(!matches!(parse("//Defs"), XPathExpr::DocumentRoot));
}

// --- Unsupported shapes -------------------------------------------

#[test]
fn unrecognized_head_is_unsupported() {
    let reason = unsupported_reason("Defs/ThingDef/statBases");
    assert!(reason.contains("Defs/ThingDef/statBases"), "{reason}");
}

/// A head predicate naming one def but with an extra `and`-joined
/// term must reject, not silently drop the extra term.
#[test]
fn head_predicate_with_an_extra_and_term_is_unsupported() {
    let xpath = r#"Defs/ThingDef[defName="X" and label="Y"]/comps"#;
    let reason = unsupported_reason(xpath);
    assert!(reason.contains("label=\"Y\""), "{reason}");
}

#[test]
fn head_predicate_with_an_extra_not_term_is_unsupported() {
    let xpath = r#"*/ThingDef[defName="X" and not(comps)]"#;
    let reason = unsupported_reason(xpath);
    assert!(reason.contains("not(comps)"), "{reason}");
}

/// The same shape, with the disjunction parenthesized: `and` in the
/// head is unsupported however it's nested (an `and` term could
/// narrow which defs match, and this module has no way to evaluate it
/// against a def it hasn't been handed).
#[test]
fn parenthesized_head_predicate_with_an_and_term_is_unsupported() {
    let xpath = r#"*/ThingDef[(defName = "X" or defName = "Y") and not(comps)]"#;
    let reason = unsupported_reason(xpath);
    assert!(reason.contains("defName"), "{reason}");
}

#[test]
fn head_predicate_naming_something_other_than_a_def_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[label="X"]/comps"#);
    assert!(reason.contains("label=\"X\""), "{reason}");
}

#[test]
fn double_dot_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]/../ThingDef"#);
    assert!(reason.contains(".."), "{reason}");
}

#[test]
fn double_slash_after_head_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]//comps"#);
    assert!(reason.contains("//"), "{reason}");
}

#[test]
fn wildcard_step_after_head_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]/*"#);
    assert!(reason.contains('*'), "{reason}");
}

#[test]
fn union_operator_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]/comps|statBases"#);
    assert!(reason.contains('|'), "{reason}");
}

#[test]
fn contains_function_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]/li[contains(foo,"bar")]"#);
    assert!(reason.contains("contains("), "{reason}");
}

#[test]
fn starts_with_function_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]/li[starts-with(foo,"bar")]"#);
    assert!(reason.contains("starts-with("), "{reason}");
}

#[test]
fn count_function_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]/li[count(foo)]"#);
    assert!(reason.contains("count("), "{reason}");
}

#[test]
fn attribute_inequality_is_unsupported() {
    let reason = unsupported_reason(r#"Defs/ThingDef[defName="X"]/li[@Class!="Y"]"#);
    assert!(reason.contains("@Class!=\"Y\""), "{reason}");
}

// -- head_content_predicate -------------------------------------------

/// A real install's own badge-fork shape: a two-step relative path in
/// head position.
#[test]
fn head_content_predicate_reads_a_nested_child_text_head() {
    let query = head_content_predicate(
        r#"Defs/ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]/comps"#,
    )
    .unwrap();
    assert_eq!(query.def_type, "ExampleRace.ThingDef_ExampleRace");
    assert_eq!(
        query.alternatives,
        vec![(
            vec!["race".to_string(), "intelligence".to_string()],
            "Humanlike".to_string()
        )]
    );
    assert_eq!(query.sub_path.as_deref(), Some("comps"));
}

/// The single-step form is recognized too, and with no sub-path when
/// there's nothing after the head.
#[test]
fn head_content_predicate_reads_a_single_step_child_text_head() {
    let query = head_content_predicate(r#"Defs/RoomRequirementDef[thingDef="Column"]"#).unwrap();
    assert_eq!(query.def_type, "RoomRequirementDef");
    assert_eq!(
        query.alternatives,
        vec![(vec!["thingDef".to_string()], "Column".to_string())]
    );
    assert_eq!(query.sub_path, None);
}

/// `or`-combined content leaves all resolve, in source order — the
/// same disjunction shape [`collect_head_names`] supports for
/// `defName`/`@Name` heads.
#[test]
fn head_content_predicate_reads_an_or_combined_head() {
    let query = head_content_predicate(
        r#"Defs/ThingDef[race/intelligence="Humanlike" or race/intelligence="ToolUser"]"#,
    )
    .unwrap();
    assert_eq!(
        query.alternatives,
        vec![
            (
                vec!["race".to_string(), "intelligence".to_string()],
                "Humanlike".to_string()
            ),
            (
                vec!["race".to_string(), "intelligence".to_string()],
                "ToolUser".to_string()
            ),
        ]
    );
}

/// An ordinary `defName=`/`@Name=` head is never mistaken for a
/// content query — that shape belongs to [`head_targets`] alone.
#[test]
fn head_content_predicate_is_none_for_an_ordinary_name_head() {
    assert!(head_content_predicate(r#"Defs/ThingDef[defName="Wall"]"#).is_none());
    assert!(head_content_predicate(r#"Defs/ThingDef[@Name="WallBase"]"#).is_none());
}

/// A head mixing a content leaf with a `defName`/`and`/anything else
/// stays unresolvable here too — this grammar's own "equality against
/// a literal only, nothing richer" scope applies in head position
/// exactly as it does everywhere else.
#[test]
fn head_content_predicate_is_none_for_a_mixed_or_richer_head() {
    assert!(
        head_content_predicate(r#"Defs/ThingDef[defName="X" or race/intelligence="Humanlike"]"#)
            .is_none()
    );
    assert!(
        head_content_predicate(r#"Defs/ThingDef[race/intelligence="Humanlike" and label="Y"]"#)
            .is_none()
    );
    assert!(head_content_predicate(r#"Defs/ThingDef[not(comps)]"#).is_none());
}

/// A three-step relative path stays unresolvable *here* even though
/// the grammar parses it: `collect_head_content_predicates` accepts
/// only `ChildText`/`NestedChildText` leaves, and a deeper path is a
/// [`Predicate::Child`]. The indexed head-content shape is therefore
/// unchanged by the wider grammar, which this pins.
#[test]
fn head_content_predicate_is_none_for_a_three_step_relative_path() {
    assert!(head_content_predicate(r#"Defs/ThingDef[a/b/c="X"]"#).is_none());
}

/// The query carries the same parsed `steps`/`selects_text` a
/// name-identifying head's [`XPathExpr::Supported`] would — needed
/// for replay (`rim_merge::patch_eval`), not just indexing.
#[test]
fn head_content_predicate_parses_steps_after_the_head_too() {
    let query = head_content_predicate(
        r#"Defs/ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]/comps/li[1]"#,
    )
    .unwrap();
    assert_eq!(
        query.steps,
        vec![
            step("comps", vec![]),
            step("li", vec![Predicate::Position(1)]),
        ]
    );
    assert!(!query.selects_text);
    assert_eq!(query.sub_path.as_deref(), Some("comps/li[1]"));
}

/// An extra root predicate on the def node itself, alongside the
/// content predicate, is parsed and excluded from `sub_path` — the
/// exact same convention [`root_predicates_do_not_become_steps`]
/// pins for the name-identifying head case.
#[test]
fn head_content_predicate_parses_extra_root_predicates() {
    let query = head_content_predicate(
        r#"Defs/ThingDef[race/intelligence="Humanlike"][not(comps)]/statBases"#,
    )
    .unwrap();
    assert_eq!(
        query.root_predicates,
        vec![Predicate::Not(Box::new(Predicate::Has(
            "comps".to_string()
        )))]
    );
    assert_eq!(query.steps, vec![step("statBases", vec![])]);
    assert_eq!(query.sub_path.as_deref(), Some("statBases"));
}

/// [`HeadContentQuery::as_predicate`] folds the alternatives into the
/// identical `Predicate` tree the ordinary root/step grammar would
/// build for the same text — proven by parsing the equivalent
/// `[...]` step predicate through the ordinary grammar and comparing.
#[test]
fn as_predicate_matches_the_ordinary_grammars_own_parse() {
    let query = head_content_predicate(
        r#"Defs/ThingDef[race/intelligence="Humanlike" or thingDef="Column"]"#,
    )
    .unwrap();

    let (_, steps) = supported(
        r#"Defs/ThingDef[defName="X"]/li[race/intelligence="Humanlike" or thingDef="Column"]"#,
    );
    let expected = steps[0].predicates[0].clone();

    assert_eq!(query.as_predicate(), Some(expected));
}

// -- selected_classes ----------------------------------------------

#[test]
fn selected_classes_reads_a_namespace_qualified_class_attribute() {
    let classes = selected_classes(
        r#"Defs/ThingDef[defName="X"]/modExtensions/li[@Class="Example.Weapons.HeavyWeapon"]"#,
    );
    assert_eq!(
        classes,
        BTreeSet::from(["Example.Weapons.HeavyWeapon".to_string()])
    );
}

#[test]
fn selected_classes_reads_a_namespace_qualified_name_predicate() {
    let classes =
        selected_classes(r#"Defs/ThingDef[defName="X"]/li[name="Some.Namespace.Worker"]"#);
    assert_eq!(
        classes,
        BTreeSet::from(["Some.Namespace.Worker".to_string()])
    );
}

/// A `Class`/`name` predicate value with no `.` is not namespace
/// qualified and never counts as a selected class.
#[test]
fn selected_classes_ignores_a_value_with_no_dot() {
    let classes = selected_classes(r#"Defs/ThingDef[defName="X"]/li[@Class="Plain"]"#);
    assert!(classes.is_empty());
}

#[test]
fn selected_classes_reads_both_sides_of_an_or_predicate() {
    let classes =
        selected_classes(r#"Defs/ThingDef[defName="X"]/li[@Class="A.B" or @Class="C.D"]"#);
    assert_eq!(
        classes,
        BTreeSet::from(["A.B".to_string(), "C.D".to_string()])
    );
}

/// An xpath outside the replayable grammar contributes no evidence —
/// never a guessed selected class.
#[test]
fn selected_classes_is_empty_for_an_unsupported_xpath() {
    let classes = selected_classes(r#"Defs/ThingDef[defName="X"]/li[contains(@Class,"A.B")]"#);
    assert!(classes.is_empty());
}

#[test]
fn selected_classes_is_empty_with_no_matching_predicate() {
    let classes = selected_classes(r#"Defs/ThingDef[defName="X"]/statBases"#);
    assert!(classes.is_empty());
}

// -- is_slow_shape -----------------------------------------------------

#[test]
fn is_slow_shape_detects_descendant_or_self() {
    assert!(is_slow_shape(r#"Defs//ThingDef[defName="Wall"]"#));
}

#[test]
fn is_slow_shape_detects_contains() {
    assert!(is_slow_shape(
        r#"Defs/ThingDef[defName="X"]/li[contains(@Class,"A.B")]"#
    ));
}

#[test]
fn is_slow_shape_detects_starts_with() {
    assert!(is_slow_shape(
        r#"Defs/ThingDef[defName="X"]/li[starts-with(@Class,"A")]"#
    ));
}

#[test]
fn is_slow_shape_detects_count() {
    assert!(is_slow_shape(
        r#"Defs/ThingDef[defName="X"][count(comps)=2]"#
    ));
}

#[test]
fn is_slow_shape_detects_last() {
    assert!(is_slow_shape(
        r#"Defs/ThingDef[defName="X"]/comps/li[position()=last()]"#
    ));
}

#[test]
fn is_slow_shape_detects_descendant_axis() {
    assert!(is_slow_shape(
        r#"Defs/ThingDef[defName="X"]/descendant::li"#
    ));
}

#[test]
fn is_slow_shape_detects_a_wildcard_step() {
    assert!(is_slow_shape(r#"Defs/ThingDef[defName="X"]/comps/*"#));
}

#[test]
fn is_slow_shape_detects_a_leading_wildcard_step() {
    assert!(is_slow_shape(r#"*/ThingDef[defName="X"]"#));
}

/// An ordinary xpath the strict grammar fully supports carries none of
/// the slow shapes.
#[test]
fn is_slow_shape_is_false_for_an_ordinary_supported_xpath() {
    assert!(!is_slow_shape(
        r#"Defs/ThingDef[defName="Wall"]/comps/li[1]"#
    ));
}

/// A quoted literal that merely contains slow-shape *text* (not a real
/// structural token) must not false-positive.
#[test]
fn is_slow_shape_ignores_a_quoted_literal_containing_slow_looking_text() {
    assert!(!is_slow_shape(
        r#"Defs/ThingDef[defName="X"]/label[text()="a//b contains(y) *"]"#
    ));
}

/// The nine real, verbatim xpath texts the skip-shape probe runs over —
/// one operation per skip shape, copied out of the `skipped` reasons of
/// a real `verify --source suggested --json` run against a 1002-mod
/// install
/// (the reasons quote the xpath `{:?}`-escaped, so these are the exact
/// bytes `parse` saw there). Nothing here is hand-simplified: the point
/// of the probe is that a *real* author's whitespace and quoting is
/// what the grammar has to survive.
const REAL_SKIP_SHAPES: [(&str, &str); 9] = [
    (
        "S1 bare @ParentName head (example.bodysizes family, 734 rows)",
        r#"Defs/HediffDef[@ParentName="EBS_DefaultRaceTracker"]/modExtensions/li[@Class="ExampleBodySizes.PawnExtension"]/apparelRestrictions"#,
    ),
    (
        "S2 child-value head and-composed with not() (example.uitweaks, 142 rows)",
        r#"Defs/ThingDef[inspectorTabs/li="ITab_Bills" and not(comps)]"#,
    ),
    (
        "S3 bare-type head, whitespace before '[' (example.jobmatrix, 102 rows)",
        r#"/Defs/ThingDef/race/mechEnabledWorkTypes [li [text()="Hauling"] ]"#,
    ),
    (
        "S4 bare-type head + contains(text(), ..) (example.shelfrules, 88 rows)",
        r#"/Defs/ThingDef [thingClass [contains(text(), "Storage")] ]/building/defaultStorageSettings"#,
    ),
    (
        "S5 the 'whitespace around =' family (four real authors' own mods, 174 rows)",
        r#"Defs/HediffDef[defName="EX_AnimalBionicLeg" or defName="EX_AnimalBionicArm" or defName="EX_AnimalBionicTail" or defName="EX_AnimalBionicJaw" or defName="EX_AnimalBionicHeart"or defName="EX_AnimalBionicSpine"or defName="EX_AnimalBionicStomach"or defName="EX_AnimalBionicEye"]"#,
    ),
    (
        "S6 @ParentName and-composed with a child test (example.bionicsfork, 32 rows)",
        r#"Defs/ThingDef[@ParentName="BodyPartProstheticBase" and costList/ComponentIndustrial]"#,
    ),
    (
        "S7 step predicate not(li[@Class=..]) (example.animalbionics, 31 rows)",
        r#"Defs/ThingDef[defName="EX_ChemfuelStoveLarge"]/comps[not(li[@Class="CompProperties_AffectedByFacilities"])]"#,
    ),
    (
        "S8 head 'comps and comps[li[@Class=..]]' (example.slantwalls, 23 rows)",
        r#"/Defs/ThingDef[comps and comps[li[@Class="ExampleFurniture.CompProperties_Mountable"]]]"#,
    ),
    (
        "S9 malformed author predicate — stays Unsupported by design (example.speciessupport)",
        r#"Defs/ThingDef[defName="Cow" or defName="Muffalo"]/race[not(petness)="petness"]"#,
    ),
];

/// The skip-shape probe prints the measured verdict for every shape and
/// asserts it, so it is a regression guard as well as a record:
/// `S5`/`S7` are resolved by the strict grammar itself, the other six
/// buildable shapes stay `Unsupported` from [`parse`] **by design**
/// (this module can never enumerate a filter head's defs) and are
/// resolved by [`head_filter_predicate`] instead, and `S9` — a mod
/// author's malformed `not(petness)="petness"` — is resolved by
/// neither, permanently.
#[test]
fn probe_records_the_parse_and_filter_verdict_for_every_real_skip_shape() {
    // Per shape: does `parse` alone answer it, and does
    // `head_filter_predicate`?
    const EXPECTED: [(bool, bool); 9] = [
        (false, true),  // S1 bare @ParentName head
        (false, true),  // S2 child-value head and-composed with not()
        (false, true),  // S3 bare-type head, whitespace before '['
        (false, true),  // S4 bare-type head + contains(text(), ..)
        (true, true),   // S5 operator directly after a closing quote
        (false, true),  // S6 @ParentName and-composed with a child test
        (true, true),   // S7 step predicate not(li[@Class=..])
        (false, true),  // S8 head 'comps and comps[li[@Class=..]]'
        (false, false), // S9 malformed author predicate — never
    ];

    for ((label, xpath), (expect_parse, expect_filter)) in REAL_SKIP_SHAPES.iter().zip(EXPECTED) {
        let verdict = match parse(xpath) {
            XPathExpr::Supported { targets, .. } => {
                format!("Supported ({} target(s))", targets.len())
            }
            XPathExpr::DocumentRoot => "DocumentRoot".to_string(),
            XPathExpr::Unsupported { reason } => format!("Unsupported: {reason}"),
        };
        let filter = head_filter_predicate(xpath);
        println!("{label}");
        println!("    parse:       {verdict}");
        println!("    head_filter: {filter:?}");

        assert_eq!(
            matches!(parse(xpath), XPathExpr::Supported { .. }),
            expect_parse,
            "parse verdict changed for {label}"
        );
        assert_eq!(
            filter.is_some(),
            expect_filter,
            "head_filter_predicate verdict changed for {label}"
        );
    }
}
