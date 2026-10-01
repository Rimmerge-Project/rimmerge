//! [`coverage`]: the patch maker's work queue, pure over an already-read
//! active list.

use std::collections::BTreeMap;

use rim_analyzer::domain::{LoadOrder, ModId};

use crate::domain::finding::DefKey;
use crate::domain::merge::FieldPath;

use super::precedence::{ExistingMatch, PrecedenceRule, Winner};
use super::project::{AssignmentProject, TargetRef};
use super::section::RowKey;

/// One already-active assignment instance's use of one target key field
/// to name one target def — what a session-layer read of every active
/// instance of the assignment type flattens down to, across the *whole*
/// active list (in or out of R/T), for [`coverage`] to group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingInstance {
    /// The instance's owning mod.
    pub owner: ModId,
    /// The instance's own `defName`.
    pub instance_def_name: String,
    /// The `FieldRole::TargetKey` field this use was found through.
    pub key_field: FieldPath,
    /// The target def named.
    pub target: DefKey,
}

/// Whether a [`CoverageRow`]'s target already has an existing match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowIntent {
    /// No existing instance references this target: a row here is new
    /// coverage.
    Cover,
    /// At least one existing instance already references this target: a
    /// row here is an override.
    Override,
}

/// One coverage work-queue entry: a candidate target, what already
/// references it, and — under a known rule — who would actually win.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverageRow {
    /// The target, matched through one key field.
    pub target: TargetRef,
    /// The target def's own owning mod.
    pub owner: ModId,
    /// Existing instances naming this target, in the selected order.
    pub matches: Vec<ExistingMatch>,
    /// [`RowIntent::Cover`] when `matches` is empty,
    /// [`RowIntent::Override`] otherwise.
    pub intent: RowIntent,
    /// `Some` only under a non-[`PrecedenceRule::Unverified`] rule, and
    /// only when there's something to rank.
    pub winner: Option<Winner>,
    /// Whether this project already has a row for this target.
    pub has_row: bool,
}

/// Every candidate target, sorted uncovered-first, then by owner, then by
/// def name — the work queue.
///
/// `applicable` is `false` (with `rows` always empty in that case) when
/// `def_type` names no section, or a free-standing ("new def") one — a
/// section whose schema has no [`super::FieldRole::TargetKey`] field at
/// all (`Section::is_standalone`) — since coverage's whole premise
/// ("which candidate targets of T already have a matching instance") has
/// no meaning without a target key to match through. A caller reports this
/// as "not applicable", never as an empty (and therefore misleadingly
/// "fully covered" or "nothing to do") queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    /// `false` for a standalone project — see this type's own doc comment.
    pub applicable: bool,
    /// Every coverage row, in display order. Always empty when
    /// `applicable` is `false`.
    pub rows: Vec<CoverageRow>,
}

impl Default for Coverage {
    /// An applicable, empty queue — the right default for "no candidates
    /// yet computed", never silently read as "not applicable".
    fn default() -> Self {
        Self {
            applicable: true,
            rows: Vec::new(),
        }
    }
}

/// Builds the coverage work queue.
///
/// **Why an explicit candidate list**: "every target-key-typed def
/// owned by T and matching the learned target shape" needs each
/// candidate's resolved tree read against `TargetShape::matches` — IO
/// this pure crate cannot do. `candidates` is that already-computed set
/// (target → the target def's own owning mod), built by the session
/// layer; this function stays pure over it. A project row
/// whose target isn't in `candidates` (e.g. its owner left T since the
/// row was made) is simply not surfaced here — that staleness is
/// `UpdateAssignment`'s own concern, not this function's.
///
/// `def_type` selects which of the project's
/// sections coverage is computed for — coverage applies per target-keyed
/// section, never to the whole (potentially multi-section) project at
/// once.
#[must_use]
pub fn coverage(
    project: &AssignmentProject,
    def_type: &str,
    candidates: &BTreeMap<TargetRef, ModId>,
    existing: &[ExistingInstance],
    order: &LoadOrder,
    rule: &PrecedenceRule,
) -> Coverage {
    let Some(section) = project.section(def_type) else {
        return Coverage {
            applicable: false,
            rows: Vec::new(),
        };
    };
    if section.is_standalone() {
        return Coverage {
            applicable: false,
            rows: Vec::new(),
        };
    }

    let mut rows = Vec::with_capacity(candidates.len());

    for (target, owner) in candidates {
        let mut matches: Vec<ExistingMatch> = existing
            .iter()
            .filter(|instance| {
                instance.key_field == target.key_field && instance.target == target.def
            })
            .map(|instance| ExistingMatch {
                owner: instance.owner.clone(),
                instance_def_name: instance.instance_def_name.clone(),
                key_field: instance.key_field.clone(),
            })
            .collect();
        matches.sort_by(|a, b| {
            (
                order.position(&a.owner).unwrap_or(usize::MAX),
                &a.owner,
                &a.instance_def_name,
            )
                .cmp(&(
                    order.position(&b.owner).unwrap_or(usize::MAX),
                    &b.owner,
                    &b.instance_def_name,
                ))
        });

        let has_row = section.rows.contains_key(&RowKey::Target(target.clone()));
        let intent = if matches.is_empty() {
            RowIntent::Cover
        } else {
            RowIntent::Override
        };
        let this_project = has_row.then(|| (project.identity().package_id(), &target.key_field));
        let winner = rule.winner(&matches, this_project, order);

        rows.push(CoverageRow {
            target: target.clone(),
            owner: owner.clone(),
            matches,
            intent,
            winner,
            has_row,
        });
    }

    rows.sort_by(|a, b| {
        (
            matches!(a.intent, RowIntent::Override),
            &a.owner,
            &a.target.def.def_name,
        )
            .cmp(&(
                matches!(b.intent, RowIntent::Override),
                &b.owner,
                &b.target.def.def_name,
            ))
    });

    Coverage {
        applicable: true,
        rows,
    }
}
