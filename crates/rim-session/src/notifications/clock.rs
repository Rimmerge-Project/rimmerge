//! Elapsed-time arithmetic over timestamps the app stored earlier,
//! tolerant of a wall clock that has since moved backward.

/// Whole seconds from `earlier` to `now`, or `None` when `earlier` lies
/// *after* `now`: the clock was set back since the timestamp was stored
/// (a timezone or RTC fix, a restored profile). Callers decide what a
/// cadence or grace check means then; every one of them treats it as
/// "enough time has passed", because a future timestamp read as "too
/// recent" would switch an automatic check off until the clock catches up
/// with it, which for a badly wrong clock is never.
pub(crate) fn seconds_since(now: jiff::Timestamp, earlier: jiff::Timestamp) -> Option<i64> {
    let elapsed = now.as_second() - earlier.as_second();
    (elapsed >= 0).then_some(elapsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_past_timestamp_reports_the_elapsed_seconds() {
        let earlier = jiff::Timestamp::UNIX_EPOCH;
        let now = earlier + jiff::Span::new().hours(1);

        assert_eq!(seconds_since(now, earlier), Some(3600));
        assert_eq!(seconds_since(now, now), Some(0));
    }

    #[test]
    fn a_future_timestamp_reports_no_elapsed_time() {
        let now = jiff::Timestamp::UNIX_EPOCH;
        let later = now + jiff::Span::new().seconds(1);

        assert_eq!(seconds_since(now, later), None);
    }
}
