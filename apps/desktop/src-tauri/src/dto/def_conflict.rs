//! DTOs for `get_def_conflict_view`: the def-shaped finding conflict
//! panel's own command.
//!
//! Paging `DefConflictView::fields` lives in `rim-session`
//! (`rim_session::def_conflict_view::page`), per `apps/desktop/CLAUDE.md`'s
//! usual rule; this module only maps that already-filtered/paged output
//! to DTOs. Values render exactly like [`super::merge::format_value`]
//! (reused directly, not reimplemented).

use rim_analyzer::domain::ModId;
use rim_merge::diff::Value;
use rim_session::def_conflict_view::{FieldRowFilter, FieldRowPage};
use rim_session::{
    DefConflictKind, DefConflictToucher, DefConflictView, FieldRow, FieldRowKind,
    InjectedNodeRelation, Preference, Problem, ToucherRole,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::defs::CompletenessDto;
use super::finding::MergeChoiceDto;
use super::merge::format_value;

/// Request shape for `get_def_conflict_view`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefConflictViewRequestDto {
    /// The finding's canonical key text.
    pub key: String,
    /// Filters and pages [`DefConflictViewDto::fields`].
    pub filter: FieldRowFilterDto,
    /// When set, build the view against that compat patch's own scoped
    /// preview and decisions instead of the profile's
    /// — mirrors [`super::merge::MergePreviewRequestDto::patch_id`]
    /// exactly.
    pub patch_id: Option<String>,
}

/// Filters and pages a [`DefConflictViewDto`]'s field-row list. Mirrors
/// [`FieldRowFilter`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct FieldRowFilterDto {
    /// Hide [`FieldRowKindDto::Unchanged`] rows.
    pub only_changed: bool,
    /// How many matching rows to skip before collecting the page.
    pub offset: usize,
    /// How many rows to return, capped at `rim_session::MAX_PAGE_SIZE`.
    pub limit: usize,
}

impl Default for FieldRowFilterDto {
    /// `onlyChanged` defaults on — unlike every other filter DTO's derived,
    /// all-`false`/`0` default, this one has an opinion: an unfiltered field
    /// list is mostly `Unchanged` rows nobody asked to see. `limit` defaults
    /// to `rim_session::MAX_PAGE_SIZE`, not `0`: `0` would return an empty
    /// page from every unfiltered call, since `page`'s own `take(limit)`
    /// takes nothing.
    fn default() -> Self {
        Self {
            only_changed: true,
            offset: 0,
            limit: rim_session::MAX_PAGE_SIZE,
        }
    }
}

impl From<&FieldRowFilterDto> for FieldRowFilter {
    fn from(value: &FieldRowFilterDto) -> Self {
        Self {
            only_changed: value.only_changed,
            offset: value.offset,
            limit: value.limit,
        }
    }
}

/// Mirrors [`ToucherRole`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ToucherRoleDto {
    /// See [`ToucherRole::Owner`].
    Owner,
    /// See [`ToucherRole::Patcher`].
    Patcher,
}

impl From<ToucherRole> for ToucherRoleDto {
    fn from(value: ToucherRole) -> Self {
        match value {
            ToucherRole::Owner => Self::Owner,
            ToucherRole::Patcher => Self::Patcher,
        }
    }
}

/// One row of a [`DefConflictViewDto`]'s touchers table. Mirrors
/// [`DefConflictToucher`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefConflictToucherDto {
    /// The mod.
    pub mod_id: String,
    /// Its position in the selected order.
    pub position: usize,
    /// Whether it's a Rimmerge-generated mod.
    pub is_generated: bool,
    /// Owner or patcher.
    pub role: ToucherRoleDto,
    /// Top-level operations targeting the def — always 0 for
    /// [`ToucherRoleDto::Owner`].
    pub op_count: usize,
}

impl From<&DefConflictToucher> for DefConflictToucherDto {
    fn from(value: &DefConflictToucher) -> Self {
        Self {
            mod_id: value.mod_id.as_str().to_string(),
            position: value.position,
            is_generated: value.is_generated,
            role: value.role.into(),
            op_count: value.op_count,
        }
    }
}

/// Which finding kind a [`DefConflictViewDto`] was built for. Mirrors
/// [`DefConflictKind`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum DefConflictKindDto {
    /// See [`DefConflictKind::DefOverride`].
    DefOverride,
    /// See [`DefConflictKind::PatchCollision`].
    #[serde(rename_all = "camelCase")]
    PatchCollision {
        /// The contested field's path text, when the sub_path grammar
        /// resolved it.
        sub_path: Option<String>,
    },
    /// See [`DefConflictKind::DuplicateTemplateName`].
    DuplicateTemplateName,
}

impl From<&DefConflictKind> for DefConflictKindDto {
    fn from(value: &DefConflictKind) -> Self {
        match value {
            DefConflictKind::DefOverride => Self::DefOverride,
            DefConflictKind::PatchCollision { sub_path } => Self::PatchCollision {
                sub_path: sub_path.as_ref().map(ToString::to_string),
            },
            DefConflictKind::DuplicateTemplateName => Self::DuplicateTemplateName,
        }
    }
}

/// Mirrors [`FieldRowKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FieldRowKindDto {
    /// See [`FieldRowKind::Conflict`].
    Conflict,
    /// See [`FieldRowKind::CleanMerge`].
    CleanMerge,
    /// See [`FieldRowKind::ListEntry`].
    ListEntry,
    /// See [`FieldRowKind::MapEntry`].
    MapEntry,
    /// See [`FieldRowKind::Unchanged`].
    Unchanged,
}

impl From<FieldRowKind> for FieldRowKindDto {
    fn from(value: FieldRowKind) -> Self {
        match value {
            FieldRowKind::Conflict => Self::Conflict,
            FieldRowKind::CleanMerge => Self::CleanMerge,
            FieldRowKind::ListEntry => Self::ListEntry,
            FieldRowKind::MapEntry => Self::MapEntry,
            FieldRowKind::Unchanged => Self::Unchanged,
        }
    }
}

/// Who wins a [`FieldRowDto`] and why. Mirrors [`Preference`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum PreferenceDto {
    /// See [`Preference::LoadOrder`].
    LoadOrder {
        /// The winning mod.
        winner: String,
    },
    /// See [`Preference::Decision`].
    Decision {
        /// The decided winner.
        winner: String,
    },
    /// See [`Preference::MergeChoice`].
    MergeChoice {
        /// The stored choice.
        choice: MergeChoiceDto,
    },
    /// See [`Preference::None`].
    None,
}

impl From<&Preference> for PreferenceDto {
    fn from(value: &Preference) -> Self {
        match value {
            Preference::LoadOrder { winner } => Self::LoadOrder {
                winner: winner.as_str().to_string(),
            },
            Preference::Decision { winner } => Self::Decision {
                winner: winner.as_str().to_string(),
            },
            Preference::MergeChoice { choice } => Self::MergeChoice {
                choice: choice.clone().into(),
            },
            Preference::None => Self::None,
        }
    }
}

/// One mod's own value at a [`FieldRowDto`]'s path — a toucher's own
/// candidate, or who/what `inGame`/`afterMerge` names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct FieldValueDto {
    /// The mod.
    pub mod_id: String,
    /// Its rendered value at this path; `null` when absent.
    pub value: Option<String>,
}

fn field_value_dto((mod_id, value): &(ModId, Value)) -> FieldValueDto {
    FieldValueDto {
        mod_id: mod_id.as_str().to_string(),
        value: format_value(value),
    }
}

/// One row of a [`DefConflictViewDto`]'s field list. Mirrors [`FieldRow`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct FieldRowDto {
    /// The field's canonical path text (see [`rim_merge::tree::FieldPath`]'s
    /// `Display`).
    pub path: String,
    /// How to group/display this row.
    pub kind: FieldRowKindDto,
    /// Every toucher that sets this path to a differing value, in the
    /// selected order.
    pub values: Vec<FieldValueDto>,
    /// Other touchers that independently added the *exact same* `li` item
    /// under a colliding identity as this row's own (the "list case"
    /// dedup) — mod ids only, in load order, excluding whichever mod
    /// already appears in [`Self::values`]. Empty for anything but a
    /// `ListEntry` row produced from this fold. Mirrors [`FieldRow::agreed_by`].
    pub agreed_by: Vec<String>,
    /// Who/what the game runs today; `null` when the field isn't in the
    /// (possibly partial) resolved tree at all.
    pub in_game: Option<FieldValueDto>,
    /// What a complete merge would produce; `null` when the cached
    /// preview isn't `Complete`.
    pub after_merge: Option<FieldValueDto>,
    /// Who wins, and why.
    pub preference: PreferenceDto,
}

impl From<&FieldRow> for FieldRowDto {
    fn from(value: &FieldRow) -> Self {
        Self {
            path: value.path.to_string(),
            kind: value.kind.into(),
            values: value.values.iter().map(field_value_dto).collect(),
            agreed_by: value
                .agreed_by
                .iter()
                .map(|mod_id| mod_id.as_str().to_string())
                .collect(),
            in_game: value.in_game.as_ref().map(field_value_dto),
            after_merge: value.after_merge.as_ref().map(field_value_dto),
            preference: (&value.preference).into(),
        }
    }
}

/// One entry of a [`DefConflictViewDto`]'s problem list. Mirrors
/// [`Problem`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ProblemDto {
    /// See [`Problem::UnsupportedOp`].
    #[serde(rename_all = "camelCase")]
    UnsupportedOp {
        /// The mod that shipped the operation.
        mod_id: String,
        /// The operation's index among every active patcher's top-level
        /// operations, in load order.
        op_index: usize,
        /// The operation's `Class`, when recovered (`"<unknown>"` sentinel
        /// when the stopper's own op summary couldn't be found — see
        /// [`Problem::UnsupportedOp`]'s own doc comment).
        class: String,
        /// The operation's `<xpath>`, when available.
        xpath: Option<String>,
        /// Why the replay stopped.
        reason: String,
    },
    /// See [`Problem::MissingTemplate`].
    #[serde(rename_all = "camelCase")]
    MissingTemplate {
        /// The def type the chain was walking.
        def_type: String,
        /// The missing template's `Name`.
        name: String,
    },
    /// See [`Problem::Cycle`].
    Cycle {
        /// The repeated name, appended to the chain that led back to it.
        chain: Vec<String>,
    },
    /// See [`Problem::PlanFailed`].
    #[serde(rename_all = "camelCase")]
    PlanFailed {
        /// The swallowed `PlanMergeError`'s own `Display` text, verbatim.
        reason: String,
    },
}

impl From<&Problem> for ProblemDto {
    fn from(value: &Problem) -> Self {
        match value {
            Problem::UnsupportedOp {
                mod_id,
                op_index,
                class,
                xpath,
                reason,
            } => Self::UnsupportedOp {
                mod_id: mod_id.as_str().to_string(),
                op_index: *op_index,
                class: class.clone(),
                xpath: xpath.clone(),
                reason: reason.clone(),
            },
            Problem::MissingTemplate { def_type, name } => Self::MissingTemplate {
                def_type: def_type.clone(),
                name: name.clone(),
            },
            Problem::Cycle { chain } => Self::Cycle {
                chain: chain.clone(),
            },
            Problem::PlanFailed { reason } => Self::PlanFailed {
                reason: reason.clone(),
            },
        }
    }
}

/// One entry of a [`DefConflictViewDto`]'s injected-node-relation list.
/// Mirrors [`InjectedNodeRelation`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct InjectedNodeRelationDto {
    /// The mod whose patch op selects [`Self::subject`].
    pub selector: String,
    /// The mod whose own patch injects [`Self::subject`].
    pub injector: String,
    /// The selected node path.
    pub subject: String,
}

impl From<&InjectedNodeRelation> for InjectedNodeRelationDto {
    fn from(value: &InjectedNodeRelation) -> Self {
        Self {
            selector: value.selector.as_str().to_string(),
            injector: value.injector.as_str().to_string(),
            subject: value.subject.clone(),
        }
    }
}

/// `get_def_conflict_view`'s result: one def-shaped finding's field-by-field
/// conflict view. Mirrors [`DefConflictView`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct DefConflictViewDto {
    /// The def or template this view is about, canonical ref text.
    pub def_ref: String,
    /// Which finding kind produced it.
    pub kind: DefConflictKindDto,
    /// Every owner and patcher, selected order.
    pub touchers: Vec<DefConflictToucherDto>,
    /// The requested page of field rows.
    pub fields: Vec<FieldRowDto>,
    /// Rows matching [`FieldRowFilterDto`], before paging.
    pub fields_total: usize,
    /// Everything that stopped evaluation partway through.
    pub problems: Vec<ProblemDto>,
    /// Whether the effective def's own fold reached every stage.
    pub effective_completeness: CompletenessDto,
    /// `PatchSelectsInjectedNode` edges naming this def — evidence two
    /// touchers interact, never an ordering requirement (surfaced here
    /// rather than as a separate inbox finding).
    pub injected_node_relations: Vec<InjectedNodeRelationDto>,
}

/// Builds a [`DefConflictViewDto`] from a [`DefConflictView`] and the
/// [`FieldRowPage`] `rim_session::def_conflict_view::page` already
/// filtered and paged from it — pure value rendering, no session access.
#[must_use]
pub fn build_def_conflict_view_dto(
    view: &DefConflictView,
    page: &FieldRowPage<'_>,
) -> DefConflictViewDto {
    DefConflictViewDto {
        def_ref: view.def_ref.to_string(),
        kind: (&view.kind).into(),
        touchers: view.touchers.iter().map(Into::into).collect(),
        fields: page.items.iter().copied().map(Into::into).collect(),
        fields_total: page.total,
        problems: view.problems.iter().map(Into::into).collect(),
        effective_completeness: (&view.effective_completeness).into(),
        injected_node_relations: view
            .injected_node_relations
            .iter()
            .map(Into::into)
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use rim_merge::tree::FieldPath;
    use rim_resolve::domain::MergeChoice;

    use super::*;

    #[test]
    fn field_row_filter_dto_defaults_only_changed_to_true() {
        let dto = FieldRowFilterDto::default();
        assert!(dto.only_changed);
        // A `0` default limit would return an empty page from every
        // unfiltered call.
        assert_eq!(dto.limit, rim_session::MAX_PAGE_SIZE);
    }

    #[test]
    fn field_row_filter_dto_maps_every_field() {
        let dto = FieldRowFilterDto {
            only_changed: false,
            offset: 5,
            limit: 50,
        };

        let filter: FieldRowFilter = (&dto).into();

        assert!(!filter.only_changed);
        assert_eq!(filter.offset, 5);
        assert_eq!(filter.limit, 50);
    }

    #[test]
    fn def_conflict_kind_dto_carries_the_parsed_sub_path() {
        let path: FieldPath = "label".parse().expect("valid field path");
        let dto: DefConflictKindDto = (&DefConflictKind::PatchCollision {
            sub_path: Some(path),
        })
            .into();

        match dto {
            DefConflictKindDto::PatchCollision { sub_path } => {
                assert_eq!(sub_path.as_deref(), Some("label"));
            }
            other => panic!("expected PatchCollision, got {other:?}"),
        }
    }

    #[test]
    fn def_conflict_kind_dto_serializes_the_tag_field() {
        let dto: DefConflictKindDto = (&DefConflictKind::DefOverride).into();
        let json = serde_json::to_value(dto).expect("serializable");
        assert_eq!(json["kind"], "defOverride");
    }

    #[test]
    fn preference_dto_maps_load_order() {
        let dto: PreferenceDto = (&Preference::LoadOrder {
            winner: ModId::new("winner.mod"),
        })
            .into();
        assert_eq!(
            dto,
            PreferenceDto::LoadOrder {
                winner: "winner.mod".to_string()
            }
        );
    }

    #[test]
    fn preference_dto_maps_merge_choice() {
        let dto: PreferenceDto = (&Preference::MergeChoice {
            choice: MergeChoice::From {
                mod_id: ModId::new("a.mod"),
            },
        })
            .into();
        assert_eq!(
            dto,
            PreferenceDto::MergeChoice {
                choice: MergeChoiceDto::From {
                    mod_id: "a.mod".to_string()
                }
            }
        );
    }

    #[test]
    fn field_row_dto_renders_values_in_game_and_after_merge() {
        let path: FieldPath = "label".parse().expect("valid field path");
        let row = FieldRow {
            path,
            kind: FieldRowKind::CleanMerge,
            values: vec![(ModId::new("a.mod"), Value::Leaf("hi".to_string()))],
            agreed_by: Vec::new(),
            in_game: Some((ModId::new("a.mod"), Value::Leaf("hi".to_string()))),
            after_merge: Some((ModId::new("a.mod"), Value::Leaf("hi".to_string()))),
            preference: Preference::None,
        };

        let dto: FieldRowDto = (&row).into();

        assert_eq!(dto.path, "label");
        assert_eq!(dto.kind, FieldRowKindDto::CleanMerge);
        assert_eq!(
            dto.values,
            vec![FieldValueDto {
                mod_id: "a.mod".to_string(),
                value: Some("hi".to_string()),
            }]
        );
        assert_eq!(
            dto.in_game,
            Some(FieldValueDto {
                mod_id: "a.mod".to_string(),
                value: Some("hi".to_string()),
            })
        );
        assert_eq!(dto.in_game, dto.after_merge);
        assert_eq!(dto.preference, PreferenceDto::None);
    }

    /// The "list case" dedup: a `ListEntry` row folded from two
    /// byte-identical `li` additions carries the second mod in `agreed_by`,
    /// as plain mod-id strings, never a second `values` entry (`values`
    /// stays the single credited mod's own value).
    #[test]
    fn field_row_dto_renders_agreed_by() {
        let path: FieldPath = "comps/li[#0]".parse().expect("valid field path");
        let row = FieldRow {
            path,
            kind: FieldRowKind::ListEntry,
            values: vec![(
                ModId::new("a.mod"),
                Value::Leaf(r#"<li Class="Alpha"/>"#.to_string()),
            )],
            agreed_by: vec![ModId::new("b.mod"), ModId::new("c.mod")],
            in_game: None,
            after_merge: None,
            preference: Preference::None,
        };

        let dto: FieldRowDto = (&row).into();

        assert_eq!(
            dto.agreed_by,
            vec!["b.mod".to_string(), "c.mod".to_string()]
        );
        assert_eq!(dto.values.len(), 1);
    }

    /// Every other fixture in this
    /// module (and the mock/Vitest/Playwright ones) happens to have
    /// `in_game == after_merge`, so a mapper bug that swaps the two
    /// fields would pass every one of them — this pins a row where a
    /// stored `MergeChoice::From` decision sends `after_merge` to a
    /// *different* mod's value than what actually runs in-game today
    /// (the load-order winner, `b.mod`), the real shape a stored merge
    /// choice on a `Conflict` row produces.
    #[test]
    fn field_row_dto_distinguishes_in_game_from_a_stored_merge_choices_after_merge() {
        let path: FieldPath = "label".parse().expect("valid field path");
        let row = FieldRow {
            path,
            kind: FieldRowKind::Conflict,
            values: vec![
                (ModId::new("a.mod"), Value::Leaf("alpha".to_string())),
                (ModId::new("b.mod"), Value::Leaf("beta".to_string())),
            ],
            agreed_by: Vec::new(),
            in_game: Some((ModId::new("b.mod"), Value::Leaf("beta".to_string()))),
            after_merge: Some((ModId::new("a.mod"), Value::Leaf("alpha".to_string()))),
            preference: Preference::MergeChoice {
                choice: MergeChoice::From {
                    mod_id: ModId::new("a.mod"),
                },
            },
        };

        let dto: FieldRowDto = (&row).into();

        assert_ne!(
            dto.in_game, dto.after_merge,
            "a stored MergeChoice::From a non-winning mod must send after_merge somewhere \
             different from what runs in-game today"
        );
        assert_eq!(
            dto.in_game,
            Some(FieldValueDto {
                mod_id: "b.mod".to_string(),
                value: Some("beta".to_string()),
            })
        );
        assert_eq!(
            dto.after_merge,
            Some(FieldValueDto {
                mod_id: "a.mod".to_string(),
                value: Some("alpha".to_string()),
            })
        );
    }

    /// A keyed map's own key:
    /// `FieldRowKind::MapEntry` maps straight through, and an `Agreeing`
    /// map entry's own credited-earliest/`agreed_by`-rest split (already
    /// folded by `rim_session::def_conflict_view::map_entry_values_and_agreed_by`
    /// before this DTO ever sees the row) carries over unchanged — the
    /// same "also added by" shape a `ListEntry` row's own dedup already
    /// produces.
    #[test]
    fn field_row_dto_renders_a_map_entry_kind_with_agreed_by() {
        let path: FieldPath = "wildAnimals/Cobra".parse().expect("valid field path");
        let row = FieldRow {
            path,
            kind: FieldRowKind::MapEntry,
            values: vec![(ModId::new("a.mod"), Value::Leaf("0.2".to_string()))],
            agreed_by: vec![ModId::new("b.mod")],
            in_game: Some((ModId::new("a.mod"), Value::Leaf("0.2".to_string()))),
            after_merge: Some((ModId::new("a.mod"), Value::Leaf("0.2".to_string()))),
            preference: Preference::None,
        };

        let dto: FieldRowDto = (&row).into();

        assert_eq!(dto.kind, FieldRowKindDto::MapEntry);
        assert_eq!(dto.agreed_by, vec!["b.mod".to_string()]);
        assert_eq!(dto.values.len(), 1);

        let json = serde_json::to_value(dto.kind).expect("serializable");
        assert_eq!(json, "mapEntry");
    }

    #[test]
    fn problem_dto_maps_unsupported_op() {
        let dto: ProblemDto = (&Problem::UnsupportedOp {
            mod_id: ModId::new("x.mod"),
            op_index: 3,
            class: "PatchOperationReplace".to_string(),
            xpath: Some("Defs/ThingDef".to_string()),
            reason: "boom".to_string(),
        })
            .into();

        assert_eq!(
            dto,
            ProblemDto::UnsupportedOp {
                mod_id: "x.mod".to_string(),
                op_index: 3,
                class: "PatchOperationReplace".to_string(),
                xpath: Some("Defs/ThingDef".to_string()),
                reason: "boom".to_string(),
            }
        );
    }

    /// `Problem::PlanFailed`
    /// (a `DefOverride`'s own swallowed `PlanMergeError` that isn't one of
    /// the two structured `Inherit` shapes) maps straight through.
    #[test]
    fn problem_dto_maps_plan_failed() {
        let dto: ProblemDto = (&Problem::PlanFailed {
            reason: "the source index has no record of ReallyMissing".to_string(),
        })
            .into();

        assert_eq!(
            dto,
            ProblemDto::PlanFailed {
                reason: "the source index has no record of ReallyMissing".to_string(),
            }
        );
    }

    #[test]
    fn injected_node_relation_dto_maps_every_field() {
        let relation = InjectedNodeRelation {
            selector: ModId::new("a.mod"),
            injector: ModId::new("b.mod"),
            subject: "ThingDef/Widget/comps".to_string(),
        };

        let dto: InjectedNodeRelationDto = (&relation).into();

        assert_eq!(
            dto,
            InjectedNodeRelationDto {
                selector: "a.mod".to_string(),
                injector: "b.mod".to_string(),
                subject: "ThingDef/Widget/comps".to_string(),
            }
        );
    }
}
