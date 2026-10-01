//! DTOs for `list_tags`/`set_manual_tag`.

use rim_resolve::domain::{TagAssignment, TagProvenance, Tagging};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::common::TagModeDto;
use super::finding::TagSignalDto;

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

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{Confidence, Tag, TagAssignment, TagProvenance};

    use super::*;

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
