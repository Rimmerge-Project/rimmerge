//! [`AssignmentId`]/[`AssignmentProject`]: the patch maker project itself.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use rim_analyzer::domain::ModId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::domain::finding::DefKey;
use crate::domain::merge::FieldPath;
use crate::domain::patch::PatchModIdentity;

use super::schema::{AssignmentSchema, Cardinality, FieldRole, FieldSpec};
use super::section::{RowKey, Section};

/// The stable identity of one [`AssignmentProject`]: the same
/// "12 hex chars of a sha256 digest, derived from the profile hash, the
/// user's chosen package id, and the creation timestamp" shape
/// [`crate::domain::patch::PatchId`] already uses — a separate type (not
/// a type alias) because an assignment project and a compat patch are
/// never interchangeable, even though their identity has the same shape.
///
/// **File-format rule**, exactly `crates/rim-resolve/CLAUDE.md`'s own:
/// this type's canonical text form is persisted (a future `rim-io`
/// store's file name under `<profile>/assignments/<id>.json`) — changing
/// `Display`/`FromStr` here is a file-format change requiring a store
/// version bump and a migration, the same as `PatchId`/`FieldPath`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AssignmentId(String);

/// [`AssignmentId`]'s canonical text form failed to parse: not exactly
/// 12 lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid assignment id {0:?}: expected exactly 12 lowercase hex characters")]
pub struct AssignmentIdParseError(String);

impl AssignmentId {
    /// Derives a new project's id, unique per creation.
    #[must_use]
    pub fn derive(profile_hash: &str, package_id: &ModId, created_at: jiff::Timestamp) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(profile_hash.as_bytes());
        hasher.update(package_id.as_str().as_bytes());
        hasher.update(created_at.to_string().as_bytes());
        let digest = hasher.finalize();
        let hex: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
        Self(hex)
    }

    /// The raw 12-hex-char text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AssignmentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for AssignmentId {
    type Err = AssignmentIdParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let is_lowercase_hex = text.len() == 12
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || matches!(b, b'a'..=b'f'));
        if is_lowercase_hex {
            Ok(Self(text.to_string()))
        } else {
            Err(AssignmentIdParseError(text.to_string()))
        }
    }
}

impl TryFrom<String> for AssignmentId {
    type Error = AssignmentIdParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<AssignmentId> for String {
    fn from(value: AssignmentId) -> Self {
        value.0
    }
}

/// One target def, matched through one target key field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TargetRef {
    /// The [`FieldRole::TargetKey`] field this target was matched
    /// through.
    pub key_field: FieldPath,
    /// The target def itself.
    pub def: DefKey,
}

/// What the user put at one field of one [`AssignmentRow`]. `Omit` means
/// "use the schema default / omit the element" — the emitted rendering
/// never writes an empty element for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowValue {
    /// Item names for an [`FieldRole::ItemSlot`] field.
    Names(Vec<String>),
    /// Floats for a [`FieldRole::Chances`] field.
    Numbers(Vec<f64>),
    /// Text for a [`FieldRole::Scalar`] field.
    Text(String),
    /// Omitted: automatic / schema default.
    Omit,
}

/// One target def's row: the item/chance/scalar values the user chose,
/// keyed by field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentRow {
    /// Per field: the value the user chose. A field absent from this map
    /// behaves exactly like [`RowValue::Omit`].
    pub values: BTreeMap<FieldPath, RowValue>,
    /// The emitted `defName`, validated unique within the project by
    /// [`AssignmentProject::set_row`].
    pub def_name: String,
    /// A free-text note.
    pub note: Option<String>,
}

/// A read-only view of which defs of a given type actually exist in the
/// active list — [`AssignmentProject::set_row`]'s item-slot validation
/// against it, the same way [`crate::domain::patch::PatchScope`]
/// validates against an `ActiveMods`-shaped view rather than reading the
/// report itself. Implemented by a session-layer cache; this pure crate
/// only ever calls through it.
///
/// [`Self::contains`] backs two different checks in [`AssignmentProject::set_row`]:
/// an item slot's names (`def_type` = the slot's own item type), and
/// `row.def_name`'s collision check (`def_type` = the *schema's* def
/// type). For the second check, **the session's
/// own view must exclude this project's own previously-exported mod**:
/// once exported, this project's generated instances are themselves
/// active defs of the schema's def type, and without that exclusion
/// every subsequent save of an already-exported row would spuriously
/// collide with itself.
pub trait KnownDefs {
    /// Whether a def of `def_type` named `name` exists in the active
    /// list.
    fn contains(&self, def_type: &str, name: &str) -> bool;

    /// This project's own free-standing ("new def") row names of
    /// `def_type` — an [`FieldRole::ItemSlot`] value naming one of these
    /// is accepted by [`AssignmentProject::set_row`] exactly like an
    /// active def of the same type — every section's free-standing rows
    /// are visible to every other section's item pickers as items owned
    /// by this project. Defaults
    /// to empty so a fake implementing only [`Self::contains`] stays
    /// small; the session's real view answers from the project's own
    /// [`Section::own_instance_names`].
    fn own_instances(&self, _def_type: &str) -> BTreeSet<String> {
        BTreeSet::new()
    }
}

/// Why [`AssignmentProject::set_row`] rejected a row.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssignmentRowError {
    /// The target's `key_field` isn't a [`FieldRole::TargetKey`] field in
    /// this project's schema.
    #[error("{0} is not a TargetKey field in this schema")]
    NotATargetKey(FieldPath),
    /// `def_name` is empty (after trimming).
    #[error("def name must not be empty")]
    EmptyDefName,
    /// `def_name` already names an active instance of the schema's own
    /// def type — refused rather than silently
    /// colliding with it on export. See [`KnownDefs`]'s own doc comment
    /// for the exclusion the session's view must apply here.
    #[error("{0:?} is already the defName of an active instance of this project's assignment type")]
    DefNameExistsInActiveList(String),
    /// A field the schema doesn't know about was given a value.
    #[error("{0} is not a field of this project's schema")]
    UnknownField(FieldPath),
    /// A field's value isn't the shape its role accepts (a `Text` value
    /// on an [`FieldRole::ItemSlot`], `Names` on a [`FieldRole::Scalar`],
    /// any value at all on a [`FieldRole::TargetKey`], ...).
    #[error("{path}: a {role} field does not accept this value")]
    WrongValueShape {
        /// The field whose value shape didn't match its role.
        path: FieldPath,
        /// The field's actual role.
        role: FieldRole,
    },
    /// A [`FieldRole::ItemSlot`] field's value names a def that doesn't
    /// exist in the active list.
    #[error("{path}: {def_type} {name:?} is not a known def")]
    UnknownItem {
        /// The slot field.
        path: FieldPath,
        /// The slot's item type.
        def_type: String,
        /// The unknown name.
        name: String,
    },
    /// A [`Cardinality::Scalar`] [`FieldRole::ItemSlot`] field's value
    /// named anything but exactly one item. `rim-merge`'s own
    /// `render_leaf` (`assign.rs`) already refuses to render such a
    /// value (it becomes a `SkippedField` at export instead) — this
    /// variant makes the same "exactly one" invariant a domain check, so
    /// `set_row` refuses it up front rather than only `rim-merge`'s
    /// render step ever catching it, silently, at export time (the CLI's
    /// own `assign set-row` has no picker to enforce this in the
    /// interface layer the way `ItemPicker.vue`'s single-select radio
    /// does).
    #[error("{path}: a scalar item slot must name exactly one item, found {actual}")]
    ScalarItemSlotWrongCount {
        /// The scalar item-slot field.
        path: FieldPath,
        /// How many names it was given.
        actual: usize,
    },
    /// A [`FieldRole::Chances`] field's value contains a `NaN` or
    /// infinite float, which cannot round-trip through JSON.
    #[error("{0} contains a non-finite (NaN or infinite) chance value")]
    NonFiniteChance(FieldPath),
    /// A [`FieldRole::Chances`] field's value doesn't have the same
    /// length as its paired slot.
    #[error("{path}: expected {expected} chance values to match its slot, found {actual}")]
    ChanceLengthMismatch {
        /// The chances field.
        path: FieldPath,
        /// The paired slot's item count.
        expected: usize,
        /// The chances value's own length.
        actual: usize,
    },
    /// The row's `def_name` is already used by another row in this
    /// project — another row in the same section, or a free-standing
    /// ("new def") row in a *different* section (the cross-section
    /// `defName` collision rule).
    #[error("def name {0:?} is already used by another row in this project")]
    DuplicateDefName(String),
    /// [`AssignmentProject::set_row`] was called with no section for
    /// `def_type` — [`AssignmentProject::add_section`] must run first.
    #[error("this project has no section for def type {0:?}")]
    UnknownSection(String),
    /// [`RowKey`]'s variant doesn't match `def_type`'s own section kind: a
    /// [`RowKey::Target`] on a free-standing section, or a [`RowKey::Own`]
    /// on a target-keyed one.
    #[error(
        "row key shape does not match section {0:?}'s own kind (target-keyed vs. free-standing)"
    )]
    RowKeyMismatch(String),
    /// A [`RowKey::Own`] key's own string didn't match `row.def_name` —
    /// the two must agree, or [`Section::own_instance_names`] (what an
    /// item picker/`remove_section`'s in-use scan see) and the row's own
    /// rendered `defName` would diverge, letting an item slot reference
    /// the *key* while the export renders a differently-named def,
    /// silently dangling.
    #[error("own row key {key:?} does not match this row's own def name {def_name:?}")]
    OwnKeyDefNameMismatch {
        /// The [`RowKey::Own`] string.
        key: String,
        /// The row's own `def_name`.
        def_name: String,
    },
}

/// Whether `value`'s shape is one `role` accepts: [`RowValue::Omit`] for
/// anything except [`FieldRole::TargetKey`] (a target-key field is
/// addressed by [`TargetRef::key_field`], never given a row value of its
/// own — "anything on `TargetKey`" is a shape mismatch, `Omit` included);
/// [`RowValue::Names`] for [`FieldRole::ItemSlot`]; [`RowValue::Numbers`]
/// for [`FieldRole::Chances`]; [`RowValue::Text`] for
/// [`FieldRole::Scalar`]. [`FieldRole::Opaque`] accepts nothing but
/// `Omit` — v1 never edits it (see that variant's own doc comment).
fn value_shape_matches(role: &FieldRole, value: &RowValue) -> bool {
    match (role, value) {
        (FieldRole::TargetKey { .. }, _) => false,
        (_, RowValue::Omit) => true,
        (FieldRole::ItemSlot { .. }, RowValue::Names(_)) => true,
        (FieldRole::Chances { .. }, RowValue::Numbers(_)) => true,
        (FieldRole::Scalar { .. }, RowValue::Text(_)) => true,
        _ => false,
    }
}

/// Validates one field's value against its role — [`AssignmentProject::set_row`]'s
/// own per-field checks, extracted so that function stays readable:
/// shape ([`value_shape_matches`]), a [`Cardinality::Scalar`]
/// [`FieldRole::ItemSlot`]'s own "exactly one name" invariant, an
/// [`FieldRole::ItemSlot`]'s names against `known`, and an
/// [`FieldRole::Chances`]'s values (finite, and the same length as its
/// paired slot — looked up in `values`, the row's whole value map, since
/// the slot's own value lives at a different path than `path`).
fn validate_value(
    path: &FieldPath,
    value: &RowValue,
    spec: &FieldSpec,
    values: &BTreeMap<FieldPath, RowValue>,
    known: &dyn KnownDefs,
) -> Result<(), AssignmentRowError> {
    if !value_shape_matches(&spec.role, value) {
        return Err(AssignmentRowError::WrongValueShape {
            path: path.clone(),
            role: spec.role.clone(),
        });
    }
    match (&spec.role, value) {
        (FieldRole::ItemSlot { def_type }, RowValue::Names(names)) => {
            if spec.cardinality == Cardinality::Scalar && names.len() != 1 {
                return Err(AssignmentRowError::ScalarItemSlotWrongCount {
                    path: path.clone(),
                    actual: names.len(),
                });
            }
            let own = known.own_instances(def_type);
            for name in names {
                if !known.contains(def_type, name) && !own.contains(name) {
                    return Err(AssignmentRowError::UnknownItem {
                        path: path.clone(),
                        def_type: def_type.clone(),
                        name: name.clone(),
                    });
                }
            }
        }
        (FieldRole::Chances { for_slot }, RowValue::Numbers(numbers)) => {
            if numbers.iter().any(|n| !n.is_finite()) {
                return Err(AssignmentRowError::NonFiniteChance(path.clone()));
            }
            let expected = match values.get(for_slot) {
                Some(RowValue::Names(names)) => names.len(),
                _ => 0,
            };
            if numbers.len() != expected {
                return Err(AssignmentRowError::ChanceLengthMismatch {
                    path: path.clone(),
                    expected,
                    actual: numbers.len(),
                });
            }
        }
        _ => {}
    }
    Ok(())
}

/// Plain, already-persisted data for every field an [`AssignmentProject`]
/// carries — mirrors [`crate::domain::patch::StoredPatchProject`]'s own
/// role: the shape [`AssignmentProject::from_stored`] trusts completely,
/// versus [`AssignmentProject::new`]'s constructor arguments for a
/// brand-new project. No invariants of its own; a future `rim-io`
/// `JsonAssignmentProjectStore` builds one from its own deserialized
/// record, the same way `JsonPatchProjectStore` does for
/// `StoredPatchProject`.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredAssignmentProject {
    /// The project's stable id.
    pub id: AssignmentId,
    /// The project's label in lists.
    pub name: String,
    /// The project's published identity.
    pub identity: PatchModIdentity,
    /// The `About.xml` `<author>` this project will export.
    pub author: String,
    /// The `About.xml` `<description>` this project will export.
    pub description: String,
    /// The selected reference set R (before the dependency closure).
    pub refs: BTreeSet<ModId>,
    /// Closure members the user opted out of.
    pub excluded_refs: BTreeSet<ModId>,
    /// The target set T.
    pub targets: BTreeSet<ModId>,
    /// Every def-type section this project assigns into, by its own
    /// [`AssignmentSchema::def_type`]. See [`Section`]'s own doc comment.
    pub sections: BTreeMap<String, Section>,
    /// The last folder this project was exported to, if any.
    pub export_dir: Option<PathBuf>,
    /// When the project was created.
    pub created_at: jiff::Timestamp,
    /// When the project was last changed.
    pub updated_at: jiff::Timestamp,
}

/// Why [`AssignmentProject::add_section`]/[`AssignmentProject::remove_section`]
/// refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SectionError {
    /// [`AssignmentProject::add_section`]: a section for this def type
    /// already exists.
    #[error("a section for def type {0:?} already exists")]
    AlreadyExists(String),
    /// [`AssignmentProject::remove_section`] without `force`: the
    /// section's own free-standing rows are still named by an
    /// [`FieldRole::ItemSlot`] value somewhere in another section. Each
    /// entry is the referencing section's def type, the referencing row's
    /// key, and the field that names the value.
    #[error("section is still referenced by {} row(s) in other sections",
        referenced_by.len()
    )]
    SectionInUse {
        /// `(referencing def type, referencing row key, referencing field)`.
        referenced_by: Vec<(String, RowKey, FieldPath)>,
    },
}

/// A generic patch maker project: a reference set R, a target set T, and
/// one [`Section`] per def type it assigns into. See
/// `crate::domain::assignment`'s own module doc comment for the pieces of
/// project behavior this pure crate does **not** implement (re-inference
/// on an R change, row-dropping on a T change) and why — both apply
/// per-section.
#[derive(Debug, Clone, PartialEq)]
pub struct AssignmentProject {
    id: AssignmentId,
    name: String,
    identity: PatchModIdentity,
    author: String,
    description: String,
    refs: BTreeSet<ModId>,
    excluded_refs: BTreeSet<ModId>,
    targets: BTreeSet<ModId>,
    sections: BTreeMap<String, Section>,
    export_dir: Option<PathBuf>,
    created_at: jiff::Timestamp,
    updated_at: jiff::Timestamp,
}

/// The default `About.xml` author for a newly created assignment
/// project (the same policy
/// [`crate::domain::patch::PatchProject::new`] applies) — a fixed
/// constant, since this crate does no IO.
const DEFAULT_ASSIGNMENT_AUTHOR: &str = "Rimmerge";

impl AssignmentProject {
    /// Builds a new assignment project with a single section, built from
    /// `schema` and keyed by its own [`AssignmentSchema::def_type`] — the
    /// project a wizard's phase-2 inference (`CreateAssignment::execute`)
    /// creates. Use [`Self::add_section`] afterward for a second section.
    #[must_use]
    pub fn new(
        id: AssignmentId,
        name: String,
        identity: PatchModIdentity,
        refs: BTreeSet<ModId>,
        targets: BTreeSet<ModId>,
        schema: AssignmentSchema,
        created_at: jiff::Timestamp,
    ) -> Self {
        let mut sections = BTreeMap::new();
        sections.insert(schema.def_type.clone(), Section::new(schema));
        Self {
            id,
            name,
            identity,
            author: DEFAULT_ASSIGNMENT_AUTHOR.to_string(),
            description: String::new(),
            refs,
            excluded_refs: BTreeSet::new(),
            targets,
            sections,
            export_dir: None,
            created_at,
            updated_at: created_at,
        }
    }

    /// The project's stable id.
    #[must_use]
    pub fn id(&self) -> &AssignmentId {
        &self.id
    }

    /// The project's label in lists.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The project's published identity.
    #[must_use]
    pub fn identity(&self) -> &PatchModIdentity {
        &self.identity
    }

    /// The `About.xml` `<author>`.
    #[must_use]
    pub fn author(&self) -> &str {
        &self.author
    }

    /// The `About.xml` `<description>`.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    /// The selected reference set (before the dependency closure).
    #[must_use]
    pub fn refs(&self) -> &BTreeSet<ModId> {
        &self.refs
    }

    /// Closure members the user opted out of.
    #[must_use]
    pub fn excluded_refs(&self) -> &BTreeSet<ModId> {
        &self.excluded_refs
    }

    /// The target set T.
    #[must_use]
    pub fn targets(&self) -> &BTreeSet<ModId> {
        &self.targets
    }

    /// Every section, by its own def type.
    #[must_use]
    pub fn sections(&self) -> &BTreeMap<String, Section> {
        &self.sections
    }

    /// The section for `def_type`, if this project has one.
    #[must_use]
    pub fn section(&self, def_type: &str) -> Option<&Section> {
        self.sections.get(def_type)
    }

    /// The section for `def_type`, if this project has one — the mutable
    /// counterpart of [`Self::section`]. **No validation of any kind**:
    /// [`Section::rows`]' own [`AssignmentRow::values`] can be edited
    /// directly through the result, bypassing every [`Self::set_row`]
    /// invariant (shape, item-slot liveness, `defName` uniqueness).
    /// Removing a value is always safe on its own (an absent field
    /// behaves exactly like [`RowValue::Omit`], per
    /// [`AssignmentRow::values`]'s own doc comment) — this exists for
    /// `rim-session`'s own schema-reconciliation step (`UpdateAssignment`),
    /// which strips a row's stored value once a reclassified field's new
    /// role no longer accepts its old shape. Prefer [`Self::set_row`] for
    /// anything that adds or changes a value.
    #[must_use]
    pub fn section_mut(&mut self, def_type: &str) -> Option<&mut Section> {
        self.sections.get_mut(def_type)
    }

    /// The last folder this project was exported to, if any.
    #[must_use]
    pub fn export_dir(&self) -> Option<&Path> {
        self.export_dir.as_deref()
    }

    /// When the project was created.
    #[must_use]
    pub fn created_at(&self) -> jiff::Timestamp {
        self.created_at
    }

    /// When the project was last changed.
    #[must_use]
    pub fn updated_at(&self) -> jiff::Timestamp {
        self.updated_at
    }

    /// Renames the project's list label.
    pub fn set_name(&mut self, name: String) {
        self.name = name;
    }

    /// Sets the `About.xml` `<author>`.
    pub fn set_author(&mut self, author: String) {
        self.author = author;
    }

    /// Sets the `About.xml` `<description>`.
    pub fn set_description(&mut self, description: String) {
        self.description = description;
    }

    /// Replaces the published identity.
    pub fn set_identity(&mut self, identity: PatchModIdentity) {
        self.identity = identity;
    }

    /// Replaces the selected reference set — a plain setter, with no
    /// re-inference (see `crate::domain::assignment`'s own module doc
    /// comment for why that belongs to a later, session-layer step).
    pub fn set_refs(&mut self, refs: BTreeSet<ModId>) {
        self.refs = refs;
    }

    /// Replaces the closure exclusions.
    pub fn set_excluded_refs(&mut self, excluded_refs: BTreeSet<ModId>) {
        self.excluded_refs = excluded_refs;
    }

    /// Replaces the target set — a plain setter, with no row-dropping
    /// (see `crate::domain::assignment`'s own module doc comment for why
    /// that belongs to a later, session-layer step).
    pub fn set_targets(&mut self, targets: BTreeSet<ModId>) {
        self.targets = targets;
    }

    /// Replaces one section's confirmed schema in place, keeping its rows —
    /// `UpdateAssignment`'s (`rim-session`) own re-inference call. A no-op
    /// when no section for `def_type` exists.
    pub fn set_section_schema(&mut self, def_type: &str, schema: AssignmentSchema) {
        if let Some(section) = self.sections.get_mut(def_type) {
            section.schema = schema;
        }
    }

    /// Sets (or clears, with `None`) the last export target.
    pub fn set_export_dir(&mut self, export_dir: Option<PathBuf>) {
        self.export_dir = export_dir;
    }

    /// Adds a new, empty section built from `schema`, keyed by its own
    /// [`AssignmentSchema::def_type`].
    ///
    /// # Errors
    ///
    /// Returns [`SectionError::AlreadyExists`] when a section for that def
    /// type already exists.
    pub fn add_section(&mut self, schema: AssignmentSchema) -> Result<(), SectionError> {
        if self.sections.contains_key(&schema.def_type) {
            return Err(SectionError::AlreadyExists(schema.def_type));
        }
        self.sections
            .insert(schema.def_type.clone(), Section::new(schema));
        Ok(())
    }

    /// Removes the section for `def_type`, if one exists. `Ok(None)` when
    /// there is no such section (idempotent, like [`Self::clear_row`]).
    ///
    /// # Errors
    ///
    /// Without `force`, returns [`SectionError::SectionInUse`] when the
    /// section's own free-standing row names are still named by an
    /// [`FieldRole::ItemSlot`] value in another section's row. With
    /// `force`, the section is removed regardless and those references are
    /// left dangling: a later
    /// export surfaces each as a skip rather than this method silently
    /// scrubbing them.
    pub fn remove_section(
        &mut self,
        def_type: &str,
        force: bool,
    ) -> Result<Option<Section>, SectionError> {
        let Some(section) = self.sections.get(def_type) else {
            return Ok(None);
        };
        if !force {
            let referenced_by = self.referencing_rows(def_type, &section.own_instance_names());
            if !referenced_by.is_empty() {
                return Err(SectionError::SectionInUse { referenced_by });
            }
        }
        Ok(self.sections.remove(def_type))
    }

    /// Every `(other def type, other row key, field)` in a section other
    /// than `def_type` whose [`FieldRole::ItemSlot`] value names one of
    /// `own_names` — [`Self::remove_section`]'s own in-use scan.
    fn referencing_rows(
        &self,
        def_type: &str,
        own_names: &BTreeSet<String>,
    ) -> Vec<(String, RowKey, FieldPath)> {
        let mut referenced_by = Vec::new();
        for (other_type, section) in &self.sections {
            if other_type == def_type {
                continue;
            }
            for (path, spec) in &section.schema.fields {
                let FieldRole::ItemSlot {
                    def_type: item_type,
                } = &spec.role
                else {
                    continue;
                };
                if item_type != def_type {
                    continue;
                }
                for (key, row) in &section.rows {
                    if matches!(row.values.get(path), Some(RowValue::Names(names)) if names.iter().any(|n| own_names.contains(n)))
                    {
                        referenced_by.push((other_type.clone(), key.clone(), path.clone()));
                    }
                }
            }
        }
        referenced_by
    }

    /// Whether `def_name` is already used by another row in this project:
    /// another row in `def_type`'s own section, or a free-standing
    /// ("new def") row in a *different* section.
    fn def_name_taken(&self, def_type: &str, key: &RowKey, def_name: &str) -> bool {
        let in_section = self.sections.get(def_type).is_some_and(|section| {
            section
                .rows
                .iter()
                .any(|(other_key, other_row)| other_key != key && other_row.def_name == def_name)
        });
        in_section
            || self.sections.iter().any(|(other_type, section)| {
                other_type != def_type
                    && section
                        .rows
                        .keys()
                        .any(|k| matches!(k, RowKey::Own(name) if name == def_name))
            })
    }

    /// [`Self::set_row`]'s [`RowKey`]-shape check: a [`RowKey::Target`] key
    /// only fits a target-keyed section (and must itself name a
    /// [`FieldRole::TargetKey`] field), a [`RowKey::Own`] key only fits a
    /// free-standing one — and, since a free-standing row's *only*
    /// identity is its own `defName`, its `RowKey::Own` string must equal
    /// `row.def_name` exactly (see [`AssignmentRowError::OwnKeyDefNameMismatch`]'s
    /// own doc comment for why: a mismatch here would let
    /// [`Section::own_instance_names`] and the rendered def diverge).
    fn validate_row_key(
        def_type: &str,
        section: &Section,
        key: &RowKey,
        row: &AssignmentRow,
    ) -> Result<(), AssignmentRowError> {
        match key {
            RowKey::Target(target) => {
                if section.is_standalone() {
                    return Err(AssignmentRowError::RowKeyMismatch(def_type.to_string()));
                }
                let is_target_key = matches!(
                    section
                        .schema
                        .fields
                        .get(&target.key_field)
                        .map(|s| &s.role),
                    Some(FieldRole::TargetKey { .. })
                );
                if !is_target_key {
                    return Err(AssignmentRowError::NotATargetKey(target.key_field.clone()));
                }
            }
            RowKey::Own(name) => {
                if !section.is_standalone() {
                    return Err(AssignmentRowError::RowKeyMismatch(def_type.to_string()));
                }
                if name != &row.def_name {
                    return Err(AssignmentRowError::OwnKeyDefNameMismatch {
                        key: name.clone(),
                        def_name: row.def_name.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    /// [`Self::set_row`]'s full validation, kept `&self` so the caller can
    /// still borrow `self.sections` mutably for the insert once this
    /// returns `Ok`.
    fn validate_row(
        &self,
        def_type: &str,
        key: &RowKey,
        row: &AssignmentRow,
        known: &dyn KnownDefs,
    ) -> Result<(), AssignmentRowError> {
        let Some(section) = self.sections.get(def_type) else {
            return Err(AssignmentRowError::UnknownSection(def_type.to_string()));
        };
        Self::validate_row_key(def_type, section, key, row)?;
        if row.def_name.trim().is_empty() {
            return Err(AssignmentRowError::EmptyDefName);
        }
        if known.contains(def_type, &row.def_name) {
            return Err(AssignmentRowError::DefNameExistsInActiveList(
                row.def_name.clone(),
            ));
        }
        for (path, value) in &row.values {
            let Some(spec) = section.schema.fields.get(path) else {
                return Err(AssignmentRowError::UnknownField(path.clone()));
            };
            validate_value(path, value, spec, &row.values, known)?;
        }
        if self.def_name_taken(def_type, key, &row.def_name) {
            return Err(AssignmentRowError::DuplicateDefName(row.def_name.clone()));
        }
        Ok(())
    }

    /// Validates and records one section's row, replacing any earlier one
    /// at the same [`RowKey`]. `key` must be [`RowKey::Target`] for a
    /// target-keyed section (naming a [`FieldRole::TargetKey`] field) or
    /// [`RowKey::Own`] for a free-standing one — the other shape is
    /// refused, not coerced.
    ///
    /// # Errors
    ///
    /// Returns [`AssignmentRowError`] (storing nothing) when there is no
    /// section for `def_type`, `key`'s shape doesn't match the section's
    /// own kind, `row.def_name` is empty or already names an active
    /// instance of the section's def type, a value's shape doesn't match
    /// its field's role, a [`Cardinality::Scalar`] [`FieldRole::ItemSlot`]
    /// value doesn't name exactly one item, an [`FieldRole::ItemSlot`]
    /// value names a def neither [`KnownDefs::contains`] nor
    /// [`KnownDefs::own_instances`] knows about, a [`FieldRole::Chances`]
    /// value contains a non-finite number or doesn't match its paired
    /// slot's length, or `row.def_name` collides with another row's (in
    /// this section, or a free-standing
    /// row in another).
    pub fn set_row(
        &mut self,
        def_type: &str,
        key: RowKey,
        row: AssignmentRow,
        known: &dyn KnownDefs,
    ) -> Result<Option<AssignmentRow>, AssignmentRowError> {
        self.validate_row(def_type, &key, &row, known)?;
        let section = self
            .sections
            .get_mut(def_type)
            .unwrap_or_else(|| unreachable!("validate_row above already confirmed it exists"));
        Ok(section.rows.insert(key, row))
    }

    /// Removes the row at `key` in `def_type`'s own section, if one
    /// exists.
    pub fn clear_row(&mut self, def_type: &str, key: &RowKey) -> Option<AssignmentRow> {
        self.sections.get_mut(def_type)?.rows.remove(key)
    }

    /// Reconstitutes a project from its own previously-persisted state,
    /// trusting it completely: a stored row may legitimately reference a
    /// field the schema no longer classifies the way it did when the row
    /// was saved (an R change re-inferred a field, `KnownDefs` isn't
    /// available at load time) — exactly the reasoning
    /// [`crate::domain::patch::PatchProject::from_stored`]'s own doc
    /// comment gives for not replaying [`Self::set_row`] per row.
    #[must_use]
    pub fn from_stored(stored: StoredAssignmentProject) -> Self {
        Self {
            id: stored.id,
            name: stored.name,
            identity: stored.identity,
            author: stored.author,
            description: stored.description,
            refs: stored.refs,
            excluded_refs: stored.excluded_refs,
            targets: stored.targets,
            sections: stored.sections,
            export_dir: stored.export_dir,
            created_at: stored.created_at,
            updated_at: stored.updated_at,
        }
    }

    /// The plain, store-ready shape of this project — the inverse of
    /// [`Self::from_stored`].
    #[must_use]
    pub fn to_stored(&self) -> StoredAssignmentProject {
        StoredAssignmentProject {
            id: self.id.clone(),
            name: self.name.clone(),
            identity: self.identity.clone(),
            author: self.author.clone(),
            description: self.description.clone(),
            refs: self.refs.clone(),
            excluded_refs: self.excluded_refs.clone(),
            targets: self.targets.clone(),
            sections: self.sections.clone(),
            export_dir: self.export_dir.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }

    /// Canonical hash of *what this project would export*: sha256 over the
    /// canonical JSON of `refs`, `excluded_refs`, `targets`, and every
    /// section in `BTreeMap` order (schema plus its rows, flattened to a
    /// `Vec` — the same "hash the wire format directly" discipline
    /// `crates/rim-resolve/CLAUDE.md` documents for
    /// `Action`/`DecisionSet::content_sha256`): changing
    /// [`FieldRole`]/[`RowValue`]/[`RowKey`]/`TargetShape`'s `Serialize`
    /// shape changes this hash for every already-exported assignment mod.
    /// Two exports of the same project are byte-identical because this
    /// hash is.
    #[must_use]
    pub fn content_sha256(&self) -> String {
        #[derive(Serialize)]
        struct RowEntry<'a> {
            key: &'a RowKey,
            row: &'a AssignmentRow,
        }
        #[derive(Serialize)]
        struct SectionEntry<'a> {
            schema: &'a AssignmentSchema,
            rows: Vec<RowEntry<'a>>,
        }
        #[derive(Serialize)]
        struct Snapshot<'a> {
            refs: &'a BTreeSet<ModId>,
            excluded_refs: &'a BTreeSet<ModId>,
            targets: &'a BTreeSet<ModId>,
            sections: BTreeMap<&'a String, SectionEntry<'a>>,
        }

        let sections = self
            .sections
            .iter()
            .map(|(def_type, section)| {
                let rows = section
                    .rows
                    .iter()
                    .map(|(key, row)| RowEntry { key, row })
                    .collect();
                (
                    def_type,
                    SectionEntry {
                        schema: &section.schema,
                        rows,
                    },
                )
            })
            .collect();
        let snapshot = Snapshot {
            refs: &self.refs,
            excluded_refs: &self.excluded_refs,
            targets: &self.targets,
            sections,
        };
        // INVARIANT: every type `Snapshot` embeds derives `Serialize`
        // over plain data reached only through `Vec`/`BTreeMap<String, _>`
        // — never a `RowKey`-keyed map, which `serde_json` cannot key a
        // map by at all — so this can never actually fail. See
        // `DecisionSet::content_sha256`'s own doc comment for why this
        // is `unwrap_or_else(|_| unreachable!(...))` rather than
        // `expect` (denied outside tests by this workspace's lints).
        let json = serde_json::to_string(&snapshot)
            .unwrap_or_else(|_| unreachable!("Snapshot always serializes"));
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let digest = hasher.finalize();
        digest.iter().map(|b| format!("{b:02x}")).collect()
    }
}
