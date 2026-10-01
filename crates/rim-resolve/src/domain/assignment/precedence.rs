//! [`PrecedenceRule`]: which of several matching instances the game
//! actually picks. Pure shape and ranking only; *which* def type carries
//! which rule is data, loaded into `rim_session::ModKnowledge`.

use rim_analyzer::domain::{LoadOrder, ModId};
use serde::{Deserialize, Serialize};

use crate::domain::merge::{FieldPath, PathSegment};

/// One existing (already-active) assignment instance that names a
/// coverage target through some key field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExistingMatch {
    /// The instance's owning mod.
    pub owner: ModId,
    /// The instance's own `defName`.
    pub instance_def_name: String,
    /// The `FieldRole::TargetKey` field the match was found through.
    pub key_field: FieldPath,
}

/// Which candidate won a target, under a known [`PrecedenceRule`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Winner {
    /// An existing, already-active instance.
    Existing(ExistingMatch),
    /// This project's own row, as if already exported.
    ThisProject,
}

/// A verified (or not) rule for which of several matching instances the
/// game actually picks. Data,
/// never inferred: which def type has which rule is loaded from the
/// rimmerge-rules data file into `rim_session::ModKnowledge`, whose own
/// empty default leaves every def type [`Unverified`](Self::Unverified)
/// until data says otherwise. There is deliberately no built-in
/// table here: the one rule this project ever verified against real
/// source is knowledge about one specific mod, and it lives in the rules
/// repo, verbatim with its citation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "rule", rename_all = "snake_case")]
pub enum PrecedenceRule {
    /// No verified rule for this def type: `coverage` lists every match
    /// and names no winner.
    Unverified,
    /// Verified against a framework's source: prefer instances owned
    /// outside `framework`, then by `key_priority` (first key that
    /// matched wins), then the first in the selected load order.
    PreferOutsideFramework {
        /// The mod whose own instances are least preferred.
        framework: ModId,
        /// Which `FieldRole::TargetKey` field wins a tie, in priority
        /// order.
        key_priority: Vec<FieldPath>,
    },
}

/// Builds one `key_priority` entry from a plain top-level tag name — a
/// single-segment [`FieldPath`], the only shape `key_priority` ever
/// needs, and exactly what the data file's own `key_priority` strings
/// are. `pub` because
/// the loader that reads that file (`rim_io::mod_knowledge`) builds the
/// same shape and must not reinvent it.
#[must_use]
pub fn top_level_field(tag: &str) -> FieldPath {
    FieldPath::new(vec![PathSegment::Child(tag.to_string())])
}

/// One candidate's rank tuple under [`PrecedenceRule::PreferOutsideFramework`] —
/// see [`rank_key`]'s own doc comment for what each component means.
type RankKey<'a> = (bool, usize, usize, &'a str);

/// A candidate's rank under [`PrecedenceRule::PreferOutsideFramework`] —
/// smaller sorts better, so the winner is the *minimum*: owned outside
/// the framework beats owned by it (`false < true`), then a lower
/// `key_priority` index (of *this candidate's own* key field — Example's own
/// tie-break compares a kind match against a race match, two different
/// fields, not just two matches of the same field) beats a higher one
/// (absent from the list sorts last), then an earlier load position
/// beats a later one (`position` is `None` — sorts last — for this
/// project's own row exactly when its package id isn't active yet; once
/// exported it ranks by its real position like any other candidate, per
/// [`PrecedenceRule::winner`]'s own doc comment), then — a same-owner,
/// same-key-field, same-position tie, which can only happen between two
/// *existing* matches from literally the same mod — the lexically
/// smaller `instance_def_name` wins, so the result never depends on
/// `matches`'s own input order. `instance_def_name` is meaningless for
/// this project's own row (nothing to compare it against without first
/// deciding it won) and is never the deciding factor in practice: ties
/// against an existing candidate are already resolved in the existing
/// candidate's favour by [`PrecedenceRule::winner`]'s strict
/// less-than before this project's row is even considered.
fn rank_key<'a>(
    owner: &ModId,
    key_field: &FieldPath,
    instance_def_name: &'a str,
    framework: &ModId,
    key_priority: &[FieldPath],
    position: Option<usize>,
) -> RankKey<'a> {
    let key_rank = key_priority
        .iter()
        .position(|f| f == key_field)
        .unwrap_or(key_priority.len());
    (
        owner.base() == framework.base(),
        key_rank,
        position.unwrap_or(usize::MAX),
        instance_def_name,
    )
}

impl PrecedenceRule {
    /// The winner among `matches` (already-active instances, each
    /// carrying the key field it was found through) plus this project's
    /// own row (`this_project`, `Some((owner, key_field))` only when the
    /// target actually has one), under this rule and `order`.
    ///
    /// This project's own row is ranked by `order.position(owner)` —
    /// exactly like an existing match — so once this project has
    /// actually been exported and its package id is active, its row
    /// competes on real load order like anything else; before that, its
    /// position is `None` (unknown, sorts last), never claiming to load
    /// before something it hasn't been placed relative to yet. Once
    /// active, this project's *own previous export* also shows up as an
    /// `ExistingMatch` in whatever built `matches` — the caller (the
    /// session layer) must filter that instance out of `matches` itself,
    /// or the same row would be ranked twice under two different
    /// identities.
    ///
    /// Always `None` for [`PrecedenceRule::Unverified`], and `None` when
    /// there is nothing to rank at all (no matches and no row) even under
    /// a known rule.
    #[must_use]
    pub fn winner(
        &self,
        matches: &[ExistingMatch],
        this_project: Option<(&ModId, &FieldPath)>,
        order: &LoadOrder,
    ) -> Option<Winner> {
        let (framework, key_priority) = match self {
            PrecedenceRule::Unverified => return None,
            PrecedenceRule::PreferOutsideFramework {
                framework,
                key_priority,
            } => (framework, key_priority),
        };

        let mut best: Option<(RankKey<'_>, Winner)> = None;
        for candidate in matches {
            let rank = rank_key(
                &candidate.owner,
                &candidate.key_field,
                &candidate.instance_def_name,
                framework,
                key_priority,
                order.position(&candidate.owner),
            );
            if best.as_ref().is_none_or(|(best_rank, _)| rank < *best_rank) {
                best = Some((rank, Winner::Existing(candidate.clone())));
            }
        }
        if let Some((owner, key_field)) = this_project {
            let rank = rank_key(
                owner,
                key_field,
                "",
                framework,
                key_priority,
                order.position(owner),
            );
            if best.as_ref().is_none_or(|(best_rank, _)| rank < *best_rank) {
                best = Some((rank, Winner::ThisProject));
            }
        }
        best.map(|(_, winner)| winner)
    }
}
