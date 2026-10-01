//! The mod-produced log formats the entry pipeline recognises, compiled from
//! the `log_shapes` rules data: a patch-reporting mod's stack-trace block, a
//! texture loader's fallback lines, and a patching library's back-reference
//! stubs. None of them is engine output, so none is in this source: the
//! formats arrive as a [`LogShapes`] value (the bundled snapshot, or a
//! fetched file's section) and [`CompiledShapes::new`] turns them into
//! matchers once per parse.
//!
//! **Safe with an untrusted data file.** A [`LogTemplate`] is literal text
//! plus typed placeholders, never a regex:
//!
//! - literals go through [`regex::escape`], so the data cannot inject regex
//!   syntax;
//! - each placeholder maps to one of four fixed, bounded sub-patterns
//!   defined here ([`placeholder_body`]);
//! - the `regex` crate never backtracks, so every search is worst-case
//!   O(pattern x text) whatever the template says, and compilation is linear
//!   in the template because the grammar has no counted repetition beyond
//!   the fixed `{1,16}` of the hex body;
//! - defence in depth: a template is at most 512 bytes with at most 8
//!   placeholders and 4 bytes of literal text ([`rim_session::ports`]), and
//!   each is compiled under [`SIZE_LIMIT_BYTES`] / [`DFA_SIZE_LIMIT_BYTES`]
//!   and a nesting limit of [`NEST_LIMIT`], so one row's memory is bounded
//!   and a role holds at most eight rows.
//!
//! A row that fails any bound never reaches this module from the store
//! (`mod_knowledge` drops it with a warning); [`CompiledShapes::new`] still
//! skips a row that does not compile rather than panicking.
//!
//! A template matches the whole **trimmed** line (`^...$`), as every scan in
//! this pipeline does. Where a role has several rows, a line matches when it
//! matches any row; a stack block's end, trailer, xpath prefix and branch
//! markers are likewise those of any row.

use regex::{Captures, Locations, Regex, RegexBuilder};
use rim_session::ports::{
    BackReferenceShape, EntryClass, LogShapes, LogTemplate, PatchStackBlockShape, PlaceholderKind,
    TemplatePart, TextureFallbackShape,
};

/// The most bytes of compiled program one template may take.
pub(super) const SIZE_LIMIT_BYTES: usize = 1 << 20;
/// The most bytes of lazy-DFA cache one template's regex may take.
pub(super) const DFA_SIZE_LIMIT_BYTES: usize = 1 << 20;
/// The deepest nesting a template's regex may have (its grammar has none, so
/// this only guards the fixed bodies).
pub(super) const NEST_LIMIT: u32 = 8;

// -- compiling templates ---------------------------------------------------

/// Why a template did not compile.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ShapeCompileError {
    /// The generated pattern exceeded a compile limit (or, for a bug, was
    /// malformed).
    #[error("the template does not compile within the size limits: {0}")]
    Regex(#[from] regex::Error),
    /// A capture the role needs is not in the compiled pattern.
    #[error("the template has no `{0}` capture")]
    MissingCapture(&'static str),
}

/// The regex a placeholder of `kind` stands for. Fixed and bounded.
fn placeholder_body(kind: PlaceholderKind) -> &'static str {
    match kind {
        PlaceholderKind::Text => ".+",
        PlaceholderKind::Token => r"\S+",
        PlaceholderKind::Int => "[0-9]+",
        PlaceholderKind::Hex => "[0-9A-Fa-f]{1,16}",
    }
}

/// The anchored regex source of `template`.
fn regex_source(template: &LogTemplate) -> String {
    let mut source = String::from("^");
    for part in template.parts() {
        match part {
            TemplatePart::Literal(text) => source.push_str(&regex::escape(text)),
            TemplatePart::Placeholder { name, kind } => {
                source.push_str("(?P<");
                source.push_str(name);
                source.push('>');
                source.push_str(placeholder_body(*kind));
                source.push(')');
            }
        }
    }
    source.push('$');
    source
}

/// A template compiled to a regex, with two cheap pre-checks: the text the
/// template starts with and its longest literal must both occur in a line
/// for the regex to be worth running.
struct PatternMatcher {
    regex: Regex,
    leading: Option<String>,
    required: String,
}

impl PatternMatcher {
    fn compile(template: &LogTemplate) -> Result<Self, ShapeCompileError> {
        let regex = RegexBuilder::new(&regex_source(template))
            .size_limit(SIZE_LIMIT_BYTES)
            .dfa_size_limit(DFA_SIZE_LIMIT_BYTES)
            .nest_limit(NEST_LIMIT)
            .build()?;
        Ok(Self {
            regex,
            leading: template.leading_literal().map(str::to_string),
            required: template.longest_literal().to_string(),
        })
    }

    fn may_match(&self, trimmed: &str) -> bool {
        self.leading
            .as_deref()
            .is_none_or(|prefix| trimmed.starts_with(prefix))
            && trimmed.contains(&self.required)
    }

    fn is_match(&self, trimmed: &str) -> bool {
        self.may_match(trimmed) && self.regex.is_match(trimmed)
    }

    fn captures<'text>(&self, trimmed: &'text str) -> Option<Captures<'text>> {
        if !self.may_match(trimmed) {
            return None;
        }
        self.regex.captures(trimmed)
    }

    /// The index of the capture group called `name`.
    fn group_of(&self, name: &'static str) -> Result<usize, ShapeCompileError> {
        self.regex
            .capture_names()
            .position(|found| found == Some(name))
            .ok_or(ShapeCompileError::MissingCapture(name))
    }
}

/// A compiled template: an exact-line match when it has no placeholder,
/// else a pattern.
enum TemplateMatcher {
    Exact(String),
    Pattern(PatternMatcher),
}

impl TemplateMatcher {
    fn compile(template: &LogTemplate) -> Result<Self, ShapeCompileError> {
        match template.exact_text() {
            Some(text) => Ok(Self::Exact(text.to_string())),
            None => PatternMatcher::compile(template).map(Self::Pattern),
        }
    }

    fn is_match(&self, trimmed: &str) -> bool {
        match self {
            Self::Exact(text) => trimmed == text,
            Self::Pattern(pattern) => pattern.is_match(trimmed),
        }
    }

    /// The text of capture `name`, for a pattern that matches `trimmed`.
    fn capture(&self, trimmed: &str, name: &str) -> Option<String> {
        match self {
            Self::Exact(_) => None,
            Self::Pattern(pattern) => pattern
                .captures(trimmed)
                .and_then(|captures| captures.name(name).map(|found| found.as_str().to_string())),
        }
    }
}

// -- compiled roles --------------------------------------------------------

struct StackBlock {
    start: TemplateMatcher,
    end: TemplateMatcher,
    trailer: TemplateMatcher,
    xpath_detail_prefix: String,
    match_marker: String,
    nomatch_marker: String,
    /// The longest fixed segment of the start template (its loose
    /// sentinel).
    start_literal: String,
}

struct TextureFallback {
    head: TemplateMatcher,
    dimensions: TemplateMatcher,
    trailer: TemplateMatcher,
    head_literal: String,
}

struct BackReference {
    original: PatternMatcher,
    stub: PatternMatcher,
    original_id_group: usize,
    stub_id_group: usize,
}

/// A texture fallback's size report, as the format gives it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TextureDimensions {
    pub(super) path: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: String,
}

/// A loose sentinel derived from a format: the longest fixed segment of its
/// head template, and the class a line holding it must sit in.
pub(super) struct ShapeSentinel {
    pub(super) name: &'static str,
    pub(super) literal: String,
    pub(super) class: EntryClass,
}

// -- back-reference matches -----------------------------------------------------

/// Whether a back-reference line names an original or repeats it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StackRefKind {
    /// The line naming an original trace, followed by the frames it stands
    /// for.
    Original,
    /// A repeat of an original's trace.
    Stub,
}

/// Where the id sits in a trimmed line, as byte offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct IdSpan {
    start: usize,
    end: usize,
}

impl IdSpan {
    /// The id text within the line the span was found in.
    pub(super) fn of(self, trimmed: &str) -> &str {
        trimmed.get(self.start..self.end).unwrap_or_default()
    }
}

/// A recognised back-reference line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StackRefMatch {
    pub(super) kind: StackRefKind,
    pub(super) id: IdSpan,
}

/// Scratch capture space for [`CompiledShapes::match_stack_ref`], reused
/// across lines so the hot path allocates nothing.
pub(super) struct RefLocations {
    rows: Vec<RowLocations>,
}

struct RowLocations {
    stub: Locations,
    original: Locations,
}

fn id_span(locations: &Locations, group: usize) -> Option<IdSpan> {
    locations
        .get(group)
        .map(|(start, end)| IdSpan { start, end })
}

// -- compiled -------------------------------------------------------------------

/// The formats of one [`LogShapes`], compiled once per parse.
#[derive(Default)]
pub(super) struct CompiledShapes {
    stack_blocks: Vec<StackBlock>,
    texture_fallbacks: Vec<TextureFallback>,
    back_references: Vec<BackReference>,
}

impl CompiledShapes {
    /// Compiles `shapes`. A row that does not compile is skipped: the store
    /// only holds rows that do (it checks them with the `check_*` functions below), so
    /// this cannot happen for shapes that came through it.
    pub(super) fn new(shapes: &LogShapes) -> Self {
        Self {
            stack_blocks: shapes
                .stack_blocks()
                .iter()
                .filter_map(|row| compile_stack_block(row).ok())
                .collect(),
            texture_fallbacks: shapes
                .texture_fallbacks()
                .iter()
                .filter_map(|row| compile_texture_fallback(row).ok())
                .collect(),
            back_references: shapes
                .back_references()
                .iter()
                .filter_map(|row| compile_back_reference(row).ok())
                .collect(),
        }
    }

    // -- stack-trace blocks --

    /// The trimmed line starts a stack-trace block.
    pub(super) fn is_stack_start(&self, trimmed: &str) -> bool {
        self.stack_blocks
            .iter()
            .any(|block| block.start.is_match(trimmed))
    }

    /// The mod a stack-trace block's start line names.
    pub(super) fn stack_start_mod(&self, trimmed: &str) -> Option<String> {
        self.stack_blocks
            .iter()
            .find_map(|block| block.start.capture(trimmed, "mod"))
    }

    /// The trimmed line ends a stack-trace block.
    pub(super) fn is_stack_end(&self, trimmed: &str) -> bool {
        self.stack_blocks
            .iter()
            .any(|block| block.end.is_match(trimmed))
    }

    /// The trimmed line is the source-file line after a block.
    pub(super) fn is_stack_trailer(&self, trimmed: &str) -> bool {
        self.stack_blocks
            .iter()
            .any(|block| block.trailer.is_match(trimmed))
    }

    /// The path a block's trailer line names, trimmed.
    pub(super) fn stack_trailer_path(&self, trimmed: &str) -> Option<String> {
        self.stack_blocks
            .iter()
            .find_map(|block| block.trailer.capture(trimmed, "path"))
            .map(|path| path.trim().to_string())
    }

    /// The xpath an operation line's parenthesized detail names, when it
    /// starts with a block's xpath prefix: the text after the prefix, quotes
    /// stripped and trimmed. A block's continuation line joins with a
    /// leading space, which would otherwise survive inside the value.
    pub(super) fn xpath_from_detail(&self, detail: &str) -> Option<String> {
        let rest = self
            .stack_blocks
            .iter()
            .find_map(|block| detail.strip_prefix(block.xpath_detail_prefix.as_str()))?;
        let unwrapped = rest
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .unwrap_or(rest);
        Some(unwrapped.trim().to_string())
    }

    /// Which branch an operation line's reason reports: `nomatch` is checked
    /// before `match`, across every block.
    pub(super) fn branch_of(&self, reason: &str) -> Option<&'static str> {
        if self
            .stack_blocks
            .iter()
            .any(|block| reason.contains(block.nomatch_marker.as_str()))
        {
            return Some("nomatch");
        }
        self.stack_blocks
            .iter()
            .any(|block| reason.contains(block.match_marker.as_str()))
            .then_some("match")
    }

    // -- texture fallbacks --

    /// The trimmed line is a head of a texture fallback (any reason).
    pub(super) fn is_texture_fallback_head(&self, trimmed: &str) -> bool {
        self.texture_fallbacks
            .iter()
            .any(|fallback| fallback.head.is_match(trimmed))
    }

    /// The trimmed line is the line that follows a fallback head.
    pub(super) fn is_texture_fallback_trailer(&self, trimmed: &str) -> bool {
        self.texture_fallbacks
            .iter()
            .any(|fallback| fallback.trailer.is_match(trimmed))
    }

    /// The texture path a fallback head names, whatever its reason.
    pub(super) fn texture_fallback_path(&self, trimmed: &str) -> Option<String> {
        self.texture_fallbacks
            .iter()
            .find_map(|fallback| fallback.head.capture(trimmed, "path"))
    }

    /// The size report of a fallback head, when the line is one and its
    /// numbers fit.
    pub(super) fn texture_dimensions(&self, trimmed: &str) -> Option<TextureDimensions> {
        let fallback = self
            .texture_fallbacks
            .iter()
            .find(|fallback| fallback.dimensions.is_match(trimmed))?;
        let field = |name| fallback.dimensions.capture(trimmed, name);
        Some(TextureDimensions {
            path: field("path")?,
            width: field("width")?.parse().ok()?,
            height: field("height")?.parse().ok()?,
            format: field("format")?,
        })
    }

    // -- back-references --

    /// Fresh scratch space for [`Self::match_stack_ref`].
    pub(super) fn new_ref_locations(&self) -> RefLocations {
        RefLocations {
            rows: self
                .back_references
                .iter()
                .map(|row| RowLocations {
                    stub: row.stub.regex.capture_locations(),
                    original: row.original.regex.capture_locations(),
                })
                .collect(),
        }
    }

    /// Recognises a back-reference line (a stub or an original) and finds
    /// its id in one pass. Stubs are tried before originals, across every
    /// row.
    pub(super) fn match_stack_ref(
        &self,
        trimmed: &str,
        locations: &mut RefLocations,
    ) -> Option<StackRefMatch> {
        for (row, scratch) in self.back_references.iter().zip(&mut locations.rows) {
            if !row.stub.may_match(trimmed) {
                continue;
            }
            if row
                .stub
                .regex
                .captures_read(&mut scratch.stub, trimmed)
                .is_some()
            {
                let id = id_span(&scratch.stub, row.stub_id_group)?;
                return Some(StackRefMatch {
                    kind: StackRefKind::Stub,
                    id,
                });
            }
        }
        for (row, scratch) in self.back_references.iter().zip(&mut locations.rows) {
            if !row.original.may_match(trimmed) {
                continue;
            }
            if row
                .original
                .regex
                .captures_read(&mut scratch.original, trimmed)
                .is_some()
            {
                let id = id_span(&scratch.original, row.original_id_group)?;
                return Some(StackRefMatch {
                    kind: StackRefKind::Original,
                    id,
                });
            }
        }
        None
    }

    // -- sentinels --

    /// The loose sentinels of the formats: the longest fixed segment of each
    /// head template, with the class a line holding it must sit in.
    pub(super) fn sentinels(&self) -> Vec<ShapeSentinel> {
        let texture = self.texture_fallbacks.iter().map(|fallback| ShapeSentinel {
            name: "texture-fallback",
            literal: fallback.head_literal.clone(),
            class: EntryClass::TextureFallback,
        });
        let stack = self.stack_blocks.iter().map(|block| ShapeSentinel {
            name: "patch-stack-trace",
            literal: block.start_literal.clone(),
            class: EntryClass::PatchStackTrace,
        });
        texture.chain(stack).collect()
    }
}

fn compile_stack_block(row: &PatchStackBlockShape) -> Result<StackBlock, ShapeCompileError> {
    Ok(StackBlock {
        start: TemplateMatcher::compile(row.start())?,
        end: TemplateMatcher::compile(row.end())?,
        trailer: TemplateMatcher::compile(row.trailer())?,
        xpath_detail_prefix: row.xpath_detail_prefix().to_string(),
        match_marker: row.match_marker().to_string(),
        nomatch_marker: row.nomatch_marker().to_string(),
        start_literal: row.start().longest_literal().to_string(),
    })
}

fn compile_texture_fallback(
    row: &TextureFallbackShape,
) -> Result<TextureFallback, ShapeCompileError> {
    Ok(TextureFallback {
        head: TemplateMatcher::compile(row.head())?,
        dimensions: TemplateMatcher::compile(row.dimensions())?,
        trailer: TemplateMatcher::compile(row.trailer())?,
        head_literal: row.head().longest_literal().to_string(),
    })
}

fn compile_back_reference(row: &BackReferenceShape) -> Result<BackReference, ShapeCompileError> {
    let original = PatternMatcher::compile(row.original())?;
    let stub = PatternMatcher::compile(row.stub())?;
    Ok(BackReference {
        original_id_group: original.group_of("id")?,
        stub_id_group: stub.group_of("id")?,
        original,
        stub,
    })
}

// -- checking rows at load time ---------------------------------------------

/// Checks that a stack-block row compiles within the limits.
pub(crate) fn check_stack_block(row: &PatchStackBlockShape) -> Result<(), ShapeCompileError> {
    compile_stack_block(row).map(|_| ())
}

/// Checks that a texture-fallback row compiles within the limits.
pub(crate) fn check_texture_fallback(row: &TextureFallbackShape) -> Result<(), ShapeCompileError> {
    compile_texture_fallback(row).map(|_| ())
}

/// Checks that a back-reference row compiles within the limits.
pub(crate) fn check_back_reference(row: &BackReferenceShape) -> Result<(), ShapeCompileError> {
    compile_back_reference(row).map(|_| ())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rim_session::ports::{StackBlockSource, TextureFallbackSource};
    use rim_session::test_support::example_log_shapes;

    use super::*;

    fn example() -> CompiledShapes {
        CompiledShapes::new(&example_log_shapes())
    }

    fn stack_block_with(start: &str, end: &str, trailer: &str) -> PatchStackBlockShape {
        PatchStackBlockShape::parse(&StackBlockSource {
            id: "row",
            start,
            end,
            trailer,
            xpath_detail_prefix: "xpath=",
            match_marker: "<match>",
            nomatch_marker: "<nomatch>",
        })
        .expect("the row parses")
    }

    #[test]
    fn the_example_shapes_recognise_the_three_formats() {
        let shapes = example();

        assert!(shapes.is_stack_start("[Some Mod - Start of stack trace]"));
        assert_eq!(
            shapes.stack_start_mod("[Some [Fork] Mod - Start of stack trace]"),
            Some("Some [Fork] Mod".to_string())
        );
        assert!(shapes.is_stack_end("[End of stack trace]"));
        assert_eq!(
            shapes.stack_trailer_path("Source file: C:\\a b\\c.xml"),
            Some("C:\\a b\\c.xml".to_string())
        );
        assert!(shapes.is_texture_fallback_head("DDS loading failed for 'a/b': any reason"));
        assert!(shapes.is_texture_fallback_trailer("Loading from png instead."));
        let mut locations = shapes.new_ref_locations();
        let stub = shapes
            .match_stack_ref(
                "[Ref 1A2B] Duplicate stacktrace, see ref for original",
                &mut locations,
            )
            .expect("a stub");
        assert_eq!(stub.kind, StackRefKind::Stub);
        let original = shapes
            .match_stack_ref("[Ref 1A2B]", &mut locations)
            .expect("an original");
        assert_eq!(original.kind, StackRefKind::Original);
        assert_eq!(original.id.of("[Ref 1A2B]"), "1A2B");
    }

    #[test]
    fn a_line_that_only_looks_like_a_format_is_not_recognised() {
        let shapes = example();
        let mut locations = shapes.new_ref_locations();

        assert!(!shapes.is_stack_start("[Some Mod - Start of stack trace] trailing"));
        assert!(!shapes.is_stack_end("[End of stack trace] x"));
        assert!(!shapes.is_stack_trailer("Source files: x"));
        assert!(!shapes.is_texture_fallback_head("DDS loading failed for x"));
        assert!(
            shapes
                .match_stack_ref("[Ref not-hex]", &mut locations)
                .is_none()
        );
        let seventeen_digits = format!("[Ref {}]", "A".repeat(17));
        assert!(
            shapes
                .match_stack_ref(&seventeen_digits, &mut locations)
                .is_none(),
            "an id is at most 16 hex digits"
        );
    }

    #[test]
    fn an_empty_set_of_shapes_recognises_nothing() {
        let shapes = CompiledShapes::new(&LogShapes::empty());
        let mut locations = shapes.new_ref_locations();

        assert!(!shapes.is_stack_start("[Some Mod - Start of stack trace]"));
        assert!(!shapes.is_stack_end("[End of stack trace]"));
        assert!(!shapes.is_texture_fallback_head("DDS loading failed for 'a': b"));
        assert!(
            shapes
                .match_stack_ref("[Ref 1A2B]", &mut locations)
                .is_none()
        );
        assert_eq!(shapes.xpath_from_detail("xpath=\"/Defs\""), None);
        assert_eq!(shapes.branch_of("Error in <nomatch>"), None);
        assert!(shapes.sentinels().is_empty());
    }

    #[test]
    fn the_xpath_prefix_and_branch_markers_come_from_the_rows() {
        let shapes = example();

        assert_eq!(
            shapes.xpath_from_detail("xpath=\"Defs/A\""),
            Some("Defs/A".to_string())
        );
        assert_eq!(
            shapes.xpath_from_detail("xpath=/Defs/A"),
            Some("/Defs/A".to_string())
        );
        assert_eq!(shapes.xpath_from_detail("a mod name"), None);
        assert_eq!(shapes.branch_of("Error in <nomatch>"), Some("nomatch"));
        assert_eq!(shapes.branch_of("Error in <match>"), Some("match"));
        assert_eq!(
            shapes.branch_of("<match> and <nomatch>"),
            Some("nomatch"),
            "nomatch is checked first"
        );
        assert_eq!(shapes.branch_of("Failed to find"), None);
    }

    #[test]
    fn the_dimensions_template_captures_an_apostrophe_and_rejects_an_empty_path() {
        let shapes = example();
        let line = |path: &str| {
            format!(
                "DDS loading failed for '{path}': Cannot load compressed texture with non \
                 multiple of 4 dimensions of 30x20 and format DXT5"
            )
        };

        let apostrophe = shapes.texture_dimensions(&line("Things/Bob's sword"));
        assert_eq!(
            apostrophe,
            Some(TextureDimensions {
                path: "Things/Bob's sword".to_string(),
                width: 30,
                height: 20,
                format: "DXT5".to_string(),
            })
        );
        assert_eq!(shapes.texture_dimensions(&line("")), None);
        assert!(
            !shapes.is_texture_fallback_head(&line("")),
            "an empty path is no head either: the head needs a path too"
        );
    }

    #[test]
    fn a_dimension_that_does_not_fit_a_u32_gives_no_record() {
        let shapes = example();

        let record = shapes.texture_dimensions(
            "DDS loading failed for 'a': Cannot load compressed texture with non multiple of 4 \
             dimensions of 99999999999x20 and format DXT5",
        );

        assert_eq!(record, None);
    }

    #[test]
    fn regex_syntax_in_a_literal_is_matched_as_plain_text() {
        let mut shapes = LogShapes::empty();
        shapes
            .push_stack_block(stack_block_with(
                "(.*)+[{mod:text}]|^$",
                "end.*",
                "Source:{path:text}",
            ))
            .expect("room");
        let compiled = CompiledShapes::new(&shapes);

        assert!(compiled.is_stack_start("(.*)+[abc]|^$"));
        assert_eq!(
            compiled.stack_start_mod("(.*)+[abc]|^$"),
            Some("abc".to_string())
        );
        assert!(!compiled.is_stack_start("anything at all"));
        assert!(!compiled.is_stack_start("[abc]"));
        assert!(compiled.is_stack_end("end.*"));
        assert!(!compiled.is_stack_end("endless"), "a dot is not a wildcard");
    }

    #[test]
    fn any_row_of_a_role_matches_and_the_end_of_any_row_ends_a_block() {
        let mut shapes = LogShapes::empty();
        shapes
            .push_stack_block(stack_block_with(
                "[{mod:text}] first",
                "first end",
                "First:{path:text}",
            ))
            .expect("room");
        shapes
            .push_stack_block(stack_block_with(
                "[{mod:text}] second",
                "second end",
                "Second:{path:text}",
            ))
            .expect("room");
        let compiled = CompiledShapes::new(&shapes);

        assert!(compiled.is_stack_start("[A] first"));
        assert!(compiled.is_stack_start("[B] second"));
        assert!(compiled.is_stack_end("second end"));
        assert_eq!(
            compiled.stack_trailer_path("Second: /x"),
            Some("/x".to_string())
        );
    }

    #[test]
    fn a_template_at_every_bound_compiles_within_the_regex_limits() {
        // The most placeholders (8), the widest types, and near-maximal
        // literal text.
        let head = format!("{}{{path:text}}", "H".repeat(400));
        let dimensions = format!(
            "{}{{path:text}} {{width:int}} {{height:int}} {{format:token}} \
             {{a:hex}} {{b:token}} {{c:text}} {{d:int}}",
            "D".repeat(400)
        );
        let trailer = "T".repeat(512);
        let row = TextureFallbackShape::parse(&TextureFallbackSource {
            id: "row",
            head: &head,
            dimensions: &dimensions,
            trailer: &trailer,
        })
        .expect("the row parses");

        check_texture_fallback(&row).expect("compiles within the limits");
    }

    /// A printable-ASCII literal, brace-escaped, at least four bytes.
    fn literal_strategy() -> impl Strategy<Value = String> {
        "[ -~]{4,24}".prop_map(|text| text.replace('{', "{{").replace('}', "}}"))
    }

    fn kind_strategy() -> impl Strategy<Value = (&'static str, &'static str)> {
        prop_oneof![
            Just(("text", "some text")),
            Just(("token", "token")),
            Just(("int", "42")),
            Just(("hex", "AbC1")),
        ]
    }

    proptest! {
        /// Every valid template compiles, and a line built by writing each
        /// literal as itself and each placeholder as a sample of its type
        /// matches: the data can only ever say literal text and typed
        /// captures.
        #[test]
        fn any_valid_template_compiles_and_matches_its_own_instance(
            literals in proptest::collection::vec(literal_strategy(), 1..6),
            kinds in proptest::collection::vec(kind_strategy(), 0..6),
        ) {
            let mut text = String::new();
            let mut line = String::new();
            for (index, literal) in literals.iter().enumerate() {
                text.push_str(literal);
                line.push_str(&literal.replace("{{", "{").replace("}}", "}"));
                if let Some((kind, sample)) = kinds.get(index) {
                    let name = char::from(b'a' + u8::try_from(index).expect("small index"));
                    text.push_str(&format!("{{{name}:{kind}}}"));
                    line.push_str(sample);
                }
            }
            let template = LogTemplate::parse(&text).expect("a valid template");
            let matcher = PatternMatcher::compile(&template).expect("compiles");

            prop_assert!(matcher.regex.is_match(&line), "{text:?} vs {line:?}");
        }
    }
}
