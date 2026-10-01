//! [`CopyFrom`]: copies every field the schema knows from an existing
//! instance into a new row.
//! Session-backed only for [`FieldRole::ItemSlot`] validation (see
//! [`CopyFromOutcome`]'s own doc comment for why); no session mutation.

use std::collections::BTreeMap;

use rim_resolve::domain::{
    AssignmentProject, AssignmentRow, FieldPath, FieldRole, InstanceValues, KnownDefs, RowValue,
    TargetRef,
};

use crate::Session;
use crate::assignment_refs::{SessionKnownDefs, default_def_name};

/// Everything that can go wrong building a copied row.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CopyFromError {
    /// `project` has no section for `def_type`.
    #[error("this project has no section for def type {0:?}")]
    UnknownSection(String),
}

/// One [`FieldRole::ItemSlot`] value [`CopyFrom::execute`] refused to copy
/// because it names a def [`KnownDefs`] doesn't recognize — see
/// [`CopyFromOutcome::dropped`]'s own doc comment for why this happens
/// disproportionately often for a field rescued by the `Def`/`Defs`-suffix
/// exemption (`rim_resolve::domain::assignment::schema`'s own
/// `has_def_suffixed_leaf_tag`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DroppedItemSlotValue {
    /// The item-slot field the value was dropped from.
    pub path: FieldPath,
    /// The slot's own item type.
    pub def_type: String,
    /// The unknown name that was dropped.
    pub name: String,
}

/// [`CopyFrom::execute`]'s result: the row it built, plus every
/// [`FieldRole::ItemSlot`] value it had to drop along the way. No `Eq`
/// (only `PartialEq`) — [`AssignmentRow::values`] can hold
/// [`RowValue::Numbers`], and `f64` has no total order.
#[derive(Debug, Clone, PartialEq)]
pub struct CopyFromOutcome {
    /// The freshly built row — still needs
    /// [`AssignmentProject::set_row`] to validate and record it (every
    /// check other than an item slot's own name liveness, already
    /// applied here, is still `set_row`'s job).
    pub row: AssignmentRow,
    /// Every [`FieldRole::ItemSlot`] value this call dropped because it
    /// named a def neither [`KnownDefs::contains`] nor
    /// [`KnownDefs::own_instances`] recognizes. A source instance's own
    /// values disproportionately include such a name for a field rescued
    /// by the `Def`/`Defs`-suffix exemption: that exemption's whole
    /// premise is that most of the field's distinct values name
    /// *inactive* defs, so a coverage winner's own value there is very
    /// likely one of them. As an `ItemSlot`, copying an
    /// inactive name verbatim would build a row `set_row` always
    /// rejects (`AssignmentRowError::UnknownItem`) naming a def the user
    /// never chose and the picker never shows. Report this to the user
    /// (e.g. "3 borrowed values were skipped because those defs are not
    /// active") rather than silently discarding it.
    pub dropped: Vec<DroppedItemSlotValue>,
}

/// Builds a fresh [`AssignmentRow`] for `target` from `source_instance`'s
/// already-flattened fields (e.g. one [`crate::use_cases::AssignmentInstances`]
/// row the user picked from a "copy from" search): a
/// [`FieldRole::Chances`]/[`FieldRole::Scalar`] field present on
/// `source_instance` is copied verbatim; a [`FieldRole::ItemSlot`]
/// field's own values are filtered to the ones [`KnownDefs`] still
/// recognizes (either an active def, or one of `project`'s own
/// free-standing instances) — see [`CopyFromOutcome::dropped`] for why —
/// and the field is dropped entirely when nothing survives; a
/// [`FieldRole::TargetKey`] field is never given a value (it always
/// renders from `target` itself — see [`AssignmentProject::set_row`]'s
/// own invariant); a [`FieldRole::Opaque`] field is never copied into the
/// row either — it has no [`RowValue`] shape of its own (v1 shows it
/// read-only, copied for *display* only, never stored — see that
/// variant's own doc comment). The result still needs
/// [`AssignmentProject::set_row`] to validate and record it (every check
/// other than item-slot name liveness is still caught there, not here).
pub struct CopyFrom;

impl CopyFrom {
    /// See this module's own doc comment.
    ///
    /// # Errors
    ///
    /// See [`CopyFromError`].
    pub fn execute(
        project: &AssignmentProject,
        def_type: &str,
        target: &TargetRef,
        source_instance: &InstanceValues,
        session: &Session,
    ) -> Result<CopyFromOutcome, CopyFromError> {
        let section = project
            .section(def_type)
            .ok_or_else(|| CopyFromError::UnknownSection(def_type.to_string()))?;
        let known =
            SessionKnownDefs::new(session, project.identity().package_id().clone(), project);
        let mut values = BTreeMap::new();
        let mut dropped = Vec::new();
        for (path, spec) in &section.schema.fields {
            if path == &target.key_field {
                continue;
            }
            let Some(occurrence) = source_instance.get(path) else {
                continue;
            };
            let value = match &spec.role {
                FieldRole::TargetKey { .. } => None,
                FieldRole::ItemSlot {
                    def_type: item_type,
                } => {
                    let own = known.own_instances(item_type);
                    let mut kept = Vec::new();
                    for name in &occurrence.values {
                        if known.contains(item_type, name) || own.contains(name) {
                            kept.push(name.clone());
                        } else {
                            dropped.push(DroppedItemSlotValue {
                                path: path.clone(),
                                def_type: item_type.clone(),
                                name: name.clone(),
                            });
                        }
                    }
                    (!kept.is_empty()).then_some(RowValue::Names(kept))
                }
                FieldRole::Chances { .. } => {
                    let numbers: Vec<f64> = occurrence
                        .values
                        .iter()
                        .filter_map(|v| v.parse().ok())
                        .collect();
                    (numbers.len() == occurrence.values.len()).then_some(RowValue::Numbers(numbers))
                }
                FieldRole::Scalar { .. } => occurrence
                    .values
                    .first()
                    .map(|text| RowValue::Text(text.clone())),
                FieldRole::Opaque => None,
            };
            if let Some(value) = value {
                values.insert(path.clone(), value);
            }
        }

        Ok(CopyFromOutcome {
            row: AssignmentRow {
                values,
                def_name: default_def_name(project, target),
                note: None,
            },
            dropped,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{session_fixture, session_with_sources_and_mods};
    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{
        AssignmentId, AssignmentSchema, Cardinality, DefKey, FieldOccurrence, FieldPath, FieldSpec,
        PatchModIdentity, ScalarKind,
    };
    use std::collections::BTreeSet;

    /// A session where `example.PartDef` `"Wrench"` is a known active def
    /// — the item-slot value most of these tests copy.
    fn known_session() -> Session {
        let mut sources = SourceIndex::default();
        sources.owners_by_def.insert(
            ("example.PartDef".to_string(), "Wrench".to_string()),
            vec![ModId::new("fixture.parts")],
        );
        session_with_sources_and_mods(
            sources,
            crate::test_support::report_fixture(&["fixture.parts"]),
            &["fixture.parts"],
        )
    }

    fn field(tag: &str) -> FieldPath {
        tag.parse().unwrap()
    }

    fn occurrence(cardinality: Cardinality, values: &[&str]) -> FieldOccurrence {
        FieldOccurrence {
            cardinality,
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn schema() -> AssignmentSchema {
        let mut fields = BTreeMap::new();
        fields.insert(
            field("speciesNames"),
            FieldSpec {
                role: FieldRole::TargetKey {
                    def_type: "ThingDef".to_string(),
                },
                cardinality: Cardinality::List,
                observed: (1, 1),
                inferred_role: None,
            },
        );
        fields.insert(
            field("primaryTool"),
            FieldSpec {
                role: FieldRole::ItemSlot {
                    def_type: "example.PartDef".to_string(),
                },
                cardinality: Cardinality::List,
                observed: (1, 1),
                inferred_role: None,
            },
        );
        fields.insert(
            field("chanceprimaryTool"),
            FieldSpec {
                role: FieldRole::Chances {
                    for_slot: field("primaryTool"),
                },
                cardinality: Cardinality::List,
                observed: (1, 1),
                inferred_role: None,
            },
        );
        fields.insert(
            field("hasSingleGender"),
            FieldSpec {
                role: FieldRole::Scalar {
                    kind: ScalarKind::Bool,
                    default: Some("false".to_string()),
                },
                cardinality: Cardinality::Scalar,
                observed: (1, 1),
                inferred_role: None,
            },
        );
        fields.insert(
            field("modExtensions"),
            FieldSpec {
                role: FieldRole::Opaque,
                cardinality: Cardinality::List,
                observed: (1, 1),
                inferred_role: None,
            },
        );
        AssignmentSchema {
            def_type: "example.PartAssignmentDef".to_string(),
            refs: BTreeSet::new(),
            fields,
            target_shapes: BTreeMap::new(),
        }
    }

    fn project() -> AssignmentProject {
        AssignmentProject::new(
            AssignmentId::derive(
                "profile",
                &ModId::new("mypatch.parts"),
                jiff::Timestamp::UNIX_EPOCH,
            ),
            "name".to_string(),
            PatchModIdentity::new("mypatch.parts", "Sample Part Patch").expect("valid identity"),
            BTreeSet::new(),
            BTreeSet::new(),
            schema(),
            jiff::Timestamp::UNIX_EPOCH,
        )
    }

    fn target() -> TargetRef {
        TargetRef {
            key_field: field("speciesNames"),
            def: DefKey {
                def_type: "ThingDef".to_string(),
                def_name: "Elf".to_string(),
            },
        }
    }

    #[test]
    fn copies_item_slot_chances_and_scalar_fields_verbatim() {
        let mut source: InstanceValues = BTreeMap::new();
        source.insert(
            field("speciesNames"),
            occurrence(Cardinality::List, &["Human"]),
        );
        source.insert(
            field("primaryTool"),
            occurrence(Cardinality::List, &["Wrench"]),
        );
        source.insert(
            field("chanceprimaryTool"),
            occurrence(Cardinality::List, &["1.0"]),
        );
        source.insert(
            field("hasSingleGender"),
            occurrence(Cardinality::Scalar, &["true"]),
        );
        source.insert(field("modExtensions"), occurrence(Cardinality::List, &[]));

        let outcome = CopyFrom::execute(
            &project(),
            "example.PartAssignmentDef",
            &target(),
            &source,
            &known_session(),
        )
        .expect("valid section");
        let row = outcome.row;

        assert_eq!(row.def_name, "mypatch_parts_Elf");
        assert_eq!(
            row.values.get(&field("primaryTool")),
            Some(&RowValue::Names(vec!["Wrench".to_string()]))
        );
        assert_eq!(
            row.values.get(&field("chanceprimaryTool")),
            Some(&RowValue::Numbers(vec![1.0]))
        );
        assert_eq!(
            row.values.get(&field("hasSingleGender")),
            Some(&RowValue::Text("true".to_string()))
        );
        assert!(
            !row.values.contains_key(&field("speciesNames")),
            "a TargetKey field is never given a value — it always renders from TargetRef"
        );
        assert!(
            !row.values.contains_key(&field("modExtensions")),
            "an Opaque field is never copied into the row's own values"
        );
        assert!(
            outcome.dropped.is_empty(),
            "every value here names a known def: {:?}",
            outcome.dropped
        );
    }

    /// An `ItemSlot` value naming a def that isn't
    /// active must be dropped, not copied verbatim into a row `set_row`
    /// would then reject with `UnknownItem`.
    #[test]
    fn an_item_slot_value_naming_an_unknown_def_is_dropped_and_reported() {
        let mut source: InstanceValues = BTreeMap::new();
        source.insert(
            field("primaryTool"),
            occurrence(Cardinality::List, &["Wrench", "NotActive"]),
        );

        let outcome = CopyFrom::execute(
            &project(),
            "example.PartAssignmentDef",
            &target(),
            &source,
            &known_session(),
        )
        .expect("valid section");

        assert_eq!(
            outcome.row.values.get(&field("primaryTool")),
            Some(&RowValue::Names(vec!["Wrench".to_string()])),
            "the known name survives, the unknown one is dropped"
        );
        assert_eq!(
            outcome.dropped,
            vec![DroppedItemSlotValue {
                path: field("primaryTool"),
                def_type: "example.PartDef".to_string(),
                name: "NotActive".to_string(),
            }]
        );
    }

    /// When *every* value of an item-slot field is unknown, the field is
    /// dropped entirely rather than copied as an empty `Names([])` — an
    /// empty item slot has no useful default to fall back to.
    #[test]
    fn an_item_slot_field_with_no_known_values_at_all_is_dropped_entirely() {
        let mut source: InstanceValues = BTreeMap::new();
        source.insert(
            field("primaryTool"),
            occurrence(Cardinality::List, &["NotActive"]),
        );

        let outcome = CopyFrom::execute(
            &project(),
            "example.PartAssignmentDef",
            &target(),
            &source,
            &session_fixture(&[]),
        )
        .expect("valid section");

        assert!(
            !outcome.row.values.contains_key(&field("primaryTool")),
            "no known value survived, so the field must be absent, not an empty Names([])"
        );
        assert_eq!(outcome.dropped.len(), 1);
    }

    #[test]
    fn a_missing_source_field_is_simply_absent_from_the_row() {
        let source: InstanceValues = BTreeMap::new();

        let outcome = CopyFrom::execute(
            &project(),
            "example.PartAssignmentDef",
            &target(),
            &source,
            &session_fixture(&[]),
        )
        .expect("valid section");

        assert!(outcome.row.values.is_empty());
    }

    #[test]
    fn a_non_numeric_chances_value_is_not_copied() {
        let mut source: InstanceValues = BTreeMap::new();
        source.insert(
            field("chanceprimaryTool"),
            occurrence(Cardinality::List, &["not-a-number"]),
        );

        let outcome = CopyFrom::execute(
            &project(),
            "example.PartAssignmentDef",
            &target(),
            &source,
            &session_fixture(&[]),
        )
        .expect("valid section");

        assert!(!outcome.row.values.contains_key(&field("chanceprimaryTool")));
    }

    #[test]
    fn rejects_a_def_type_with_no_section() {
        let source: InstanceValues = BTreeMap::new();

        let result = CopyFrom::execute(
            &project(),
            "no.such.type",
            &target(),
            &source,
            &session_fixture(&[]),
        );

        assert_eq!(
            result,
            Err(CopyFromError::UnknownSection("no.such.type".to_string()))
        );
    }
}
