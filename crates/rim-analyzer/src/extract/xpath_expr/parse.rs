//! The supported-subset parser: head, steps, and what follows the head.

use crate::domain::{DefTarget, Selector};

use super::lex::{contains_outside_quotes, extract_brackets, is_valid_name, split_top_level};
use super::predicates::parse_predicate;
use super::{Predicate, Step, XPathExpr, is_document_root};
use crate::extract::xpath_target;

/// [`parse`]'s fallible body — every `Err` becomes an
/// [`XPathExpr::Unsupported`] reason verbatim.
pub(super) fn parse_supported(xpath: &str) -> Result<XPathExpr, String> {
    if is_document_root(xpath) {
        return Ok(XPathExpr::DocumentRoot);
    }
    let Some(head) = xpath_target::locate_head(xpath) else {
        return Err(format!(
            "xpath does not match a recognized Defs/<Type>[defName=...] head: '{xpath}'"
        ));
    };

    let after_head = parse_after_head(xpath, head.end)?;
    let targets = head_targets(head.def_type, head.predicate, after_head.sub_path.as_ref())?;

    Ok(XPathExpr::Supported {
        targets,
        root_predicates: after_head.root_predicates,
        steps: after_head.steps,
        selects_text: after_head.selects_text,
    })
}

/// Everything after a head's own closing `]`, parsed once — shared by
/// [`parse_supported`] (the name-identifying head case) and
/// [`head_content_predicate`] (the content-query one): extra `[...]`
/// predicates on the def node itself, the `/`-separated steps (with their own
/// `text()` handling), and the raw joined sub-path text
/// `analysis::edges::child_value_targets`/`xpath_target::parse_all` key a
/// `DefTarget` collision on. Two callers computing this independently would
/// risk drifting on exactly the details that must stay right (root predicates
/// excluded from `sub_path`, a bare `//` rejected rather than silently
/// stripped).
pub(super) struct AfterHead {
    pub(super) root_predicates: Vec<Predicate>,
    pub(super) steps: Vec<Step>,
    pub(super) selects_text: bool,
    pub(super) sub_path: Option<String>,
}

pub(super) fn parse_after_head(xpath: &str, head_end: usize) -> Result<AfterHead, String> {
    let after_head = &xpath[head_end..];
    let Some((root_brackets, remainder)) = xpath_target::root_predicate_brackets(after_head) else {
        return Err(format!("unbalanced '[' after the head: '{after_head}'"));
    };
    let root_predicates = root_brackets
        .into_iter()
        .map(parse_predicate)
        .collect::<Result<Vec<_>, _>>()?;

    // `trim_start`: a real op wraps its xpath across lines, so the sub-path
    // can begin with a newline and tabs before its own `/`. Only the *steps*
    // parse is trimmed — `sub_path` below deliberately keeps computing
    // exactly what `xpath_target::parse_all` computes
    // (`trim_start_matches('/')` on the untrimmed remainder), because the two
    // must keep addressing the identical `DefTarget` collision key.
    let (steps, selects_text) = parse_remainder(remainder.trim_start())?;

    // The same `sub_path` `xpath_target::parse_all` computes, so a target
    // built here and one built there address the same collision key.
    let trimmed = remainder.trim_start_matches('/');
    let sub_path = (!trimmed.is_empty()).then(|| trimmed.to_string());

    Ok(AfterHead {
        root_predicates,
        steps,
        selects_text,
        sub_path,
    })
}

/// Everything after the head and its root predicates: nothing, or a plain
/// `/step` continuation. A bare `//` right after the head is the case
/// that matters — `xpath_target`'s own sub-path extraction strips *every*
/// leading `/`, which would silently erase the distinction if reused here.
fn parse_remainder(remainder: &str) -> Result<(Vec<Step>, bool), String> {
    if remainder.is_empty() {
        return Ok((Vec::new(), false));
    }
    if remainder.starts_with("//") {
        return Err(format!(
            "'//' is not supported outside the head: '{remainder}'"
        ));
    }
    match remainder.strip_prefix('/') {
        Some(sub_path) => parse_steps(sub_path),
        None => Err(format!("unsupported token after head: '{remainder}'")),
    }
}

/// Turns a strictly parsed head predicate into one [`DefTarget`] per def
/// it names. The predicate must be a pure `or` disjunction of
/// `defName="..."` / `@Name="..."` equalities: anything else (an `and`
/// term, a `not(...)`, some other equality) is dropped silently by
/// `xpath_target`'s looser scan, which is fine for collision detection
/// but would mean replaying an op against defs it doesn't really match.
fn head_targets(
    def_type: &str,
    head_predicate: &str,
    sub_path: Option<&String>,
) -> Result<Vec<DefTarget>, String> {
    let predicate = parse_predicate(head_predicate)
        .map_err(|reason| format!("unsupported head predicate ({reason}): '{head_predicate}'"))?;
    let mut names = Vec::new();
    collect_head_names(&predicate, &mut names).map_err(|()| {
        format!("head predicate must be one or more defName=\"...\"/@Name=\"...\" equalities joined by 'or', with no other terms: '{head_predicate}'"
        )
    })?;

    let mut targets: Vec<DefTarget> = Vec::with_capacity(names.len());
    for (def_name, selector) in names {
        let target = DefTarget {
            def_type: def_type.to_string(),
            def_name,
            selector,
            sub_path: sub_path.cloned(),
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    Ok(targets)
}

/// Flattens an `or`-only predicate tree into its `(def_name, selector)`
/// leaves, in source order. `Err(())` for any other shape.
fn collect_head_names(predicate: &Predicate, out: &mut Vec<(String, Selector)>) -> Result<(), ()> {
    match predicate {
        Predicate::Or(left, right) => {
            collect_head_names(left, out)?;
            collect_head_names(right, out)
        }
        Predicate::ChildText(name, value) if name == "defName" => {
            out.push((value.clone(), Selector::DefName));
            Ok(())
        }
        Predicate::Attr(name, value) if name == "Name" => {
            out.push((value.clone(), Selector::NameAttr));
            Ok(())
        }
        _ => Err(()),
    }
}

/// The one head shape [`xpath_target::locate_head`] cannot find, because
/// it has no bracket to anchor on: a bare `Defs/<Type>` (or `*/<Type>`,
/// `//<Type>`) followed by `/` or nothing — `/Defs/ThingDef/race/…`, the
/// 102-row `example.jobmatrix` shape. Returns the type and the
/// byte index just past it, the same `(def_type, end)` contract
/// [`xpath_target::HeadSpan`] carries, so [`parse_after_head`] can take
/// it unchanged.
pub(super) fn locate_bare_type_head(xpath: &str) -> Option<(&str, usize)> {
    const ROOT_PREFIXES: [&str; 3] = ["Defs/", "*/", "//"];
    let start = ROOT_PREFIXES
        .iter()
        .filter_map(|prefix| xpath.find(prefix).map(|index| index + prefix.len()))
        .min()?;
    let rest = xpath.get(start..)?;
    let name_len = rest
        .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-')))
        .unwrap_or(rest.len());
    let def_type = &rest[..name_len];
    if !def_type
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
    {
        return None;
    }
    let end = start + name_len;
    let tail = xpath.get(end..)?.trim_start();
    // A `[` here means the head *does* have a predicate and
    // `locate_head` already owns it; anything else that isn't a step
    // separator is not a head this function understands.
    if !(tail.is_empty() || tail.starts_with('/')) {
        return None;
    }
    Some((def_type, end))
}

/// Splits a `/`-separated sub-path into steps, recognizing a trailing
/// `text()` step (which selects the previous step's text content rather
/// than adding a step of its own).
fn parse_steps(sub_path: &str) -> Result<(Vec<Step>, bool), String> {
    if contains_outside_quotes(sub_path, "..") {
        return Err("'..' is not supported".to_string());
    }
    if contains_outside_quotes(sub_path, "//") {
        return Err("'//' is not supported outside the head".to_string());
    }
    if contains_outside_quotes(sub_path, "|") {
        return Err("'|' (union) is not supported".to_string());
    }

    let raw_steps = split_top_level(sub_path, '/');
    let last_index = raw_steps.len() - 1;
    let mut steps = Vec::with_capacity(raw_steps.len());
    let mut selects_text = false;
    for (index, raw) in raw_steps.into_iter().enumerate() {
        if raw.trim() == "text()" {
            if index != last_index {
                return Err("'text()' is only supported as the final step".to_string());
            }
            selects_text = true;
            continue;
        }
        steps.push(parse_step(raw)?);
    }
    Ok((steps, selects_text))
}

fn parse_step(raw: &str) -> Result<Step, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("empty step".to_string());
    }
    if raw.starts_with('*') {
        return Err("'*' is not supported outside the head".to_string());
    }

    let bracket_start = raw.find('[');
    // `.trim()`: real authors write `mechEnabledWorkTypes [li
    // [text()="Hauling"] ]`, whose step name would otherwise be
    // `"mechEnabledWorkTypes "` — trailing space and all — and fail
    // `is_valid_name`.
    let name = bracket_start.map_or(raw, |index| &raw[..index]).trim();
    if !is_valid_name(name) {
        return Err(format!("unsupported step name: '{name}'"));
    }

    let predicates = match bracket_start {
        None => Vec::new(),
        Some(index) => extract_brackets(&raw[index..])?
            .into_iter()
            .map(parse_predicate)
            .collect::<Result<Vec<_>, _>>()?,
    };

    Ok(Step {
        name: name.to_string(),
        predicates,
    })
}
