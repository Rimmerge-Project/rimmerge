//! The one rule for a time read from disk or from Steam: when is it a time
//! worth showing. Shared by `mod_timestamps` (a mod's update time) and
//! `saves` (a save file's modified time).

use std::fs::Metadata;
use std::time::UNIX_EPOCH;

/// 9999-12-31T23:59:59Z, the last second a date formatter can show. A
/// larger time is corrupt, not a time.
pub(crate) const MAX_PLAUSIBLE_SECONDS: u64 = 253_402_300_799;

/// A time past the year 9999 is not a Steam or file time: it would make
/// the display's date formatting throw. Zero is no time either.
pub(crate) fn plausible(seconds: u64) -> Option<u64> {
    (1..=MAX_PLAUSIBLE_SECONDS)
        .contains(&seconds)
        .then_some(seconds)
}

/// The plausible modified time of a file, in whole seconds since the Unix
/// epoch; `None` when the platform has none, it predates the epoch, or it
/// is not plausible.
pub(crate) fn modified_seconds(metadata: &Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs())
        .and_then(plausible)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_plausible_second_is_still_a_time() {
        assert_eq!(
            plausible(MAX_PLAUSIBLE_SECONDS),
            Some(MAX_PLAUSIBLE_SECONDS)
        );
        assert_eq!(plausible(MAX_PLAUSIBLE_SECONDS + 1), None);
        assert_eq!(plausible(0), None);
    }
}
