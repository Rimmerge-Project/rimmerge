//! The live baseline the Mods page measures pending changes against: the
//! active-mod list the on-disk `ModsConfig.xml` last held, plus whether the
//! latest attempt to read it worked.
//!
//! The baseline is the *live* file. What Rimmerge itself last loaded or wrote
//! is kept apart (`FileWatch`), so a fact about the user's own unsaved work
//! is measured against that, never against an outside edit.
//!
//! Everything here is pure. [`crate::use_cases::RefreshFileBaseline`] does
//! the read through the port; `Session::observe_file` swaps the
//! result in. Nothing in this module writes the file.

use std::collections::BTreeSet;
use std::path::PathBuf;

use rim_analyzer::domain::ModId;

use crate::ports::ConfigErrorKind;

/// Why the latest read of the file failed. The last valid list stays the
/// baseline meanwhile; a failure never reads as "no mods".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineIssue {
    /// The file does not exist.
    Missing,
    /// The file could not be opened or read.
    Unreadable,
    /// The file is not a usable `ModsConfig.xml`: truncated or partly
    /// written, the wrong shape, or over the size bound.
    Malformed,
}

impl From<ConfigErrorKind> for BaselineIssue {
    fn from(kind: ConfigErrorKind) -> Self {
        match kind {
            ConfigErrorKind::Missing => Self::Missing,
            ConfigErrorKind::Unreadable => Self::Unreadable,
            ConfigErrorKind::Malformed => Self::Malformed,
        }
    }
}

/// Whether the baseline is what the file holds now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileBaseline {
    /// The latest read succeeded (or none has failed since the load).
    Fresh,
    /// The latest read failed; the baseline is the last list that read
    /// fine.
    Stale {
        /// What went wrong.
        reason: BaselineIssue,
    },
}

/// How a pending change relates to the file's active list, by base id
/// (`X` and `X_steam` are the same mod). A reorder alone is never a mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MembershipMark {
    /// Same membership as the file.
    None,
    /// Active here, not in the file: applying would add it.
    Added,
    /// In the file, not active here: applying would remove it.
    Removed,
}

/// The base ids the file's active list holds, for marking list rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct FileMembership {
    bases: BTreeSet<ModId>,
    /// The base ids in file order, for telling an outside reorder.
    sequence: Vec<ModId>,
}

impl FileMembership {
    pub(crate) fn of(file_ids: &[ModId]) -> Self {
        let sequence: Vec<ModId> = file_ids.iter().map(ModId::base).collect();
        Self {
            bases: sequence.iter().cloned().collect(),
            sequence,
        }
    }

    /// The mark for a row of the Active list.
    pub(crate) fn mark_active(&self, id: &ModId) -> MembershipMark {
        if self.bases.contains(&id.base()) {
            MembershipMark::None
        } else {
            MembershipMark::Added
        }
    }

    /// The mark for a row of the Inactive list.
    pub(crate) fn mark_inactive(&self, id: &ModId) -> MembershipMark {
        if self.bases.contains(&id.base()) {
            MembershipMark::Removed
        } else {
            MembershipMark::None
        }
    }

    /// How the file's membership differs from `known`'s.
    pub(crate) fn change_from(&self, known: &Self) -> ExternalFileChange {
        let added = self.bases.difference(&known.bases).count();
        let removed = known.bases.difference(&self.bases).count();
        if added > 0 || removed > 0 {
            return ExternalFileChange::Membership { added, removed };
        }
        if self.sequence != known.sequence {
            return ExternalFileChange::Reordered;
        }
        ExternalFileChange::None
    }
}

/// What someone else did to the file since Rimmerge last loaded or wrote
/// it. A membership change already says the file was edited, so a reorder
/// is only reported when the membership is the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalFileChange {
    /// The file is what Rimmerge last loaded or wrote (by base id).
    None,
    /// Mods the file lists now that it did not, and mods it no longer
    /// lists, counted by base id. At least one count is not zero.
    Membership {
        /// Mods the file lists now that it did not.
        added: usize,
        /// Mods the file no longer lists.
        removed: usize,
    },
    /// The same mods (by base id) in another sequence.
    Reordered,
}

/// Whether `Session::observe_file` changed anything a reader of
/// the session could see. The desktop emits its change event only for
/// [`BaselineChange::Changed`], so polling does not invalidate every query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineChange {
    /// The list and the status are what they were (or the observation was
    /// out of date and discarded).
    Unchanged,
    /// The list or the status differs now.
    Changed,
}

/// What to read, taken from a session under its lock: the file's path and
/// the session's file epoch. The epoch lets the session discard a read
/// that an `Apply` overtook while it ran outside the lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileProbe {
    pub(crate) path: PathBuf,
    pub(crate) epoch: u64,
}

impl FileProbe {
    /// The file to read.
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

/// What a session tracks about the file besides its list: what Rimmerge
/// itself last loaded or wrote (to tell an outside change from its own),
/// whether the latest read worked, and a counter bumped on every load or
/// write of Rimmerge's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileWatch {
    acknowledged: Vec<ModId>,
    baseline: FileBaseline,
    epoch: u64,
}

impl FileWatch {
    /// The state right after a load: the file is what Rimmerge read.
    pub(crate) fn loaded(file_ids: &[ModId]) -> Self {
        Self {
            acknowledged: file_ids.to_vec(),
            baseline: FileBaseline::Fresh,
            epoch: 0,
        }
    }

    /// Rimmerge itself just loaded or wrote `file_ids`: nothing about it is
    /// an outside change, and any read still in flight is out of date.
    pub(crate) fn reset(&mut self, file_ids: &[ModId]) {
        self.acknowledged = file_ids.to_vec();
        self.baseline = FileBaseline::Fresh;
        self.epoch += 1;
    }

    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(crate) fn baseline(&self) -> FileBaseline {
        self.baseline
    }

    pub(crate) fn set_baseline(&mut self, baseline: FileBaseline) {
        self.baseline = baseline;
    }

    /// The exact ids, in file order, Rimmerge last loaded or wrote: the
    /// anchor every "your own unsaved work" fact compares against.
    pub(crate) fn acknowledged(&self) -> &[ModId] {
        &self.acknowledged
    }

    pub(crate) fn change_to(&self, file_ids: &[ModId]) -> ExternalFileChange {
        FileMembership::of(file_ids).change_from(&FileMembership::of(&self.acknowledged))
    }
}
