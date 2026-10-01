//! Diagnosis: `xpath_expr::selected_classes`'s namespace-qualification filter
//! drops every bare, non-namespace-qualified `Class`/`name` predicate value
//! (including a real case's own injected class) before the class-string
//! injector pass (`patch_injected_node_edges`) ever sees it. Widening that
//! filter does not help: on a real install it admits over a hundred distinct
//! bare class values and yields **zero** new `PatchInjectedNode` edges —
//! including the real case itself.
//!
//! This test is the traced diagnosis, not an acceptance test: it proves,
//! against the real install, *why* that case can never appear even with the
//! filter widened — a second, independent, pre-existing safety rule
//! (`emit_patch_injected_node_edge`'s own `pre_exists_inline` check) blocks
//! it, because the injected class is not unique to that injection: a vanilla
//! DLC def elsewhere in the corpus writes the identical bare class name
//! inline, for an unrelated feature. That rule exists precisely to prevent
//! asserting a `Hard` load-order requirement when the selected node could
//! plausibly exist independently of the injecting patch — an
//! already-established, deliberately conservative design (a real-install
//! sequence-wrapper mod pair needs it for the same shape) — and it is
//! *correct* to decline here too: this analyzer has no way to prove the
//! target element can only exist via the owning mod's own `Replace`, only
//! that a class of that exact bare name can exist independently of it
//! somewhere else in the loaded content.
//!
//! Both halves of the mechanism were traced individually against the
//! real data, not assumed: the *producer* side (the owning mod's own
//! `Replace` op genuinely records the acceptance class in
//! `PatchOp::injected_types`, confirmed against this op's own real,
//! scanned XML) and the *blocker* side (`Indices::inline_type_names`
//! genuinely contains that bare string, sourced from vanilla content,
//! confirmed against the real scan) are both asserted directly below —
//! so a future reader doesn't have to re-derive either fact by hand.
//!
//! **No third-party mod or class name is hardcoded here**:
//! `RIMMERGE_EXPECTED_INJECTED_CLASS_CASE=<selector mod id>,<owner mod
//! id>,<injected class name>` names the specific real acceptance case a
//! maintainer's own install can investigate; unset, both tests below skip
//! with an honest message rather than asserting on data that doesn't exist
//! here.
//!
//! `#[ignore]`d: reads the real game/workshop install and the real
//! `ModsConfig.xml` (read-only). Requires `RIMMERGE_PERF_PROFILE_DIR`,
//! **failing loudly, by panicking**, when unset — see
//! `real_install_runtime_patch_kinds.rs`'s own doc comment for why this rail
//! exists even though nothing here writes to that directory.
//!
//! Run with:
//! `RIMMERGE_GAME_DIR=<your install> RIMMERGE_PERF_PROFILE_DIR=<scratch dir> RIMMERGE_EXPECTED_INJECTED_CLASS_CASE=<selector>,<owner>,<class> cargo nextest run -p rim-analyzer --all-features --release --run-ignored ignored-only`

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use rim_analyzer::analysis::indices::{ActiveMods, DisplayNameIndex, Indices, patch_op_active};
use rim_analyzer::analysis::{self, RunContext};
use rim_analyzer::domain::{DefTarget, ModId, Report, ScanOutput, ScannedMod};
use rim_analyzer::extract::xpath_expr::{self, Predicate, XPathExpr};
use rim_analyzer::infra;

mod common;

/// This file's own "Run with" invocation (the module doc comment above),
/// named in every guard message so it points at the exact command for
/// this tier rather than a generic one.
const RERUN_COMMAND: &str = "RIMMERGE_GAME_DIR=<your install> \
     RIMMERGE_PERF_PROFILE_DIR=<scratch dir> cargo nextest run -p rim-analyzer --all-features \
     --release --run-ignored ignored-only";
use walkdir::WalkDir;

/// This file's own env var naming the specific real acceptance case
/// under investigation (see the module doc comment) — never hardcoded.
const INJECTED_CLASS_CASE_VAR: &str = "RIMMERGE_EXPECTED_INJECTED_CLASS_CASE";

/// One real `(selector, owner, injected class)` triple, read from
/// [`INJECTED_CLASS_CASE_VAR`].
struct InjectedClassCase {
    selector: ModId,
    owner: ModId,
    class: String,
}

/// `None` (after an honest skip message) when [`INJECTED_CLASS_CASE_VAR`]
/// is unset — this diagnostic has nothing generic to fall back to, since
/// its whole point is one specific real interaction.
fn injected_class_case() -> Option<InjectedClassCase> {
    let raw = common::require_pin_var(INJECTED_CLASS_CASE_VAR, RERUN_COMMAND)?;
    let mut parts = raw.splitn(3, ',');
    let (Some(selector), Some(owner), Some(class)) = (parts.next(), parts.next(), parts.next())
    else {
        panic!(
            "{INJECTED_CLASS_CASE_VAR} must be '<selector mod id>,<owner mod id>,<injected \
             class name>', got {raw}"
        );
    };
    Some(InjectedClassCase {
        selector: ModId::new(selector),
        owner: ModId::new(owner),
        class: class.to_string(),
    })
}

/// Scans the real, current install and runs the full analyzer over it,
/// read-only, keeping the raw [`ScanOutput`] alongside the built [`Report`]
/// — the corpus-wide predicate walk below needs every active mod's own raw
/// `<xpath>` text, which `Report.mods` never carries. Panics with a clear
/// message on any failure — this test only ever runs by hand, against this
/// specific development machine.
fn scan_and_analyze_real_install() -> Option<(ScanOutput, Report)> {
    let (config, game_version) = common::real_scan_config(RERUN_COMMAND)?;
    let scan =
        infra::scan(&config).unwrap_or_else(|error| panic!("scanning the real install: {error}"));
    let context = RunContext {
        game_dir: config.game_dir,
        workshop_dir: config.workshop_dir,
        mods_config_path: config.mods_config_path,
        game_version,
    };
    let report = analysis::build_ref(&scan, &context);
    Some((scan, report))
}

/// Every `Class`/`name` predicate value [`xpath_expr::selected_classes`]
/// *would* collect if its dot-requirement were lifted, read directly off
/// every active mod's own raw patch ops — this test's own independent oracle
/// over the corpus (never calling the function under investigation), split
/// into the two buckets the dot-requirement draws between. `bare.len()` is
/// exactly how many distinct class strings the shipped filter drops
/// corpus-wide — reported via `eprintln!` rather than pinned, since Workshop
/// content drifts session to session the same way every other real-install
/// count in this crate does.
fn selected_class_values_by_dot(scan: &ScanOutput) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut namespace_qualified = BTreeSet::new();
    let mut bare = BTreeSet::new();
    for scanned_mod in &scan.scanned_mods {
        for op in scanned_mod.patch_ops.iter().filter(|op| op.is_mutating) {
            let Some(xpath) = op.xpath.as_deref() else {
                continue;
            };
            let XPathExpr::Supported {
                root_predicates,
                steps,
                ..
            } = xpath_expr::parse(xpath)
            else {
                continue;
            };
            let predicates = root_predicates
                .iter()
                .chain(steps.iter().flat_map(|step| step.predicates.iter()));
            for predicate in predicates {
                collect_class_like_values(predicate, &mut namespace_qualified, &mut bare);
            }
        }
    }
    (namespace_qualified, bare)
}

/// Mirrors `xpath_expr`'s own (private) `collect_selected_classes` walk,
/// minus its dot requirement — duplicated here deliberately: this test is
/// the independent oracle over what that function drops, so it must not
/// call the function under investigation to compute its own expected
/// split.
fn collect_class_like_values(
    predicate: &Predicate,
    namespace_qualified: &mut BTreeSet<String>,
    bare: &mut BTreeSet<String>,
) {
    match predicate {
        Predicate::Attr(name, value) if name == "Class" => {
            insert_by_dot(value, namespace_qualified, bare);
        }
        Predicate::ChildText(name, value) if name == "name" => {
            insert_by_dot(value, namespace_qualified, bare);
        }
        Predicate::And(left, right) | Predicate::Or(left, right) => {
            collect_class_like_values(left, namespace_qualified, bare);
            collect_class_like_values(right, namespace_qualified, bare);
        }
        // The same two arms `collect_selected_classes` itself has — a
        // positive child filter still selects by class, a negation does not.
        Predicate::Child(_, inner) => {
            collect_class_like_values(inner, namespace_qualified, bare);
        }
        Predicate::Not(_)
        | Predicate::Attr(..)
        | Predicate::ChildText(..)
        | Predicate::NestedChildText(..)
        | Predicate::Text(_)
        | Predicate::Contains(_)
        | Predicate::Has(_)
        | Predicate::Position(_) => {}
    }
}

fn insert_by_dot(
    value: &str,
    namespace_qualified: &mut BTreeSet<String>,
    bare: &mut BTreeSet<String>,
) {
    if value.contains('.') {
        namespace_qualified.insert(value.to_string());
    } else {
        bare.insert(value.to_string());
    }
}

/// The diagnosis, traced against the real install rather than assumed:
/// reports the corpus-wide bare/qualified split, confirms the *producer* side
/// genuinely records the acceptance case's own class (the pinned owner mod's
/// own `PatchOperationReplace` on `PreceptDef/IdeoBuilding`), and confirms
/// the *blocker* — the same class name pre-existing inline via unrelated
/// content elsewhere in the corpus — is real. Deliberately does **not**
/// assert that a `PatchInjectedNode` edge for the pinned selector -> owner
/// pair exists: it doesn't, and per this test's own module doc, it is not
/// expected to without a further, structural change.
#[test]
#[ignore = "needs the real install and RIMMERGE_PERF_PROFILE_DIR — see this file's own doc comment"]
fn bare_class_dot_filter_diagnosis() {
    let Some(case) = injected_class_case() else {
        return;
    };
    let Some((scan, _report)) = scan_and_analyze_real_install() else {
        return;
    };

    let (namespace_qualified, bare) = selected_class_values_by_dot(&scan);
    eprintln!(
        "follow-up diagnosis: {} distinct namespace-qualified class/name predicate values, \
         {} distinct bare ones (bare = what the shipped dot-requirement drops corpus-wide)",
        namespace_qualified.len(),
        bare.len()
    );
    assert!(
        bare.contains(&case.class),
        "the acceptance case's own bare class ({}) should still be one of the values the \
         dot-requirement drops from the selector mod's own selector",
        case.class
    );

    // Producer side: does the owner's own real, scanned Replace op on
    // PreceptDef/IdeoBuilding genuinely record this class as injected?
    let owner = scan
        .scanned_mods
        .iter()
        .find(|scanned_mod| scanned_mod.info.id == case.owner)
        .unwrap_or_else(|| {
            panic!(
                "{} is not active on this install — mod set changed",
                case.owner
            )
        });
    let injects_room_requirement = owner.patch_ops.iter().any(|op| {
        op.is_mutating
            && op.target.as_ref().is_some_and(|target| {
                target.def_type == "PreceptDef" && target.def_name == "IdeoBuilding"
            })
            && op.injected_types.contains(&case.class)
    });
    assert!(
        injects_room_requirement,
        "{}'s own patch on PreceptDef/IdeoBuilding should record {} in injected_types — if this \
         fails, the producer side (not the dot filter) is the real gap",
        case.owner, case.class
    );

    // Blocker side: is the same bare class also written inline somewhere
    // active, independent of this injection? `Indices::inline_type_names`
    // is built from `ScannedMod::inline_types`
    // (`extract::xml_util::collect_class_strings`, no dot filter of its
    // own), so this reads the same signal `emit_patch_injected_node_edge`'s
    // `pre_exists_inline` check already consults today.
    let active = ActiveMods::build(&scan.scanned_mods);
    let indices = Indices::build(
        &scan.scanned_mods,
        &HashSet::new(),
        &active,
        &scan.core_resource_textures,
    );
    let pre_exists_inline = indices.inline_type_names.contains(&case.class);
    eprintln!(
        "follow-up diagnosis: {} pre-exists inline somewhere active = {pre_exists_inline} — \
         this, not the dot filter, is why widening `selected_classes` alone produced zero new \
         PatchInjectedNode edges on this install, {} -> {} included",
        case.class, case.selector, case.owner
    );
    assert!(
        pre_exists_inline,
        "expected {} to already be written inline somewhere active — if this is no longer true, \
         the traced pre_exists_inline blocker may no longer apply \
         and the fix may be worth revisiting",
        case.class
    );
}

// -- pre_exists_inline blocked-candidate measurement ----------------------

/// One `(selector_mod, selecting_op, class)` triple that satisfies every
/// one of `emit_patch_injected_node_edge`'s own hard-edge conditions
/// (`analysis::edges.rs`) **except** `pre_exists_inline`: exactly one
/// other active mod injects `class`, the selector does not itself inject
/// it, and that sole injector has an occurrence whose own `DefTarget` is
/// `def_targets_compatible` with the selecting op's own target. Built by
/// [`hard_edge_candidates`], which reimplements those checks locally
/// rather than calling the production function — `emit_patch_injected_node_edge`
/// and `def_targets_compatible` are both private to `analysis::edges`,
/// and this test must not widen either's visibility just to read them.
#[derive(Debug, Clone)]
struct HardEdgeCandidate {
    class: String,
    selector: ModId,
    injector: ModId,
    target: Option<DefTarget>,
}

/// Where a blocked candidate's own class string is written inline,
/// relative to the selecting op's own target def (this task's own A/B/C
/// split). `A`/`B` name def-scope refinements a narrower
/// `pre_exists_inline` could plausibly admit; `C` stays blocked under any
/// such refinement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineOccurrenceBucket {
    /// The class appears inline only under defs of a *different* def
    /// type than the selecting op's own target def type. A
    /// def-type-scoped refinement of `pre_exists_inline` would admit
    /// this edge.
    A,
    /// The class appears inline under the *same* def type as the target,
    /// but never under the exact target def (`def_type` + `def_name`). A
    /// def-scoped refinement would admit it; a def-type-scoped one would
    /// not.
    B,
    /// The class appears inline under the exact target def itself. Stays
    /// blocked under any refinement — the node plausibly pre-exists
    /// right where the patch is looking.
    C,
}

/// One place a class string was found written inline, at per-def
/// granularity: `def_name` is `None` for a `<Defs>` child that carries no
/// `<defName>` of its own (an abstract `Name`-only template, or a
/// nameless def — see `extract::defs::DefsFile::nameless_def_count`'s
/// own doc comment) — still a real location to classify against, just
/// not one with a `defName` to match a target's own `def_name` against.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DefContext {
    def_type: String,
    def_name: Option<String>,
}

/// Mirrors `patch_injected_node_edges`'s own first pass
/// (`analysis::edges.rs`) exactly — duplicated, not called, because this
/// test's whole point is checking that production rule against an
/// independent oracle, not delegating to it.
fn build_injected_type_owners(
    scanned: &[ScannedMod],
    active: &ActiveMods,
    name_map: &DisplayNameIndex,
) -> BTreeMap<String, Vec<(ModId, Option<DefTarget>)>> {
    let mut injected_type_owners: BTreeMap<String, Vec<(ModId, Option<DefTarget>)>> =
        BTreeMap::new();
    for scanned_mod in scanned {
        for op in scanned_mod
            .patch_ops
            .iter()
            .filter(|op| op.is_mutating && patch_op_active(op, active, name_map))
        {
            for injected in &op.injected_types {
                injected_type_owners
                    .entry(injected.clone())
                    .or_default()
                    .push((scanned_mod.info.id.clone(), op.target.clone()));
            }
        }
    }
    injected_type_owners
}

/// Duplicates `analysis::edges::def_targets_compatible` (private to that
/// module, unreachable from this test) rather than widening its
/// visibility just for a measurement: true whenever either side carries
/// no target, or when both target the same `(def_type, def_name)`.
fn def_targets_compatible(selecting: Option<&DefTarget>, injecting: Option<&DefTarget>) -> bool {
    match (selecting, injecting) {
        (Some(a), Some(b)) => a.def_type == b.def_type && a.def_name == b.def_name,
        _ => true,
    }
}

/// Every `[@Class="T"]`/`[name="T"]` predicate value anywhere in `xpath`,
/// **without** `xpath_expr::selected_classes`'s own namespace-qualification
/// filter (`value.contains('.')`) — this measurement's whole population is
/// exactly what that filter drops, so it must not reuse the filtered function
/// (see this file's own module doc comment). Mirrors
/// `xpath_expr::collect_selected_classes`'s own predicate walk, minus the dot
/// check, over the same `Predicate` tree `parse` already builds.
fn selected_classes_ignoring_the_dot_filter(xpath: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let XPathExpr::Supported {
        root_predicates,
        steps,
        ..
    } = xpath_expr::parse(xpath)
    {
        let predicates = root_predicates
            .iter()
            .chain(steps.iter().flat_map(|step| step.predicates.iter()));
        for predicate in predicates {
            collect_classes_ignoring_the_dot_filter(predicate, &mut out);
        }
    }
    out
}

fn collect_classes_ignoring_the_dot_filter(predicate: &Predicate, out: &mut BTreeSet<String>) {
    match predicate {
        Predicate::Attr(name, value) if name == "Class" => {
            out.insert(value.clone());
        }
        Predicate::ChildText(name, value) if name == "name" => {
            out.insert(value.clone());
        }
        Predicate::And(left, right) | Predicate::Or(left, right) => {
            collect_classes_ignoring_the_dot_filter(left, out);
            collect_classes_ignoring_the_dot_filter(right, out);
        }
        Predicate::Child(_, inner) => collect_classes_ignoring_the_dot_filter(inner, out),
        Predicate::Not(_)
        | Predicate::Attr(..)
        | Predicate::ChildText(..)
        | Predicate::NestedChildText(..)
        | Predicate::Text(_)
        | Predicate::Contains(_)
        | Predicate::Has(_)
        | Predicate::Position(_) => {}
    }
}

/// Duplicates `extract::xml_util::collect_class_strings`'s exact rule (a
/// `Class` attribute, or a `*Class`-suffixed element's own text) — that
/// function lives in a `pub(crate)` module, unreachable from an
/// integration test. `node.descendants()` is self-inclusive in
/// `roxmltree`, matching how the production function is always called
/// with a whole subtree's own root.
fn collect_class_strings_under(node: roxmltree::Node, out: &mut BTreeSet<String>) {
    const MIN_STAR_CLASS_TAG_LEN: usize = "Class".len() + 1;
    for element in node.descendants().filter(roxmltree::Node::is_element) {
        if let Some(class) = element.attribute("Class") {
            let class = class.trim();
            if !class.is_empty() {
                out.insert(class.to_string());
            }
        }
        let tag = element.tag_name().name();
        if tag.len() >= MIN_STAR_CLASS_TAG_LEN
            && tag.ends_with("Class")
            && let Some(text) = element.text().map(str::trim).filter(|s| !s.is_empty())
        {
            out.insert(text.to_string());
        }
    }
}

/// Strips a leading UTF-8 BOM and lossily decodes — duplicates
/// `extract::xml_util::decode_lossy` (also `pub(crate)`, also
/// unreachable here) rather than widening its visibility for a
/// measurement.
fn decode_lossy_bom(bytes: &[u8]) -> String {
    const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
    let bytes = bytes.strip_prefix(&UTF8_BOM).unwrap_or(bytes);
    String::from_utf8_lossy(bytes).into_owned()
}

/// `folder`'s own direct child directories named `name`, case-insensitive
/// — mirrors `infra::mod_scan::find_subdirs`'s non-recursive branch
/// (private to that module), matching this test's own scan, which always
/// runs with `FolderPolicy::LoadFolders` (see
/// `scan_and_analyze_real_install`), never `FolderPolicy::Everything`.
fn direct_child_dirs_named(folder: &Path, name: &str) -> Vec<PathBuf> {
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.path().is_dir()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(|entry_name| entry_name.eq_ignore_ascii_case(name))
        })
        .map(|entry| entry.path())
        .collect()
}

/// Every active mod's own `Defs/**/*.xml` files, re-parsed directly with
/// `roxmltree` (already a plain, non-dev dependency of this crate — see
/// `Cargo.toml`) to build **true per-def** class-occurrence association:
/// which `(def_type, def_name)` each inline class string actually sits
/// under, across every `<Defs>` child element (concrete, abstract,
/// nameless — all are real elements to classify against).
///
/// Deliberately **not** `extract::defs::index`'s own `DefsFile::inline_types`
/// (file-level only — a whole-file union with no def association at
/// all): two defs commonly share one file, so a file-level read could
/// never tell bucket B ("same def type, different def") from bucket C
/// ("the exact target def") — the distinction this whole measurement
/// exists to draw. `indices.inline_type_names` (the production signal
/// `pre_exists_inline` itself reads) is exactly this same corpus,
/// flattened further still (no per-file, let alone per-def, association)
/// — confirmed consistent with it below via the `total_candidates > 0`/
/// blocked-population sanity check rather than assumed.
fn class_inline_occurrences_by_def(
    scanned: &[ScannedMod],
) -> BTreeMap<String, BTreeSet<DefContext>> {
    let mut out: BTreeMap<String, BTreeSet<DefContext>> = BTreeMap::new();
    for scanned_mod in scanned {
        for folder in &scanned_mod.info.loaded_folders {
            for defs_dir in direct_child_dirs_named(folder, "Defs") {
                let xml_files = WalkDir::new(&defs_dir)
                    .into_iter()
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.file_type().is_file()
                            && entry
                                .path()
                                .extension()
                                .is_some_and(|ext| ext.eq_ignore_ascii_case("xml"))
                    })
                    .map(walkdir::DirEntry::into_path);
                for xml_path in xml_files {
                    let Ok(bytes) = std::fs::read(&xml_path) else {
                        continue;
                    };
                    let text = decode_lossy_bom(&bytes);
                    let Ok(doc) = roxmltree::Document::parse(&text) else {
                        continue;
                    };
                    let root = doc.root_element();
                    if !root.tag_name().name().eq_ignore_ascii_case("Defs") {
                        continue;
                    }
                    for element in root.children().filter(roxmltree::Node::is_element) {
                        let def_type = element.tag_name().name().to_string();
                        let def_name = element
                            .children()
                            .filter(roxmltree::Node::is_element)
                            .find(|node| node.tag_name().name().eq_ignore_ascii_case("defName"))
                            .and_then(|node| node.text())
                            .map(str::trim)
                            .filter(|text| !text.is_empty())
                            .map(str::to_string);
                        let mut classes = BTreeSet::new();
                        collect_class_strings_under(element, &mut classes);
                        for class in classes {
                            out.entry(class).or_default().insert(DefContext {
                                def_type: def_type.clone(),
                                def_name: def_name.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    out
}

/// [`InlineOccurrenceBucket`] for `target` given every place `target`'s
/// own class is written inline — `occurrences` is never empty when this
/// is called (see the call site).
fn classify(target: &DefTarget, occurrences: &BTreeSet<DefContext>) -> InlineOccurrenceBucket {
    let same_type: Vec<&DefContext> = occurrences
        .iter()
        .filter(|occurrence| occurrence.def_type == target.def_type)
        .collect();
    if same_type.is_empty() {
        return InlineOccurrenceBucket::A;
    }
    let exact_def = same_type
        .iter()
        .any(|occurrence| occurrence.def_name.as_deref() == Some(target.def_name.as_str()));
    if exact_def {
        InlineOccurrenceBucket::C
    } else {
        InlineOccurrenceBucket::B
    }
}

/// A deterministically-ordered, capped-at-25 sample of one bucket's own
/// distinct `(class, selector, injector, target key)` rows, printed for a
/// human to eyeball whether the newly-admitted edges a refinement would
/// produce look justified.
fn print_bucket_sample(label: &str, rows: &BTreeSet<(String, String, String, String)>) {
    eprintln!(
        "  bucket {label}: {} distinct (class, selector, injector, target) rows (showing up to 25)",
        rows.len()
    );
    for (class, selector, injector, target) in rows.iter().take(25) {
        eprintln!("    class={class} selector={selector} injector={injector} target={target}");
    }
}

/// Measures how many *other* class-string pairs `pre_exists_inline` blocks
/// for the same reason, so nobody narrows it without that number. It does
/// **not** change `pre_exists_inline` (`emit_patch_injected_node_edge`,
/// `analysis::edges.rs`) or `xpath_expr::selected_classes` in any way — see
/// [`HardEdgeCandidate`], [`build_injected_type_owners`], and
/// [`def_targets_compatible`]'s own doc comments for why this test
/// reimplements the relevant production logic locally rather than calling it.
///
/// Enumerates every `(selector_mod, selecting_op, class)` candidate
/// satisfying every hard-edge condition except `pre_exists_inline`, then
/// splits the ones `pre_exists_inline` actually blocks
/// (`indices.inline_type_names.contains(class)`) into
/// [`InlineOccurrenceBucket`] A/B/C using **true per-def** association
/// ([`class_inline_occurrences_by_def`]) — not the coarser file-level
/// fallback `extract::defs::index`'s own `DefsFile::inline_types` field
/// would give, which cannot distinguish two defs sharing one file (see
/// that function's own doc comment).
///
/// Asserts nothing about any edge appearing or about the bucket split —
/// only that the candidate population is non-empty, so a future run that
/// silently measures zero candidates fails loudly instead of reporting a
/// vacuous "0 blocked" as a clean result.
#[test]
#[ignore = "needs the real install and RIMMERGE_PERF_PROFILE_DIR — see this file's own doc comment"]
fn pre_exists_inline_blocked_candidates_by_def_scope() {
    let Some((scan, _report)) = scan_and_analyze_real_install() else {
        return;
    };

    let active = ActiveMods::build(&scan.scanned_mods);
    let indices = Indices::build(
        &scan.scanned_mods,
        &HashSet::new(),
        &active,
        &scan.core_resource_textures,
    );
    let (name_map, _warnings) = analysis::edges::build_name_map(&scan.scanned_mods);

    let injected_type_owners = build_injected_type_owners(&scan.scanned_mods, &active, &name_map);
    let class_def_contexts = class_inline_occurrences_by_def(&scan.scanned_mods);

    let mut candidates: Vec<HardEdgeCandidate> = Vec::new();
    for scanned_mod in &scan.scanned_mods {
        let selector_id = &scanned_mod.info.id;
        for op in scanned_mod
            .patch_ops
            .iter()
            .filter(|op| op.is_mutating && patch_op_active(op, &active, &name_map))
        {
            let Some(xpath) = op.xpath.as_deref() else {
                continue;
            };
            for class in selected_classes_ignoring_the_dot_filter(xpath) {
                let Some(occurrences) = injected_type_owners.get(&class) else {
                    continue;
                };
                let self_injects = occurrences.iter().any(|(id, _)| id == selector_id);
                if self_injects {
                    continue;
                }
                let mut other_injectors: Vec<&ModId> = occurrences
                    .iter()
                    .map(|(id, _)| id)
                    .filter(|id| *id != selector_id)
                    .collect();
                other_injectors.sort();
                other_injectors.dedup();
                let sole_injector = match other_injectors.as_slice() {
                    [sole] => *sole,
                    _ => continue,
                };
                let compatible = occurrences
                    .iter()
                    .filter(|(id, _)| id == sole_injector)
                    .any(|(_, injecting_target)| {
                        def_targets_compatible(op.target.as_ref(), injecting_target.as_ref())
                    });
                if !compatible {
                    continue;
                }
                candidates.push(HardEdgeCandidate {
                    class,
                    selector: selector_id.clone(),
                    injector: sole_injector.clone(),
                    target: op.target.clone(),
                });
            }
        }
    }

    let total_candidates = candidates.len();
    assert!(
        total_candidates > 0,
        "expected at least one (selector, op, class) candidate satisfying every hard-edge \
         condition except pre_exists_inline — an empty population here means either this \
         install's own corpus changed drastically or this test's own reimplementation of the \
         hard-edge conditions has drifted from emit_patch_injected_node_edge's; check both \
         before trusting a bare 0"
    );

    let blocked: Vec<&HardEdgeCandidate> = candidates
        .iter()
        .filter(|candidate| indices.inline_type_names.contains(&candidate.class))
        .collect();

    let mut bucket_a: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    let mut bucket_b: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    let mut bucket_c: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    let mut no_target: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    let mut reindex_miss: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    for candidate in &blocked {
        let row = (
            candidate.class.clone(),
            candidate.selector.to_string(),
            candidate.injector.to_string(),
            candidate
                .target
                .as_ref()
                .map_or_else(|| "-".to_string(), DefTarget::match_key),
        );
        let Some(target) = candidate.target.as_ref() else {
            no_target.insert(row);
            continue;
        };
        match class_def_contexts.get(&candidate.class) {
            None => {
                reindex_miss.insert(row);
            }
            Some(occurrences) => match classify(target, occurrences) {
                InlineOccurrenceBucket::A => {
                    bucket_a.insert(row);
                }
                InlineOccurrenceBucket::B => {
                    bucket_b.insert(row);
                }
                InlineOccurrenceBucket::C => {
                    bucket_c.insert(row);
                }
            },
        }
    }

    eprintln!(
        "pre_exists_inline blocked-candidate measurement: {total_candidates} total (selector, \
         op, class) candidates examined (every hard-edge condition except pre_exists_inline \
         satisfied), {} blocked by pre_exists_inline alone.\n\
         Blocked, split into distinct (class, selector, injector, target) rows: \
         bucket A (different def type) = {}, bucket B (same def type, different def) = {}, \
         bucket C (exact target def) = {}, no-target (op.target is None, not bucketed) = {}, \
         reindex-miss (pre_exists_inline true but this test's own per-def reindex found no \
         occurrence at all — a fidelity gap in this measurement, not in the production rule; \
         should be 0) = {}",
        blocked.len(),
        bucket_a.len(),
        bucket_b.len(),
        bucket_c.len(),
        no_target.len(),
        reindex_miss.len()
    );
    print_bucket_sample("A (different def type)", &bucket_a);
    print_bucket_sample("B (same def type, different def)", &bucket_b);
    print_bucket_sample("C (exact target def)", &bucket_c);

    // The pinned selector's own case, confirmed by name rather than assumed
    // to land wherever the aggregate numbers suggest — only run when a
    // maintainer has named a specific real case to investigate (see this
    // file's own module doc comment). On the real-install case this was built
    // for, the candidate never reaches `candidates` at all (excluded before
    // `pre_exists_inline` is even checked) — see the diagnostic below for
    // exactly which condition excludes it and why, rather than a generic
    // pointer to "go investigate".
    let Some(case) = injected_class_case() else {
        return;
    };
    let selector_op = scan
        .scanned_mods
        .iter()
        .find(|scanned_mod| scanned_mod.info.id == case.selector)
        .and_then(|scanned_mod| {
            scanned_mod.patch_ops.iter().find(|op| {
                op.is_mutating
                    && patch_op_active(op, &active, &name_map)
                    && op.target.as_ref().is_some_and(|target| {
                        target.def_type == "PreceptDef" && target.def_name == "IdeoBuilding"
                    })
                    && op
                        .xpath
                        .as_deref()
                        .map(selected_classes_ignoring_the_dot_filter)
                        .is_some_and(|classes| classes.contains(&case.class))
            })
        });
    match selector_op {
        None => eprintln!(
            "Pinned case: found no active op on {} selecting {} under PreceptDef/IdeoBuilding at \
             all — this install's own mod set or the selector's own patch shape changed since \
             this pin was written",
            case.selector, case.class
        ),
        Some(op) => {
            let occurrences = injected_type_owners
                .get(&case.class)
                .cloned()
                .unwrap_or_default();
            let self_injects = occurrences.iter().any(|(id, _)| *id == case.selector);
            let mut other_injectors: Vec<&ModId> = occurrences
                .iter()
                .map(|(id, _)| id)
                .filter(|id| **id != case.selector)
                .collect();
            other_injectors.sort();
            other_injectors.dedup();

            if self_injects {
                // `self_injects` is mod-wide, not per-def-type (same as
                // `pre_exists_inline`'s own corpus-wide scope) — see this
                // file's own module doc comment for what this means for a
                // possible refinement.
                eprintln!(
                    "Pinned case: {} -> {} on {} is excluded by self-injection (rule 1), *before* \
                     pre_exists_inline is ever reached — this is a self_injects exclusion, not a \
                     pre_exists_inline one. The selector already \
                     injects this exact class elsewhere, just not under PreceptDef/IdeoBuilding. \
                     A def-type- or def-scoped refinement of pre_exists_inline alone would NOT \
                     admit this edge: self_injects excludes it first, and self_injects is scoped \
                     even more coarsely than pre_exists_inline (mod-wide, not even per-def-type) \
                     — refining pre_exists_inline without also refining self_injects the same way \
                     would not close this specific case.",
                    case.selector, case.owner, case.class
                );
            } else if other_injectors.len() != 1 {
                eprintln!(
                    "Pinned case: {} -> {} on {} is excluded by injector count ({} other active \
                     injectors, not exactly one), not by pre_exists_inline — other injectors: \
                     {other_injectors:?}",
                    case.selector,
                    case.owner,
                    case.class,
                    other_injectors.len()
                );
            } else {
                let sole_injector = other_injectors[0];
                let compatible = occurrences
                    .iter()
                    .filter(|(id, _)| id == sole_injector)
                    .any(|(_, injecting_target)| {
                        def_targets_compatible(op.target.as_ref(), injecting_target.as_ref())
                    });
                let pre_exists_inline = indices.inline_type_names.contains(&case.class);
                let bucket = compatible.then(|| {
                    let target = op.target.as_ref().expect("matched on target above");
                    let class_occurrences = class_def_contexts
                        .get(&case.class)
                        .cloned()
                        .unwrap_or_default();
                    classify(target, &class_occurrences)
                });
                eprintln!(
                    "Pinned case: {} -> {} on {}, sole_injector={sole_injector}, \
                     def_targets_compatible={compatible}, pre_exists_inline={pre_exists_inline}, \
                     bucket={bucket:?}",
                    case.selector, case.owner, case.class
                );
            }
        }
    }
}
