//! Lexical helpers: bracket and quote aware splitting, quote stripping, operator boundaries, names.

use crate::extract::xpath_target;

/// Splits a sequence of `[...][...]` blocks into their inner text, e.g.
/// `[@Class="X"][2]` -> `["@Class=\"X\"", "2"]`.
pub(super) fn extract_brackets(mut s: &str) -> Result<Vec<&str>, String> {
    let mut result = Vec::new();
    while !s.is_empty() {
        if !s.starts_with('[') {
            return Err(format!("expected '[', found: '{s}'"));
        }
        let Some(end) = xpath_target::find_matching_bracket(s) else {
            return Err(format!("unbalanced '[' in: '{s}'"));
        };
        result.push(&s[1..end]);
        s = &s[end + 1..];
    }
    Ok(result)
}

/// Whether `needle` (a short, plain-ASCII structural token: `..`, `//`,
/// `|`) occurs in `s` outside of any single- or double-quoted span —
/// quoted literal values (`label="Fish and Chips"`, `label="http://x"`,
/// `@Class="A..B"`) must never trip a structural check meant for the
/// surrounding grammar, not the string content mods actually author.
pub(super) fn contains_outside_quotes(s: &str, needle: &str) -> bool {
    let bytes = s.as_bytes();
    let needle_bytes = needle.as_bytes();
    if needle_bytes.is_empty() {
        return false;
    }
    let mut quote: Option<u8> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(q) = quote {
            if byte == q {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if index + needle_bytes.len() <= bytes.len()
            && bytes[index..index + needle_bytes.len()] == *needle_bytes
        {
            return true;
        }
        index += 1;
    }
    false
}

/// Splits `s` on `delim` only at bracket/paren depth 0 and outside any
/// quoted span — so a `/` inside a step's `[...]` predicate (or inside a
/// quoted literal value) never ends the step early.
pub(super) fn split_top_level(s: &str, delim: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (index, ch) in s.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            c if c == delim && depth == 0 => {
                parts.push(&s[start..index]);
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

/// The `name = "value"` split [`parse_relative_path`] needs, done at
/// bracket/paren depth 0 and outside quoted spans — unlike
/// [`split_equality`], whose plain `find('=')` would stop at an `=` inside
/// a step predicate (`li[@Class="X"]/compClass="Y"`).
pub(super) fn split_top_level_equality(text: &str) -> Option<(&str, &str)> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    for (index, byte) in bytes.iter().enumerate() {
        if let Some(open) = quote {
            if *byte == open {
                quote = None;
            }
            continue;
        }
        match byte {
            b'"' | b'\'' => quote = Some(*byte),
            b'[' | b'(' => depth += 1,
            b']' | b')' => depth -= 1,
            b'=' if depth == 0 => {
                if index > 0 && matches!(bytes[index - 1], b'!' | b'<' | b'>') {
                    return None;
                }
                let value = strip_quotes(text[index + 1..].trim())?;
                return Some((text[..index].trim_end(), value));
            }
            _ => {}
        }
    }
    None
}

/// `name = "value"` or `name = 'value'`. Rejects `!=`/`<=`/`>=` (only
/// plain equality is in the supported grammar) and anything whose value
/// portion isn't a single, cleanly-quoted literal (see [`strip_quotes`]).
pub(super) fn split_equality(s: &str) -> Option<(&str, &str)> {
    let eq_index = s.find('=')?;
    if eq_index > 0 {
        let previous = s.as_bytes()[eq_index - 1];
        if matches!(previous, b'!' | b'<' | b'>') {
            return None;
        }
    }
    let name = s[..eq_index].trim();
    let value = strip_quotes(s[eq_index + 1..].trim())?;
    Some((name, value))
}

/// Strips one matching pair of leading/trailing quotes (`"` or `'`) from
/// `s`, rejecting the match unless the quoted span is genuinely
/// self-contained: the inner text must not itself contain the same quote
/// character. Without this check, a malformed run-on like
/// `a="x"and b="y"` (no space before `and`) would have its "value"
/// naively read as everything between the *first* and *last* `"` in the
/// whole string — `x"and b="y` — silently producing a nonsense
/// `ChildText` instead of failing to parse.
pub(super) fn strip_quotes(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let quote = bytes[0];
    if (quote != b'"' && quote != b'\'') || bytes[bytes.len() - 1] != quote {
        return None;
    }
    let inner = &s[1..s.len() - 1];
    if inner.as_bytes().contains(&quote) {
        return None;
    }
    Some(inner)
}

/// The first top-level (bracket/paren depth 0, outside any quoted span)
/// occurrence of `op` (`and`/`or`) used as an *operator* — surrounded by
/// whitespace on both sides, any amount of it: real head predicates wrap
/// across lines and indent with tabs, so a fixed `" or "` needle would
/// miss a predicate whose `or` is preceded by a newline and tabs. The
/// whitespace requirement is also what keeps a name ending in the
/// operator (`brand`, `nor`) from splitting.
pub(super) fn split_boolean<'a>(text: &'a str, op: &str) -> Option<(&'a str, &'a str)> {
    let bytes = text.as_bytes();
    let op_bytes = op.as_bytes();
    let mut depth = 0i32;
    let mut quote: Option<u8> = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(q) = quote {
            if byte == q {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' | b'\'' => {
                quote = Some(byte);
                index += 1;
                continue;
            }
            // `[`/`]` counted alongside `(`/`)`, like `split_top_level` and
            // `strip_matching_parens` — otherwise `comps[a="1" or b="2"]`
            // would split at the *inner* `or`, producing two halves that each
            // fail to parse and an error naming the wrong thing. With
            // brackets counted the split happens where XPath says it does,
            // and the child filter's own `or` is parsed by the recursive call
            // that owns it.
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            _ => {}
        }
        if depth == 0
            && index > 0
            && is_operator_boundary(bytes[index - 1])
            && index + op_bytes.len() < bytes.len()
            && bytes[index..index + op_bytes.len()] == *op_bytes
            && bytes[index + op_bytes.len()].is_ascii_whitespace()
        {
            return Some((&text[..index], &text[index + op_bytes.len()..]));
        }
        index += 1;
    }
    None
}

/// What may sit immediately *before* a top-level `and`/`or` operator:
/// whitespace, or the closing quote of a string literal. The quote case is a
/// real family that looks like "whitespace around `=`" but is nothing of the
/// sort: the real texts are `defName = "A"or defName = "B"`, i.e. a *missing*
/// space before `or`, which XPath 1.0 itself allows (a string literal is
/// self-delimiting, so the following `or` is unambiguously an operator). The
/// trailing side still requires whitespace, which is what keeps an identifier
/// merely *ending* in the operator (`brand`, `nor`) from splitting; the
/// leading side can only be a quote we already know closed a literal, since
/// this scan skips quoted spans entirely.
fn is_operator_boundary(byte: u8) -> bool {
    byte.is_ascii_whitespace() || byte == b'"' || byte == b'\''
}

/// Whether `text` is fully wrapped in one matching pair of parens (not,
/// say, `(a) and (b)`, which merely starts with `(` and ends with `)`),
/// skipping any parens inside a quoted span.
pub(super) fn strip_matching_parens(text: &str) -> Option<&str> {
    if !(text.starts_with('(') && text.ends_with(')') && text.len() >= 2) {
        return None;
    }
    let inner = &text[1..text.len() - 1];
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    for ch in inner.chars() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
    }
    (depth == 0).then_some(inner)
}

pub(super) fn is_valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}
