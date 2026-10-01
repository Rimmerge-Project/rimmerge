//! [`LoadOrder`]: the active mod list in its current on-disk order, with
//! O(1) position lookup.

use std::collections::HashMap;

use super::mod_id::ModId;

/// The current active load order: an ordered mod list plus a position
/// index for `O(1)` before/after comparisons.
#[derive(Debug, Clone)]
pub struct LoadOrder {
    order: Vec<ModId>,
    position: HashMap<ModId, usize>,
}

impl LoadOrder {
    #[must_use]
    pub fn new(order: Vec<ModId>) -> Self {
        let position = order
            .iter()
            .enumerate()
            .map(|(index, id)| (id.clone(), index))
            .collect();
        Self { order, position }
    }

    /// The zero-based load position of `id`, or `None` if it isn't active.
    #[must_use]
    pub fn position(&self, id: &ModId) -> Option<usize> {
        self.position.get(id).copied()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[ModId] {
        &self.order
    }

    /// Returns `Some(true)` when `a` loads strictly before `b`, `Some(false)`
    /// when it doesn't, or `None` when either mod is not in the order.
    #[must_use]
    pub fn is_before(&self, a: &ModId, b: &ModId) -> Option<bool> {
        Some(self.position(a)? < self.position(b)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order() -> LoadOrder {
        LoadOrder::new(vec![ModId::new("a"), ModId::new("b"), ModId::new("c")])
    }

    #[test]
    fn reports_position_of_active_mods() {
        let order = order();
        assert_eq!(order.position(&ModId::new("b")), Some(1));
    }

    #[test]
    fn reports_no_position_for_inactive_mods() {
        let order = order();
        assert_eq!(order.position(&ModId::new("z")), None);
    }

    #[test]
    fn is_before_compares_positions() {
        let order = order();
        assert_eq!(
            order.is_before(&ModId::new("a"), &ModId::new("c")),
            Some(true)
        );
        assert_eq!(
            order.is_before(&ModId::new("c"), &ModId::new("a")),
            Some(false)
        );
        assert_eq!(order.is_before(&ModId::new("a"), &ModId::new("z")), None);
    }
}
