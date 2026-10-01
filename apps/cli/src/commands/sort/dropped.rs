//! The `sort --dropped` listing: every edge the sorter had to drop, with the
//! cycle it would have closed and, for a direct two-mod contradiction, the
//! edge that overruled it.

use std::fmt::Write as _;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::RuleOrigin;
use rim_resolve::sort::{DroppedEdge, EdgeProvenance, Layer, OrderingEdge, Tier};

use crate::common::TerminalSafe;

/// Renders the dropped edges (those touching `only_mod`, when given) as one
/// block each, in `(edge, witness cycle)` order.
///
/// # Errors
///
/// Fails only if an edge kind cannot be named, which a unit enum never does.
pub(super) fn render(dropped: &[DroppedEdge], only_mod: Option<&ModId>) -> anyhow::Result<String> {
    let mut selected: Vec<&DroppedEdge> = dropped
        .iter()
        .filter(|entry| only_mod.is_none_or(|id| touches(&entry.edge, id)))
        .collect();
    selected.sort_by(|a, b| (&a.edge, &a.witness_cycle).cmp(&(&b.edge, &b.witness_cycle)));

    let mut text = String::new();
    writeln!(
        text,
        "Dropped edges ({} of {}):",
        selected.len(),
        dropped.len()
    )?;
    for entry in selected {
        write_block(&mut text, entry)?;
    }
    Ok(text)
}

/// Whether either end of `edge` is `id`, compared through `ModId::base()`
/// so a `_steam` suffix never hides a mod.
fn touches(edge: &OrderingEdge, id: &ModId) -> bool {
    let wanted = id.base();
    edge.after.base() == wanted || edge.before.base() == wanted
}

fn write_block(text: &mut String, entry: &DroppedEdge) -> anyhow::Result<()> {
    writeln!(text, "  {}", describe_edge(&entry.edge)?)?;
    let cycle: Vec<String> = entry
        .witness_cycle
        .iter()
        .map(|id| TerminalSafe::line(id).to_string())
        .collect();
    writeln!(text, "      cycle: {}", cycle.join(" -> "))?;
    if let Some(winner) = &entry.winner {
        writeln!(text, "      overruled by: {}", describe_edge(winner)?)?;
    }
    Ok(())
}

/// `after <- before  [layer] kind: detail`: `after` must load after `before`.
fn describe_edge(edge: &OrderingEdge) -> anyhow::Result<String> {
    Ok(format!(
        "{} <- {}  [{}] {}",
        TerminalSafe::line(&edge.after),
        TerminalSafe::line(&edge.before),
        format_layer(edge.layer),
        describe_provenance(&edge.provenance)?
    ))
}

fn describe_provenance(provenance: &EdgeProvenance) -> anyhow::Result<String> {
    Ok(match provenance {
        EdgeProvenance::Engine { kind, detail } => {
            let name = serde_json::to_string(kind)?;
            format!("{}: {}", name.trim_matches('"'), TerminalSafe::line(detail))
        }
        EdgeProvenance::AnyOf { assembly } => {
            format!("any-of choice for {}", TerminalSafe::line(assembly))
        }
        EdgeProvenance::Rule { origin, comment } => {
            let base = format_rule_origin(*origin);
            match comment {
                Some(note) => format!("{base}: {}", TerminalSafe::line(note)),
                None => base.to_string(),
            }
        }
        EdgeProvenance::Tier { tier } => format!("tier boundary ({})", format_tier(*tier)),
    })
}

fn format_tier(tier: Tier) -> &'static str {
    match tier {
        Tier::Core => "Core",
        Tier::Dlc => "Dlc",
        Tier::Top => "Top",
        Tier::Body => "Body",
        Tier::Bottom => "Bottom",
    }
}

fn format_rule_origin(origin: RuleOrigin) -> &'static str {
    match origin {
        RuleOrigin::UserDecision => "your decision",
        RuleOrigin::RimSortUser => "RimSort user rule",
        RuleOrigin::RimSortCommunity => "RimSort community rule",
        RuleOrigin::SteamDb => "Steam dependency",
    }
}

fn format_layer(layer: Layer) -> &'static str {
    match layer {
        Layer::Hard => "hard",
        Layer::AnyOf => "any-of",
        Layer::DeclaredOverride => "declared-override",
        Layer::Declared => "declared",
        Layer::UserDecision => "user-decision",
        Layer::RimSortUser => "rimsort-user",
        Layer::RimSortCommunity => "rimsort-community",
        Layer::SteamDb => "steam-db",
        Layer::Inferred => "inferred",
        Layer::Soft => "soft",
        Layer::Awareness => "awareness",
    }
}

#[cfg(test)]
#[path = "dropped_tests.rs"]
mod tests;
