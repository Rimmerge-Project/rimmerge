//! The mod update-time port.

use std::collections::BTreeMap;
use std::path::Path;

use rim_analyzer::domain::ModId;

use crate::mod_times::{ModTimeSubject, UpdateTime};

/// Reads when each mod was last updated. Sync, like every other port.
///
/// Infallible by design: a source that cannot be read answers
/// [`UpdateTime::Unavailable`] for the mods it concerns, so the Mods page
/// can always sort.
pub trait ModTimestampReader {
    /// An answer for every subject. `workshop_dir` is the Workshop content
    /// folder, from which an adapter finds Steam's own record.
    fn read_update_times(
        &self,
        workshop_dir: &Path,
        subjects: &[ModTimeSubject],
    ) -> BTreeMap<ModId, UpdateTime>;
}

/// Forwards to `T`'s own impl, so a composition root can hold the reader
/// behind an `Arc` (swappable for a fake in tests).
impl<T: ModTimestampReader + ?Sized> ModTimestampReader for std::sync::Arc<T> {
    fn read_update_times(
        &self,
        workshop_dir: &Path,
        subjects: &[ModTimeSubject],
    ) -> BTreeMap<ModId, UpdateTime> {
        (**self).read_update_times(workshop_dir, subjects)
    }
}
