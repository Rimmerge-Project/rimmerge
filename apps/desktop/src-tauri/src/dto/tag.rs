//! DTOs for `list_tags`/`set_manual_tag`.

use std::collections::BTreeSet;

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{TagAssignment, TagProvenance, Tagging};
use rim_session::ManualTagsChange;
use rim_session::organization::OrganizationLimits;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::TagModeDto;
use super::finding::TagSignalDto;
use crate::error::CommandError;

/// Why a mod carries a tag. Mirrors [`TagProvenance`], flattened: the
/// matched signals (when inferred) become display text, since they're
/// never round-tripped back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum TagProvenanceDto {
    /// Assigned by inference.
    Inferred {
        /// Every signal that matched.
        matched: Vec<TagSignalDto>,
        /// The combined confidence, 0..=100.
        confidence: u8,
    },
    /// Set directly by the user.
    Manual,
}

impl From<&TagProvenance> for TagProvenanceDto {
    fn from(value: &TagProvenance) -> Self {
        match value {
            TagProvenance::Inferred {
                matched,
                confidence,
            } => Self::Inferred {
                matched: matched.iter().map(TagSignalDto::from).collect(),
                confidence: confidence.percent(),
            },
            TagProvenance::Manual => Self::Manual,
        }
    }
}

/// One mod/tag assignment. Mirrors [`TagAssignment`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct TagAssignmentDto {
    /// The tagged mod.
    pub mod_id: String,
    /// The tag it carries.
    pub tag: String,
    /// Why it carries the tag.
    pub provenance: TagProvenanceDto,
}

impl From<&TagAssignment> for TagAssignmentDto {
    fn from(value: &TagAssignment) -> Self {
        Self {
            mod_id: value.mod_id.as_str().to_string(),
            tag: value.tag.as_str().to_string(),
            provenance: (&value.provenance).into(),
        }
    }
}

/// The full result of tag inference plus manual overrides. Mirrors
/// [`Tagging`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct TaggingDto {
    /// Every assignment currently in effect.
    pub assignments: Vec<TagAssignmentDto>,
}

impl From<&Tagging> for TaggingDto {
    fn from(value: &Tagging) -> Self {
        Self {
            assignments: value.assignments().iter().map(Into::into).collect(),
        }
    }
}

/// Request shape for `set_manual_tag`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SetManualTagRequestDto {
    /// The mod to tag or untag.
    pub mod_id: String,
    /// The tag being added or removed.
    pub tag: String,
    /// Whether this adds or removes the tag.
    pub mode: TagModeDto,
}

/// Request shape for `set_manual_tags`: one tag, many mods, one save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct SetManualTagsRequestDto {
    /// The mods to tag or untag, by exact id. Each counts once.
    pub mod_ids: Vec<String>,
    /// The tag being added or removed.
    pub tag: String,
    /// Whether this adds or removes the tag.
    pub mode: TagModeDto,
}

/// What `set_manual_tags` did, in mods. Mirrors `ManualTagsChange`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct ManualTagsOutcomeDto {
    /// Mods whose tag override changed.
    pub changed: usize,
    /// Mods that still show the tag after a removal because a decision on the Findings page
    /// puts it there; no override can hide it, so none was recorded.
    pub kept_by_decision: usize,
    /// Mods the request did not apply to (an add already in place, or a removal of a tag the
    /// mod does not show).
    pub unchanged: usize,
}

impl SetManualTagsRequestDto {
    /// The request's mods as exact ids (the rules file keys tags by exact id, so unlike the
    /// organization requests nothing is folded to a base id), each once.
    ///
    /// # Errors
    ///
    /// [`CommandError::invalid_input`] for more ids than one request may carry, or an id that
    /// is empty, overlong or holds a control character.
    pub fn parse_mods(&self) -> Result<BTreeSet<ModId>, CommandError> {
        parse_exact_mod_ids(&self.mod_ids)
    }
}

/// Parses `raw` into a set of exact [`ModId`]s, bounded like every organization request.
fn parse_exact_mod_ids(raw: &[String]) -> Result<BTreeSet<ModId>, CommandError> {
    if raw.len() > OrganizationLimits::MAX_TOTAL_MEMBERSHIPS {
        return Err(CommandError::invalid_input(format!(
            "at most {} mods per request",
            OrganizationLimits::MAX_TOTAL_MEMBERSHIPS
        )));
    }
    raw.iter()
        .map(|id| {
            let is_valid = !id.is_empty()
                && id.chars().count() <= OrganizationLimits::MAX_ID_CHARS
                && !id.chars().any(char::is_control);
            if is_valid {
                Ok(ModId::new(id))
            } else {
                Err(CommandError::invalid_input(
                    "a mod id is empty, too long or holds a control character",
                ))
            }
        })
        .collect()
}

impl From<ManualTagsChange> for ManualTagsOutcomeDto {
    fn from(value: ManualTagsChange) -> Self {
        Self {
            changed: value.changed,
            kept_by_decision: value.kept_by_decision,
            unchanged: value.unchanged,
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Confidence, Tag, TagAssignment, TagProvenance};

    use super::*;

    #[test]
    fn parse_mods_keeps_exact_ids_once_and_refuses_unsound_ones() {
        let request = |ids: &[&str]| SetManualTagsRequestDto {
            mod_ids: ids.iter().map(ToString::to_string).collect(),
            tag: "framework".to_string(),
            mode: TagModeDto::Add,
        };

        let parsed = request(&["a_steam", "a", "a"]).parse_mods().expect("valid");

        assert_eq!(
            parsed,
            [ModId::new("a"), ModId::new("a_steam")]
                .into_iter()
                .collect()
        );
        assert!(request(&[""]).parse_mods().is_err());
        assert!(request(&["bad\nid"]).parse_mods().is_err());
    }

    #[test]
    fn tagging_dto_maps_manual_and_inferred_assignments() {
        let tag = Tag::new("framework").expect("valid tag");
        let tagging = Tagging::new(vec![
            TagAssignment {
                mod_id: ModId::new("a.mod"),
                tag: tag.clone(),
                provenance: TagProvenance::Manual,
            },
            TagAssignment {
                mod_id: ModId::new("b.mod"),
                tag,
                provenance: TagProvenance::Inferred {
                    matched: Vec::new(),
                    confidence: Confidence::new(90).expect("valid confidence"),
                },
            },
        ]);

        let dto: TaggingDto = (&tagging).into();
        assert_eq!(dto.assignments.len(), 2);
        assert_eq!(dto.assignments[0].mod_id, "a.mod");
        assert!(matches!(
            dto.assignments[0].provenance,
            TagProvenanceDto::Manual
        ));
        match &dto.assignments[1].provenance {
            TagProvenanceDto::Inferred { confidence, .. } => assert_eq!(*confidence, 90),
            other => panic!("expected Inferred, got {other:?}"),
        }
    }
}
