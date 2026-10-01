//! [`FsModKnowledgeStore`]: the real `rim_session::ports::ModKnowledgeStore`
//! — "the fetched `rimmergeRules.json` if it is present and parses, else
//! the bundled snapshot compiled into this binary".
//!
//! Four sections of mod-specific knowledge are data rather than this
//! workspace's source: which def type has a verified precedence rule, which
//! third-party patch-operation class names map onto which modelled
//! behaviour, which def-cache plugin DLL to look for, and which
//! `Player.log` formats mods (not the game) print (the log shapes). A
//! fifth section, tag rules, rides in the same fetched file but is
//! **deliberately not read here** — see [`ModKnowledgeFile::tag_rules`].
//!
//! **The bundled snapshot.** This data lives in a sibling repository,
//! [`rimmerge-rules`](https://github.com/Rimmerge-Project/rimmerge-rules)
//! (CC0), checked out as a git submodule at `rules/` in this workspace.
//! `build.rs` fails the build with a clear message when that checkout is
//! missing (`git submodule update --init`), and `EMBEDDED_RULES_BUNDLE`
//! `include_str!`s `rules/rimmerge-rules.json` straight into this binary
//! at compile time — the same single, already-concatenated envelope a
//! fetched `rimmergeRules.json` has, parsed by the identical code path.
//!
//! **Why an embedded snapshot exists at all.** A cached copy of this data
//! *can* change what `verify` predicts and what `assign coverage` names —
//! that is the whole point of fetching it. What the embedded snapshot buys
//! is the **no-cache case**: without it, a first run, an offline run, or a
//! run with `fetch_rimmerge_rules` off would silently be *worse* than a
//! binary with that knowledge built in (every custom class `Unsupported`,
//! no def-cache carrier detected). There is never a state with *no*
//! knowledge at all merely because no cache file exists on this machine —
//! only the embedded snapshot's own value standing in for a fresher
//! fetched one that isn't there. (The workspace's own determinism contract
//! — byte-identical output *across runs* — is separately unaffected either
//! way: the cache does not change between two runs of the same binary
//! against the same profile.)
//!
//! **Forward compatibility is a hard rule, at both layers — but the two
//! layers fall back to different things.** Every degradation is per-thing
//! and reported, never fatal, and never wider than it has to be:
//!
//! - a `behaviour`/`match`/`rule`/conditional value this binary does not
//!   implement -> that **row** is ignored, with a warning (a log-shape row
//!   also for an unknown role, placeholder type, oversized template or
//!   missing required capture);
//! - in a **fetched** file, a section that's missing or whose own shape
//!   doesn't parse -> that **section** falls back to the *embedded
//!   snapshot's* own value for it (a warning names it only when parsing
//!   actually failed, never for a section simply absent), and the
//!   other sections are untouched;
//! - in the **embedded snapshot** itself, a section that's missing or
//!   malformed -> that section falls back to **empty** instead, since
//!   there is nothing further underneath it to fall back to
//!   (see `vendored_sections`, below);
//! - an unknown top-level key -> ignored, with a warning naming it, so a
//!   misspelled section is visible instead of silently empty;
//! - a fetched file that can't even be read/parsed as JSON at all -> the
//!   whole file is set aside and the embedded snapshot is used in its
//!   place, with a warning (see [`FsModKnowledgeStore::load`]); the
//!   equivalent failure on the embedded bundle itself has no further
//!   fallback, so every section reads empty instead.
//!
//! A newer data file must never break an older binary, and an older
//! binary must never pretend it understood one. This applies equally to
//! the embedded snapshot and a fetched file — the embedded bundle is
//! just as forward-compatibility-tested as a fetched one, since the
//! `rules` repo's own release cadence is independent of this binary's.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rim_merge::patch_behaviours::{
    ClassBehaviour, ClassGate, ClassMatch, ConditionalKind, CustomBehaviour, GateBehaviour,
    GateFields, PatchOperationBehaviours,
};
use rim_resolve::domain::{PrecedenceRule, top_level_field};
use rim_session::ModKnowledge;
use rim_session::ports::{
    BackReferenceShape, DefCacheCarrier, LoadedModKnowledge, LogShapes, MAX_ROWS_PER_ROLE,
    ModKnowledgeStore, ModKnowledgeValueKind, PatchStackBlockShape, RoleFullError,
    RulesLoadWarning, ShapeError, StackBlockSource, TemplateError, TextureFallbackShape,
    TextureFallbackSource,
};
use serde::{Deserialize, Serialize};

use crate::game_log::{
    ShapeCompileError, check_back_reference, check_stack_block, check_texture_fallback,
};

/// The `rules` git submodule's own built envelope, embedded verbatim.
/// `build.rs` guarantees this path exists (or fails the build first with
/// a clear message), and the `rules` repo's own CI guarantees its shape:
/// `schema` plus the five section keys this module knows.
const EMBEDDED_RULES_BUNDLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../rules/rimmerge-rules.json"
));

// -- wire shapes ----------------------------------------------------------

/// The fetched file: one envelope over the five sections the rules repo
/// authors separately and its own CI concatenates (one URL, one sha, one
/// cache file, one settings toggle, one Databases-card row).
///
/// **Each section is held as a raw [`serde_json::Value`], not as its own
/// typed shape**: typing them here would make *one*
/// malformed row anywhere in the file fail the whole deserialization,
/// discarding the good sections along with the bad one. Each is instead
/// parsed on its own by [`cached_section`], so a bad section falls back
/// exactly as a missing one does, with a warning naming it, and the
/// others are untouched. This same struct is also what
/// `EMBEDDED_RULES_BUNDLE` itself parses into.
///
/// `schema` is **required**, and [`unknown`](Self::unknown) catches every
/// other top-level key — that is what stops an arbitrary JSON
/// object (`{"hello": "world"}`) validating as a rules file and getting
/// cached, and what makes a misspelled section name visible as a warning
/// instead of silently reading as "this section is absent".
/// `deny_unknown_fields` is deliberately **not** used: an unknown key
/// must be a warning, never an error, or a newer data file breaks an
/// older binary.
#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct ModKnowledgeFile {
    /// The envelope's own version. Required: its absence is what tells a
    /// rules file apart from any other JSON object.
    schema: u32,
    /// Which def type has a verified precedence rule.
    #[serde(default)]
    precedence: Option<serde_json::Value>,
    /// Which custom patch-operation classes map onto which behaviour.
    #[serde(default)]
    patch_operations: Option<serde_json::Value>,
    /// Which def-cache plugins to detect.
    #[serde(default)]
    def_cache_carriers: Option<serde_json::Value>,
    /// Which `Player.log` formats mods print.
    #[serde(default)]
    log_shapes: Option<serde_json::Value>,
    /// **Never read into [`ModKnowledge`].** Tag rules feed
    /// cluster/placement rules, so a tag rule can move a mod; reading one
    /// straight out of the cache would break the guarantee that no sort
    /// path touches the network or the cache — this crate parses it
    /// (so a malformed section is still caught) and then discards it.
    /// Unlike the RimSort-sourced databases, there is currently no
    /// import path from *this* section into a profile's `rules.json`
    /// either; carried here only so the key is *known* (an unknown one
    /// would warn, per [`unknown`](Self::unknown)).
    #[serde(default)]
    tag_rules: Option<serde_json::Value>,
    /// Every top-level key this binary does not know — one warning each,
    /// then ignored. See this struct's own doc comment.
    #[serde(flatten)]
    unknown: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct PrecedenceSection {
    #[serde(default)]
    rules: BTreeMap<String, PrecedenceRow>,
}

/// One precedence row. `rule` mirrors [`PrecedenceRule`]'s own
/// `#[serde(tag = "rule", rename_all = "snake_case")]` discriminants, but
/// is read as a plain string here so an unknown discriminant is a warning
/// rather than a whole-file parse failure. `key_priority` entries are
/// plain top-level tag names, built into single-segment `FieldPath`s by
/// [`rim_resolve::domain::top_level_field`] — the same shape the code
/// always used. `evidence` is required by the rules repo's PR template
/// and ignored here.
#[derive(Debug, Deserialize, Serialize)]
struct PrecedenceRow {
    rule: String,
    #[serde(default)]
    framework: Option<String>,
    #[serde(default)]
    key_priority: Vec<String>,
    #[serde(default)]
    evidence: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct PatchOperationsSection {
    #[serde(default)]
    classes: Vec<ClassRow>,
    #[serde(default)]
    gates: Vec<GateRow>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ClassRow {
    class: String,
    #[serde(rename = "match")]
    match_kind: String,
    behaviour: String,
    #[serde(default)]
    evidence: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GateRow {
    class: String,
    #[serde(rename = "match")]
    match_kind: String,
    behaviour: String,
    fields: GateFieldsRow,
    #[serde(default)]
    conditional_types: BTreeMap<String, String>,
    #[serde(default)]
    evidence: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GateFieldsRow {
    requires_all: String,
    conditional_type: String,
    conditional_param: String,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct DefCacheCarriersSection {
    #[serde(default)]
    carriers: Vec<CarrierRow>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CarrierRow {
    id: String,
    plugin_dir: String,
    file_prefix: String,
    file_extension: String,
    /// The only matching mode this binary implements. Any other value is
    /// ignored with a warning — a newer file may add one.
    #[serde(rename = "match")]
    match_kind: String,
    #[serde(default)]
    recursive: bool,
    log_line_prefix: String,
    /// Free text for the rules repo's own PR template; ignored here.
    #[serde(default)]
    evidence: Option<String>,
}

/// The matching mode [`CarrierRow::match_kind`] must name for the row to
/// be used. Structural, not mod-specific: it describes how to compare a
/// file name, which is a fact about filesystems, not about any mod.
const CARRIER_MATCH_CASE_INSENSITIVE_PREFIX: &str = "case_insensitive_prefix";

/// The `log_shapes` section: one array per role. A role this binary does not
/// know lands in [`unknown`](Self::unknown) and is reported, never an error.
#[derive(Debug, Default, Deserialize, Serialize)]
struct LogShapesSection {
    /// The section's own version; unused, declared so it is not reported as
    /// an unknown role.
    #[serde(default)]
    schema: Option<u32>,
    #[serde(default)]
    patch_stack_blocks: Vec<StackBlockRow>,
    #[serde(default)]
    texture_fallbacks: Vec<TextureFallbackRow>,
    #[serde(default)]
    stack_back_references: Vec<BackReferenceRow>,
    /// Every role this binary does not know.
    #[serde(flatten)]
    unknown: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
struct StackBlockRow {
    id: String,
    /// The only recogniser language this binary implements.
    #[serde(rename = "match")]
    match_kind: String,
    start: String,
    end: String,
    trailer: String,
    xpath_detail_prefix: String,
    branch_markers: BranchMarkersRow,
    /// Free text for the rules repo's own PR template; ignored here.
    #[serde(default)]
    evidence: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BranchMarkersRow {
    #[serde(rename = "match")]
    matched: String,
    nomatch: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TextureFallbackRow {
    id: String,
    #[serde(rename = "match")]
    match_kind: String,
    head: String,
    dimensions: String,
    trailer: String,
    #[serde(default)]
    evidence: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct BackReferenceRow {
    id: String,
    #[serde(rename = "match")]
    match_kind: String,
    original: String,
    stub: String,
    #[serde(default)]
    evidence: Option<String>,
}

/// The recogniser language every log-shape row's `match` must name for the
/// row to be used.
const LOG_SHAPE_MATCH_TEMPLATE: &str = "template";

#[derive(Debug, Default, Deserialize, Serialize)]
struct TagRulesSection {
    #[serde(default)]
    rules: Vec<rim_resolve::domain::TagRule>,
}

// -- parsing --------------------------------------------------------------

/// Collects warnings while parsing, so an unknown value never aborts a
/// load.
#[derive(Debug, Default)]
struct Warnings(Vec<RulesLoadWarning>);

impl Warnings {
    fn unknown(&mut self, section: &str, what: ModKnowledgeValueKind, value: &str) {
        self.0.push(RulesLoadWarning::UnknownModKnowledgeValue {
            section: section.to_string(),
            what,
            value: value.to_string(),
        });
    }

    fn rows_over_role_limit(&mut self, section: &str, role: &str, ignored: usize) {
        self.0
            .push(RulesLoadWarning::ModKnowledgeRowsOverRoleLimit {
                section: section.to_string(),
                role: role.to_string(),
                ignored,
            });
    }

    fn unreadable(&mut self, reason: String) {
        self.0
            .push(RulesLoadWarning::ModKnowledgeCacheUnreadable(reason));
    }
}

fn parse_precedence(
    section: &PrecedenceSection,
    warnings: &mut Warnings,
) -> BTreeMap<String, PrecedenceRule> {
    let mut rules = BTreeMap::new();
    for (def_type, row) in &section.rules {
        match row.rule.as_str() {
            "unverified" => {
                rules.insert(def_type.clone(), PrecedenceRule::Unverified);
            }
            "prefer_outside_framework" => {
                let Some(framework) = row.framework.as_deref() else {
                    warnings
                        .0
                        .push(RulesLoadWarning::PrecedenceRuleMissingFramework {
                            def_type: def_type.clone(),
                        });
                    continue;
                };
                rules.insert(
                    def_type.clone(),
                    PrecedenceRule::PreferOutsideFramework {
                        framework: rim_analyzer::domain::ModId::new(framework),
                        key_priority: row
                            .key_priority
                            .iter()
                            .map(|tag| top_level_field(tag))
                            .collect(),
                    },
                );
            }
            other => warnings.unknown("precedence", ModKnowledgeValueKind::PrecedenceRule, other),
        }
    }
    rules
}

fn parse_patch_operations(
    section: &PatchOperationsSection,
    warnings: &mut Warnings,
) -> PatchOperationBehaviours {
    let mut classes = BTreeMap::new();
    for row in &section.classes {
        let Some(match_kind) = ClassMatch::from_wire(&row.match_kind) else {
            warnings.unknown(
                "patch-operations",
                ModKnowledgeValueKind::MatchMode,
                &row.match_kind,
            );
            continue;
        };
        let Some(behaviour) = CustomBehaviour::from_wire(&row.behaviour) else {
            warnings.unknown(
                "patch-operations",
                ModKnowledgeValueKind::Behaviour,
                &row.behaviour,
            );
            continue;
        };
        classes.insert(
            row.class.clone(),
            ClassBehaviour {
                match_kind,
                behaviour,
            },
        );
    }

    let mut gates = Vec::new();
    for row in &section.gates {
        let Some(match_kind) = ClassMatch::from_wire(&row.match_kind) else {
            warnings.unknown(
                "patch-operations",
                ModKnowledgeValueKind::GateMatchMode,
                &row.match_kind,
            );
            continue;
        };
        let Some(behaviour) = GateBehaviour::from_wire(&row.behaviour) else {
            warnings.unknown(
                "patch-operations",
                ModKnowledgeValueKind::GateBehaviour,
                &row.behaviour,
            );
            continue;
        };
        let mut conditional_types = BTreeMap::new();
        for (declared, kind) in &row.conditional_types {
            match ConditionalKind::from_wire(kind) {
                Some(kind) => {
                    conditional_types.insert(declared.clone(), kind);
                }
                None => warnings.unknown(
                    "patch-operations",
                    ModKnowledgeValueKind::ConditionalType,
                    kind,
                ),
            }
        }
        gates.push(ClassGate {
            class: row.class.clone(),
            match_kind,
            behaviour,
            fields: GateFields {
                requires_all: row.fields.requires_all.clone(),
                conditional_type: row.fields.conditional_type.clone(),
                conditional_param: row.fields.conditional_param.clone(),
            },
            conditional_types,
        });
    }

    PatchOperationBehaviours::new(classes, gates)
}

fn parse_carriers(
    section: &DefCacheCarriersSection,
    warnings: &mut Warnings,
) -> Vec<DefCacheCarrier> {
    section
        .carriers
        .iter()
        .filter_map(|row| {
            if row.match_kind != CARRIER_MATCH_CASE_INSENSITIVE_PREFIX {
                warnings.unknown(
                    "def-cache-carriers",
                    ModKnowledgeValueKind::MatchMode,
                    &row.match_kind,
                );
                return None;
            }
            Some(DefCacheCarrier {
                id: row.id.clone(),
                plugin_dir: row.plugin_dir.clone(),
                file_prefix: row.file_prefix.clone(),
                file_extension: row.file_extension.clone(),
                recursive: row.recursive,
                log_line_prefix: row.log_line_prefix.clone(),
            })
        })
        .collect()
}

const LOG_SHAPES_SECTION: &str = "log-shapes";

/// Warns about a row ignored because its recogniser language is not one this
/// binary implements. `true` when the row may be used.
fn is_template_match(match_kind: &str, warnings: &mut Warnings) -> bool {
    if match_kind == LOG_SHAPE_MATCH_TEMPLATE {
        return true;
    }
    warnings.unknown(
        LOG_SHAPES_SECTION,
        ModKnowledgeValueKind::MatchMode,
        match_kind,
    );
    false
}

/// Warns about a row that failed to parse into a shape.
fn warn_shape_error(error: &ShapeError, row_id: &str, warnings: &mut Warnings) {
    let (kind, value) = match error {
        ShapeError::Template {
            error: TemplateError::UnknownPlaceholderType(kind),
            ..
        } => (ModKnowledgeValueKind::PlaceholderType, kind.as_str()),
        ShapeError::Template { .. } => (ModKnowledgeValueKind::TemplateInvalid, row_id),
        ShapeError::MissingCapture { .. } => (ModKnowledgeValueKind::CaptureMissing, row_id),
        ShapeError::MarkerLength { .. } => (ModKnowledgeValueKind::MarkerLength, row_id),
        // The id is the thing that is too long: never echo it.
        ShapeError::IdTooLong => (ModKnowledgeValueKind::IdTooLong, "id"),
    };
    warnings.unknown(LOG_SHAPES_SECTION, kind, value);
}

fn warn_uncompilable(error: &ShapeCompileError, row_id: &str, warnings: &mut Warnings) {
    let kind = match error {
        ShapeCompileError::Regex(_) => ModKnowledgeValueKind::TemplateUncompilable,
        ShapeCompileError::MissingCapture(_) => ModKnowledgeValueKind::CaptureMissing,
    };
    warnings.unknown(LOG_SHAPES_SECTION, kind, row_id);
}

/// The rows of one role that fit [`MAX_ROWS_PER_ROLE`]. Rows beyond the limit
/// are never parsed or compiled; they produce one aggregated warning, so a
/// hostile file cannot make the loader spend time or memory per extra row.
fn within_role_limit<'rows, Row>(
    role: &str,
    rows: &'rows [Row],
    warnings: &mut Warnings,
) -> &'rows [Row] {
    let Some((kept, ignored)) = rows
        .split_at_checked(MAX_ROWS_PER_ROLE)
        .filter(|(_, ignored)| !ignored.is_empty())
    else {
        return rows;
    };
    warnings.rows_over_role_limit(LOG_SHAPES_SECTION, role, ignored.len());
    kept
}

/// Every usable row of the section. A role's rows beyond the per-role limit
/// are ignored with one warning for the role. Of the rest, a row is ignored,
/// with one warning, when its `match` mode is not `template`, its id or a
/// template is out of bounds or a template lacks a capture its role needs, or
/// it does not compile within the regex size limits. A role this binary does
/// not know is reported once. Everything else loads.
fn parse_log_shapes(section: &LogShapesSection, warnings: &mut Warnings) -> LogShapes {
    for role in section.unknown.keys() {
        warnings.unknown(
            LOG_SHAPES_SECTION,
            ModKnowledgeValueKind::LogShapeRole,
            role,
        );
    }
    let mut shapes = LogShapes::empty();
    let stack_blocks =
        within_role_limit("patch_stack_blocks", &section.patch_stack_blocks, warnings);
    for row in stack_blocks {
        if let Some(shape) = usable_stack_block(row, warnings) {
            push_within_limit(
                shapes.push_stack_block(shape),
                "patch_stack_blocks",
                warnings,
            );
        }
    }
    let fallbacks = within_role_limit("texture_fallbacks", &section.texture_fallbacks, warnings);
    for row in fallbacks {
        if let Some(shape) = usable_texture_fallback(row, warnings) {
            push_within_limit(
                shapes.push_texture_fallback(shape),
                "texture_fallbacks",
                warnings,
            );
        }
    }
    let references = within_role_limit(
        "stack_back_references",
        &section.stack_back_references,
        warnings,
    );
    for row in references {
        if let Some(shape) = usable_back_reference(row, warnings) {
            push_within_limit(
                shapes.push_back_reference(shape),
                "stack_back_references",
                warnings,
            );
        }
    }
    shapes
}

/// Reports a push the role refused. `within_role_limit` already caps every
/// role, so this is a second line of defence, not an expected path.
fn push_within_limit(pushed: Result<(), RoleFullError>, role: &str, warnings: &mut Warnings) {
    if pushed.is_err() {
        warnings.rows_over_role_limit(LOG_SHAPES_SECTION, role, 1);
    }
}

fn usable_stack_block(
    row: &StackBlockRow,
    warnings: &mut Warnings,
) -> Option<PatchStackBlockShape> {
    if !is_template_match(&row.match_kind, warnings) {
        return None;
    }
    let parsed = PatchStackBlockShape::parse(&StackBlockSource {
        id: &row.id,
        start: &row.start,
        end: &row.end,
        trailer: &row.trailer,
        xpath_detail_prefix: &row.xpath_detail_prefix,
        match_marker: &row.branch_markers.matched,
        nomatch_marker: &row.branch_markers.nomatch,
    });
    let shape = parsed
        .inspect_err(|error| warn_shape_error(error, &row.id, warnings))
        .ok()?;
    check_stack_block(&shape)
        .inspect_err(|error| warn_uncompilable(error, &row.id, warnings))
        .ok()?;
    Some(shape)
}

fn usable_texture_fallback(
    row: &TextureFallbackRow,
    warnings: &mut Warnings,
) -> Option<TextureFallbackShape> {
    if !is_template_match(&row.match_kind, warnings) {
        return None;
    }
    let parsed = TextureFallbackShape::parse(&TextureFallbackSource {
        id: &row.id,
        head: &row.head,
        dimensions: &row.dimensions,
        trailer: &row.trailer,
    });
    let shape = parsed
        .inspect_err(|error| warn_shape_error(error, &row.id, warnings))
        .ok()?;
    check_texture_fallback(&shape)
        .inspect_err(|error| warn_uncompilable(error, &row.id, warnings))
        .ok()?;
    Some(shape)
}

fn usable_back_reference(
    row: &BackReferenceRow,
    warnings: &mut Warnings,
) -> Option<BackReferenceShape> {
    if !is_template_match(&row.match_kind, warnings) {
        return None;
    }
    let shape = BackReferenceShape::parse(&row.id, &row.original, &row.stub)
        .inspect_err(|error| warn_shape_error(error, &row.id, warnings))
        .ok()?;
    check_back_reference(&shape)
        .inspect_err(|error| warn_uncompilable(error, &row.id, warnings))
        .ok()?;
    Some(shape)
}

/// One section, parsed on its own: `None` (the section is absent) and a
/// section whose value fails to parse are the *same* outcome as far as
/// this function is concerned — a caller decides what `None` falls back
/// to (empty, in [`vendored_sections`]; the embedded snapshot's own
/// value, in [`knowledge_from`]) — differing only in that a parse
/// failure also pushes a warning naming the section. Neither can touch
/// the other sections.
fn cached_section<T: serde::de::DeserializeOwned>(
    value: Option<&serde_json::Value>,
    section: &str,
    warnings: &mut Warnings,
) -> Option<T> {
    let value = value?;
    match serde_json::from_value(value.clone()) {
        Ok(parsed) => Some(parsed),
        Err(error) => {
            warnings.unreadable(format!("{section} section: {error}"));
            None
        }
    }
}

/// The embedded snapshot's own sha256, hex-encoded — `build.rs` hashes
/// `EMBEDDED_RULES_BUNDLE` at compile time (the same digest a fetched
/// copy of the identical bytes would record in the cache manifest), so
/// a caller can show it next to a fetched cache's own sha256 without
/// hashing a multi-KB string at runtime on every call.
#[must_use]
pub fn embedded_bundle_sha256() -> &'static str {
    env!("RIMMERGE_RULES_BUNDLE_SHA256")
}

/// The embedded snapshot alone — no cache read. The exact behaviour a
/// first run, an offline run, or a run with `fetch_rimmerge_rules` off
/// gets.
#[must_use]
pub fn vendored_knowledge() -> LoadedModKnowledge {
    let mut warnings = Warnings::default();
    let loaded = vendored_sections(&mut warnings);

    LoadedModKnowledge {
        knowledge: loaded,
        warnings: warnings.0,
    }
}

/// [`vendored_knowledge`]'s own body, sharing one [`Warnings`] with a
/// caller that also has cached sections to fold in.
///
/// Parses `EMBEDDED_RULES_BUNDLE` the same way [`knowledge_from`]
/// parses a fetched file — a malformed embedded envelope (which `build.rs`
/// and the `rules` repo's own CI both make practically unreachable, but
/// this code never *assumes* that) degrades to every section reading
/// empty, with one warning, rather than panicking the binary.
fn vendored_sections(warnings: &mut Warnings) -> ModKnowledge {
    let file: ModKnowledgeFile = match serde_json::from_str(EMBEDDED_RULES_BUNDLE) {
        Ok(file) => file,
        Err(error) => {
            warnings.unreadable(format!("embedded rules bundle: {error}"));
            ModKnowledgeFile::default()
        }
    };
    for key in file.unknown.keys() {
        warnings.unknown(
            "rimmerge-rules",
            ModKnowledgeValueKind::TopLevelSection,
            key,
        );
    }

    let precedence =
        cached_section::<PrecedenceSection>(file.precedence.as_ref(), "precedence", warnings)
            .unwrap_or_default();
    let patch_operations = cached_section::<PatchOperationsSection>(
        file.patch_operations.as_ref(),
        "patch-operations",
        warnings,
    )
    .unwrap_or_default();
    let carriers = cached_section::<DefCacheCarriersSection>(
        file.def_cache_carriers.as_ref(),
        "def-cache-carriers",
        warnings,
    )
    .unwrap_or_default();
    let log_shapes =
        cached_section::<LogShapesSection>(file.log_shapes.as_ref(), LOG_SHAPES_SECTION, warnings)
            .unwrap_or_default();
    // Parsed and discarded: the embedded tag rules are not read into
    // `ModKnowledge` (see `ModKnowledgeFile::tag_rules`), but a malformed
    // one must still be caught here, not silently ignored.
    let _tag_rules =
        cached_section::<TagRulesSection>(file.tag_rules.as_ref(), "tag-rules", warnings)
            .unwrap_or_default();

    ModKnowledge::new(
        parse_precedence(&precedence, warnings),
        parse_patch_operations(&patch_operations, warnings),
        parse_carriers(&carriers, warnings),
        parse_log_shapes(&log_shapes, warnings),
    )
}

/// The embedded snapshot with every section the cached file supplies
/// *and* parses overriding its own embedded counterpart — per section,
/// independently. Also parses (and discards) `tag_rules`, purely so a
/// malformed cached copy of that section is reported too — see
/// [`ModKnowledgeFile::tag_rules`] for why it never reaches the result.
fn knowledge_from(file: &ModKnowledgeFile) -> LoadedModKnowledge {
    let mut warnings = Warnings::default();
    let vendored = vendored_sections(&mut warnings);

    // A key this binary doesn't know is reported, never
    // silently swallowed — a misspelled `def_cache_carrier` would
    // otherwise read exactly like "this file ships no carriers".
    for key in file.unknown.keys() {
        warnings.unknown(
            "rimmerge-rules",
            ModKnowledgeValueKind::TopLevelSection,
            key,
        );
    }

    let precedence =
        cached_section::<PrecedenceSection>(file.precedence.as_ref(), "precedence", &mut warnings)
            .map_or_else(
                || vendored.precedence().clone(),
                |section| parse_precedence(&section, &mut warnings),
            );
    let patch_operations = cached_section::<PatchOperationsSection>(
        file.patch_operations.as_ref(),
        "patch-operations",
        &mut warnings,
    )
    .map_or_else(
        || vendored.patch_operations().clone(),
        |section| parse_patch_operations(&section, &mut warnings),
    );
    let carriers = cached_section::<DefCacheCarriersSection>(
        file.def_cache_carriers.as_ref(),
        "def-cache-carriers",
        &mut warnings,
    )
    .map_or_else(
        || vendored.def_cache_carriers().to_vec(),
        |section| parse_carriers(&section, &mut warnings),
    );
    let log_shapes = cached_section::<LogShapesSection>(
        file.log_shapes.as_ref(),
        LOG_SHAPES_SECTION,
        &mut warnings,
    )
    .map_or_else(
        || vendored.log_shapes().clone(),
        |section| parse_log_shapes(&section, &mut warnings),
    );
    // Tag rules are parsed for validity here too — never read into
    // `ModKnowledge` (see `ModKnowledgeFile::tag_rules`), but a malformed
    // cached copy is still reported at load time rather than silently
    // accepted with no diagnostic at all, mirroring `vendored_sections`'
    // identical treatment of the embedded bundle's own tag_rules.
    let _ = cached_section::<TagRulesSection>(file.tag_rules.as_ref(), "tag-rules", &mut warnings);

    LoadedModKnowledge {
        knowledge: ModKnowledge::new(precedence, patch_operations, carriers, log_shapes),
        warnings: warnings.0,
    }
}

/// Whether `bytes` are a rules file at all — the cache's own
/// parse-before-commit check.
///
/// This checks the **envelope**, not the sections: `schema` must be
/// present and every known section must be a JSON value, which is what
/// rejects an arbitrary object such as `{"hello": "world"}`.
/// A section whose own *contents* are malformed still validates here on
/// purpose — it degrades to its vendored default at load time, with a
/// warning, and refusing to cache the whole file over one bad row would
/// be exactly the "a newer file breaks an older binary" failure this
/// design forbids.
pub(crate) fn parses(bytes: &[u8]) -> bool {
    serde_json::from_slice::<ModKnowledgeFile>(bytes).is_ok()
}

// -- the store ------------------------------------------------------------

/// Real [`ModKnowledgeStore`], reading
/// `<cache_dir>/`[`RIMMERGE_RULES_FILE`](crate::databases::RIMMERGE_RULES_FILE)
/// when the source is enabled and the file exists, and falling back to
/// the vendored defaults otherwise.
///
/// The cache directory is held by the adapter, not passed per call, so
/// `rim-session`'s own port keeps a narrow `load(source_enabled)` — the
/// session layer has no cache-directory concept at all (the cache is
/// app-global, the profile is not), only the `Settings` toggle.
#[derive(Debug, Clone)]
pub struct FsModKnowledgeStore {
    cache_dir: Option<PathBuf>,
}

impl FsModKnowledgeStore {
    /// Reads `<cache_dir>/rimmergeRules.json` when
    /// `Settings::fetch_rimmerge_rules` is on and the file is there.
    #[must_use]
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: Some(cache_dir.into()),
        }
    }

    /// The vendored defaults only — no cache is ever consulted, whatever
    /// the toggle says. For a caller with no cache directory in hand (a
    /// test, or a tool that deliberately wants the shipped behaviour).
    #[must_use]
    pub fn vendored() -> Self {
        Self { cache_dir: None }
    }

    fn cache_path(&self) -> Option<PathBuf> {
        self.cache_dir
            .as_ref()
            .map(|dir| dir.join(crate::databases::RIMMERGE_RULES_FILE))
    }
}

impl ModKnowledgeStore for FsModKnowledgeStore {
    fn load(&self, source_enabled: bool) -> LoadedModKnowledge {
        // The toggle gates *consumption*, not just fetching — the same
        // rule `should_import_from_cache` applies to the other two
        // sources. Reading a cached copy with `fetch_rimmerge_rules` off
        // would contradict both that setting's own doc comment and every
        // other source's behaviour. Off means the vendored defaults, full stop.
        if !source_enabled {
            return vendored_knowledge();
        }
        let Some(path) = self.cache_path() else {
            return vendored_knowledge();
        };
        if !path.is_file() {
            return vendored_knowledge();
        }
        match read_file(&path) {
            Ok(file) => knowledge_from(&file),
            Err(reason) => {
                let mut loaded = vendored_knowledge();
                loaded
                    .warnings
                    .push(RulesLoadWarning::ModKnowledgeCacheUnreadable(reason));
                loaded
            }
        }
    }
}

fn read_file(path: &Path) -> Result<ModKnowledgeFile, String> {
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use rim_merge::patch_behaviours::CustomBehaviour;
    use tempfile::tempdir;

    use super::*;

    /// `Settings::fetch_rimmerge_rules` as these tests pass it: on,
    /// except where a test is specifically about the toggle being off.
    const ENABLED: bool = true;

    fn write_cache(dir: &Path, json: &str) {
        fs::write(dir.join(crate::databases::RIMMERGE_RULES_FILE), json)
            .expect("write the cache file");
    }

    /// The one contract test allowed to read the real embedded bundle:
    /// it parses with zero warnings, and every section the `rules` repo
    /// authors separately made it into the envelope. Deliberately
    /// **not** an assertion on any section's specific content (a class
    /// name, a def type, a carrier id) — that content is the `rules`
    /// repo's own to change, on its own release cadence, and a test
    /// here pinning it would fail on every such change whether or not
    /// this binary's own parsing logic regressed. Behaviour (class ->
    /// behaviour mapping, carrier detection, gates, fallback) is
    /// exercised by this module's other tests below, every one of them
    /// against synthetic in-test JSON.
    #[test]
    fn the_embedded_rules_bundle_parses_with_zero_warnings_and_every_section_present() {
        let file: ModKnowledgeFile = serde_json::from_str(EMBEDDED_RULES_BUNDLE)
            .expect("the embedded bundle must be a valid envelope");
        assert!(file.precedence.is_some(), "precedence section missing");
        assert!(
            file.patch_operations.is_some(),
            "patch_operations section missing"
        );
        assert!(
            file.def_cache_carriers.is_some(),
            "def_cache_carriers section missing"
        );
        assert!(file.tag_rules.is_some(), "tag_rules section missing");
        assert!(file.log_shapes.is_some(), "log_shapes section missing");
        assert!(
            file.unknown.is_empty(),
            "unknown top-level keys: {:?}",
            file.unknown
        );

        let loaded = vendored_knowledge();
        assert!(
            loaded.warnings.is_empty(),
            "the embedded bundle must parse cleanly, with nothing ignored: {:?}",
            loaded.warnings
        );
    }

    #[test]
    fn a_missing_cache_file_yields_exactly_the_vendored_defaults() {
        let dir = tempdir().expect("tempdir");
        let store = FsModKnowledgeStore::new(dir.path());

        let loaded = store.load(ENABLED);

        assert_eq!(loaded.knowledge, vendored_knowledge().knowledge);
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    fn a_cached_section_replaces_its_own_vendored_counterpart_and_leaves_the_others() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{
                 "schema": 1,
                 "precedence": {
                   "schema": 1,
                   "rules": {
                     "example.PartAssignmentDef": {
                       "rule": "prefer_outside_framework",
                       "framework": "example.framework",
                       "key_priority": ["kindNames", "speciesNames"],
                       "evidence": { "source": "invented", "verified": "2026-09-13" }
                     }
                   }
                 }
               }"#,
        );

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert!(matches!(
            loaded.knowledge.precedence_for("example.PartAssignmentDef"),
            PrecedenceRule::PreferOutsideFramework { .. }
        ));
        // The sections the file left out still come from the embedded
        // snapshot, not from nothing.
        let vendored = vendored_knowledge().knowledge;
        assert_eq!(
            loaded.knowledge.patch_operations(),
            vendored.patch_operations()
        );
        assert_eq!(
            loaded.knowledge.def_cache_carriers(),
            vendored.def_cache_carriers()
        );
    }

    /// The forward-compatibility rule: a newer data file must never break
    /// an older binary.
    #[test]
    fn an_unknown_behaviour_is_ignored_with_a_warning_and_the_rest_still_loads() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{
                 "schema": 1,
                 "patch_operations": {
                   "schema": 1,
                   "classes": [
                     { "class": "Future.PatchOperationTeleport", "match": "suffix",
                       "behaviour": "teleport_the_pawn" },
                     { "class": "Example.PatchOperationAddOrReplace", "match": "suffix",
                       "behaviour": "add_or_replace" }
                   ],
                   "gates": []
                 }
               }"#,
        );

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        assert_eq!(
            loaded
                .knowledge
                .patch_operations()
                .behaviour_for("Example.PatchOperationAddOrReplace"),
            Some(CustomBehaviour::AddOrReplace),
            "the row this binary understands must still load"
        );
        assert_eq!(
            loaded
                .knowledge
                .patch_operations()
                .behaviour_for("Future.PatchOperationTeleport"),
            None
        );
        assert!(
            loaded.warnings.iter().any(|warning| {
                matches!(warning, RulesLoadWarning::UnknownModKnowledgeValue { value, .. }
                    if value == "teleport_the_pawn")
            }),
            "and the ignored row must be reported: {:?}",
            loaded.warnings
        );
    }

    #[test]
    fn an_unparseable_cache_file_falls_back_to_the_vendored_defaults_with_a_warning() {
        let dir = tempdir().expect("tempdir");
        write_cache(dir.path(), "{ not json at all");

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        assert_eq!(loaded.knowledge, vendored_knowledge().knowledge);
        assert!(
            loaded
                .warnings
                .iter()
                .any(|w| matches!(w, RulesLoadWarning::ModKnowledgeCacheUnreadable(_))),
            "{:?}",
            loaded.warnings
        );
    }

    /// Tag rules ride in the same fetched file but must never reach
    /// [`ModKnowledge`] — the sorter reads the profile, never the cache.
    /// They're still parsed for validity, same as the other
    /// sections (see the next test for what a malformed one does).
    #[test]
    fn a_cached_tag_rules_section_is_parsed_for_validity_but_never_read_into_knowledge() {
        let dir = tempdir().expect("tempdir");
        let json = r#"{
                 "schema": 1,
                 "tag_rules": {
                   "schema": 1,
                   "rules": [
                     { "tag": "framework",
                       "any_of": [ { "kind": "url_contains", "value": "mods.example" } ] }
                   ]
                 }
               }"#;
        write_cache(dir.path(), json);

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        assert!(parses(json.as_bytes()), "the section must be well-formed");
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        // Nothing about tag rules is reachable from `ModKnowledge` at all
        // — there is no accessor for them, by design; this asserts the
        // rest of the load is untouched by their presence.
        assert_eq!(loaded.knowledge, vendored_knowledge().knowledge);
    }

    /// The teeth on the test above: a cached `tag_rules` section that
    /// doesn't even match `TagRulesSection`'s own shape is still caught
    /// and reported at load time, exactly like the other sections
    /// — even though nothing ever reads the parsed result into
    /// `ModKnowledge`. Without `knowledge_from`'s own `cached_section`
    /// call on `file.tag_rules`, this case would pass through with no
    /// warning at all, since `ModKnowledgeFile::tag_rules` is a bare
    /// `Option<serde_json::Value>` that accepts almost anything.
    #[test]
    fn a_malformed_cached_tag_rules_section_is_reported_even_though_never_read() {
        let dir = tempdir().expect("tempdir");
        let json = r#"{
                 "schema": 1,
                 "tag_rules": { "schema": 1, "rules": "not an array" }
               }"#;
        write_cache(dir.path(), json);

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        assert!(
            loaded.warnings.iter().any(|warning| matches!(warning,
                RulesLoadWarning::ModKnowledgeCacheUnreadable(reason)
                    if reason.contains("tag-rules")
            )),
            "a malformed tag-rules section must be reported: {:?}",
            loaded.warnings
        );
        // Still never reaches `ModKnowledge`, and the other sections are
        // untouched by the bad one.
        assert_eq!(loaded.knowledge, vendored_knowledge().knowledge);
    }

    #[test]
    fn a_carrier_with_an_unimplemented_match_mode_is_ignored_with_a_warning() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{
                 "schema": 1,
                 "def_cache_carriers": {
                   "schema": 1,
                   "carriers": [
                     { "id": "future", "plugin_dir": "Plugins", "file_prefix": "X",
                       "file_extension": ".dll", "match": "regex", "recursive": true,
                       "log_line_prefix": "X:" }
                   ]
                 }
               }"#,
        );

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        assert!(loaded.knowledge.def_cache_carriers().is_empty());
        assert!(
            loaded.warnings.iter().any(|warning| {
                matches!(warning, RulesLoadWarning::UnknownModKnowledgeValue { value, .. }
                    if value == "regex")
            }),
            "{:?}",
            loaded.warnings
        );
    }

    #[test]
    fn a_framework_precedence_rule_without_a_framework_is_ignored_with_its_own_warning() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{
                 "schema": 1,
                 "precedence": {
                   "schema": 1,
                   "rules": {
                     "example.PartAssignmentDef": { "rule": "prefer_outside_framework" }
                   }
                 }
               }"#,
        );

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        assert_eq!(
            loaded.knowledge.precedence_for("example.PartAssignmentDef"),
            PrecedenceRule::Unverified
        );
        assert!(
            loaded
                .warnings
                .contains(&RulesLoadWarning::PrecedenceRuleMissingFramework {
                    def_type: "example.PartAssignmentDef".to_string()
                }),
            "{:?}",
            loaded.warnings
        );
    }

    #[test]
    fn an_unknown_precedence_rule_discriminant_is_ignored_with_a_warning() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{
                 "schema": 1,
                 "precedence": {
                   "schema": 1,
                   "rules": {
                     "example.PartAssignmentDef": { "rule": "prefer_the_newest_mod" }
                   }
                 }
               }"#,
        );

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        assert_eq!(
            loaded.knowledge.precedence_for("example.PartAssignmentDef"),
            PrecedenceRule::Unverified
        );
        assert!(
            loaded.warnings.iter().any(|warning| {
                matches!(warning, RulesLoadWarning::UnknownModKnowledgeValue { value, .. }
                    if value == "prefer_the_newest_mod")
            }),
            "{:?}",
            loaded.warnings
        );
    }

    #[test]
    fn the_vendored_store_never_reads_a_cache_file_even_when_one_exists() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{ "schema": 1, "def_cache_carriers": { "schema": 1, "carriers": [] } }"#,
        );

        let loaded = FsModKnowledgeStore::vendored().load(ENABLED);

        assert_eq!(loaded.knowledge, vendored_knowledge().knowledge);
    }

    // -- the toggle gates consumption, not just fetching -----------------

    /// `Settings::fetch_rimmerge_rules` off means the vendored defaults
    /// **even with a cached copy sitting on disk** — the same rule
    /// `should_import_from_cache` applies to the other two sources. A
    /// toggle that only stopped a *refresh* would keep reading a
    /// previously fetched file forever, contradicting that setting's own
    /// doc comment.
    #[test]
    fn the_toggle_off_ignores_a_present_cache_file_and_uses_the_vendored_defaults() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{
                 "schema": 1,
                 "def_cache_carriers": { "schema": 1, "carriers": [] }
               }"#,
        );
        let store = FsModKnowledgeStore::new(dir.path());

        let off = store.load(false);
        let on = store.load(true);

        assert_eq!(
            off.knowledge,
            vendored_knowledge().knowledge,
            "off must be exactly the vendored behaviour"
        );
        assert_eq!(
            off.knowledge.def_cache_carriers(),
            vendored_knowledge().knowledge.def_cache_carriers(),
            "the embedded carrier list is still there with the toggle off"
        );
        assert!(
            on.knowledge.def_cache_carriers().is_empty(),
            "and the same store *does* read that cache when the toggle is on — \
             otherwise this test would pass with the cache never being read at all"
        );
    }

    // -- one malformed section never discards the others -----------------

    /// A section whose own shape doesn't parse falls back to *its* vendored
    /// default alone, with a warning naming it; every other section in the
    /// same file still loads. A fully typed envelope would let one bad row
    /// discard the other good sections.
    #[test]
    fn a_malformed_section_falls_back_alone_and_the_others_still_load() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{
                 "schema": 1,
                 "precedence": { "schema": 1, "rules": { "a.Def": { "rule": 17 } } },
                 "def_cache_carriers": {
                   "schema": 1,
                   "carriers": [
                     { "id": "from-the-file", "plugin_dir": "Plugins",
                       "file_prefix": "FromTheFile", "file_extension": ".dll",
                       "match": "case_insensitive_prefix", "recursive": false,
                       "log_line_prefix": "FROMTHEFILE:" }
                   ]
                 }
               }"#,
        );

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        // The good section is the file's, not the vendored one.
        assert_eq!(
            loaded
                .knowledge
                .def_cache_carriers()
                .iter()
                .map(|carrier| carrier.id.as_str())
                .collect::<Vec<_>>(),
            vec!["from-the-file"],
            "a bad `precedence` section must not cost us the good carriers"
        );
        // The bad one degraded on its own, to the embedded snapshot's own
        // precedence (not to nothing) — same fallback a missing section gets.
        assert_eq!(
            loaded.knowledge.precedence(),
            vendored_knowledge().knowledge.precedence()
        );
        assert!(
            loaded.warnings.iter().any(|warning| matches!(warning,
                RulesLoadWarning::ModKnowledgeCacheUnreadable(reason)
                    if reason.contains("precedence")
            )),
            "the failing section must be named: {:?}",
            loaded.warnings
        );
        // And the patch-operation section, absent from the file entirely,
        // is still the vendored one.
        assert_eq!(
            loaded.knowledge.patch_operations(),
            vendored_knowledge().knowledge.patch_operations()
        );
    }

    // -- unknown top-level keys are reported, not swallowed --------------

    /// A misspelled section reads exactly like an absent one unless it is
    /// reported — "this file ships no carriers" and "I typed
    /// `def_cache_carrier`" are indistinguishable otherwise.
    #[test]
    fn an_unknown_top_level_key_is_warned_about_and_otherwise_ignored() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{
                 "schema": 1,
                 "def_cache_carrier": { "schema": 1, "carriers": [] },
                 "future_section": { "anything": true }
               }"#,
        );

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        assert_eq!(
            loaded.knowledge,
            vendored_knowledge().knowledge,
            "an unknown key changes nothing"
        );
        for key in ["def_cache_carrier", "future_section"] {
            assert!(
                loaded.warnings.iter().any(|warning| matches!(warning,
                    RulesLoadWarning::UnknownModKnowledgeValue { value, .. } if value == key
                )),
                "{key} must be reported: {:?}",
                loaded.warnings
            );
        }
    }

    // -- log shapes -------------------------------------------------------

    fn stack_row() -> serde_json::Value {
        serde_json::json!({
            "id": "example-stack-block",
            "match": "template",
            "start": "[{mod:text} - Begin trace]",
            "end": "[End trace]",
            "trailer": "Source:{path:text}",
            "xpath_detail_prefix": "xpath=",
            "branch_markers": { "match": "<match>", "nomatch": "<nomatch>" },
            "evidence": "invented"
        })
    }

    fn texture_row() -> serde_json::Value {
        serde_json::json!({
            "id": "example-texture",
            "match": "template",
            "head": "Texture failed for '{path:text}': {reason:text}",
            "dimensions": "Texture failed for '{path:text}': size {width:int}x{height:int} format {format:token}",
            "trailer": "Using png.",
            "evidence": "invented"
        })
    }

    fn reference_row() -> serde_json::Value {
        serde_json::json!({
            "id": "example-reference",
            "match": "template",
            "original": "[Trace {id:hex}]",
            "stub": "[Trace {id:hex}] repeated",
            "evidence": "invented"
        })
    }

    /// A cached file holding only a `log_shapes` section built from `rows`
    /// (each role's rows), then loaded.
    fn load_shapes(
        stack: Vec<serde_json::Value>,
        texture: Vec<serde_json::Value>,
        reference: Vec<serde_json::Value>,
        extra: serde_json::Value,
    ) -> LoadedModKnowledge {
        let mut section = serde_json::json!({
            "schema": 1,
            "patch_stack_blocks": stack,
            "texture_fallbacks": texture,
            "stack_back_references": reference,
        });
        if let (Some(target), serde_json::Value::Object(more)) = (section.as_object_mut(), extra) {
            target.extend(more);
        }
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            &serde_json::json!({ "schema": 1, "log_shapes": section }).to_string(),
        );
        FsModKnowledgeStore::new(dir.path()).load(ENABLED)
    }

    /// The one warning a load must hold: a log-shapes value warning of kind
    /// `kind` whose value is `value`.
    fn assert_one_log_shapes_warning(
        loaded: &LoadedModKnowledge,
        kind: ModKnowledgeValueKind,
        value: &str,
    ) {
        assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
        assert!(
            matches!(&loaded.warnings[0],
                RulesLoadWarning::UnknownModKnowledgeValue { section, what, value: found }
                    if section == "log-shapes" && *what == kind && found == value),
            "{:?}",
            loaded.warnings
        );
    }

    /// The one warning a load must hold: `ignored` rows of `role` over the limit.
    fn assert_one_row_limit_warning(loaded: &LoadedModKnowledge, role: &str, ignored: usize) {
        assert_eq!(
            loaded.warnings,
            vec![RulesLoadWarning::ModKnowledgeRowsOverRoleLimit {
                section: "log-shapes".to_string(),
                role: role.to_string(),
                ignored,
            }]
        );
    }

    #[test]
    fn a_cached_log_shapes_section_replaces_the_embedded_one() {
        let loaded = load_shapes(
            vec![stack_row()],
            vec![texture_row()],
            vec![reference_row()],
            serde_json::json!({}),
        );

        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        let shapes = loaded.knowledge.log_shapes();
        assert_eq!(shapes.stack_blocks().len(), 1);
        assert_eq!(shapes.stack_blocks()[0].id(), "example-stack-block");
        assert_eq!(shapes.texture_fallbacks()[0].id(), "example-texture");
        assert_eq!(shapes.back_references()[0].id(), "example-reference");
        assert_ne!(
            shapes,
            vendored_knowledge().knowledge.log_shapes(),
            "the cached rows, not the embedded ones"
        );
    }

    #[test]
    fn a_fetched_file_without_log_shapes_keeps_the_embedded_value() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{ "schema": 1, "def_cache_carriers": { "schema": 1, "carriers": [] } }"#,
        );

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        let embedded = vendored_knowledge().knowledge;
        assert!(!embedded.log_shapes().is_empty(), "the bundle ships shapes");
        assert_eq!(loaded.knowledge.log_shapes(), embedded.log_shapes());
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    }

    #[test]
    fn a_malformed_log_shapes_section_falls_back_to_the_embedded_value_with_a_warning() {
        let dir = tempdir().expect("tempdir");
        write_cache(
            dir.path(),
            r#"{ "schema": 1, "log_shapes": { "schema": 1, "patch_stack_blocks": "nope" } }"#,
        );

        let loaded = FsModKnowledgeStore::new(dir.path()).load(ENABLED);

        assert_eq!(
            loaded.knowledge.log_shapes(),
            vendored_knowledge().knowledge.log_shapes()
        );
        assert!(
            loaded.warnings.iter().any(|warning| matches!(warning,
                RulesLoadWarning::ModKnowledgeCacheUnreadable(reason)
                    if reason.contains("log-shapes")
            )),
            "{:?}",
            loaded.warnings
        );
    }

    #[test]
    fn an_unknown_log_shape_role_is_reported_and_the_known_roles_still_load() {
        let loaded = load_shapes(
            vec![stack_row()],
            vec![],
            vec![],
            serde_json::json!({ "future_role": [ { "id": "x" } ] }),
        );

        assert_one_log_shapes_warning(&loaded, ModKnowledgeValueKind::LogShapeRole, "future_role");
        assert_eq!(loaded.knowledge.log_shapes().stack_blocks().len(), 1);
    }

    #[test]
    fn a_log_shape_row_with_an_unknown_match_mode_is_ignored_and_the_rest_load() {
        let mut regex_row = texture_row();
        regex_row["match"] = serde_json::json!("regex");
        let loaded = load_shapes(
            vec![stack_row()],
            vec![regex_row],
            vec![reference_row()],
            serde_json::json!({}),
        );

        assert_one_log_shapes_warning(&loaded, ModKnowledgeValueKind::MatchMode, "regex");
        let shapes = loaded.knowledge.log_shapes();
        assert!(shapes.texture_fallbacks().is_empty(), "the row is ignored");
        assert_eq!(shapes.stack_blocks().len(), 1);
        assert_eq!(shapes.back_references().len(), 1);
    }

    #[test]
    fn a_log_shape_row_with_an_unknown_placeholder_type_is_ignored_and_the_rest_load() {
        let mut row = stack_row();
        row["start"] = serde_json::json!("[{mod:float} - Begin trace]");
        let loaded = load_shapes(
            vec![row, stack_row()],
            vec![],
            vec![],
            serde_json::json!({}),
        );

        assert_one_log_shapes_warning(&loaded, ModKnowledgeValueKind::PlaceholderType, "float");
        assert_eq!(loaded.knowledge.log_shapes().stack_blocks().len(), 1);
    }

    #[test]
    fn an_oversized_log_shape_template_is_ignored_and_the_rest_load() {
        let mut row = stack_row();
        row["end"] = serde_json::json!("x".repeat(513));
        let loaded = load_shapes(
            vec![row, stack_row()],
            vec![],
            vec![],
            serde_json::json!({}),
        );

        assert_one_log_shapes_warning(
            &loaded,
            ModKnowledgeValueKind::TemplateInvalid,
            "example-stack-block",
        );
        assert_eq!(loaded.knowledge.log_shapes().stack_blocks().len(), 1);
    }

    #[test]
    fn a_log_shape_row_missing_a_required_capture_is_ignored_and_the_rest_load() {
        let mut row = reference_row();
        row["stub"] = serde_json::json!("[Trace] repeated");
        let loaded = load_shapes(vec![stack_row()], vec![], vec![row], serde_json::json!({}));

        assert_one_log_shapes_warning(
            &loaded,
            ModKnowledgeValueKind::CaptureMissing,
            "example-reference",
        );
        let shapes = loaded.knowledge.log_shapes();
        assert!(shapes.back_references().is_empty());
        assert_eq!(shapes.stack_blocks().len(), 1);
    }

    #[test]
    fn a_role_over_the_row_limit_keeps_the_first_rows_and_reports_once() {
        let rows = (0..10)
            .map(|index| {
                let mut row = reference_row();
                row["id"] = serde_json::json!(format!("row-{index}"));
                row
            })
            .collect();
        let loaded = load_shapes(vec![], vec![], rows, serde_json::json!({}));

        assert_one_row_limit_warning(&loaded, "stack_back_references", 2);
        let kept = loaded.knowledge.log_shapes().back_references();
        assert_eq!(kept.len(), 8);
        assert_eq!(kept[7].id(), "row-7");
    }

    #[test]
    fn rows_over_the_limit_are_never_parsed() {
        // The rows past the cap are malformed; were they parsed there would
        // be one warning each on top of the aggregated one.
        let mut rows: Vec<serde_json::Value> =
            (0..MAX_ROWS_PER_ROLE).map(|_| reference_row()).collect();
        for _ in 0..50 {
            let mut bad = reference_row();
            bad["stub"] = serde_json::json!("no capture here");
            rows.push(bad);
        }
        let loaded = load_shapes(vec![], vec![], rows, serde_json::json!({}));

        assert_one_row_limit_warning(&loaded, "stack_back_references", 50);
        assert_eq!(loaded.knowledge.log_shapes().back_references().len(), 8);
    }

    #[test]
    fn a_log_shape_row_with_an_over_long_id_is_ignored_and_the_rest_load() {
        let mut row = stack_row();
        row["id"] = serde_json::json!("i".repeat(65));
        let loaded = load_shapes(
            vec![row, stack_row()],
            vec![],
            vec![],
            serde_json::json!({}),
        );

        assert_one_log_shapes_warning(&loaded, ModKnowledgeValueKind::IdTooLong, "id");
        assert_eq!(loaded.knowledge.log_shapes().stack_blocks().len(), 1);
    }

    #[test]
    fn a_fetched_stack_block_row_without_branch_markers_drops_the_whole_section() {
        // `branch_markers` is a required field of the row, so a fetched file
        // that omits it fails the *section*, not just the row: the embedded
        // `log_shapes` stays and one unreadable-cache warning names it.
        // Making a listed field optional or removing it is therefore a
        // breaking change for older binaries.
        let mut row = stack_row();
        row.as_object_mut()
            .expect("the row is an object")
            .remove("branch_markers");
        let loaded = load_shapes(vec![row], vec![], vec![], serde_json::json!({}));

        assert_eq!(
            loaded.knowledge.log_shapes(),
            vendored_knowledge().knowledge.log_shapes()
        );
        assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
        assert!(
            matches!(&loaded.warnings[0],
                RulesLoadWarning::ModKnowledgeCacheUnreadable(reason)
                    if reason.contains("log-shapes")),
            "{:?}",
            loaded.warnings
        );
    }

    #[test]
    fn a_log_shape_template_with_regex_metacharacters_loads_without_warning() {
        // That the metacharacters then match only as plain text is pinned in
        // `game_log::shapes`' `regex_syntax_in_a_literal_is_matched_as_plain_text`;
        // this only shows the loader accepts such a literal.
        let mut row = stack_row();
        row["start"] = serde_json::json!("(.*)+[{mod:text}]|^$");
        let loaded = load_shapes(vec![row], vec![], vec![], serde_json::json!({}));

        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.knowledge.log_shapes().stack_blocks().len(), 1);
    }

    /// `parses` is the cache's own parse-before-commit gate, and it must
    /// reject an arbitrary JSON object — otherwise any 200 response at
    /// all would be written into the cache as a rules file.
    #[test]
    fn parses_requires_the_envelope_shape_and_rejects_arbitrary_json() {
        assert!(parses(br#"{"schema":1}"#), "the minimal envelope");
        assert!(
            parses(br#"{"schema":1,"precedence":{"schema":1,"rules":{}}}"#),
            "a real section"
        );
        assert!(
            parses(br#"{"schema":1,"precedence":{"rules":"nonsense"}}"#),
            "a malformed *section* still validates — it degrades per-section at \
             load time, with a warning; refusing to cache the whole file \
             over one bad row is how a newer file breaks an older binary"
        );

        assert!(!parses(br#"{"hello":"world"}"#), "no schema field");
        assert!(!parses(b"[]"), "not an object");
        assert!(!parses(b"not json at all"), "not JSON");
        assert!(
            !parses(br#"{"schema":"one"}"#),
            "schema must be a number, not any value"
        );
    }
}
