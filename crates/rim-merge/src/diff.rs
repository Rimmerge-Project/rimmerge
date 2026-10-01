//! Three-way, per-field diff of a contested def's owners.

mod collisions;
mod field_diff;
mod structural;
mod values;

pub use collisions::collision_fields;
pub use field_diff::{
    DiffClass, EntryKind, FieldDiff, OwnerVersion, ThreeWayDiff, Value, three_way,
};
pub use structural::{BaseNotAnOwner, StructuralChange, StructuralField, structural_change};
pub(crate) use values::{field_value, is_confirmed_keyed_map, is_keyed_map_at};

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "diff/diff_tests.rs"]
mod tests;
