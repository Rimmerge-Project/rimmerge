//! Hard problems in the order about to be written to `ModsConfig.xml`.
//!
//! A *hard problem* is a fact about one concrete [`LoadOrder`] that breaks
//! loading or that the user should confirm before it is written (such as
//! a not-installed mod dropping out of the active list), whatever confidence the
//! ledger put on its own suggestion. "Needs input" is a different axis
//! (how sure Rimmerge is of a suggestion); this module is deliberately
//! independent of it. The set is closed:
//!
//! 1. a required dependency that is not in the order,
//! 2. two mods in the order declared incompatible,
//! 3. a `ModsConfig.xml` mod that is not on disk,
//! 4. a violated `Hard`-strength edge,
//! 5. a load-time any-of assembly constraint with no candidate loading
//!    first (a lazily resolved one is `Soft`, like a lazy single edge).
//!
//! The ledger is read for one thing only: whether the user already decided
//! the problem's own finding (see [`PreflightItem::acknowledged`]).
//!
//! Pure: no IO, and every collection is BTree-ordered, so the output is
//! byte-identical across runs.

use std::collections::BTreeSet;

use rim_analyzer::domain::{
    Constraint, ConstraintStatus, EdgeKind, EdgeStatus, EdgeStrength, LoadOrder, MissingDependency,
    ModId, Report,
};

use crate::domain::{FindingKey, Ledger, ResolutionStatus};
use crate::evaluate;

/// Whether a missing required dependency is on disk at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Availability {
    /// Discovered on disk, but not in the order (activating it fixes this).
    InstalledInactive,
    /// Not on disk.
    NotInstalled,
}

/// What writing the order does to a `ModsConfig.xml` mod that is not on
/// disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MissingModOutcome {
    /// The order omits it, so the written list drops it.
    RemovedFromActiveList,
    /// The order keeps it; the game skips it at load.
    KeptInActiveList,
}

/// A fact about the order being written that the game will report or that
/// breaks loading. Closed; each variant carries only its own fields.
///
/// `Ord` is derived: declaration order is the display order, then the ids.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HardProblem {
    /// `mod_id` requires `dependency`, which is not in the written order.
    MissingDependency {
        /// The mod that declares the requirement.
        mod_id: ModId,
        /// The required mod.
        dependency: ModId,
        /// The author's name for the dependency, when `About.xml` gave one.
        display_name: Option<String>,
        /// Whether the dependency is installed.
        availability: Availability,
    },
    /// Both mods are in the written order and one declares the other
    /// incompatible. `a <= b`.
    IncompatiblePair {
        /// The smaller id of the pair.
        a: ModId,
        /// The larger id of the pair.
        b: ModId,
    },
    /// Listed in `ModsConfig.xml`, not on disk.
    MissingMod {
        /// The missing mod.
        mod_id: ModId,
        /// What the written order does with it.
        outcome: MissingModOutcome,
    },
    /// A `Hard`-strength edge the written order violates.
    LoadRequirementViolated {
        /// The edge's dependent side (must load later).
        after: ModId,
        /// The edge's dependency side (must load first).
        before: ModId,
        /// The kind of edge.
        kind: EdgeKind,
    },
    /// A load-time any-of assembly constraint with no candidate loading
    /// first.
    AnyOfUnsatisfied {
        /// The mod that needs one of the candidates first.
        after: ModId,
        /// The candidates, sorted.
        candidates: BTreeSet<ModId>,
    },
}

/// One hard problem and whether the user already answered it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PreflightItem {
    /// What is wrong.
    pub problem: HardProblem,
    /// True when the problem's own finding carries a user decision. A
    /// violated edge with no finding (the `Current` order) and an
    /// unsatisfied any-of are never acknowledged.
    pub acknowledged: bool,
}

impl HardProblem {
    /// The finding whose decision acknowledges this problem, if any kind
    /// of finding can.
    fn finding_key(&self) -> Option<FindingKey> {
        match self {
            Self::MissingDependency {
                mod_id, dependency, ..
            } => Some(FindingKey::MissingDependency {
                mod_id: mod_id.clone(),
                dependency: dependency.clone(),
            }),
            Self::IncompatiblePair { a, b } => Some(FindingKey::IncompatiblePair {
                pair: (a.clone(), b.clone()),
            }),
            Self::MissingMod { mod_id, .. } => Some(FindingKey::MissingMod {
                mod_id: mod_id.clone(),
            }),
            Self::LoadRequirementViolated {
                after,
                before,
                kind,
            } => Some(FindingKey::EdgeDropped {
                after: after.clone(),
                before: before.clone(),
                kind: *kind,
            }),
            Self::AnyOfUnsatisfied { .. } => None,
        }
    }
}

/// Every hard problem in `order`, sorted and de-duplicated.
///
/// `order` is the order that will be written; `ledger` is that order's
/// ledger, read only to set [`PreflightItem::acknowledged`].
#[must_use]
pub fn hard_problems(report: &Report, order: &LoadOrder, ledger: &Ledger) -> Vec<PreflightItem> {
    let decided: BTreeSet<&FindingKey> = ledger
        .entries
        .iter()
        .filter(|entry| entry.status == ResolutionStatus::UserOverridden)
        .map(|entry| &entry.key)
        .collect();

    let problems = missing_dependencies(report, order)
        .chain(incompatible_pairs(report, order))
        .chain(missing_mods(report, order))
        .chain(violated_hard_edges(report, order))
        .chain(unsatisfied_any_of(report, order));

    problems
        .map(|problem| PreflightItem {
            acknowledged: problem
                .finding_key()
                .is_some_and(|key| decided.contains(&key)),
            problem,
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn missing_dependencies<'a>(
    report: &'a Report,
    order: &LoadOrder,
) -> impl Iterator<Item = HardProblem> + 'a {
    // Declared ids never carry `_steam`; compare through `base()`. A mod
    // the order keeps but that is not on disk cannot satisfy a requirement.
    let missing: BTreeSet<ModId> = report.missing_mods.iter().map(ModId::base).collect();
    let loaded: BTreeSet<ModId> = order
        .as_slice()
        .iter()
        .map(ModId::base)
        .filter(|id| !missing.contains(id))
        .collect();
    let installed: BTreeSet<ModId> = report
        .inactive_mods
        .iter()
        .map(|inactive| inactive.id.base())
        .collect();
    let requirers: BTreeSet<ModId> = order.as_slice().iter().cloned().collect();

    report
        .missing_dependencies
        .iter()
        .filter(move |entry| requirers.contains(&entry.mod_id))
        .filter(move |entry| !loaded.contains(&entry.dependency.id.base()))
        .map(move |entry| dependency_problem(entry, &installed))
}

fn dependency_problem(entry: &MissingDependency, installed: &BTreeSet<ModId>) -> HardProblem {
    let availability = if installed.contains(&entry.dependency.id.base()) {
        Availability::InstalledInactive
    } else {
        Availability::NotInstalled
    };
    HardProblem::MissingDependency {
        mod_id: entry.mod_id.clone(),
        dependency: entry.dependency.id.clone(),
        display_name: entry.dependency.display_name.clone(),
        availability,
    }
}

fn incompatible_pairs<'a>(
    report: &'a Report,
    order: &'a LoadOrder,
) -> impl Iterator<Item = HardProblem> + 'a {
    report
        .incompatible_active_pairs
        .iter()
        .filter(|pair| order.position(&pair.a).is_some() && order.position(&pair.b).is_some())
        .map(|pair| {
            let (a, b) = if pair.a <= pair.b {
                (pair.a.clone(), pair.b.clone())
            } else {
                (pair.b.clone(), pair.a.clone())
            };
            HardProblem::IncompatiblePair { a, b }
        })
}

fn missing_mods<'a>(
    report: &'a Report,
    order: &'a LoadOrder,
) -> impl Iterator<Item = HardProblem> + 'a {
    report.missing_mods.iter().map(|mod_id| {
        let outcome = if order.position(mod_id).is_some() {
            MissingModOutcome::KeptInActiveList
        } else {
            MissingModOutcome::RemovedFromActiveList
        };
        HardProblem::MissingMod {
            mod_id: mod_id.clone(),
            outcome,
        }
    })
}

fn violated_hard_edges<'a>(
    report: &'a Report,
    order: &'a LoadOrder,
) -> impl Iterator<Item = HardProblem> + 'a {
    // Re-evaluated against `order`: `EdgeReport::status` is only the
    // verdict for the order active at scan time.
    report
        .edges
        .iter()
        .map(|entry| &entry.edge)
        .filter(|edge| edge.strength() == EdgeStrength::Hard)
        .filter(|edge| evaluate::edge_status(edge, order) == EdgeStatus::Violated)
        .map(|edge| HardProblem::LoadRequirementViolated {
            after: edge.after.clone(),
            before: edge.before.clone(),
            kind: edge.kind,
        })
}

fn unsatisfied_any_of<'a>(
    report: &'a Report,
    order: &'a LoadOrder,
) -> impl Iterator<Item = HardProblem> + 'a {
    report.constraints.iter().filter_map(|constraint| {
        let Constraint::AnyOf {
            after,
            candidates,
            load_time,
            ..
        } = constraint;
        // A lazily resolved reference is `Soft` strength, like the edges
        // `EdgeStrength` already keeps out of the hard problems: the game
        // loads without any candidate first.
        if !load_time {
            return None;
        }
        // A constraint on a mod the order does not contain is not a
        // problem with this order (`constraint_status` calls it violated
        // because nothing could satisfy it).
        order.position(after)?;
        if evaluate::constraint_status(constraint, order) == ConstraintStatus::Satisfied {
            return None;
        }
        Some(HardProblem::AnyOfUnsatisfied {
            after: after.clone(),
            candidates: candidates.iter().cloned().collect(),
        })
    })
}

#[cfg(test)]
#[path = "preflight/preflight_tests.rs"]
mod tests;
