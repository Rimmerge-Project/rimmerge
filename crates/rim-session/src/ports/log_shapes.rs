//! The mod-produced `Player.log` formats, as data: [`LogShapes`].
//!
//! Three formats in a real log are printed by mods, not by the game: a
//! patch-reporting mod's stack-trace block, a texture loader's fallback
//! lines, and a patching library's back-reference stubs. They are loaded
//! from the `rimmerge-rules` `log_shapes` section (see
//! `docs/concepts/rules-databases.md`) and reach the log parser as a
//! parameter ([`super::LogFormats`]), never as adapter state.
//!
//! **Format templates, not regexes.** A mod prints these lines from format
//! strings, and a [`LogTemplate`] mirrors one: literal text plus typed
//! placeholders written `{name:type}`.
//!
//! ```text
//! template    := ( literal | placeholder )*
//! placeholder := "{" name ":" type "}"
//! name        := [a-z][a-z_]{0,31}      unique within a template
//! type        := text | token | int | hex
//! literal     := any text; "{{" and "}}" are literal braces
//! ```
//!
//! The placeholder types are fixed in code ([`PlaceholderKind`]); the data
//! can never supply regex syntax. This module is pure string work and holds
//! every bound the data must respect; the crate that compiles a template
//! into a matcher (`rim-io`) owns the regex translation.
//!
//! Every type here is correct by construction: a [`LogTemplate`], a role
//! shape or a [`LogShapes`] that exists is within its bounds and carries the
//! captures its role needs.

use std::collections::BTreeSet;

/// The most bytes in one template.
pub const MAX_TEMPLATE_BYTES: usize = 512;
/// The most placeholders in one template.
pub const MAX_PLACEHOLDERS: usize = 8;
/// The fewest bytes of literal text a template must hold. A template made of
/// placeholders alone would match every line of a log.
pub const MIN_LITERAL_BYTES: usize = 4;
/// The most bytes in a placeholder's name.
pub const MAX_PLACEHOLDER_NAME_BYTES: usize = 32;
/// The most rows one role holds.
pub const MAX_ROWS_PER_ROLE: usize = 8;
/// The most bytes in one of a row's plain strings (an xpath prefix, a
/// branch marker).
pub const MAX_MARKER_BYTES: usize = 32;
/// The most bytes in a row's id.
pub const MAX_ROW_ID_BYTES: usize = 64;

// -- templates ------------------------------------------------------------

/// What a placeholder matches. The sub-patterns are fixed in `rim-io`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceholderKind {
    /// One or more characters, spaces included.
    Text,
    /// One or more non-space characters.
    Token,
    /// One or more ASCII digits.
    Int,
    /// One to sixteen hex digits (a 64-bit id).
    Hex,
}

impl PlaceholderKind {
    fn from_wire(text: &str) -> Option<Self> {
        match text {
            "text" => Some(Self::Text),
            "token" => Some(Self::Token),
            "int" => Some(Self::Int),
            "hex" => Some(Self::Hex),
            _ => None,
        }
    }
}

/// One piece of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePart {
    /// Text that must appear as written. Never empty.
    Literal(String),
    /// A captured stretch of the line.
    Placeholder {
        /// The capture's name.
        name: String,
        /// What it matches.
        kind: PlaceholderKind,
    },
}

/// Why a template text was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TemplateError {
    /// The text is empty.
    #[error("the template is empty")]
    Empty,
    /// The text is over [`MAX_TEMPLATE_BYTES`].
    #[error("the template is {0} bytes; the limit is {MAX_TEMPLATE_BYTES}")]
    TooLong(usize),
    /// More than [`MAX_PLACEHOLDERS`] placeholders.
    #[error("the template has {0} placeholders; the limit is {MAX_PLACEHOLDERS}")]
    TooManyPlaceholders(usize),
    /// A `{` that opens no placeholder, or a `}` that closes none.
    #[error("the template has an unbalanced brace")]
    UnbalancedBrace,
    /// A placeholder that is not `{name:type}` with a valid name.
    #[error("malformed placeholder `{0}`")]
    MalformedPlaceholder(String),
    /// A placeholder type this binary does not implement.
    #[error("unknown placeholder type `{0}`")]
    UnknownPlaceholderType(String),
    /// Two placeholders share a name.
    #[error("duplicate placeholder name `{0}`")]
    DuplicateName(String),
    /// Fewer than [`MIN_LITERAL_BYTES`] bytes of literal text.
    #[error("the template has {0} bytes of literal text; at least {MIN_LITERAL_BYTES} are needed")]
    TooLittleLiteral(usize),
}

/// A parsed format template. See the module documentation for the grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogTemplate {
    parts: Vec<TemplatePart>,
}

impl LogTemplate {
    /// Parses `text`.
    ///
    /// # Errors
    ///
    /// Returns [`TemplateError`] when the text is empty, over a bound, holds
    /// a malformed or unknown placeholder, or has too little literal text.
    pub fn parse(text: &str) -> Result<Self, TemplateError> {
        if text.is_empty() {
            return Err(TemplateError::Empty);
        }
        if text.len() > MAX_TEMPLATE_BYTES {
            return Err(TemplateError::TooLong(text.len()));
        }
        let parts = split_parts(text)?;
        let placeholders = parts
            .iter()
            .filter(|part| matches!(part, TemplatePart::Placeholder { .. }))
            .count();
        if placeholders > MAX_PLACEHOLDERS {
            return Err(TemplateError::TooManyPlaceholders(placeholders));
        }
        let literal_bytes: usize = parts
            .iter()
            .map(|part| match part {
                TemplatePart::Literal(text) => text.len(),
                TemplatePart::Placeholder { .. } => 0,
            })
            .sum();
        if literal_bytes < MIN_LITERAL_BYTES {
            return Err(TemplateError::TooLittleLiteral(literal_bytes));
        }
        Ok(Self { parts })
    }

    /// The template's parts, in order.
    #[must_use]
    pub fn parts(&self) -> &[TemplatePart] {
        &self.parts
    }

    /// Whether the template names a placeholder called `name`.
    #[must_use]
    pub fn has_placeholder(&self, name: &str) -> bool {
        self.parts.iter().any(
            |part| matches!(part, TemplatePart::Placeholder { name: found, .. } if found == name),
        )
    }

    /// The whole text when the template has no placeholder (an exact-line
    /// match), else `None`.
    #[must_use]
    pub fn exact_text(&self) -> Option<&str> {
        match self.parts.as_slice() {
            [TemplatePart::Literal(text)] => Some(text),
            _ => None,
        }
    }

    /// The literal text the template starts with, when it starts with one.
    #[must_use]
    pub fn leading_literal(&self) -> Option<&str> {
        match self.parts.first() {
            Some(TemplatePart::Literal(text)) => Some(text),
            _ => None,
        }
    }

    /// The longest single run of literal text (the first, on a tie).
    /// Loose sentinels are derived from it.
    #[must_use]
    pub fn longest_literal(&self) -> &str {
        let mut longest = "";
        for part in &self.parts {
            if let TemplatePart::Literal(text) = part
                && text.len() > longest.len()
            {
                longest = text;
            }
        }
        longest
    }
}

/// Splits `text` into literal and placeholder parts; brace escapes fold into
/// the literals.
fn split_parts(text: &str) -> Result<Vec<TemplatePart>, TemplateError> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut names = BTreeSet::new();
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                literal.push('{');
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                literal.push('}');
            }
            '}' => return Err(TemplateError::UnbalancedBrace),
            '{' => {
                let mut body = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some('{') | None => return Err(TemplateError::UnbalancedBrace),
                        Some(next) => body.push(next),
                    }
                }
                if !literal.is_empty() {
                    parts.push(TemplatePart::Literal(std::mem::take(&mut literal)));
                }
                let placeholder = parse_placeholder(&body)?;
                if let TemplatePart::Placeholder { name, .. } = &placeholder
                    && !names.insert(name.clone())
                {
                    return Err(TemplateError::DuplicateName(name.clone()));
                }
                parts.push(placeholder);
            }
            other => literal.push(other),
        }
    }
    if !literal.is_empty() {
        parts.push(TemplatePart::Literal(literal));
    }
    Ok(parts)
}

fn parse_placeholder(body: &str) -> Result<TemplatePart, TemplateError> {
    let Some((name, kind_text)) = body.split_once(':') else {
        return Err(TemplateError::MalformedPlaceholder(body.to_string()));
    };
    if !is_valid_name(name) {
        return Err(TemplateError::MalformedPlaceholder(body.to_string()));
    }
    let Some(kind) = PlaceholderKind::from_wire(kind_text) else {
        return Err(TemplateError::UnknownPlaceholderType(kind_text.to_string()));
    };
    Ok(TemplatePart::Placeholder {
        name: name.to_string(),
        kind,
    })
}

/// `[a-z][a-z_]{0,31}`.
fn is_valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && name.len() <= MAX_PLACEHOLDER_NAME_BYTES
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

// -- role shapes ----------------------------------------------------------

/// Why a role row was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ShapeError {
    /// One of the row's templates failed to parse.
    #[error("`{field}`: {error}")]
    Template {
        /// The row field that held the template.
        field: &'static str,
        /// What was wrong with it.
        error: TemplateError,
    },
    /// A template lacks a placeholder its role needs.
    #[error("`{field}` must capture `{name}`")]
    MissingCapture {
        /// The row field.
        field: &'static str,
        /// The capture it lacks.
        name: &'static str,
    },
    /// A plain string is empty or over [`MAX_MARKER_BYTES`].
    #[error("`{field}` must be 1 to {MAX_MARKER_BYTES} bytes")]
    MarkerLength {
        /// The row field.
        field: &'static str,
    },
    /// The row's id is over [`MAX_ROW_ID_BYTES`].
    #[error("`id` is over {MAX_ROW_ID_BYTES} bytes")]
    IdTooLong,
}

/// The row id, when it is within [`MAX_ROW_ID_BYTES`].
fn row_id(id: &str) -> Result<String, ShapeError> {
    if id.len() > MAX_ROW_ID_BYTES {
        return Err(ShapeError::IdTooLong);
    }
    Ok(id.to_string())
}

/// Parses `text` as the template of `field`, requiring each of `captures`.
fn template_capturing(
    field: &'static str,
    text: &str,
    captures: &[&'static str],
) -> Result<LogTemplate, ShapeError> {
    let template =
        LogTemplate::parse(text).map_err(|error| ShapeError::Template { field, error })?;
    for &name in captures {
        if !template.has_placeholder(name) {
            return Err(ShapeError::MissingCapture { field, name });
        }
    }
    Ok(template)
}

fn marker(field: &'static str, text: &str) -> Result<String, ShapeError> {
    if text.is_empty() || text.len() > MAX_MARKER_BYTES {
        return Err(ShapeError::MarkerLength { field });
    }
    Ok(text.to_string())
}

/// The raw text of a stack-block row, as the data file holds it.
#[derive(Debug, Clone, Copy)]
pub struct StackBlockSource<'a> {
    /// The row's id.
    pub id: &'a str,
    /// The block's first line; captures `mod`.
    pub start: &'a str,
    /// The block's last line.
    pub end: &'a str,
    /// The optional line after the end line; captures `path`.
    pub trailer: &'a str,
    /// What an operation line's detail starts with when it names an xpath.
    pub xpath_detail_prefix: &'a str,
    /// The marker of a `match` branch in an operation line's reason.
    pub match_marker: &'a str,
    /// The marker of a `nomatch` branch.
    pub nomatch_marker: &'a str,
}

/// A patch-reporting mod's stack-trace block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchStackBlockShape {
    id: String,
    start: LogTemplate,
    end: LogTemplate,
    trailer: LogTemplate,
    xpath_detail_prefix: String,
    match_marker: String,
    nomatch_marker: String,
}

impl PatchStackBlockShape {
    /// Parses a row.
    ///
    /// # Errors
    ///
    /// Returns [`ShapeError`] when a template or marker is out of bounds or
    /// a required capture (`start`: `mod`, `trailer`: `path`) is missing.
    pub fn parse(source: &StackBlockSource<'_>) -> Result<Self, ShapeError> {
        Ok(Self {
            id: row_id(source.id)?,
            start: template_capturing("start", source.start, &["mod"])?,
            end: template_capturing("end", source.end, &[])?,
            trailer: template_capturing("trailer", source.trailer, &["path"])?,
            xpath_detail_prefix: marker("xpath_detail_prefix", source.xpath_detail_prefix)?,
            match_marker: marker("branch_markers.match", source.match_marker)?,
            nomatch_marker: marker("branch_markers.nomatch", source.nomatch_marker)?,
        })
    }

    /// The row's id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The block's first line.
    #[must_use]
    pub fn start(&self) -> &LogTemplate {
        &self.start
    }

    /// The block's last line.
    #[must_use]
    pub fn end(&self) -> &LogTemplate {
        &self.end
    }

    /// The optional line after the end line.
    #[must_use]
    pub fn trailer(&self) -> &LogTemplate {
        &self.trailer
    }

    /// What an operation line's detail starts with when it names an xpath.
    #[must_use]
    pub fn xpath_detail_prefix(&self) -> &str {
        &self.xpath_detail_prefix
    }

    /// The marker of a `match` branch.
    #[must_use]
    pub fn match_marker(&self) -> &str {
        &self.match_marker
    }

    /// The marker of a `nomatch` branch.
    #[must_use]
    pub fn nomatch_marker(&self) -> &str {
        &self.nomatch_marker
    }
}

/// The raw text of a texture-fallback row.
#[derive(Debug, Clone, Copy)]
pub struct TextureFallbackSource<'a> {
    /// The row's id.
    pub id: &'a str,
    /// Any head line of the fallback; captures `path`.
    pub head: &'a str,
    /// The head line's form that reports the size; captures `path`,
    /// `width`, `height` and `format`.
    pub dimensions: &'a str,
    /// The line that follows the head.
    pub trailer: &'a str,
}

/// A texture loader's fallback to another format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureFallbackShape {
    id: String,
    head: LogTemplate,
    dimensions: LogTemplate,
    trailer: LogTemplate,
}

impl TextureFallbackShape {
    /// Parses a row.
    ///
    /// # Errors
    ///
    /// Returns [`ShapeError`] when a template is out of bounds or a required
    /// capture is missing.
    pub fn parse(source: &TextureFallbackSource<'_>) -> Result<Self, ShapeError> {
        Ok(Self {
            id: row_id(source.id)?,
            head: template_capturing("head", source.head, &["path"])?,
            dimensions: template_capturing(
                "dimensions",
                source.dimensions,
                &["path", "width", "height", "format"],
            )?,
            trailer: template_capturing("trailer", source.trailer, &[])?,
        })
    }

    /// The row's id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Any head line of the fallback.
    #[must_use]
    pub fn head(&self) -> &LogTemplate {
        &self.head
    }

    /// The head line's form that reports the texture's size.
    #[must_use]
    pub fn dimensions(&self) -> &LogTemplate {
        &self.dimensions
    }

    /// The line that follows the head.
    #[must_use]
    pub fn trailer(&self) -> &LogTemplate {
        &self.trailer
    }
}

/// A patching library's back-reference: a trace printed once under an id,
/// then repeats that only name the id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackReferenceShape {
    id: String,
    original: LogTemplate,
    stub: LogTemplate,
}

impl BackReferenceShape {
    /// Parses a row; both templates must capture `id`.
    ///
    /// # Errors
    ///
    /// Returns [`ShapeError`] when a template is out of bounds or lacks the
    /// `id` capture.
    pub fn parse(id: &str, original: &str, stub: &str) -> Result<Self, ShapeError> {
        Ok(Self {
            id: row_id(id)?,
            original: template_capturing("original", original, &["id"])?,
            stub: template_capturing("stub", stub, &["id"])?,
        })
    }

    /// The row's id.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// The line that names the original trace.
    #[must_use]
    pub fn original(&self) -> &LogTemplate {
        &self.original
    }

    /// The line that repeats it.
    #[must_use]
    pub fn stub(&self) -> &LogTemplate {
        &self.stub
    }
}

// -- the section ----------------------------------------------------------

/// A role already holds [`MAX_ROWS_PER_ROLE`] rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the role already holds {MAX_ROWS_PER_ROLE} rows")]
pub struct RoleFullError;

/// Every mod-produced log format, one list per role. A [`Default`] value
/// knows no format: the log parser then finds no stack block, texture
/// fallback or back-reference, and their lines land in the classes the
/// engine's own shapes give them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogShapes {
    stack_blocks: Vec<PatchStackBlockShape>,
    texture_fallbacks: Vec<TextureFallbackShape>,
    back_references: Vec<BackReferenceShape>,
}

impl LogShapes {
    /// A value knowing no format.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            stack_blocks: Vec::new(),
            texture_fallbacks: Vec::new(),
            back_references: Vec::new(),
        }
    }

    /// Adds a stack-block row.
    ///
    /// # Errors
    ///
    /// Returns [`RoleFullError`] when the role holds [`MAX_ROWS_PER_ROLE`].
    pub fn push_stack_block(&mut self, shape: PatchStackBlockShape) -> Result<(), RoleFullError> {
        push_bounded(&mut self.stack_blocks, shape)
    }

    /// Adds a texture-fallback row.
    ///
    /// # Errors
    ///
    /// Returns [`RoleFullError`] when the role holds [`MAX_ROWS_PER_ROLE`].
    pub fn push_texture_fallback(
        &mut self,
        shape: TextureFallbackShape,
    ) -> Result<(), RoleFullError> {
        push_bounded(&mut self.texture_fallbacks, shape)
    }

    /// Adds a back-reference row.
    ///
    /// # Errors
    ///
    /// Returns [`RoleFullError`] when the role holds [`MAX_ROWS_PER_ROLE`].
    pub fn push_back_reference(&mut self, shape: BackReferenceShape) -> Result<(), RoleFullError> {
        push_bounded(&mut self.back_references, shape)
    }

    /// The stack-block rows.
    #[must_use]
    pub fn stack_blocks(&self) -> &[PatchStackBlockShape] {
        &self.stack_blocks
    }

    /// The texture-fallback rows.
    #[must_use]
    pub fn texture_fallbacks(&self) -> &[TextureFallbackShape] {
        &self.texture_fallbacks
    }

    /// The back-reference rows.
    #[must_use]
    pub fn back_references(&self) -> &[BackReferenceShape] {
        &self.back_references
    }

    /// Whether no role holds a row.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stack_blocks.is_empty()
            && self.texture_fallbacks.is_empty()
            && self.back_references.is_empty()
    }
}

fn push_bounded<Row>(rows: &mut Vec<Row>, row: Row) -> Result<(), RoleFullError> {
    if rows.len() >= MAX_ROWS_PER_ROLE {
        return Err(RoleFullError);
    }
    rows.push(row);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_splits_into_literals_and_typed_placeholders() {
        let template = LogTemplate::parse("[{mod:text} - Start]").expect("parses");

        assert_eq!(
            template.parts(),
            [
                TemplatePart::Literal("[".to_string()),
                TemplatePart::Placeholder {
                    name: "mod".to_string(),
                    kind: PlaceholderKind::Text
                },
                TemplatePart::Literal(" - Start]".to_string()),
            ]
        );
        assert_eq!(template.leading_literal(), Some("["));
        assert_eq!(template.longest_literal(), " - Start]");
        assert!(template.has_placeholder("mod"));
        assert_eq!(template.exact_text(), None);
    }

    #[test]
    fn a_template_with_no_placeholder_is_an_exact_text() {
        let template = LogTemplate::parse("[End of trace]").expect("parses");

        assert_eq!(template.exact_text(), Some("[End of trace]"));
    }

    #[test]
    fn doubled_braces_are_literal_braces() {
        let template = LogTemplate::parse("a {{b}} {x:int}").expect("parses");

        assert_eq!(template.leading_literal(), Some("a {b} "));
    }

    #[test]
    fn a_malformed_template_is_refused_with_its_own_reason() {
        let cases = [
            ("", TemplateError::Empty),
            ("abcd {x", TemplateError::UnbalancedBrace),
            ("abcd }", TemplateError::UnbalancedBrace),
            ("abcd {{x}", TemplateError::UnbalancedBrace),
            (
                "abcd {x}",
                TemplateError::MalformedPlaceholder("x".to_string()),
            ),
            (
                "abcd {X:text}",
                TemplateError::MalformedPlaceholder("X:text".to_string()),
            ),
            (
                "abcd {x:float}",
                TemplateError::UnknownPlaceholderType("float".to_string()),
            ),
            (
                "abcd {x:int} {x:int}",
                TemplateError::DuplicateName("x".to_string()),
            ),
            ("{a:text}", TemplateError::TooLittleLiteral(0)),
            ("ab{a:text}", TemplateError::TooLittleLiteral(2)),
        ];
        for (text, expected) in cases {
            assert_eq!(LogTemplate::parse(text), Err(expected), "template {text:?}");
        }
    }

    #[test]
    fn the_size_and_placeholder_bounds_are_enforced() {
        let long = "x".repeat(MAX_TEMPLATE_BYTES + 1);
        let at_limit = "x".repeat(MAX_TEMPLATE_BYTES);
        let with_placeholders = |count: usize| {
            let placeholders: String = ('a'..='z')
                .take(count)
                .map(|name| format!("{{{name}:int}} "))
                .collect();
            format!("abcd {placeholders}")
        };

        assert!(LogTemplate::parse(&at_limit).is_ok());
        assert_eq!(
            LogTemplate::parse(&long),
            Err(TemplateError::TooLong(MAX_TEMPLATE_BYTES + 1))
        );
        assert!(LogTemplate::parse(&with_placeholders(MAX_PLACEHOLDERS)).is_ok());
        assert_eq!(
            LogTemplate::parse(&with_placeholders(MAX_PLACEHOLDERS + 1)),
            Err(TemplateError::TooManyPlaceholders(MAX_PLACEHOLDERS + 1))
        );
    }

    #[test]
    fn a_row_missing_a_required_capture_is_refused() {
        let source = StackBlockSource {
            id: "row",
            start: "[start of trace]",
            end: "[end of trace]",
            trailer: "Source file:{path:text}",
            xpath_detail_prefix: "xpath=",
            match_marker: "<match>",
            nomatch_marker: "<nomatch>",
        };

        assert_eq!(
            PatchStackBlockShape::parse(&source),
            Err(ShapeError::MissingCapture {
                field: "start",
                name: "mod"
            })
        );
    }

    #[test]
    fn a_row_with_an_empty_marker_is_refused() {
        let source = StackBlockSource {
            id: "row",
            start: "[{mod:text} - start]",
            end: "[end of trace]",
            trailer: "Source file:{path:text}",
            xpath_detail_prefix: "",
            match_marker: "<match>",
            nomatch_marker: "<nomatch>",
        };

        assert_eq!(
            PatchStackBlockShape::parse(&source),
            Err(ShapeError::MarkerLength {
                field: "xpath_detail_prefix"
            })
        );
    }

    #[test]
    fn a_row_id_over_the_limit_is_refused_in_every_role() {
        let long = "i".repeat(MAX_ROW_ID_BYTES + 1);
        let at_limit = "i".repeat(MAX_ROW_ID_BYTES);

        assert!(BackReferenceShape::parse(&at_limit, "[Ref {id:hex}]", "[Ref {id:hex}] x").is_ok());
        assert_eq!(
            BackReferenceShape::parse(&long, "[Ref {id:hex}]", "[Ref {id:hex}] x"),
            Err(ShapeError::IdTooLong)
        );
        assert_eq!(
            TextureFallbackShape::parse(&TextureFallbackSource {
                id: &long,
                head: "failed {path:text}",
                dimensions: "failed {path:text} {width:int}x{height:int} {format:token}",
                trailer: "using png",
            }),
            Err(ShapeError::IdTooLong)
        );
        assert_eq!(
            PatchStackBlockShape::parse(&StackBlockSource {
                id: &long,
                start: "[{mod:text} - start]",
                end: "[end of trace]",
                trailer: "Source file:{path:text}",
                xpath_detail_prefix: "xpath=",
                match_marker: "<match>",
                nomatch_marker: "<nomatch>",
            }),
            Err(ShapeError::IdTooLong)
        );
    }

    #[test]
    fn a_role_holds_at_most_the_row_limit() {
        let row = BackReferenceShape::parse("row", "[Ref {id:hex}]", "[Ref {id:hex}] repeat")
            .expect("parses");
        let mut shapes = LogShapes::empty();

        for _ in 0..MAX_ROWS_PER_ROLE {
            shapes.push_back_reference(row.clone()).expect("room");
        }

        assert_eq!(shapes.push_back_reference(row), Err(RoleFullError));
        assert_eq!(shapes.back_references().len(), MAX_ROWS_PER_ROLE);
    }
}
