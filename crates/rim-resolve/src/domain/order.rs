//! [`OrderSource`] and [`BothOrders`]: which load order a ledger or sort
//! result was built against, and the pair every session keeps at once.

use rim_analyzer::domain::LoadOrder;
use serde::{Deserialize, Serialize};

/// Which load order a ledger, suggestion, or sort result refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderSource {
    /// The order currently active in `ModsConfig.xml`.
    Current,
    /// The order the sorter proposes.
    Suggested,
}

/// The current and suggested load orders, kept side by side so a ledger
/// can be built for either without re-running the sorter.
#[derive(Debug, Clone)]
pub struct BothOrders {
    /// The order currently active in `ModsConfig.xml`.
    pub current: LoadOrder,
    /// The order the sorter proposes.
    pub suggested: LoadOrder,
}

impl BothOrders {
    /// Returns the order named by `source`.
    #[must_use]
    pub fn get(&self, source: OrderSource) -> &LoadOrder {
        match source {
            OrderSource::Current => &self.current,
            OrderSource::Suggested => &self.suggested,
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;

    use super::*;

    fn order(ids: &[&str]) -> LoadOrder {
        LoadOrder::new(ids.iter().map(|id| ModId::new(*id)).collect())
    }

    #[test]
    fn get_returns_the_order_named_by_source() {
        let both = BothOrders {
            current: order(&["a", "b"]),
            suggested: order(&["b", "a"]),
        };

        assert_eq!(
            both.get(OrderSource::Current).as_slice(),
            [ModId::new("a"), ModId::new("b")]
        );
        assert_eq!(
            both.get(OrderSource::Suggested).as_slice(),
            [ModId::new("b"), ModId::new("a")]
        );
    }
}
