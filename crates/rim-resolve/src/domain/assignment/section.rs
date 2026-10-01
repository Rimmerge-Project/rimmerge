//! [`Section`]/[`RowKey`]: one def-type slice of an
//! [`super::project::AssignmentProject`]. A project carries one
//! [`Section`] per def type it assigns into, each independently
//! target-keyed or free-standing depending on its own
//! [`AssignmentSchema`] — a single free-standing ("new def") project is
//! just a project with exactly one free-standing section.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::project::{AssignmentRow, TargetRef};
use super::schema::AssignmentSchema;

/// Addresses one row within a [`Section`]: a target-keyed section's rows
/// are addressed by the target def they were matched through, a
/// free-standing section's by their own `defName`.
///
/// **File-format rule** (`crates/rim-resolve/CLAUDE.md`): this type never
/// serializes as a `serde_json` map *key* (a struct-shaped variant can't
/// — the reason [`Section::rows`] itself carries no `Serialize`/
/// `Deserialize` derive of its own; a store flattens it to a `Vec` the
/// same way it always flattened [`TargetRef`]-keyed rows). It does derive
/// `Serialize`/`Deserialize` as an ordinary value (a `Vec` element, a
/// struct field) — changing its shape is still a file-format change
/// requiring a store version bump and a migration.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RowKey {
    /// A target-keyed row, matched through one target def.
    Target(TargetRef),
    /// A free-standing ("new def") row, by its own `defName`.
    Own(String),
}

/// One def-type slice of an [`super::project::AssignmentProject`]: its own
/// confirmed schema plus the rows addressed against it.
///
/// Target-keyed when [`AssignmentSchema::has_target_key`] is `true` (every
/// row is [`RowKey::Target`]); free-standing otherwise (every row is
/// [`RowKey::Own`]). [`AssignmentProject::set_row`](super::project::AssignmentProject::set_row)
/// is the only way rows are added, and it refuses a [`RowKey`] variant
/// that doesn't match this section's own kind — the two shapes never mix
/// within one section.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    /// The confirmed schema this section's rows are validated against.
    pub schema: AssignmentSchema,
    /// Every row on file, by the target (or own `defName`) it addresses.
    pub rows: BTreeMap<RowKey, AssignmentRow>,
}

impl Section {
    /// Builds a new, empty (no rows) section from a confirmed schema.
    #[must_use]
    pub fn new(schema: AssignmentSchema) -> Self {
        Self {
            schema,
            rows: BTreeMap::new(),
        }
    }

    /// Whether this section has no [`super::schema::FieldRole::TargetKey`]
    /// field at all — a "new def" section, whose rows are free-standing
    /// instances ([`RowKey::Own`]) rather than target-addressed.
    #[must_use]
    pub fn is_standalone(&self) -> bool {
        !self.schema.has_target_key()
    }

    /// Every free-standing row's own `defName` — what
    /// [`super::project::KnownDefs::own_instances`] for this section's
    /// def type returns, and what
    /// [`super::project::AssignmentProject::remove_section`] checks other
    /// sections' item slots against before removing this one.
    #[must_use]
    pub fn own_instance_names(&self) -> BTreeSet<String> {
        self.rows
            .keys()
            .filter_map(|key| match key {
                RowKey::Own(name) => Some(name.clone()),
                RowKey::Target(_) => None,
            })
            .collect()
    }
}
