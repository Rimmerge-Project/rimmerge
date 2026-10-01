use rim_analyzer::domain::{EdgeKind, ModId};
use rim_resolve::domain::RuleOrigin;
use rim_resolve::sort::{DroppedEdge, EdgeProvenance, Layer, OrderingEdge};

use super::render;

fn engine_edge(after: &str, before: &str, layer: Layer, kind: EdgeKind) -> OrderingEdge {
    OrderingEdge {
        after: ModId::new(after),
        before: ModId::new(before),
        layer,
        provenance: EdgeProvenance::Engine {
            kind,
            detail: format!("{after} needs {before}"),
        },
    }
}

fn contradiction(after: &str, before: &str) -> DroppedEdge {
    DroppedEdge {
        edge: engine_edge(after, before, Layer::Declared, EdgeKind::LoadAfter),
        witness_cycle: vec![ModId::new(after), ModId::new(before)],
        winner: Some(engine_edge(
            before,
            after,
            Layer::Hard,
            EdgeKind::AssemblyRef,
        )),
    }
}

fn longer_cycle() -> DroppedEdge {
    DroppedEdge {
        edge: engine_edge(
            "example.c",
            "example.a",
            Layer::Declared,
            EdgeKind::LoadAfter,
        ),
        witness_cycle: ["example.c", "example.b", "example.a"]
            .map(ModId::new)
            .to_vec(),
        winner: None,
    }
}

#[test]
fn a_two_mod_contradiction_names_its_cycle_and_the_edge_that_overruled_it() {
    let text = render(&[contradiction("example.a", "example.b")], None).expect("renders");

    assert!(text.contains("Dropped edges (1 of 1):"), "{text}");
    assert!(
        text.contains("example.a <- example.b  [declared] load_after:"),
        "{text}"
    );
    assert!(text.contains("cycle: example.a -> example.b"), "{text}");
    assert!(
        text.contains("overruled by: example.b <- example.a  [hard] assembly_ref:"),
        "{text}"
    );
}

#[test]
fn a_longer_cycle_has_no_overruled_by_line() {
    let text = render(&[longer_cycle()], None).expect("renders");

    assert!(
        text.contains("cycle: example.c -> example.b -> example.a"),
        "{text}"
    );
    assert!(!text.contains("overruled by"), "{text}");
}

#[test]
fn the_mod_filter_keeps_edges_touching_the_mod_at_either_end() {
    let dropped = [
        contradiction("example.a", "example.b"),
        longer_cycle(),
        contradiction("example.x", "example.y"),
    ];

    let text = render(&dropped, Some(&ModId::new("example.a_steam"))).expect("renders");

    assert!(text.contains("Dropped edges (2 of 3):"), "{text}");
    assert!(text.contains("example.a <- example.b"), "{text}");
    assert!(text.contains("example.c <- example.a"), "{text}");
    assert!(!text.contains("example.x"), "{text}");
}

#[test]
fn the_listing_does_not_depend_on_the_order_the_sorter_dropped_edges_in() {
    let forward = [
        contradiction("example.b", "example.c"),
        contradiction("example.a", "example.c"),
    ];
    let reversed = [forward[1].clone(), forward[0].clone()];

    let first = render(&forward, None).expect("renders");
    let second = render(&reversed, None).expect("renders");

    assert_eq!(first, second);
    assert!(first.find("example.a <-").expect("a") < first.find("example.b <-").expect("b"));
}

#[test]
fn text_from_mods_and_rules_never_carries_a_control_character() {
    let hostile = "evil\u{1b}[2J\u{9b}x\nforged";
    let engine = DroppedEdge {
        edge: OrderingEdge {
            after: ModId::new(hostile),
            before: ModId::new("example.b"),
            layer: Layer::Hard,
            provenance: EdgeProvenance::Engine {
                kind: EdgeKind::AssemblyRef,
                detail: hostile.to_string(),
            },
        },
        witness_cycle: vec![ModId::new(hostile), ModId::new("example.b")],
        winner: Some(OrderingEdge {
            after: ModId::new("example.b"),
            before: ModId::new("example.a"),
            layer: Layer::AnyOf,
            provenance: EdgeProvenance::AnyOf {
                assembly: hostile.to_string(),
            },
        }),
    };
    let rule = DroppedEdge {
        edge: OrderingEdge {
            after: ModId::new("example.a"),
            before: ModId::new("example.b"),
            layer: Layer::UserDecision,
            provenance: EdgeProvenance::Rule {
                origin: RuleOrigin::UserDecision,
                comment: Some(hostile.to_string()),
            },
        },
        witness_cycle: vec![ModId::new("example.a")],
        winner: None,
    };

    let text = render(&[engine, rule], None).expect("renders");

    assert!(!text.contains('\u{1b}'), "{text:?}");
    assert!(!text.contains('\u{9b}'), "{text:?}");
    assert!(!text.contains("\nforged"), "{text:?}");
    assert!(text.contains("evil[2Jxforged"), "{text:?}");
}
