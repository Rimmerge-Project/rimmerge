//! Schema DTOs: cardinality, scalar kinds, field roles and specs, target shapes, and the assignment schema itself.

use std::collections::{BTreeMap, BTreeSet};

use rim_analyzer::domain::ModId;
use rim_resolve::domain::{Cardinality, FieldPath, FieldRole, FieldSpec, ScalarKind, TargetShape};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::CommandError;

/// Parses `text` as a [`FieldPath`], mapping a malformed path to
/// [`CommandError::invalid_input`] — the same inline pattern
/// `dto::finding`'s `MergeActionDto` `TryFrom` already uses for
/// [`FieldPath`], not a blanket `From` impl in `error.rs` (this module is
/// the only other place a wire-supplied field path needs parsing).
pub(super) fn parse_field_path(text: &str) -> Result<FieldPath, CommandError> {
    text.parse()
        .map_err(|error: rim_resolve::domain::FieldPathParseError| {
            CommandError::invalid_input(error.to_string())
        })
}

// -- Cardinality / ScalarKind / FieldRole -----------------------------

/// Mirrors [`Cardinality`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum CardinalityDto {
    /// See [`Cardinality::Scalar`].
    Scalar,
    /// See [`Cardinality::List`].
    List,
}

impl From<Cardinality> for CardinalityDto {
    fn from(value: Cardinality) -> Self {
        match value {
            Cardinality::Scalar => Self::Scalar,
            Cardinality::List => Self::List,
        }
    }
}

impl From<CardinalityDto> for Cardinality {
    fn from(value: CardinalityDto) -> Self {
        match value {
            CardinalityDto::Scalar => Self::Scalar,
            CardinalityDto::List => Self::List,
        }
    }
}

/// Mirrors [`ScalarKind`]; tagged `"type"` for the same reason the domain
/// type itself is (see [`ScalarKind`]'s own doc comment).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum ScalarKindDto {
    /// See [`ScalarKind::Bool`].
    Bool,
    /// See [`ScalarKind::Number`].
    Number,
    /// See [`ScalarKind::Enum`].
    Enum {
        /// The distinct values observed.
        values: BTreeSet<String>,
    },
    /// See [`ScalarKind::Text`].
    Text,
}

impl From<ScalarKind> for ScalarKindDto {
    fn from(value: ScalarKind) -> Self {
        match value {
            ScalarKind::Bool => Self::Bool,
            ScalarKind::Number => Self::Number,
            ScalarKind::Enum { values } => Self::Enum { values },
            ScalarKind::Text => Self::Text,
        }
    }
}

impl From<ScalarKindDto> for ScalarKind {
    fn from(value: ScalarKindDto) -> Self {
        match value {
            ScalarKindDto::Bool => Self::Bool,
            ScalarKindDto::Number => Self::Number,
            ScalarKindDto::Enum { values } => Self::Enum { values },
            ScalarKindDto::Text => Self::Text,
        }
    }
}

/// Mirrors [`FieldRole`]; tagged `"type"` for the same reason the domain
/// type is. `Chances::forSlot` and `TargetKey`/`ItemSlot`'s `defType`
/// cross exactly as their domain counterparts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub enum FieldRoleDto {
    /// See [`FieldRole::TargetKey`].
    #[serde(rename_all = "camelCase")]
    TargetKey {
        /// The target granularity.
        def_type: String,
    },
    /// See [`FieldRole::ItemSlot`].
    #[serde(rename_all = "camelCase")]
    ItemSlot {
        /// The item type this slot picks from.
        def_type: String,
    },
    /// See [`FieldRole::Chances`].
    #[serde(rename_all = "camelCase")]
    Chances {
        /// The slot field this list is paired to, by its canonical path text.
        for_slot: String,
    },
    /// See [`FieldRole::Scalar`].
    Scalar {
        /// The scalar's type.
        kind: ScalarKindDto,
        /// The most common observed value, if any were observed.
        default: Option<String>,
    },
    /// See [`FieldRole::Opaque`].
    Opaque,
}

impl From<FieldRole> for FieldRoleDto {
    fn from(value: FieldRole) -> Self {
        match value {
            FieldRole::TargetKey { def_type } => Self::TargetKey { def_type },
            FieldRole::ItemSlot { def_type } => Self::ItemSlot { def_type },
            FieldRole::Chances { for_slot } => Self::Chances {
                for_slot: for_slot.to_string(),
            },
            FieldRole::Scalar { kind, default } => Self::Scalar {
                kind: kind.into(),
                default,
            },
            FieldRole::Opaque => Self::Opaque,
        }
    }
}

impl TryFrom<FieldRoleDto> for FieldRole {
    type Error = CommandError;

    fn try_from(value: FieldRoleDto) -> Result<Self, Self::Error> {
        Ok(match value {
            FieldRoleDto::TargetKey { def_type } => Self::TargetKey { def_type },
            FieldRoleDto::ItemSlot { def_type } => Self::ItemSlot { def_type },
            FieldRoleDto::Chances { for_slot } => Self::Chances {
                for_slot: parse_field_path(&for_slot)?,
            },
            FieldRoleDto::Scalar { kind, default } => Self::Scalar {
                kind: kind.into(),
                default,
            },
            FieldRoleDto::Opaque => Self::Opaque,
        })
    }
}

/// Mirrors [`FieldSpec`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct FieldSpecDto {
    /// The field's current role.
    pub role: FieldRoleDto,
    /// Whether the field is a list or a scalar.
    pub cardinality: CardinalityDto,
    /// `[carried, read]` — how many instances carried the field, out of
    /// how many were read.
    pub observed: (usize, usize),
    /// `Some` when `role` was changed by the user.
    pub inferred_role: Option<FieldRoleDto>,
}

impl From<FieldSpec> for FieldSpecDto {
    fn from(value: FieldSpec) -> Self {
        Self {
            role: value.role.into(),
            cardinality: value.cardinality.into(),
            observed: value.observed,
            inferred_role: value.inferred_role.map(Into::into),
        }
    }
}

impl TryFrom<FieldSpecDto> for FieldSpec {
    type Error = CommandError;

    fn try_from(value: FieldSpecDto) -> Result<Self, Self::Error> {
        Ok(Self {
            role: value.role.try_into()?,
            cardinality: value.cardinality.into(),
            observed: value.observed,
            inferred_role: value.inferred_role.map(TryInto::try_into).transpose()?,
        })
    }
}

/// `BTreeMap<FieldPath, FieldSpec>`, keyed by each field's canonical path
/// text. Wrapped (rather than exposed as a bare `BTreeMap<FieldPath,
/// FieldSpecDto>`, which `ts-rs` can't key by a non-`String` type at all)
/// so it round-trips through this crate's own `String`-keyed map,
/// exported as `{ [key in string]?: FieldSpecDto }` — **not**
/// [`super::mods::ModNamesDto`](crate::dto::mods::ModNamesDto)'s own `#[ts(type = "Record<string,
/// string>")]` override: that pattern only works when the map's value is
/// a primitive. `ts-rs`'s `type` override discards the overridden type's
/// own dependency graph entirely (confirmed against `ts-rs` 10.1.0's own
/// `type_override_struct`, which always builds an empty `Dependencies`
/// set) — a value type this crate exports separately, like
/// [`FieldSpecDto`], would never actually get an `import` statement
/// emitted into this file, only a name TypeScript can't resolve. Letting
/// `ts-rs`'s own `BTreeMap` support render the field (visiting real
/// dependencies) is what actually produces a working import; the mapped-
/// type spelling is the price for that, not a stylistic slip.
// No `#[serde(transparent)]`: serde already serializes a single-field
// tuple struct as its inner value, and ts-rs 10 exports it as an alias of
// the inner type — the attribute was redundant for both and only made
// ts-rs's derive warn "failed to parse serde attribute" on every build.
// `newtype_map_dtos_serialize_as_the_bare_map` (tests, below) pins the
// JSON shape this relies on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct FieldSpecMapDto(pub BTreeMap<String, FieldSpecDto>);

impl From<&BTreeMap<FieldPath, FieldSpec>> for FieldSpecMapDto {
    fn from(value: &BTreeMap<FieldPath, FieldSpec>) -> Self {
        Self(
            value
                .iter()
                .map(|(path, spec)| (path.to_string(), spec.clone().into()))
                .collect(),
        )
    }
}

impl TryFrom<FieldSpecMapDto> for BTreeMap<FieldPath, FieldSpec> {
    type Error = CommandError;

    fn try_from(value: FieldSpecMapDto) -> Result<Self, Self::Error> {
        value
            .0
            .into_iter()
            .map(|(path, spec)| Ok((parse_field_path(&path)?, spec.try_into()?)))
            .collect()
    }
}

/// Mirrors [`TargetShape`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct TargetShapeDto {
    /// The target granularity this shape describes.
    pub def_type: String,
    /// Top-level child tags every candidate target must carry.
    pub required_children: BTreeSet<String>,
}

impl From<&TargetShape> for TargetShapeDto {
    fn from(value: &TargetShape) -> Self {
        Self {
            def_type: value.def_type.clone(),
            required_children: value.required_children.clone(),
        }
    }
}

impl From<TargetShapeDto> for TargetShape {
    fn from(value: TargetShapeDto) -> Self {
        Self {
            def_type: value.def_type,
            required_children: value.required_children,
        }
    }
}

/// `BTreeMap<FieldPath, TargetShape>`, wrapped the same way
/// [`FieldSpecMapDto`] is (see that type's own doc comment for why this
/// has no `#[ts(type = "Record<...>")]` override).
// See `FieldSpecMapDto` for why there is no `#[serde(transparent)]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct TargetShapeMapDto(pub BTreeMap<String, TargetShapeDto>);

impl From<&BTreeMap<FieldPath, TargetShape>> for TargetShapeMapDto {
    fn from(value: &BTreeMap<FieldPath, TargetShape>) -> Self {
        Self(
            value
                .iter()
                .map(|(path, shape)| (path.to_string(), shape.into()))
                .collect(),
        )
    }
}

impl TryFrom<TargetShapeMapDto> for BTreeMap<FieldPath, TargetShape> {
    type Error = CommandError;

    fn try_from(value: TargetShapeMapDto) -> Result<Self, Self::Error> {
        value
            .0
            .into_iter()
            .map(|(path, shape)| Ok((parse_field_path(&path)?, shape.into())))
            .collect()
    }
}

/// Mirrors [`rim_resolve::domain::AssignmentSchema`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export, export_to = "../../src/types/generated/")]
pub struct AssignmentSchemaDto {
    /// The assignment def type, e.g. `example.PartAssignmentDef`.
    pub def_type: String,
    /// The reference set the schema was inferred against — the effective
    /// (closure-applied) set, exactly [`rim_resolve::domain::AssignmentSchema::refs`].
    pub refs: Vec<String>,
    /// Every field observed on at least one instance, by canonical path text.
    pub fields: FieldSpecMapDto,
    /// Per target key: the learned shape, by canonical path text.
    pub target_shapes: TargetShapeMapDto,
}

impl From<&rim_resolve::domain::AssignmentSchema> for AssignmentSchemaDto {
    fn from(value: &rim_resolve::domain::AssignmentSchema) -> Self {
        Self {
            def_type: value.def_type.clone(),
            refs: value
                .refs
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
            fields: (&value.fields).into(),
            target_shapes: (&value.target_shapes).into(),
        }
    }
}

impl TryFrom<AssignmentSchemaDto> for rim_resolve::domain::AssignmentSchema {
    type Error = CommandError;

    fn try_from(value: AssignmentSchemaDto) -> Result<Self, Self::Error> {
        Ok(Self {
            def_type: value.def_type,
            refs: value.refs.into_iter().map(ModId::new).collect(),
            fields: value.fields.try_into()?,
            target_shapes: value.target_shapes.try_into()?,
        })
    }
}

// -- TargetRef / RowValue / AssignmentRow -----------------------------
