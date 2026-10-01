//! [`Tag`]: a user- or rule-assigned label grouping mods (e.g. one naming
//! a framework and its addons), the evidence tag inference matches against,
//! and the result of running it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use rim_analyzer::domain::{ModId, Source};
use serde::{Deserialize, Serialize};

use super::resolution::Confidence;

/// A lowercase `[a-z0-9_-]+` label grouping mods for display and manual
/// organization. Validated at construction so every [`Tag`] in the system
/// is well-formed.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Tag(String);

/// [`Tag::new`] rejects text that isn't non-empty lowercase `[a-z0-9_-]+`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid tag {0:?}: must be non-empty lowercase [a-z0-9_-]+")]
pub struct TagError(String);

impl Tag {
    /// Validates and constructs a tag.
    ///
    /// # Errors
    ///
    /// Returns [`TagError`] when `raw` is empty or contains anything
    /// outside lowercase ascii letters, digits, `_`, and `-`.
    pub fn new(raw: impl Into<String>) -> Result<Self, TagError> {
        let raw = raw.into();
        if is_valid_slug(&raw) {
            Ok(Self(raw))
        } else {
            Err(TagError(raw))
        }
    }

    /// The tag's normalized text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Tag {
    type Error = TagError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Tag> for String {
    fn from(value: Tag) -> Self {
        value.0
    }
}

/// Whether `raw` is a non-empty lowercase `[a-z0-9_-]+` slug. Shared by
/// [`Tag::new`] and [`super::rule::ClusterRuleId::new`], which validate
/// the same shape for two different purposes.
pub(super) fn is_valid_slug(raw: &str) -> bool {
    !raw.is_empty()
        && raw
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// One piece of evidence a [`TagRule`] can match on.
///
/// Tags are labels driven by `UrlContains`/`AssemblyRefTo`/`DependsOn` —
/// evidence an author actually controls. A mod's source is never
/// evidence (a mod being local should not interfere with where it's put),
/// and def-type namespaces or def-name prefixes are not signals either:
/// [`TagEvidence`]'s matching sets for them are always empty from
/// [`crate::tags::evidence_from_report`] (the common, no-live-scan path),
/// so they would never reliably fire.
///
/// **Adjacently tagged, not internally tagged**: every variant
/// here is a newtype over a *string*, and serde's internally-tagged
/// representation cannot serialise that — `serde_json::to_string` fails
/// at runtime with "cannot serialize tagged newtype variant
/// TagSignal::UrlContains containing a string". Every `rules.json` test
/// writes `"tag_rules": []`, so nothing else would catch it.
/// `tag = "kind", content = "value"` is the shape the rules repo's own
/// `tag-rules.json` uses, so the two agree by construction. See
/// [`TagRule`]'s own round-trip test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum TagSignal {
    /// The `About.xml` `url` contains this substring, case-insensitive.
    UrlContains(String),
    /// The mod ships an `AssemblyRef` edge (Hard or Soft) onto this mod.
    AssemblyRefTo(ModId),
    /// The mod declares `modDependencies`/`loadAfter` naming this mod.
    DependsOn(ModId),
}

/// A named rule: a mod earns [`TagRule::tag`] when it matches at least
/// one of [`TagRule::any_of`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagRule {
    /// The tag this rule assigns.
    pub tag: Tag,
    /// The signals that earn the tag; matching any one is enough.
    pub any_of: Vec<TagSignal>,
}

/// Whether a manual tag assignment adds or removes the tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagMode {
    /// The user asserts the mod carries this tag.
    Add,
    /// The user asserts the mod does not carry this tag, overriding
    /// inference.
    Remove,
}

/// A user's explicit override of inference for one mod/tag pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualTag {
    /// The mod the override applies to.
    pub mod_id: ModId,
    /// The tag being added or removed.
    pub tag: Tag,
    /// Whether this adds or removes the tag.
    pub mode: TagMode,
}

/// Per-mod facts captured once at scan time so inference can re-run
/// without touching the filesystem when rules change (see
/// [`crate::tags::collect_evidence`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagEvidence {
    /// The mod this evidence describes.
    pub mod_id: ModId,
    /// Its `About.xml` `url`, if any.
    pub url: Option<String>,
    /// Where its files come from.
    pub source: Source,
    /// Lowercased namespace token of every def type it ships
    /// (`example.PartDef` -> `example`).
    pub def_type_namespaces: BTreeSet<String>,
    /// Lowercased prefix token of every def name it ships
    /// (`Example_Xxx` -> `example`).
    pub def_name_prefixes: BTreeSet<String>,
    /// Mods it ships an `AssemblyRef` edge (Hard or Soft) onto.
    pub assembly_refs_to: BTreeSet<ModId>,
    /// Mods it declares `loadAfter`/`forceLoadAfter`/`modDependencies`
    /// relations to.
    pub declares_after: BTreeSet<ModId>,
}

/// Why a mod carries a tag: inferred from evidence, or set manually.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TagProvenance {
    /// Assigned by [`crate::tags::infer_tags`] because at least one
    /// signal matched.
    Inferred {
        /// Every signal that matched, in rule declaration order.
        matched: Vec<TagSignal>,
        /// The combined confidence (see `tags::infer::combined_confidence`).
        confidence: Confidence,
    },
    /// Set directly by the user via [`ManualTag`].
    Manual,
}

/// One mod/tag assignment and how it came to be.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagAssignment {
    /// The tagged mod.
    pub mod_id: ModId,
    /// The tag it carries.
    pub tag: Tag,
    /// Why it carries the tag.
    pub provenance: TagProvenance,
}

/// The full result of tag inference plus manual overrides: every mod's
/// tag set, and the assignments that produced it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tagging {
    by_mod: BTreeMap<ModId, BTreeSet<Tag>>,
    assignments: Vec<TagAssignment>,
}

impl Tagging {
    /// Builds a [`Tagging`] from its final assignments (after manual
    /// overrides have already been applied — see
    /// [`crate::tags::infer_tags`]).
    #[must_use]
    pub fn new(assignments: Vec<TagAssignment>) -> Self {
        let mut by_mod: BTreeMap<ModId, BTreeSet<Tag>> = BTreeMap::new();
        for assignment in &assignments {
            by_mod
                .entry(assignment.mod_id.clone())
                .or_default()
                .insert(assignment.tag.clone());
        }
        Self {
            by_mod,
            assignments,
        }
    }

    /// Every mod currently carrying `tag`, in id order.
    pub fn members<'a>(&'a self, tag: &'a Tag) -> impl Iterator<Item = &'a ModId> {
        self.by_mod
            .iter()
            .filter(move |(_, tags)| tags.contains(tag))
            .map(|(id, _)| id)
    }

    /// The tags `id` carries; empty when it carries none.
    #[must_use]
    pub fn tags_of(&self, id: &ModId) -> &BTreeSet<Tag> {
        static EMPTY: BTreeSet<Tag> = BTreeSet::new();
        self.by_mod.get(id).unwrap_or(&EMPTY)
    }

    /// Every assignment that produced this tagging, in the order
    /// inference and overrides produced them.
    #[must_use]
    pub fn assignments(&self) -> &[TagAssignment] {
        &self.assignments
    }

    /// This tagging, kept only where the assignment is trustworthy enough
    /// to *act* on: [`TagProvenance::Manual`] (the user said so directly)
    /// or [`TagProvenance::Inferred`] whose confidence meets `threshold`.
    ///
    /// [`rim_resolve::sort::SortInput::tagging`](crate::sort::SortInput::tagging)
    /// is unused by the sorter (there is no tag-based scheduling), but
    /// the field stays as a seam for a future tag-driven ordering
    /// feature, and the caution below is written for that future
    /// consumer: a tag *driving where a mod is scheduled* is a stronger
    /// claim than a tag merely *labeling* it for display — a
    /// low-confidence inferred tag should not be allowed to move a mod on
    /// a guess the ledger itself would otherwise ask the user about (see
    /// `ledger::suggest`'s own confidence table). A future
    /// scheduling consumer of [`SortInput`](crate::sort::SortInput) should
    /// pass `tagging.accepted(threshold)`, not the raw, unfiltered
    /// [`Tagging`] `infer_tags` produces, so only tags the ledger would
    /// already auto-accept can move a mod.
    #[must_use]
    pub fn accepted(&self, threshold: Confidence) -> Self {
        let kept: Vec<TagAssignment> = self
            .assignments
            .iter()
            .filter(|assignment| match &assignment.provenance {
                TagProvenance::Manual => true,
                TagProvenance::Inferred { confidence, .. } => confidence.meets(threshold),
            })
            .cloned()
            .collect();
        Self::new(kept)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_valid_slug() {
        assert_eq!(Tag::new("framework").unwrap().as_str(), "framework");
        assert_eq!(
            Tag::new("example-quirks_2").unwrap().as_str(),
            "example-quirks_2"
        );
    }

    #[test]
    fn rejects_the_empty_string() {
        assert!(Tag::new("").is_err());
    }

    #[test]
    fn rejects_uppercase() {
        assert!(Tag::new("FrameWork").is_err());
    }

    #[test]
    fn rejects_characters_outside_the_slug_alphabet() {
        assert!(Tag::new("frame work").is_err());
        assert!(Tag::new("framework!").is_err());
        assert!(Tag::new("framework.addon").is_err());
    }

    #[test]
    fn serializes_as_its_bare_string() {
        let tag = Tag::new("framework").unwrap();
        assert_eq!(serde_json::to_string(&tag).unwrap(), "\"framework\"");
    }

    #[test]
    fn deserialize_rejects_an_invalid_slug() {
        let result: Result<Tag, _> = serde_json::from_str("\"Not Valid\"");
        assert!(result.is_err());
    }

    /// **The only non-empty tag-rule round-trip.** Every `rules.json` test
    /// in `crates/rim-io/src/rules.rs` writes `"tag_rules": []`, so this
    /// is the one place a non-empty [`TagRule`] is serialised in this
    /// workspace. Serde's *internally* tagged representation cannot
    /// serialise a tagged newtype variant whose payload is not a map — it
    /// fails at runtime, not at compile time — which is why
    /// [`TagSignal`] is **adjacently** tagged (`tag = "kind", content =
    /// "value"`). Keep this test: it is the only thing standing between a
    /// `#[serde(tag = ...)]` "tidy-up" and a `rules.json` save that panics
    /// on the first real tag rule anyone writes.
    #[test]
    fn a_tag_rule_round_trips_through_json_with_every_signal_kind() {
        let rule = TagRule {
            tag: Tag::new("framework").unwrap(),
            any_of: vec![
                TagSignal::UrlContains("mods.example".to_string()),
                TagSignal::AssemblyRefTo(ModId::new("example.framework")),
                TagSignal::DependsOn(ModId::new("example.framework")),
            ],
        };

        let json = serde_json::to_string(&rule).expect("a tag rule must serialise");
        assert!(
            json.contains(r#"{"kind":"url_contains","value":"mods.example"}"#),
            "the adjacently-tagged wire shape is what rules.json v3 stores: {json}"
        );

        let back: TagRule = serde_json::from_str(&json).expect("and deserialise");
        assert_eq!(back, rule);
    }

    #[test]
    fn tagging_reports_no_tags_for_an_unknown_mod() {
        let tagging = Tagging::new(Vec::new());
        assert!(tagging.tags_of(&ModId::new("nobody")).is_empty());
    }

    #[test]
    fn tagging_members_lists_every_mod_carrying_the_tag() {
        let tag = Tag::new("framework").unwrap();
        let other = Tag::new("other").unwrap();
        let tagging = Tagging::new(vec![
            TagAssignment {
                mod_id: ModId::new("b.mod"),
                tag: tag.clone(),
                provenance: TagProvenance::Manual,
            },
            TagAssignment {
                mod_id: ModId::new("a.mod"),
                tag: tag.clone(),
                provenance: TagProvenance::Manual,
            },
            TagAssignment {
                mod_id: ModId::new("c.mod"),
                tag: other,
                provenance: TagProvenance::Manual,
            },
        ]);

        let members: Vec<&ModId> = tagging.members(&tag).collect();
        assert_eq!(
            members,
            vec![&ModId::new("a.mod"), &ModId::new("b.mod")],
            "members must be in id order regardless of assignment order"
        );
    }

    #[test]
    fn accepted_keeps_manual_and_meeting_confidence_but_drops_below_threshold() {
        let tag = Tag::new("framework").unwrap();
        let tagging = Tagging::new(vec![
            TagAssignment {
                mod_id: ModId::new("manual.mod"),
                tag: tag.clone(),
                provenance: TagProvenance::Manual,
            },
            TagAssignment {
                mod_id: ModId::new("confident.mod"),
                tag: tag.clone(),
                provenance: TagProvenance::Inferred {
                    matched: Vec::new(),
                    confidence: Confidence::new(90).unwrap(),
                },
            },
            TagAssignment {
                mod_id: ModId::new("unsure.mod"),
                tag: tag.clone(),
                provenance: TagProvenance::Inferred {
                    matched: Vec::new(),
                    confidence: Confidence::new(50).unwrap(),
                },
            },
        ]);

        let accepted = tagging.accepted(Confidence::new(80).unwrap());

        let members: Vec<&ModId> = accepted.members(&tag).collect();
        assert_eq!(
            members,
            vec![&ModId::new("confident.mod"), &ModId::new("manual.mod")]
        );
    }
}
