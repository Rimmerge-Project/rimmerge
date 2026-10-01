//! [`Decision`]: the user's own resolution of a finding, persisted across
//! sessions and re-applied by [`FindingKey`] (the rerere scheme).
//! [`DecisionSet`] holds every decision on file; [`SorterOverrides`] is
//! the subset that changes what the sorter builds.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::{EdgeKind, ModId};

use super::finding::{DefKey, FindingKey};
use super::resolution::Action;
use super::rule::ClusterRuleId;
use super::tag::Tag;

/// Why a [`Decision`] was rejected outright, before it's ever persisted.
///
/// Currently uninhabited: every [`Action`] — including [`Action::Merge`]
/// and [`Action::ShipAsset`] — is representable as a stored decision.
/// Kept (rather than removed) so a validation rule, such as a per-field
/// `MergeChoice` naming a path or owner the merge preview doesn't
/// recognize, has an error type to return without changing
/// [`DecisionSet::insert`]'s signature.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {}

/// Validates an action before it's turned into a [`Decision`]. Lives here
/// (not `ledger`) so [`DecisionSet::insert`] can enforce it directly:
/// `domain` is the foundation every other module builds on, so the
/// validation a domain invariant depends on has to live in `domain` too,
/// not in a downstream module `domain` itself never imports.
///
/// # Errors
///
/// Never returns an error today — [`ResolveError`] is currently
/// uninhabited. Exists as a seam for a future rule.
pub fn validate_action(_action: &Action) -> Result<(), ResolveError> {
    Ok(())
}

/// One user decision on a finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    /// The finding this decides.
    pub key: FindingKey,
    /// The chosen action.
    pub action: Action,
    /// An optional free-text note explaining the choice.
    pub note: Option<String>,
    /// When the decision was made.
    pub decided_at: jiff::Timestamp,
}

/// Every decision on file, keyed by [`FindingKey`] so re-applying them
/// against a fresh ledger is an exact-match lookup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DecisionSet {
    by_key: BTreeMap<FindingKey, Decision>,
}

/// Decisions that change what the sorter builds, grouped by
/// [`crate::sort`]'s own vocabulary so it can consume them without
/// re-walking every decision.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SorterOverrides {
    /// `Reorder` decisions, plus the pairs `PreferWinner` expands to:
    /// `(after, before)` meaning "after must load after before".
    pub reorders: Vec<(ModId, ModId)>,
    /// `ChooseCandidate` decisions for an any-of constraint, keyed by
    /// `(after, assembly)`.
    pub chosen_candidates: BTreeMap<(ModId, String), ModId>,
    /// `DropEdge` decisions: never add this edge.
    pub dropped_edges: BTreeSet<(ModId, ModId, EdgeKind)>,
    /// `DropRule` decisions: never add this pair rule, regardless of
    /// its origin.
    pub dropped_rules: BTreeSet<(ModId, ModId)>,
    /// `KeepEdge` decisions: never drop this edge as a cycle-break.
    pub kept_edges: BTreeSet<(ModId, ModId, EdgeKind)>,
    /// `AddTag` decisions, applied after inference.
    pub added_tags: Vec<(ModId, Tag)>,
    /// `RemoveTag` decisions, applied after inference.
    pub removed_tags: Vec<(ModId, Tag)>,
    /// `ExcludeFromCluster` decisions.
    pub excluded_from_cluster: BTreeSet<(ClusterRuleId, ModId)>,
    /// `MissingMod` findings the user chose to `Ignore`: keep the id in
    /// the emitted order instead of dropping it.
    pub kept_missing_mods: BTreeSet<ModId>,
}

/// The owner set of the `DefOverride`/`PatchCollision`/`TextureOverride`/
/// `RuntimePatchCollision`/`DuplicateTemplateName` finding matching
/// `def_key`, or empty when `finding_key` isn't one of those (or names a
/// different def/texture/target/template) — used to expand `PreferWinner`
/// into concrete `Reorder` pairs without needing anything beyond the key
/// itself. Every kind but `DefOverride`/`PatchCollision` has no real
/// `DefKey` of its own — `def_key` matches each via its own synthesized
/// stand-in (`DefKey::texture_path`/`runtime_target`/`template_name`), the
/// same synthesized key each finding kind's own `ledger::suggest` function
/// offers as its `PreferWinner` alternative.
fn owners_of(finding_key: &FindingKey, def_key: &DefKey) -> Vec<ModId> {
    match finding_key {
        FindingKey::DefOverride { key, owners } if key == def_key => {
            owners.iter().cloned().collect()
        }
        FindingKey::PatchCollision { key, mods, .. } if key == def_key => {
            mods.iter().cloned().collect()
        }
        FindingKey::TextureOverride {
            texture_path,
            owners,
        } if def_key.texture_path() == Some(texture_path.as_str()) => {
            owners.iter().cloned().collect()
        }
        FindingKey::RuntimePatchCollision {
            target_type,
            target_method,
            owners,
        } if def_key.runtime_target() == Some((target_type.as_str(), target_method.as_str())) => {
            owners.iter().cloned().collect()
        }
        FindingKey::DuplicateTemplateName { name, owners }
            if def_key.template_name() == Some(name.as_str()) =>
        {
            owners.iter().cloned().collect()
        }
        _ => Vec::new(),
    }
}

impl DecisionSet {
    /// An empty decision set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The decision on `key`, if one exists.
    #[must_use]
    pub fn get(&self, key: &FindingKey) -> Option<&Decision> {
        self.by_key.get(key)
    }

    /// Every decision on file, in `FindingKey`'s `Ord` order — stable
    /// across insertions and across process runs, since it comes straight
    /// from the backing `BTreeMap`'s own iteration order.
    pub fn iter(&self) -> impl Iterator<Item = &Decision> {
        self.by_key.values()
    }

    /// How many decisions are on file.
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_key.len()
    }

    /// Canonical hash of *what was decided*, not when: sha256 over
    /// `"{key}\n{action as json}\n"` for every decision in [`Self::iter`]'s
    /// own stable key order. The canonical form of `action` is exactly
    /// [`Action`]'s `serde_json` wire format — the same bytes
    /// `decisions.json` itself persists, never a bespoke encoding — so
    /// changing `Action`'s (or anything it embeds, e.g.
    /// [`super::merge::MergeChoice`]'s) `Serialize` impl changes this
    /// hash for every already-exported merge mod or compat patch. See
    /// `crates/rim-resolve/CLAUDE.md`'s file-format rule, which this fact
    /// extends. `note`/`decided_at` are deliberately excluded, so two
    /// decision sets with the same decisions hash the same — the number
    /// a `rimmerge.json`'s `decisionsSha256` and a later "unchanged since
    /// last export" check both rely on. The `action`'s JSON serialization
    /// orders a `Merge`'s `choices` map by `FieldPath`'s own `Ord` (a
    /// `BTreeMap`), so the hash is stable regardless of the order choices
    /// were made in. Shared by [`super::patch::PatchProject::decisions_sha256`]
    /// (a compat patch's own decisions) and the profile merge mod's own
    /// render (`rim-session::use_cases::render_merge_mod`), so both kinds
    /// of generated mod's `rimmerge.json` carry the same canonical hash.
    #[must_use]
    pub fn content_sha256(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        for decision in self.iter() {
            hasher.update(decision.key.to_string().as_bytes());
            hasher.update(b"\n");
            // INVARIANT: `Action` and everything it can embed (`MergeChoice`,
            // `FieldPath`, `ModId`, `DefKey`, ...) derive `Serialize` over
            // plain data — no manual impl, no fallible `serialize_with`
            // anywhere in the chain — so this can never actually fail.
            // `unwrap_or_else(|_| unreachable!(...))` rather than
            // `expect`/`unwrap` (denied outside tests by this workspace's
            // lints) so a future change that *does* introduce a fallible
            // path panics loudly here instead of silently hashing nothing.
            let action_json = serde_json::to_string(&decision.action)
                .unwrap_or_else(|_| unreachable!("Action always serializes"));
            hasher.update(action_json.as_bytes());
            hasher.update(b"\n");
        }
        let digest = hasher.finalize();
        digest.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// Whether no decisions are on file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }

    /// Records a decision, replacing any earlier one on the same key.
    ///
    /// # Errors
    ///
    /// Returns [`ResolveError`] (and stores nothing) when
    /// [`validate_action`] rejects `decision.action` — currently
    /// unreachable, since every action validates; kept so a future
    /// validation rule has somewhere to reject before persisting.
    pub fn insert(&mut self, decision: Decision) -> Result<Option<Decision>, ResolveError> {
        validate_action(&decision.action)?;
        Ok(self.by_key.insert(decision.key.clone(), decision))
    }

    /// Removes the decision on `key`, if one exists.
    pub fn remove(&mut self, key: &FindingKey) -> Option<Decision> {
        self.by_key.remove(key)
    }

    /// Decisions whose key no longer names a currently-live finding — an
    /// owner set changed, or the underlying condition is simply gone.
    /// Kept on file (never silently dropped) but worth surfacing as
    /// prunable.
    pub fn orphaned<'a>(
        &'a self,
        live: &'a BTreeSet<FindingKey>,
    ) -> impl Iterator<Item = &'a Decision> {
        self.by_key
            .iter()
            .filter(move |(key, _)| !live.contains(*key))
            .map(|(_, decision)| decision)
    }

    /// Classifies every decision into the shape the sorter consumes.
    ///
    /// `template_children` is an extra input for
    /// `Verse.XmlInheritance.GetBestParentFor`'s real "nearest at or
    /// before, per child" rule (ground-truthed from the decompiled
    /// engine) — every non-vanilla, non-registrant mod with a child
    /// referencing a duplicated template `Name`, keyed by that `Name`
    /// (`rim-resolve` has no XML/analyzer access of its own, so this is
    /// the caller's job: `rim-session`'s `Session` builds it from
    /// `session.sources().children_by_template`, deliberately independent
    /// of `self` — see its own doc comment for why — so a before/after
    /// comparison across one mutation can reuse the identical map for
    /// both calls). An empty map is always safe: a `PreferWinner` on a
    /// `DuplicateTemplateName` finding simply gets no children-side
    /// reorders.
    #[must_use]
    pub fn sorter_overrides(
        &self,
        template_children: &BTreeMap<String, BTreeSet<ModId>>,
    ) -> SorterOverrides {
        let mut overrides = SorterOverrides::default();

        for decision in self.by_key.values() {
            match &decision.action {
                Action::Reorder { after, before } => {
                    overrides.reorders.push((after.clone(), before.clone()));
                }
                Action::PreferWinner { key, winner } => {
                    // The chosen winner still beats every *other*
                    // registrant of the same key (`owners_of` now
                    // includes `DuplicateTemplateName` in that set too —
                    // "loads after every co-registrant" is correct there
                    // exactly as it is for the other three finding kinds
                    // this same loop serves).
                    for other in owners_of(&decision.key, key) {
                        if other != *winner {
                            overrides.reorders.push((winner.clone(), other));
                        }
                    }
                    // Additionally, the winner must load *before*
                    // every genuinely-eligible child of this template
                    // name — "before", not "after" like the loop above,
                    // and the opposite of what "last wins" would produce
                    // (a registration after a child is invisible to it).
                    // Combined with the loop above (winner after every
                    // other registrant), this places the winner exactly
                    // between the two groups — immediately before every
                    // child, since nothing else can be nearer to a child
                    // than the winner without also being a registrant
                    // this decision already orders before the winner.
                    if let Some(name) = key.template_name()
                        && let Some(children) = template_children.get(name)
                    {
                        for child in children {
                            overrides.reorders.push((child.clone(), winner.clone()));
                        }
                    }
                }
                Action::ChooseCandidate { after, chosen } => {
                    if let FindingKey::AnyOfChoice { assembly, .. } = &decision.key {
                        overrides
                            .chosen_candidates
                            .insert((after.clone(), assembly.clone()), chosen.clone());
                    }
                }
                Action::DropEdge {
                    after,
                    before,
                    kind,
                } => {
                    overrides
                        .dropped_edges
                        .insert((after.clone(), before.clone(), *kind));
                }
                Action::KeepEdge {
                    after,
                    before,
                    kind,
                } => {
                    overrides
                        .kept_edges
                        .insert((after.clone(), before.clone(), *kind));
                }
                Action::DropRule { after, before } => {
                    overrides
                        .dropped_rules
                        .insert((after.clone(), before.clone()));
                }
                Action::AddTag { mod_id, tag } => {
                    overrides.added_tags.push((mod_id.clone(), tag.clone()));
                }
                Action::RemoveTag { mod_id, tag } => {
                    overrides.removed_tags.push((mod_id.clone(), tag.clone()));
                }
                Action::ExcludeFromCluster { rule, mod_id } => {
                    overrides
                        .excluded_from_cluster
                        .insert((rule.clone(), mod_id.clone()));
                }
                Action::Ignore => {
                    if let FindingKey::MissingMod { mod_id } = &decision.key {
                        overrides.kept_missing_mods.insert(mod_id.clone());
                    }
                }
                // `PromoteRule` changes the `RuleSet` a session holds, not
                // a `SorterOverrides` layered on top of one — see
                // `Action::PromoteRule`'s own doc comment. Applied by
                // `rim_session::Session::decide` calling its own
                // `promote_imported_rule` the moment this decision is
                // recorded, the same application-layer-effect shape as
                // `RemoveMod`/`Merge`/`ShipAsset` below.
                Action::Accept
                | Action::RemoveMod { .. }
                | Action::Merge { .. }
                | Action::ShipAsset { .. }
                | Action::PromoteRule { .. } => {}
            }
        }

        overrides
    }
}

#[cfg(test)]
mod tests {
    use jiff::Timestamp;

    use super::*;

    fn decision(key: FindingKey, action: Action) -> Decision {
        Decision {
            key,
            action,
            note: None,
            decided_at: Timestamp::UNIX_EPOCH,
        }
    }

    #[test]
    fn get_and_remove_round_trip_a_decision() {
        let key = FindingKey::MissingMod {
            mod_id: ModId::new("gone.mod"),
        };
        let mut set = DecisionSet::new();
        set.insert(decision(
            key.clone(),
            Action::RemoveMod {
                mod_id: ModId::new("gone.mod"),
            },
        ))
        .unwrap();

        assert!(set.get(&key).is_some());
        let removed = set.remove(&key);
        assert!(removed.is_some());
        assert!(set.get(&key).is_none());
    }

    #[test]
    fn len_and_is_empty_track_the_number_of_decisions_on_file() {
        let mut set = DecisionSet::new();
        assert_eq!(set.len(), 0);
        assert!(set.is_empty());

        set.insert(decision(
            FindingKey::MissingMod {
                mod_id: ModId::new("a"),
            },
            Action::Ignore,
        ))
        .unwrap();

        assert_eq!(set.len(), 1);
        assert!(!set.is_empty());
    }

    #[test]
    fn iter_yields_decisions_in_stable_finding_key_order() {
        let key_b = FindingKey::MissingMod {
            mod_id: ModId::new("b"),
        };
        let key_a = FindingKey::MissingMod {
            mod_id: ModId::new("a"),
        };
        let mut set = DecisionSet::new();
        // Inserted out of key order — `iter()` must still come back sorted.
        set.insert(decision(key_b.clone(), Action::Ignore)).unwrap();
        set.insert(decision(key_a.clone(), Action::Ignore)).unwrap();

        let keys: Vec<&FindingKey> = set.iter().map(|d| &d.key).collect();

        assert_eq!(keys, vec![&key_a, &key_b]);
    }

    #[test]
    fn orphaned_reports_decisions_whose_key_is_no_longer_live() {
        let live_key = FindingKey::MissingMod {
            mod_id: ModId::new("still.here"),
        };
        let orphan_key = FindingKey::MissingMod {
            mod_id: ModId::new("long.gone"),
        };
        let mut set = DecisionSet::new();
        set.insert(decision(
            live_key.clone(),
            Action::RemoveMod {
                mod_id: ModId::new("still.here"),
            },
        ))
        .unwrap();
        set.insert(decision(
            orphan_key.clone(),
            Action::RemoveMod {
                mod_id: ModId::new("long.gone"),
            },
        ))
        .unwrap();

        let live: BTreeSet<FindingKey> = [live_key].into_iter().collect();
        let orphaned: Vec<&FindingKey> = set.orphaned(&live).map(|d| &d.key).collect();

        assert_eq!(orphaned, vec![&orphan_key]);
    }

    /// Stored decisions across a patch-collision re-key: keying
    /// `patch_collisions` by
    /// effective path changes `FindingKey::PatchCollision.sub_path`'s
    /// *value* for some collisions, never its type or `decisions.json`'s
    /// own schema — `DecisionSet` needs no migration code, since exact
    /// `FindingKey` equality already gives both halves of the contract
    /// for free: an install where the split changes nothing for a given
    /// key still matches it exactly (survives), and one where a
    /// whole-def collision splits into a different `sub_path`/`mods`
    /// shape can never accidentally equal the old key (never
    /// mis-applied), so it simply becomes [`DecisionSet::orphaned`] — the
    /// same, pre-existing "kept on file, surfaced as prunable" contract
    /// every other stale decision already gets (`patch.rs`'s own "never
    /// deletes anything" rerere semantics), never a silent delete — no
    /// `rules.json`-style silent-delete migration runs on top of this.
    #[test]
    fn a_patch_collision_decision_survives_an_unaffected_key_and_orphans_a_split_one() {
        use rim_analyzer::domain::Selector;

        let whole_def_key = FindingKey::PatchCollision {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
            },
            selector: Selector::DefName,
            sub_path: None,
            mods: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        };
        let mut set = DecisionSet::new();
        set.insert(decision(whole_def_key.clone(), Action::Accept))
            .unwrap();

        // Nothing else contested this def root, so the path split leaves the
        // key exactly as it was — an exact match, not an orphan.
        let unaffected_live: BTreeSet<FindingKey> = [whole_def_key.clone()].into_iter().collect();
        assert!(
            set.orphaned(&unaffected_live).next().is_none(),
            "an unaffected key must survive the split, not be treated as orphaned"
        );

        // Elsewhere, the same whole-def key now splits into a per-element
        // one covering only part of the original `mods` — the old key no
        // longer names a live finding at all.
        let split_live: BTreeSet<FindingKey> = [FindingKey::PatchCollision {
            key: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Wall".to_string(),
            },
            selector: Selector::DefName,
            sub_path: Some("comps".to_string()),
            mods: [ModId::new("a")].into_iter().collect(),
        }]
        .into_iter()
        .collect();

        let orphaned: Vec<&FindingKey> = set.orphaned(&split_live).map(|d| &d.key).collect();
        assert_eq!(
            orphaned,
            vec![&whole_def_key],
            "a split key must never equal the old whole-def one, so the stored decision is \
             reported as orphaned rather than silently mis-applied to the new split key"
        );
    }

    #[test]
    fn sorter_overrides_classifies_reorder_and_drop_edge_and_ignore_missing_mod() {
        let mut set = DecisionSet::new();
        set.insert(decision(
            FindingKey::UndeclaredHardDependency {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
            Action::Reorder {
                after: ModId::new("a"),
                before: ModId::new("b"),
            },
        ))
        .unwrap();
        set.insert(decision(
            FindingKey::EdgeDropped {
                after: ModId::new("c"),
                before: ModId::new("d"),
                kind: EdgeKind::MayRequire,
            },
            Action::DropEdge {
                after: ModId::new("c"),
                before: ModId::new("d"),
                kind: EdgeKind::MayRequire,
            },
        ))
        .unwrap();
        let missing_key = FindingKey::MissingMod {
            mod_id: ModId::new("e"),
        };
        set.insert(decision(missing_key, Action::Ignore)).unwrap();

        let overrides = set.sorter_overrides(&BTreeMap::new());

        assert_eq!(overrides.reorders, vec![(ModId::new("a"), ModId::new("b"))]);
        assert!(overrides.dropped_edges.contains(&(
            ModId::new("c"),
            ModId::new("d"),
            EdgeKind::MayRequire
        )));
        assert!(overrides.kept_missing_mods.contains(&ModId::new("e")));
    }

    #[test]
    fn sorter_overrides_expands_prefer_winner_using_the_findings_owner_set() {
        let def_key = DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        };
        let finding_key = FindingKey::DefOverride {
            key: def_key.clone(),
            owners: [ModId::new("a"), ModId::new("b"), ModId::new("c")]
                .into_iter()
                .collect(),
        };
        let mut set = DecisionSet::new();
        set.insert(decision(
            finding_key,
            Action::PreferWinner {
                key: def_key,
                winner: ModId::new("b"),
            },
        ))
        .unwrap();

        let overrides = set.sorter_overrides(&BTreeMap::new());

        let mut reorders = overrides.reorders;
        reorders.sort();
        assert_eq!(
            reorders,
            vec![
                (ModId::new("b"), ModId::new("a")),
                (ModId::new("b"), ModId::new("c"))
            ]
        );
    }

    /// Choosing `PreferWinner` on a `TextureOverride` finding must expand
    /// to real `Reorder` pairs, the same as it does for `DefOverride` —
    /// never a silent no-op, since `owners_of` recognizes the synthesized
    /// texture key.
    #[test]
    fn sorter_overrides_expands_prefer_winner_for_a_texture_override() {
        let finding_key = FindingKey::TextureOverride {
            texture_path: "Things/Wall.png".to_string(),
            owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        };
        let mut set = DecisionSet::new();
        set.insert(decision(
            finding_key,
            Action::PreferWinner {
                key: DefKey::synthesize_for_texture("Things/Wall.png"),
                winner: ModId::new("b"),
            },
        ))
        .unwrap();

        let overrides = set.sorter_overrides(&BTreeMap::new());

        assert_eq!(overrides.reorders, vec![(ModId::new("b"), ModId::new("a"))]);
    }

    /// Ground-truthed against
    /// `Verse.XmlInheritance.GetBestParentFor`'s decompiled source:
    /// choosing `PreferWinner` on a `DuplicateTemplateName` finding must
    /// order the winner after every *other registrant* (same shape as
    /// every other `PreferWinner` kind) **and** before every genuinely
    /// eligible *child* — the opposite direction, and the whole reason
    /// `template_children` exists as a separate input from `owners_of`'s
    /// registrant-only view.
    #[test]
    fn sorter_overrides_expands_prefer_winner_for_a_duplicate_template_name_with_children() {
        let finding_key = FindingKey::DuplicateTemplateName {
            name: "WallBase".to_string(),
            owners: [ModId::new("a"), ModId::new("b"), ModId::new("c")]
                .into_iter()
                .collect(),
        };
        let mut set = DecisionSet::new();
        set.insert(decision(
            finding_key,
            Action::PreferWinner {
                key: DefKey::synthesize_for_template_name("WallBase"),
                winner: ModId::new("b"),
            },
        ))
        .unwrap();
        let mut template_children = BTreeMap::new();
        template_children.insert(
            "WallBase".to_string(),
            [ModId::new("d"), ModId::new("e")].into_iter().collect(),
        );

        let overrides = set.sorter_overrides(&template_children);

        let mut reorders = overrides.reorders;
        reorders.sort();
        assert_eq!(
            reorders,
            vec![
                (ModId::new("b"), ModId::new("a")), // winner after other registrant a
                (ModId::new("b"), ModId::new("c")), // winner after other registrant c
                (ModId::new("d"), ModId::new("b")), // winner before child d
                (ModId::new("e"), ModId::new("b")), // winner before child e
            ]
        );
    }

    /// The children side is additive, never required: an empty (or
    /// missing-entry) `template_children` map leaves a `DuplicateTemplateName`
    /// `PreferWinner` exactly as registrant-only as every other kind —
    /// never a panic or a spurious edge.
    #[test]
    fn sorter_overrides_expands_prefer_winner_for_a_duplicate_template_name_with_no_known_children()
    {
        let finding_key = FindingKey::DuplicateTemplateName {
            name: "WallBase".to_string(),
            owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        };
        let mut set = DecisionSet::new();
        set.insert(decision(
            finding_key,
            Action::PreferWinner {
                key: DefKey::synthesize_for_template_name("WallBase"),
                winner: ModId::new("b"),
            },
        ))
        .unwrap();

        let overrides = set.sorter_overrides(&BTreeMap::new());

        assert_eq!(overrides.reorders, vec![(ModId::new("b"), ModId::new("a"))]);
    }

    #[test]
    fn sorter_overrides_ignores_accept_and_remove_mod() {
        let mut set = DecisionSet::new();
        set.insert(decision(
            FindingKey::UnsupportedVersion {
                mod_id: ModId::new("a"),
            },
            Action::Accept,
        ))
        .unwrap();
        set.insert(decision(
            FindingKey::IncompatiblePair {
                pair: (ModId::new("a"), ModId::new("b")),
            },
            Action::RemoveMod {
                mod_id: ModId::new("a"),
            },
        ))
        .unwrap();

        let overrides = set.sorter_overrides(&BTreeMap::new());

        assert!(overrides.reorders.is_empty());
        assert!(overrides.dropped_edges.is_empty());
        assert!(overrides.kept_missing_mods.is_empty());
    }

    /// [`Action::Merge`] is a fully representable stored decision —
    /// [`DecisionSet::insert`] accepts it like any other
    /// action, and it reaches [`DecisionSet::sorter_overrides`]'s silent
    /// no-op arm rather than being rejected before it ever gets there.
    #[test]
    fn insert_accepts_a_merge_decision_and_stores_it() {
        let def_key = DefKey {
            def_type: "ThingDef".to_string(),
            def_name: "Wall".to_string(),
        };
        let key = FindingKey::DefOverride {
            key: def_key.clone(),
            owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        };
        let mut set = DecisionSet::new();

        let result = set.insert(decision(
            key.clone(),
            Action::Merge {
                key: def_key,
                choices: BTreeMap::new(),
            },
        ));

        assert!(result.is_ok());
        assert!(set.get(&key).is_some());
    }

    /// [`Action::ShipAsset`] is likewise fully representable and ignored
    /// by the sorter, same as [`Action::Merge`].
    #[test]
    fn insert_accepts_a_ship_asset_decision_and_sorter_overrides_ignores_it() {
        let key = FindingKey::TextureOverride {
            texture_path: "Things/Wall.png".to_string(),
            owners: [ModId::new("a"), ModId::new("b")].into_iter().collect(),
        };
        let mut set = DecisionSet::new();
        set.insert(decision(
            key.clone(),
            Action::ShipAsset {
                texture_path: "Things/Wall.png".to_string(),
                from: ModId::new("a"),
            },
        ))
        .unwrap();

        assert!(set.get(&key).is_some());
        let overrides = set.sorter_overrides(&BTreeMap::new());
        assert!(overrides.reorders.is_empty());
    }

    #[test]
    fn content_sha256_is_stable_for_the_same_decisions() {
        let mut set = DecisionSet::new();
        set.insert(decision(
            FindingKey::MissingMod {
                mod_id: ModId::new("a"),
            },
            Action::Ignore,
        ))
        .unwrap();

        assert_eq!(set.content_sha256(), set.content_sha256());
    }

    #[test]
    fn content_sha256_ignores_note_and_decided_at() {
        let key = FindingKey::MissingMod {
            mod_id: ModId::new("a"),
        };
        let mut a = DecisionSet::new();
        a.insert(Decision {
            key: key.clone(),
            action: Action::Ignore,
            note: None,
            decided_at: Timestamp::UNIX_EPOCH,
        })
        .unwrap();
        let mut b = DecisionSet::new();
        b.insert(Decision {
            key,
            action: Action::Ignore,
            note: Some("different note".to_string()),
            decided_at: Timestamp::now(),
        })
        .unwrap();

        assert_eq!(a.content_sha256(), b.content_sha256());
    }

    #[test]
    fn content_sha256_changes_when_an_action_changes() {
        let key = FindingKey::MissingMod {
            mod_id: ModId::new("a"),
        };
        let mut set = DecisionSet::new();
        set.insert(decision(key.clone(), Action::Ignore)).unwrap();
        let before = set.content_sha256();

        set.insert(decision(
            key,
            Action::RemoveMod {
                mod_id: ModId::new("a"),
            },
        ))
        .unwrap();
        let after = set.content_sha256();

        assert_ne!(before, after);
    }

    #[test]
    fn content_sha256_of_an_empty_set_is_stable() {
        assert_eq!(
            DecisionSet::new().content_sha256(),
            DecisionSet::new().content_sha256()
        );
    }
}
