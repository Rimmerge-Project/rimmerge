//! Real-install verification for "what texture does a def show": the
//! `resolve_def_graphic` / `read_def_texture` use cases run over a broad,
//! deterministic cross-section of the install's own defs. Same
//! `RIMMERGE_PERF_PROFILE_DIR` convention as `def_inspector_timing.rs` (a
//! scratch profile directory, never the live one).
//!
//! The test asserts shape and bands only, never a real def or mod
//! identity: the sample is picked from the inventory by stride, so it
//! keeps covering the same mix of shapes as the mod list drifts.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop --all-features --release --run-ignored ignored-only -E 'test(def_graphics_real_install)'`

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use rim_analyzer::domain::Selector;
use rim_resolve::domain::{DefKey, DefRef, OrderSource};
use rim_session::Session;
use rim_session::use_cases::{
    DefGraphic, DefTexture, GraphicSet, ImageSource, ReadDefTexture, ReadDefTextureError,
    ResolveDefGraphic, ResolveDefGraphicError, TextureKey,
};

/// This file's own "Run with" invocation, named in every guard message.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch copy> cargo nextest run -p rimmerge-desktop \
     --all-features --release --run-ignored ignored-only";

/// Def types whose defs show a texture of their own (or, for a pawn kind,
/// through its race).
const GRAPHIC_DEF_TYPES: [&str; 8] = [
    "ThingDef",
    "PawnKindDef",
    "TerrainDef",
    "HairDef",
    "BeardDef",
    "HeadTypeDef",
    "BodyTypeDef",
    "TattooDef",
];

/// How many defs the stride sample aims for.
const SAMPLE_SIZE: usize = 800;

/// Per-def budget for resolve plus the default read.
const MEDIAN_BUDGET: Duration = Duration::from_millis(50);
const P95_BUDGET: Duration = Duration::from_millis(250);

/// What one sampled def came to; compared across the two runs.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    def: String,
    graphic: String,
    read_label: &'static str,
    image_bytes: usize,
}

/// The measurements of one pass over the sample.
struct Pass {
    outcomes: Vec<Outcome>,
    timings: Vec<Duration>,
    inspect_errors: usize,
    resolved: usize,
    no_graphic: usize,
    composed: usize,
    keys_checked: usize,
    read_labels: BTreeMap<&'static str, usize>,
}

fn load_real_session() -> Option<(Session, crate::state::Adapters)> {
    let profile_dir = crate::real_install_support::require_profile_dir(RERUN_COMMAND)?;
    let paths = crate::real_install_support::real_install_paths(profile_dir, RERUN_COMMAND);
    let state = crate::state::AppState::default();
    let adapters = state.adapters.clone();
    let use_case = rim_session::use_cases::LoadProject::new(
        adapters.scanner,
        adapters.config_store,
        adapters.decision_store,
        adapters.rule_store,
        adapters.patch_store,
        adapters.assignment_store,
        rim_io::FsModKnowledgeStore::vendored(),
        true,
    );
    let mut session = use_case.execute(paths, &mut |_| {}).expect("load project");
    session.set_def_source_reader(adapters.def_reader.clone());
    Some((session, adapters))
}

/// Every def of a graphic-bearing type, sorted, thinned by a fixed stride
/// to about [`SAMPLE_SIZE`]. Deterministic for a given inventory.
fn sample_defs(session: &Session) -> Vec<DefRef> {
    let candidates: Vec<&(String, String)> = session
        .sources()
        .owners_by_def
        .keys()
        .filter(|(def_type, _)| GRAPHIC_DEF_TYPES.contains(&def_type.as_str()))
        .collect();
    let stride = (candidates.len() / SAMPLE_SIZE).max(1);
    candidates
        .into_iter()
        .step_by(stride)
        .map(|(def_type, def_name)| {
            DefRef::new(
                DefKey {
                    def_type: def_type.clone(),
                    def_name: def_name.clone(),
                },
                Selector::DefName,
            )
        })
        .collect()
}

/// Every key the set produced must survive the reader's own refusal check
/// unchanged; returns how many were checked.
fn check_keys(set: &GraphicSet) -> usize {
    let mut checked = 0;
    for slot in set.slots() {
        for variant in slot.variants() {
            for face in variant.faces.all() {
                let reparsed = TextureKey::parse(face.key.as_str())
                    .unwrap_or_else(|error| panic!("key {} is refused: {error}", face.key));
                assert_eq!(reparsed, face.key, "key is not already normalized");
                checked += 1;
            }
        }
    }
    checked
}

/// A stable name for each read outcome; the match is exhaustive so a new
/// variant is a compile error here.
fn read_label(texture: &DefTexture) -> (&'static str, usize) {
    match texture {
        DefTexture::Image { texture, from, .. } => (
            match from {
                ImageSource::Direct => "image-direct",
                ImageSource::PngSibling => "image-png-sibling",
            },
            texture.bytes.len(),
        ),
        DefTexture::DdsNotPreviewable { .. } => ("dds-not-previewable", 0),
        DefTexture::UndecodableInGame { .. } => ("undecodable-in-game", 0),
        DefTexture::NotViewable { is_uncertain } => (
            if *is_uncertain {
                "not-viewable-uncertain"
            } else {
                "not-viewable-built-in"
            },
            0,
        ),
        DefTexture::NotFound => ("not-found", 0),
        DefTexture::Unreadable(_) => ("unreadable", 0),
    }
}

fn run_pass(session: &mut Session, adapters: &crate::state::Adapters, sample: &[DefRef]) -> Pass {
    let resolver = ResolveDefGraphic::new(adapters.def_reader.clone());
    let reader = ReadDefTexture::new(adapters.def_reader.clone(), adapters.asset_locator.clone());
    let mut pass = Pass {
        outcomes: Vec::new(),
        timings: Vec::new(),
        inspect_errors: 0,
        resolved: 0,
        no_graphic: 0,
        composed: 0,
        keys_checked: 0,
        read_labels: BTreeMap::new(),
    };
    for def_ref in sample {
        let start = Instant::now();
        let graphic = match resolver.execute(session, def_ref) {
            Ok(graphic) => graphic.clone(),
            Err(ResolveDefGraphicError::Inspect(_)) => {
                pass.inspect_errors += 1;
                continue;
            }
        };
        let mut label = ("", 0);
        match &graphic {
            DefGraphic::NoGraphic => pass.no_graphic += 1,
            DefGraphic::ComposedAtRuntime => pass.composed += 1,
            DefGraphic::Resolved(set) => {
                pass.resolved += 1;
                pass.keys_checked += check_keys(set);
                let view = set.default_view();
                let key = set.slots()[view.slot].variants()[view.variant]
                    .faces
                    .default_face()
                    .key
                    .clone();
                match reader.execute(session, def_ref, &key) {
                    Ok(texture) => label = read_label(&texture),
                    Err(ReadDefTextureError::Resolve(_)) => {
                        pass.inspect_errors += 1;
                        continue;
                    }
                    Err(other) => panic!("reading {key} of {def_ref}: {other}"),
                }
                pass.timings.push(start.elapsed());
                *pass.read_labels.entry(label.0).or_default() += 1;
            }
        }
        pass.outcomes.push(Outcome {
            def: def_ref.to_string(),
            graphic: format!("{graphic:?}"),
            read_label: label.0,
            image_bytes: label.1,
        });
    }
    pass
}

fn percentile(sorted: &[Duration], fraction: f64) -> Duration {
    let last = sorted.len().saturating_sub(1);
    let index = ((last as f64) * fraction).round() as usize;
    sorted.get(index).copied().unwrap_or_default()
}

#[tokio::test]
#[ignore = "needs the real install plus RIMMERGE_PERF_PROFILE_DIR pointing at a scratch profile copy"]
async fn def_graphics_real_install() {
    let Some((mut session, adapters)) = load_real_session() else {
        return;
    };
    let sample = sample_defs(&session);
    eprintln!(
        "loaded {} mods; sampled {} defs",
        session.report().mods.len(),
        sample.len()
    );
    assert!(
        sample.len() >= 200,
        "the install has too few graphic-bearing defs to sample: {}",
        sample.len()
    );

    let first = run_pass(&mut session, &adapters, &sample);

    // A second pass from a cold def-graphic cache: setting the current
    // order (to itself) is the session's own cache-clearing path.
    let current = session.orders().get(OrderSource::Current).clone();
    session.set_current_order(current);
    let second = run_pass(&mut session, &adapters, &sample);

    let mut sorted = first.timings.clone();
    sorted.sort();
    let median = percentile(&sorted, 0.5);
    let p95 = percentile(&sorted, 0.95);
    let image_reads: usize = first
        .read_labels
        .iter()
        .filter(|(label, _)| label.starts_with("image"))
        .map(|(_, count)| count)
        .sum();
    eprintln!(
        "sampled={} resolved={} no_graphic={} composed={} inspect_errors={} keys_checked={}",
        sample.len(),
        first.resolved,
        first.no_graphic,
        first.composed,
        first.inspect_errors,
        first.keys_checked
    );
    eprintln!("default-face reads: {:?}", first.read_labels);
    eprintln!(
        "image fraction of resolved: {:.3}",
        image_reads as f64 / first.resolved.max(1) as f64
    );
    eprintln!("resolve+read per def: median={median:?} p95={p95:?} (cold first pass)");

    assert_eq!(
        first.outcomes, second.outcomes,
        "two passes over the same sample must give identical results"
    );
    assert!(
        first.resolved >= 50,
        "too few sampled defs resolved to a graphic: {}",
        first.resolved
    );
    assert!(
        first.inspect_errors * 50 <= sample.len(),
        "more than 2% of sampled defs failed to inspect: {} of {}",
        first.inspect_errors,
        sample.len()
    );
    assert!(first.keys_checked >= first.resolved, "every set has a key");
    // Bands, set from a measured run (87% images, 12% built-in or
    // DDS-only, nothing unreadable) with room for a different mod list.
    let count_of = |label: &str| first.read_labels.get(label).copied().unwrap_or(0);
    let not_shown = count_of("not-viewable-built-in")
        + count_of("not-viewable-uncertain")
        + count_of("dds-not-previewable")
        + count_of("undecodable-in-game")
        + count_of("not-found");
    assert!(
        image_reads * 2 >= first.resolved,
        "fewer than half of the resolved defs reached a located image: {image_reads} of {}",
        first.resolved
    );
    assert!(
        not_shown >= 1,
        "no resolved def came back as built-in, DDS-only, undecodable or missing: the \
         availability classification looks stuck on one answer"
    );
    assert!(
        count_of("unreadable") * 50 <= first.resolved,
        "more than 2% of located images were unreadable: {}",
        count_of("unreadable")
    );
    assert!(
        median <= MEDIAN_BUDGET,
        "median resolve+read {median:?} exceeds {MEDIAN_BUDGET:?}"
    );
    assert!(
        p95 <= P95_BUDGET,
        "p95 resolve+read {p95:?} exceeds {P95_BUDGET:?}"
    );
}
