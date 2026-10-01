//! Predicate parsing: boolean combinations, atoms, relative paths, negation,
//! child filters, `contains(text(), ...)`.

use super::lex::{
    extract_brackets, is_valid_name, split_boolean, split_equality, split_top_level,
    split_top_level_equality, strip_matching_parens, strip_quotes,
};
use super::{FILTER_DEPTH_COST, MAX_PREDICATE_DEPTH, NOT_DEPTH_COST, Predicate};

/// Parses one bracketed predicate string (the entry point [`parse_step`]
/// and [`head_unsupported_reason`] both use) into a [`Predicate`] tree.
pub(super) fn parse_predicate(text: &str) -> Result<Predicate, String> {
    parse_boolean(text, 0)
}

/// The boolean-composition layer of the grammar: strips wrapping parens,
/// then splits on `or` before `and` (XPath's `and` binds tighter, so
/// splitting on the loosest operator first at each level builds the tree
/// with the correct precedence — see the module doc for worked examples),
/// falling through to [`parse_atom`] for a single non-boolean predicate.
///
/// `depth` counts recursive calls (one per paren-unwrap or per `and`/`or`
/// split) and is capped at [`MAX_PREDICATE_DEPTH`] so a pathological
/// multi-thousand-term chain or paren nest fails cleanly instead of
/// overflowing the stack.
fn parse_boolean(text: &str, depth: u32) -> Result<Predicate, String> {
    if depth > MAX_PREDICATE_DEPTH {
        return Err("predicate too deep".to_string());
    }
    let text = text.trim();

    if let Some(inner) = strip_matching_parens(text) {
        return parse_boolean(inner, depth + 1);
    }
    // `or` binds loosest, so it splits first: the resulting two sides may
    // still contain `and`, which then splits on the recursive call.
    if let Some((left, right)) = split_boolean(text, "or") {
        return Ok(Predicate::Or(
            Box::new(parse_boolean(left, depth + 1)?),
            Box::new(parse_boolean(right, depth + 1)?),
        ));
    }
    if let Some((left, right)) = split_boolean(text, "and") {
        return Ok(Predicate::And(
            Box::new(parse_boolean(left, depth + 1)?),
            Box::new(parse_boolean(right, depth + 1)?),
        ));
    }

    parse_atom(text, depth)
}

/// A single non-boolean predicate: attribute/child-text/`text()`
/// equality, `not(...)`, `contains(text(), …)`, a `name[...]` child
/// filter, or a 1-based position — the leaves [`parse_boolean`]'s tree
/// bottoms out at. `depth` is [`parse_boolean`]'s own recursion budget,
/// threaded through because the `not(...)` and `name[...]` arms now
/// recurse back into it.
fn parse_atom(text: &str, depth: u32) -> Result<Predicate, String> {
    if let Some(rest) = text.strip_prefix('@') {
        let (name, value) = split_equality(rest)
            .ok_or_else(|| format!("unsupported attribute predicate: '@{rest}'"))?;
        return Ok(Predicate::Attr(name.to_string(), value.to_string()));
    }

    if let Some(negation) = parse_negation(text, depth) {
        return negation;
    }

    if let Some(rest) = text.strip_prefix("text()") {
        let rest = rest.trim_start();
        let value = rest
            .strip_prefix('=')
            .and_then(|after_eq| strip_quotes(after_eq.trim()))
            .ok_or_else(|| format!("unsupported text() predicate: 'text(){rest}'"))?;
        return Ok(Predicate::Text(value.to_string()));
    }

    if let Some(rest) = text.strip_prefix("position()") {
        let rest = rest.trim_start();
        let Some(value) = rest.strip_prefix('=').map(str::trim_start) else {
            return Err(format!("unsupported position() predicate: '{text}'"));
        };
        let Ok(position) = value.parse::<u32>() else {
            return Err(format!("unsupported position() predicate: '{text}'"));
        };
        if position == 0 {
            return Err(format!(
                "position predicates are 1-based; '{text}' is not valid"
            ));
        }
        return Ok(Predicate::Position(position));
    }

    // The one `contains` shape this grammar admits, checked *before* the
    // blanket function rejection below so every other form of it
    // (`contains(@Class, …)`, `contains(defName, …)`) still lands there.
    if let Some(value) = parse_contains_text(text) {
        return Ok(Predicate::Contains(value.to_string()));
    }

    for function in ["contains(", "starts-with(", "count(", "last("] {
        if text.starts_with(function) {
            return Err(format!("unsupported function: '{function}'"));
        }
    }

    // `name[pred][pred]…` — a child filter at arbitrary depth. Checked before
    // `split_equality` because a bracketed form (`li [text()="X"]`) has an
    // `=` inside the brackets that would otherwise be read as this
    // predicate's own equality and rejected. A `[` inside a quoted literal
    // (`label="a[b]"`) never reaches here: the name portion in front of it
    // then fails `is_valid_name`.
    if let Some(filter) = parse_child_filter(text, depth)? {
        return Ok(filter);
    }

    if let Ok(position) = text.parse::<u32>() {
        if position == 0 {
            return Err(format!(
                "position predicates are 1-based; '{text}' is not valid"
            ));
        }
        return Ok(Predicate::Position(position));
    }

    if let Some((name, value)) = split_equality(text) {
        if is_valid_name(name) {
            return Ok(Predicate::ChildText(name.to_string(), value.to_string()));
        }
        // A two-step relative path (`things/li="Column"`) — `is_valid_name`
        // rejects a `/`, so a longer chain (`a/b/c="x"`, `inner` still
        // containing one) never reaches this arm. It falls through to
        // `parse_relative_path` rather than to the final `Err`, and is
        // modelled as nested `Child`s; keeping *this* arm first is what
        // preserves the exact `NestedChildText` shape the scan-time index
        // pattern-matches (see that variant's own doc comment).
        if let Some((outer, inner)) = name.split_once('/')
            && is_valid_name(outer)
            && is_valid_name(inner)
        {
            return Ok(Predicate::NestedChildText(
                outer.to_string(),
                inner.to_string(),
                value.to_string(),
            ));
        }
    }

    // A bare name is XPath's own child-existence test (`[comps]`).
    if is_valid_name(text) {
        return Ok(Predicate::Has(text.to_string()));
    }

    // A *relative location path*, with or without a trailing equality —
    // `costList/ComponentIndustrial` (the 32-row `example.bionicsfork` shape)
    // and
    // `./comps/li[@Class="CompProperties_Power"]/compClass="CompPowerTrader"`
    // (a real install's device-standby mod one). Checked last, so every shape
    // an earlier arm already models — `ChildText`, `NestedChildText`'s exact
    // two-segment form — keeps the tree it has always built.
    if let Some(predicate) = parse_relative_path(text, depth)? {
        return Ok(predicate);
    }

    Err(format!("unrecognized predicate: '{text}'"))
}

/// One `name('[' pred ']')*` segment of a relative location path: its
/// element name, and the `and`-combination of its own bracket predicates
/// (`None` when it has none). `depth` is threaded so a nested filter
/// counts against [`MAX_PREDICATE_DEPTH`] like any other.
fn parse_path_segment(
    raw: &str,
    depth: u32,
) -> Result<Option<(String, Option<Predicate>)>, String> {
    let raw = raw.trim();
    let bracket_start = raw.find('[');
    let name = bracket_start.map_or(raw, |index| &raw[..index]).trim();
    if !is_valid_name(name) {
        return Ok(None);
    }
    let Some(bracket_start) = bracket_start else {
        return Ok(Some((name.to_string(), None)));
    };
    let filter = extract_brackets(raw[bracket_start..].trim_end())?
        .into_iter()
        .map(|block| parse_boolean(block, depth + FILTER_DEPTH_COST))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .reduce(|left, right| Predicate::And(Box::new(left), Box::new(right)));
    Ok(Some((name.to_string(), filter)))
}

/// A relative location path used as a predicate: two or more
/// `/`-separated [`parse_path_segment`]s, optionally ending in an
/// `="literal"` value test. `Ok(None)` — not an error — for anything
/// that isn't one, so [`parse_atom`] falls through to its own
/// "unrecognized predicate" message rather than this function's.
///
/// **Scope, and why it is wider than [`Predicate::NestedChildText`]'s own
/// two-segment rule**: `a/b/c="x"` has exactly one reading in XPath ("some
/// `a` has some `b` that has a `c` whose text is `x`"), and
/// [`Predicate::Child`]'s evaluation expresses it exactly, so refusing it
/// would buy no safety. Real installs need it: over a hundred skipped rows
/// can sit behind a single head of this shape (a device-standby mod's,
/// `EX_ChemfuelStoveLarge` among them). `NestedChildText` itself is
/// unaffected: its arm runs first, so the two-segment shape
/// `analysis::edges::child_value_targets` and `extract::defs` pattern-match
/// still parses to exactly that variant.
///
/// A leading `./` (XPath's explicit self axis) is stripped; it means the
/// same thing as its absence, which is why real authors write both.
fn parse_relative_path(text: &str, depth: u32) -> Result<Option<Predicate>, String> {
    let text = text.trim();
    let text = text.strip_prefix("./").unwrap_or(text);
    let (path, value) = match split_top_level_equality(text) {
        Some((path, value)) => (path, Some(value)),
        None => (text, None),
    };

    let raw_segments = split_top_level(path, '/');
    if raw_segments.len() < 2 {
        return Ok(None);
    }
    // Each segment becomes one more `Predicate::Child` level, so it costs one
    // unit of budget like an `or` term does — the fold below is iterative,
    // but the tree it builds is dropped, and evaluated, recursively (without
    // this a 20,000-segment path overflows the stack even in release).
    let mut segments = Vec::with_capacity(raw_segments.len());
    for (index, raw) in raw_segments.into_iter().enumerate() {
        let level = depth.saturating_add(u32::try_from(index).unwrap_or(u32::MAX));
        if level > MAX_PREDICATE_DEPTH {
            return Err("predicate too deep".to_string());
        }
        let Some(segment) = parse_path_segment(raw, level)? else {
            return Ok(None);
        };
        segments.push(segment);
    }

    // Innermost first: the last segment carries the value test, if any.
    let Some((name, filter)) = segments.pop() else {
        return Ok(None);
    };
    let mut predicate = match (filter, value) {
        (None, Some(value)) => Predicate::ChildText(name, value.to_string()),
        (None, None) => Predicate::Has(name),
        (Some(filter), None) => Predicate::Child(name, Box::new(filter)),
        (Some(filter), Some(value)) => Predicate::Child(
            name,
            Box::new(Predicate::And(
                Box::new(filter),
                Box::new(Predicate::Text(value.to_string())),
            )),
        ),
    };
    while let Some((name, filter)) = segments.pop() {
        let inner = match filter {
            None => predicate,
            Some(filter) => Predicate::And(Box::new(filter), Box::new(predicate)),
        };
        predicate = Predicate::Child(name, Box::new(inner));
    }
    Ok(Some(predicate))
}

/// `not(` … `)` over any predicate this grammar can express — `None` (not an
/// error) when `text` is not that shape at all, so [`parse_atom`] falls
/// through to its own later arms. `not(comps)` still parses to
/// `Not(Has("comps"))`. `strip_suffix(')')` is what keeps the real malformed
/// author predicate `not(petness)="petness"` out — it ends in a quote, so it
/// is not this shape, falls through, and stays `Unsupported` permanently.
fn parse_negation(text: &str, depth: u32) -> Option<Result<Predicate, String>> {
    let inner = text.strip_prefix("not(")?.strip_suffix(')')?.trim();
    if inner.is_empty() {
        return Some(Err("unsupported not() argument: ''".to_string()));
    }
    Some(
        parse_boolean(inner, depth + NOT_DEPTH_COST)
            .map(|argument| Predicate::Not(Box::new(argument)))
            .map_err(|reason| format!("unsupported not() argument ({reason}): '{inner}'")),
    )
}

/// `name[pred][pred]…` — a child filter, at any depth. `Ok(None)` when
/// `text` carries no bracket at all, or when what precedes the bracket
/// isn't a valid element name (a quoted literal containing one,
/// `label="a[b]"`, lands here and is correctly declined).
fn parse_child_filter(text: &str, depth: u32) -> Result<Option<Predicate>, String> {
    if !text.contains('[') {
        return Ok(None);
    }
    let Some((name, Some(filter))) = parse_path_segment(text, depth)? else {
        return Ok(None);
    };
    Ok(Some(Predicate::Child(name, Box::new(filter))))
}

/// The literal inside `contains(text(), "value")` — `None` for every
/// other `contains(...)` form, which then falls through to
/// [`parse_atom`]'s blanket function rejection. Whitespace is tolerated
/// around the comma and inside the parens (real authors write
/// `contains(text(), "Storage")` with a space, and
/// `thingClass [contains(text(),"Storage")]` without one).
fn parse_contains_text(text: &str) -> Option<&str> {
    let inner = text.strip_prefix("contains(")?.strip_suffix(')')?;
    let (subject, value) = inner.split_once(',')?;
    if subject.trim() != "text()" {
        return None;
    }
    strip_quotes(value.trim())
}
