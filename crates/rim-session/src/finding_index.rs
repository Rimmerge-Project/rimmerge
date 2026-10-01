//! [`FindingIndex`]: per-`OrderSource` paging/filtering over a
//! [`Ledger`]'s entries, rebuilt whenever the ledger it indexes changes
//! (see [`crate::Session`]).

use std::collections::{BTreeMap, HashMap};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{FindingKey, Ledger, ResolutionStatus};

/// The maximum number of entries [`FindingIndex::page`] ever returns in
/// one call, regardless of the requested [`FindingFilter::limit`].
pub const MAX_PAGE_SIZE: usize = 200;

/// A finding's kind, independent of its specific payload — lets
/// [`FindingFilter::kinds`] filter without matching on every
/// [`FindingKey`] field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FindingKind {
    /// See [`FindingKey::EdgeDropped`].
    EdgeDropped,
    /// See [`FindingKey::AnyOfChoice`].
    AnyOfChoice,
    /// See [`FindingKey::DefOverride`].
    DefOverride,
    /// See [`FindingKey::PatchCollision`].
    PatchCollision,
    /// See [`FindingKey::TextureOverride`].
    TextureOverride,
    /// See [`FindingKey::DuplicateAssembly`].
    DuplicateAssembly,
    /// See [`FindingKey::LikelyDuplicateMod`].
    LikelyDuplicateMod,
    /// See [`FindingKey::MissingMod`].
    MissingMod,
    /// See [`FindingKey::MissingDependency`].
    MissingDependency,
    /// See [`FindingKey::IncompatiblePair`].
    IncompatiblePair,
    /// See [`FindingKey::UnsupportedVersion`].
    UnsupportedVersion,
    /// See [`FindingKey::UndeclaredHardDependency`].
    UndeclaredHardDependency,
    /// See [`FindingKey::LazyReferenceViolated`].
    LazyReferenceViolated,
    /// See [`FindingKey::DeclarationQuestioned`].
    DeclarationQuestioned,
    /// See [`FindingKey::DeclarationOverridden`].
    DeclarationOverridden,
    /// See [`FindingKey::DuplicateTemplateName`].
    DuplicateTemplateName,
    /// See [`FindingKey::KeyedTranslationCollision`].
    KeyedTranslationCollision,
    /// See [`FindingKey::SoundOverride`].
    SoundOverride,
    /// See [`FindingKey::UndeclaredTypeDependency`].
    UndeclaredTypeDependency,
    /// See [`FindingKey::RuntimePatchCollision`].
    RuntimePatchCollision,
    /// See [`FindingKey::TranspilerCollision`].
    TranspilerCollision,
    /// See [`FindingKey::TagInferred`].
    TagInferred,
    /// See [`FindingKey::RuleOverruled`].
    RuleOverruled,
    /// See [`FindingKey::PlacementOverruled`].
    PlacementOverruled,
    /// See [`FindingKey::PlacementQuestioned`].
    PlacementQuestioned,
    /// See [`FindingKey::PlacementOrderingOverridden`].
    PlacementOrderingOverridden,
    /// See [`FindingKey::PlacementPromotesDependents`].
    PlacementPromotesDependents,
    /// See [`FindingKey::MissingTexturePath`].
    MissingTexturePath,
    /// See [`FindingKey::PatchWillFail`].
    PatchWillFail,
    /// See [`FindingKey::ContributesNothing`].
    ContributesNothing,
    /// See [`FindingKey::UndecodableTexture`].
    UndecodableTexture,
    /// See [`FindingKey::BrokenInheritance`].
    BrokenInheritance,
    /// See [`FindingKey::NearMissModReference`].
    NearMissModReference,
    /// See [`FindingKey::DiscardedAddition`].
    DiscardedAddition,
    /// See [`FindingKey::DanglingDefReference`].
    DanglingDefReference,
}

impl FindingKind {
    /// The kind of `key`.
    #[must_use]
    pub fn of(key: &FindingKey) -> Self {
        match key {
            FindingKey::EdgeDropped { .. } => Self::EdgeDropped,
            FindingKey::AnyOfChoice { .. } => Self::AnyOfChoice,
            FindingKey::DefOverride { .. } => Self::DefOverride,
            FindingKey::PatchCollision { .. } => Self::PatchCollision,
            FindingKey::TextureOverride { .. } => Self::TextureOverride,
            FindingKey::DuplicateAssembly { .. } => Self::DuplicateAssembly,
            FindingKey::LikelyDuplicateMod { .. } => Self::LikelyDuplicateMod,
            FindingKey::MissingMod { .. } => Self::MissingMod,
            FindingKey::MissingDependency { .. } => Self::MissingDependency,
            FindingKey::IncompatiblePair { .. } => Self::IncompatiblePair,
            FindingKey::UnsupportedVersion { .. } => Self::UnsupportedVersion,
            FindingKey::UndeclaredHardDependency { .. } => Self::UndeclaredHardDependency,
            FindingKey::LazyReferenceViolated { .. } => Self::LazyReferenceViolated,
            FindingKey::DeclarationQuestioned { .. } => Self::DeclarationQuestioned,
            FindingKey::DeclarationOverridden { .. } => Self::DeclarationOverridden,
            FindingKey::DuplicateTemplateName { .. } => Self::DuplicateTemplateName,
            FindingKey::KeyedTranslationCollision { .. } => Self::KeyedTranslationCollision,
            FindingKey::SoundOverride { .. } => Self::SoundOverride,
            FindingKey::UndeclaredTypeDependency { .. } => Self::UndeclaredTypeDependency,
            FindingKey::RuntimePatchCollision { .. } => Self::RuntimePatchCollision,
            FindingKey::TranspilerCollision { .. } => Self::TranspilerCollision,
            FindingKey::TagInferred { .. } => Self::TagInferred,
            FindingKey::RuleOverruled { .. } => Self::RuleOverruled,
            FindingKey::PlacementOverruled { .. } => Self::PlacementOverruled,
            FindingKey::PlacementQuestioned { .. } => Self::PlacementQuestioned,
            FindingKey::PlacementOrderingOverridden { .. } => Self::PlacementOrderingOverridden,
            FindingKey::PlacementPromotesDependents { .. } => Self::PlacementPromotesDependents,
            FindingKey::MissingTexturePath { .. } => Self::MissingTexturePath,
            FindingKey::PatchWillFail { .. } => Self::PatchWillFail,
            FindingKey::ContributesNothing { .. } => Self::ContributesNothing,
            FindingKey::UndecodableTexture { .. } => Self::UndecodableTexture,
            FindingKey::BrokenInheritance { .. } => Self::BrokenInheritance,
            FindingKey::NearMissModReference { .. } => Self::NearMissModReference,
            FindingKey::DiscardedAddition { .. } => Self::DiscardedAddition,
            FindingKey::DanglingDefReference { .. } => Self::DanglingDefReference,
        }
    }
}

/// Every [`ModId`] `key` names, for [`FindingFilter::mod_id`].
fn mod_ids_of(key: &FindingKey) -> Vec<&ModId> {
    match key {
        FindingKey::EdgeDropped { after, before, .. }
        | FindingKey::UndeclaredHardDependency { after, before }
        | FindingKey::LazyReferenceViolated { after, before } => vec![after, before],
        FindingKey::DeclarationQuestioned {
            declared_after,
            declared_before,
            ..
        }
        | FindingKey::DeclarationOverridden {
            declared_after,
            declared_before,
            ..
        } => vec![declared_after, declared_before],
        FindingKey::AnyOfChoice { after, .. } => vec![after],
        FindingKey::DefOverride { owners, .. } => owners.iter().collect(),
        FindingKey::PatchCollision { mods, .. } => mods.iter().collect(),
        FindingKey::TextureOverride { owners, .. } => owners.iter().collect(),
        FindingKey::DuplicateAssembly { owners, .. } => owners.iter().collect(),
        FindingKey::DuplicateTemplateName { owners, .. } => owners.iter().collect(),
        FindingKey::SoundOverride { owners, .. } => owners.iter().collect(),
        FindingKey::LikelyDuplicateMod { pair }
        | FindingKey::IncompatiblePair { pair }
        | FindingKey::KeyedTranslationCollision { pair } => {
            vec![&pair.0, &pair.1]
        }
        FindingKey::RuntimePatchCollision { owners, .. }
        | FindingKey::TranspilerCollision { owners, .. } => owners.iter().collect(),
        FindingKey::UndeclaredTypeDependency { user, provider, .. } => vec![user, provider],
        FindingKey::MissingMod { mod_id } | FindingKey::UnsupportedVersion { mod_id } => {
            vec![mod_id]
        }
        FindingKey::MissingDependency { mod_id, dependency } => vec![mod_id, dependency],
        FindingKey::TagInferred { mod_id, .. } => vec![mod_id],
        FindingKey::RuleOverruled { after, before, .. } => vec![after, before],
        FindingKey::PlacementOverruled { mod_id, .. }
        | FindingKey::PlacementQuestioned { mod_id, .. } => vec![mod_id],
        FindingKey::PlacementOrderingOverridden { mod_id, pinned, .. } => vec![mod_id, pinned],
        FindingKey::PlacementPromotesDependents { mod_id, .. } => vec![mod_id],
        FindingKey::MissingTexturePath { referrer, .. } => vec![referrer],
        FindingKey::PatchWillFail { mod_id, .. } => vec![mod_id],
        FindingKey::ContributesNothing { mod_id } => vec![mod_id],
        FindingKey::UndecodableTexture { mod_id, .. } => vec![mod_id],
        FindingKey::BrokenInheritance { mod_id, .. } => vec![mod_id],
        FindingKey::NearMissModReference { referrer, .. } => vec![referrer],
        FindingKey::DiscardedAddition {
            replacer, adder, ..
        } => vec![replacer, adder],
        // The key names no mod at all — see this variant's own doc
        // comment (`referrers` are `Finding` evidence, never part of the
        // identity), so a `mod_id` filter never matches this finding.
        FindingKey::DanglingDefReference { .. } => Vec::new(),
    }
}

/// Filters and paging for [`FindingIndex::page`]. `limit` is capped at
/// [`MAX_PAGE_SIZE`] regardless of the requested value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FindingFilter {
    /// Only findings with this status.
    pub status: Option<ResolutionStatus>,
    /// Only findings of one of these kinds.
    pub kinds: Option<Vec<FindingKind>>,
    /// Only findings naming this mod (see [`mod_ids_of`]).
    pub mod_id: Option<ModId>,
    /// Only findings whose canonical key text contains this substring
    /// (case-insensitive).
    pub search: Option<String>,
    /// How many matching entries to skip before collecting the page.
    pub offset: usize,
    /// How many entries to collect, capped at [`MAX_PAGE_SIZE`].
    pub limit: usize,
}

/// One page of [`FindingIndex::page`], plus the total number of entries
/// matching the filter (before paging).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingPage {
    /// Total entries matching the filter, before `offset`/`limit`.
    pub total: usize,
    /// This page's keys, in the index's stable sort order.
    pub items: Vec<FindingKey>,
}

fn status_rank(status: ResolutionStatus) -> u8 {
    match status {
        ResolutionStatus::NeedsInput => 0,
        ResolutionStatus::Auto => 1,
        ResolutionStatus::UserOverridden => 2,
    }
}

/// Sorted-and-searchable view over one [`Ledger`]'s entries: an index
/// sorted by `(status, confidence ascending, key)` — needs-input first,
/// lowest confidence first within a status — for stable paging, plus a
/// key -> entry-index map for direct lookup.
#[derive(Debug, Clone)]
pub struct FindingIndex {
    /// Indices into the ledger's own `entries`, in stable sort order.
    sorted: Vec<usize>,
    by_key: HashMap<FindingKey, usize>,
    /// Live `NeedsInput` finding counts per mod, keyed by [`ModId::base`]
    /// so a `_steam`-suffixed variant and its base id share one count.
    /// Computed once here (rather than per `list_order` row) since it
    /// only ever changes when the ledger itself is rebuilt.
    needs_input_by_mod: BTreeMap<ModId, usize>,
}

impl FindingIndex {
    /// Builds an index over `ledger`'s entries.
    #[must_use]
    pub fn build(ledger: &Ledger) -> Self {
        let mut sorted: Vec<usize> = (0..ledger.entries.len()).collect();
        sorted.sort_by(|&a, &b| {
            let left = &ledger.entries[a];
            let right = &ledger.entries[b];
            status_rank(left.status)
                .cmp(&status_rank(right.status))
                .then(left.suggestion.confidence.cmp(&right.suggestion.confidence))
                .then(left.key.cmp(&right.key))
        });
        let by_key = ledger
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.key.clone(), index))
            .collect();
        let mut needs_input_by_mod: BTreeMap<ModId, usize> = BTreeMap::new();
        for entry in &ledger.entries {
            if entry.status != ResolutionStatus::NeedsInput {
                continue;
            }
            // A `BTreeSet` first so a key naming the same base mod more
            // than once (e.g. a pair-shaped key with both sides equal)
            // still counts as one finding for that mod, not two.
            let mods: std::collections::BTreeSet<ModId> = mod_ids_of(&entry.key)
                .into_iter()
                .map(ModId::base)
                .collect();
            for mod_id in mods {
                *needs_input_by_mod.entry(mod_id).or_default() += 1;
            }
        }
        Self {
            sorted,
            by_key,
            needs_input_by_mod,
        }
    }

    /// The entry-index for `key`, if it's in this ledger.
    #[must_use]
    pub fn index_of(&self, key: &FindingKey) -> Option<usize> {
        self.by_key.get(key).copied()
    }

    /// Live `NeedsInput` finding counts per mod (keyed by
    /// [`ModId::base`]), computed once when this index was built.
    #[must_use]
    pub fn needs_input_by_mod(&self) -> &BTreeMap<ModId, usize> {
        &self.needs_input_by_mod
    }

    /// Filters and pages `ledger`'s entries (the same ledger this index
    /// was built from), returning keys in stable sort order.
    #[must_use]
    pub fn page(&self, ledger: &Ledger, filter: &FindingFilter) -> FindingPage {
        let limit = filter.limit.min(MAX_PAGE_SIZE);
        let search = filter.search.as_deref().map(str::to_lowercase);

        let matching: Vec<&FindingKey> = self
            .sorted
            .iter()
            .map(|&index| &ledger.entries[index])
            .filter(|entry| filter.status.is_none_or(|status| entry.status == status))
            .filter(|entry| {
                filter
                    .kinds
                    .as_ref()
                    .is_none_or(|kinds| kinds.contains(&FindingKind::of(&entry.key)))
            })
            .filter(|entry| {
                filter.mod_id.as_ref().is_none_or(|id| {
                    let target = id.base();
                    mod_ids_of(&entry.key)
                        .iter()
                        .any(|candidate| candidate.base() == target)
                })
            })
            .filter(|entry| {
                search
                    .as_deref()
                    .is_none_or(|needle| entry.key.to_string().to_lowercase().contains(needle))
            })
            .map(|entry| &entry.key)
            .collect();

        let total = matching.len();
        let items = matching
            .into_iter()
            .skip(filter.offset)
            .take(limit)
            .cloned()
            .collect();
        FindingPage { total, items }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::EdgeKind;
    use rim_resolve::domain::{Action, Confidence, Finding, OrderSource, Resolution, Suggestion};

    use super::*;

    fn resolution(key: FindingKey, status: ResolutionStatus, confidence: u8) -> Resolution {
        Resolution {
            key: key.clone(),
            finding: Finding::MissingMod {
                mod_id: ModId::new("placeholder"),
            },
            suggestion: Suggestion {
                action: Action::Accept,
                confidence: Confidence::new(confidence).unwrap_or_else(|_| unreachable!()),
                rationale: rim_resolve::domain::Rationale::MissingModNotInstalled,
                alternatives: Vec::new(),
            },
            status,
            effective: Action::Accept,
            decision: None,
            resolved_by_suggested: None,
            merge: None,
            structural_guard_field: None,
            scope: None,
        }
    }

    fn sample_ledger() -> Ledger {
        Ledger {
            source: OrderSource::Current,
            entries: vec![
                resolution(
                    FindingKey::MissingMod {
                        mod_id: ModId::new("z.mod"),
                    },
                    ResolutionStatus::Auto,
                    90,
                ),
                resolution(
                    FindingKey::MissingMod {
                        mod_id: ModId::new("a.mod"),
                    },
                    ResolutionStatus::NeedsInput,
                    40,
                ),
                resolution(
                    FindingKey::EdgeDropped {
                        after: ModId::new("x"),
                        before: ModId::new("y"),
                        kind: EdgeKind::MayRequire,
                    },
                    ResolutionStatus::NeedsInput,
                    20,
                ),
            ],
            stats: rim_resolve::domain::LedgerStats::default(),
        }
    }

    #[test]
    fn sorts_by_status_then_confidence_then_key() {
        let ledger = sample_ledger();
        let index = FindingIndex::build(&ledger);
        let page = index.page(
            &ledger,
            &FindingFilter {
                limit: 10,
                ..FindingFilter::default()
            },
        );

        assert_eq!(
            page.items,
            vec![
                FindingKey::EdgeDropped {
                    after: ModId::new("x"),
                    before: ModId::new("y"),
                    kind: EdgeKind::MayRequire,
                },
                FindingKey::MissingMod {
                    mod_id: ModId::new("a.mod"),
                },
                FindingKey::MissingMod {
                    mod_id: ModId::new("z.mod"),
                },
            ],
            "NeedsInput before Auto; within NeedsInput, lower confidence first"
        );
    }

    #[test]
    fn filters_by_status() {
        let ledger = sample_ledger();
        let index = FindingIndex::build(&ledger);
        let page = index.page(
            &ledger,
            &FindingFilter {
                status: Some(ResolutionStatus::Auto),
                limit: 10,
                ..FindingFilter::default()
            },
        );

        assert_eq!(page.total, 1);
        assert_eq!(
            page.items,
            vec![FindingKey::MissingMod {
                mod_id: ModId::new("z.mod"),
            }]
        );
    }

    #[test]
    fn filters_by_mod_id() {
        let ledger = sample_ledger();
        let index = FindingIndex::build(&ledger);
        let page = index.page(
            &ledger,
            &FindingFilter {
                mod_id: Some(ModId::new("x")),
                limit: 10,
                ..FindingFilter::default()
            },
        );

        assert_eq!(page.total, 1);
        assert!(matches!(page.items[0], FindingKey::EdgeDropped { .. }));
    }

    #[test]
    fn search_matches_the_canonical_key_text_case_insensitively() {
        let ledger = sample_ledger();
        let index = FindingIndex::build(&ledger);
        let page = index.page(
            &ledger,
            &FindingFilter {
                search: Some("Z.MOD".to_string()),
                limit: 10,
                ..FindingFilter::default()
            },
        );

        assert_eq!(page.total, 1);
    }

    /// A ledger with more entries than [`MAX_PAGE_SIZE`] — the only way
    /// to prove the cap is real rather than trivially true because the
    /// sample ledger happens to be small.
    fn oversized_ledger() -> Ledger {
        let entries = (0..MAX_PAGE_SIZE + 50)
            .map(|i| {
                resolution(
                    FindingKey::MissingMod {
                        mod_id: ModId::new(format!("mod{i}")),
                    },
                    ResolutionStatus::NeedsInput,
                    50,
                )
            })
            .collect();
        Ledger {
            source: OrderSource::Current,
            entries,
            stats: rim_resolve::domain::LedgerStats::default(),
        }
    }

    #[test]
    fn limit_is_capped_at_max_page_size() {
        let ledger = oversized_ledger();
        let index = FindingIndex::build(&ledger);
        let page = index.page(
            &ledger,
            &FindingFilter {
                limit: 10_000,
                ..FindingFilter::default()
            },
        );

        assert_eq!(page.total, MAX_PAGE_SIZE + 50);
        assert_eq!(
            page.items.len(),
            MAX_PAGE_SIZE,
            "the page must be capped even though more matched and a larger limit was requested"
        );
    }

    #[test]
    fn filters_by_mod_id_compare_base_ids() {
        let mut ledger = sample_ledger();
        ledger.entries.push(resolution(
            FindingKey::UndeclaredHardDependency {
                after: ModId::new("addon_steam"),
                before: ModId::new("framework"),
            },
            ResolutionStatus::NeedsInput,
            60,
        ));
        let index = FindingIndex::build(&ledger);

        let page = index.page(
            &ledger,
            &FindingFilter {
                mod_id: Some(ModId::new("addon")),
                limit: 10,
                ..FindingFilter::default()
            },
        );

        assert_eq!(
            page.total, 1,
            "a search for the base id must match a _steam-suffixed variant"
        );
    }

    #[test]
    fn offset_skips_leading_matches() {
        let ledger = sample_ledger();
        let index = FindingIndex::build(&ledger);
        let page = index.page(
            &ledger,
            &FindingFilter {
                offset: 1,
                limit: 10,
                ..FindingFilter::default()
            },
        );

        assert_eq!(page.total, 3);
        assert_eq!(page.items.len(), 2);
    }

    #[test]
    fn index_of_finds_the_entry_index_for_a_key() {
        let ledger = sample_ledger();
        let index = FindingIndex::build(&ledger);
        let key = FindingKey::MissingMod {
            mod_id: ModId::new("a.mod"),
        };

        assert_eq!(index.index_of(&key), Some(1));
        assert_eq!(
            index.index_of(&FindingKey::MissingMod {
                mod_id: ModId::new("nobody"),
            }),
            None
        );
    }

    #[test]
    fn needs_input_by_mod_counts_only_needs_input_entries_by_base_id() {
        let ledger = Ledger {
            source: OrderSource::Current,
            entries: vec![
                // NeedsInput, `_steam`-suffixed: must count under the base id.
                resolution(
                    FindingKey::MissingMod {
                        mod_id: ModId::new("a.mod_steam"),
                    },
                    ResolutionStatus::NeedsInput,
                    40,
                ),
                // NeedsInput, same base id via the plain (non-suffixed) form:
                // must add to the same bucket as the entry above.
                resolution(
                    FindingKey::UnsupportedVersion {
                        mod_id: ModId::new("a.mod"),
                    },
                    ResolutionStatus::NeedsInput,
                    10,
                ),
                // Auto, not NeedsInput: must not be counted at all.
                resolution(
                    FindingKey::MissingMod {
                        mod_id: ModId::new("z.mod"),
                    },
                    ResolutionStatus::Auto,
                    90,
                ),
            ],
            stats: rim_resolve::domain::LedgerStats::default(),
        };

        let index = FindingIndex::build(&ledger);
        let counts = index.needs_input_by_mod();

        assert_eq!(counts.get(&ModId::new("a.mod")), Some(&2));
        assert_eq!(counts.get(&ModId::new("a.mod_steam")), None);
        assert_eq!(counts.get(&ModId::new("z.mod")), None);
    }
}
