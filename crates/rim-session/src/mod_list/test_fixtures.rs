//! Builders shared by this module's unit tests.

use super::{ListedName, ListedPackageId, SharedModEntry, SharedModList, WorkshopId};

/// An entry for `id` with no name and no Workshop id.
pub(crate) fn entry(id: &str) -> SharedModEntry {
    SharedModEntry {
        id: ListedPackageId::try_from(id).expect("fixture id is valid"),
        name: None,
        workshop_id: None,
    }
}

/// An entry with a name and an optional Workshop id.
pub(crate) fn named_entry(id: &str, name: &str, workshop_id: Option<u64>) -> SharedModEntry {
    SharedModEntry {
        name: ListedName::new(name),
        workshop_id: workshop_id.and_then(WorkshopId::new),
        ..entry(id)
    }
}

/// A list of bare entries, in order.
pub(crate) fn list_of(ids: &[&str]) -> SharedModList {
    SharedModList::new(None, ids.iter().map(|id| entry(id)).collect()).expect("fixture list")
}
