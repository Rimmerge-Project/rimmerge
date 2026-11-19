//! "Last updated" facts for the Mods page's display sort: when each mod
//! was last updated, or an explicit reason it cannot be known.
//!
//! Deliberately not part of `rim_analyzer::Report` (whose JSON is a
//! byte-identical contract): a session carries these beside the report,
//! read through [`crate::ports::ModTimestampReader`].

use std::collections::BTreeMap;
use std::path::PathBuf;

use rim_analyzer::domain::{ModId, Report, Source};

/// A point in time as whole seconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnixSeconds(u64);

impl UnixSeconds {
    /// Wraps `seconds` since the Unix epoch.
    #[must_use]
    pub fn new(seconds: u64) -> Self {
        Self(seconds)
    }

    /// The wrapped seconds.
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Why a mod has no update time. Closed, so an interface words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateUnavailable {
    /// Core and the DLCs ship with the game; they have no update time of
    /// their own.
    ShippedWithGame,
    /// The source could not say: no Steam record for a Workshop mod, no
    /// readable `About.xml` for a local one, or no reader configured.
    Unknown,
}

/// When a mod was last updated, or why that is not known. Never a
/// guessed default such as the epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateTime {
    /// The recorded time.
    Known(UnixSeconds),
    /// No time; see [`UpdateUnavailable`].
    Unavailable(UpdateUnavailable),
}

impl UpdateTime {
    /// The time, when known.
    #[must_use]
    pub fn known(self) -> Option<UnixSeconds> {
        match self {
            Self::Known(seconds) => Some(seconds),
            Self::Unavailable(_) => None,
        }
    }
}

/// One mod a [`crate::ports::ModTimestampReader`] is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModTimeSubject {
    /// The mod's exact id, the key of the answer.
    pub id: ModId,
    /// Where its files come from.
    pub source: Source,
    /// Its folder.
    pub path: PathBuf,
    /// Its Steam Workshop id, when it has one.
    pub workshop_id: Option<u64>,
}

impl ModTimeSubject {
    /// Every mod the report knows, scanned and unscanned alike, in the
    /// report's own order.
    #[must_use]
    pub fn all_in(report: &Report) -> Vec<Self> {
        let scanned = report.mods.iter().map(|m| Self {
            id: m.id.clone(),
            source: m.source,
            path: m.path.clone(),
            workshop_id: m.workshop_id,
        });
        let unscanned = report.inactive_mods.iter().map(|m| Self {
            id: m.id.clone(),
            source: m.source,
            path: m.path.clone(),
            workshop_id: m.workshop_id,
        });
        scanned.chain(unscanned).collect()
    }
}

/// A session's update times, by exact mod id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModUpdateTimes(BTreeMap<ModId, UpdateTime>);

impl ModUpdateTimes {
    /// Wraps a reader's answer.
    #[must_use]
    pub fn new(times: BTreeMap<ModId, UpdateTime>) -> Self {
        Self(times)
    }

    /// `id`'s update time; [`UpdateUnavailable::Unknown`] for an id the
    /// reader never answered for (a missing mod, or a session built with
    /// no reader).
    #[must_use]
    pub fn get(&self, id: &ModId) -> UpdateTime {
        self.0
            .get(id)
            .copied()
            .unwrap_or(UpdateTime::Unavailable(UpdateUnavailable::Unknown))
    }
}
