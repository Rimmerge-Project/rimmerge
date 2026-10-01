//! Rendering `rimmerge.json`.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;

use super::input::Provenance;

/// Escapes `"`, `\`, and control characters for a JSON string literal —
/// `rimmerge.json` is JSON, not XML; [`escape_text`](crate::xml::escape_text)'s `&`/`<`/`>` rules
/// don't apply here and its own quoting (none) doesn't protect a literal
/// `"` or `\` a value might contain.
fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Renders a JSON array of `ids`' own text, each element double-quoted and
/// JSON-escaped — `BTreeSet`'s iteration order makes the result
/// deterministic regardless of the set's construction order.
fn json_id_array(ids: &BTreeSet<ModId>) -> String {
    ids.iter()
        .map(|id| format!("\"{}\"", escape_json(id.as_str())))
        .collect::<Vec<_>>()
        .join(",")
}

pub(super) fn render_rimmerge_json(provenance: &Provenance<'_>, rimmerge_version: &str) -> String {
    match provenance {
        Provenance::ProfileMerge {
            generated_at,
            profile_hash,
            decisions_sha256,
        } => format!(
            "{{\"kind\":\"merge\",\"generatedAt\":\"{}\",\"profileHash\":\"{}\",\"decisionsSha256\":\"{}\",\"rimmergeVersion\":\"{}\"}}\n",
            escape_json(generated_at),
            escape_json(profile_hash),
            escape_json(decisions_sha256),
            escape_json(rimmerge_version)
        ),
        Provenance::CompatPatch {
            patch_id,
            profile_hash,
            scope,
            decisions_sha256,
        } => format!(
            "{{\"kind\":\"patch\",\"patchId\":\"{}\",\"profileHash\":\"{}\",\"scope\":[{}],\"decisionsSha256\":\"{}\",\"rimmergeVersion\":\"{}\"}}\n",
            escape_json(patch_id),
            escape_json(profile_hash),
            json_id_array(scope),
            escape_json(decisions_sha256),
            escape_json(rimmerge_version)
        ),
        Provenance::Assignment {
            assignment_id,
            profile_hash,
            scope,
            content_sha256,
        } => format!(
            "{{\"kind\":\"assignment\",\"assignmentId\":\"{}\",\"profileHash\":\"{}\",\"scope\":[{}],\"contentSha256\":\"{}\",\"rimmergeVersion\":\"{}\"}}\n",
            escape_json(assignment_id),
            escape_json(profile_hash),
            json_id_array(scope),
            escape_json(content_sha256),
            escape_json(rimmerge_version)
        ),
    }
}

pub(super) fn display_name_for<'a>(
    mod_names: &'a BTreeMap<ModId, String>,
    id: &'a ModId,
) -> &'a str {
    mod_names
        .get(id)
        .map_or_else(|| id.as_str(), String::as_str)
}
