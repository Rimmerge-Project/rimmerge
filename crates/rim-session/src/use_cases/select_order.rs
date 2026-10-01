//! [`SelectOrder`]: changes which order (current/suggested) a session's
//! ledger and finding queries read from. Pure — no ports.

use rim_resolve::domain::OrderSource;

use crate::Session;

/// Changes a session's selected [`OrderSource`].
#[derive(Debug, Default, Clone, Copy)]
pub struct SelectOrder;

impl SelectOrder {
    /// Builds the use case. Takes no ports: selecting an order is a pure
    /// in-memory change, nothing to persist.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Selects `source`.
    pub fn execute(self, session: &mut Session, source: OrderSource) {
        session.select(source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::session_fixture;

    #[test]
    fn selects_the_given_source() {
        let mut session = session_fixture(&["a"]);

        SelectOrder::new().execute(&mut session, OrderSource::Suggested);

        assert_eq!(session.selected(), OrderSource::Suggested);
    }
}
