//! Real disturbance metrics between the current order and the suggested
//! one — replacing a bare "N mods moved" count, which conflates "shifted
//! by one slot because something else was inserted ahead of it" with
//! "genuinely reordered relative to other mods".

use std::collections::BTreeMap;

use rim_analyzer::domain::{LoadOrder, ModId};

/// How far the suggested order strays from the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DisturbanceStats {
    /// The number of mod pairs whose relative order flipped between the
    /// current order and the suggested one — the Kendall tau distance.
    /// Zero means the suggested order is consistent with (a superset-safe
    /// refinement of) the current one; large means substantial reshuffling.
    pub kendall_tau_inversions: u64,
    /// Mods whose position moved by more than 10 slots.
    pub mods_displaced_over_10: usize,
    /// Mods whose position moved by more than 50 slots.
    pub mods_displaced_over_50: usize,
    /// The single largest displacement, in slots.
    pub max_displacement: usize,
    /// Mods whose position changed at all (including a mod absent from
    /// the current order, which trivially "changed").
    pub positions_changed: usize,
}

/// Counts inversions in `sequence` (the number of pairs `i < j` with
/// `sequence[i] > sequence[j]`) in `O(n log n)` via merge-sort.
fn count_inversions(sequence: &[usize]) -> u64 {
    let mut buffer = sequence.to_vec();
    let len = buffer.len();
    let mut scratch = vec![0usize; len];
    merge_count(&mut buffer, &mut scratch, 0, len)
}

fn merge_count(buffer: &mut [usize], scratch: &mut [usize], start: usize, end: usize) -> u64 {
    if end - start < 2 {
        return 0;
    }
    let mid = start + (end - start) / 2;
    let mut inversions = merge_count(buffer, scratch, start, mid);
    inversions += merge_count(buffer, scratch, mid, end);

    let (mut left, mut right, mut out) = (start, mid, start);
    while left < mid && right < end {
        if buffer[left] <= buffer[right] {
            scratch[out] = buffer[left];
            left += 1;
        } else {
            // Every remaining element in the left half is greater than
            // `buffer[right]` (both halves are already sorted).
            inversions += (mid - left) as u64;
            scratch[out] = buffer[right];
            right += 1;
        }
        out += 1;
    }
    while left < mid {
        scratch[out] = buffer[left];
        left += 1;
        out += 1;
    }
    while right < end {
        scratch[out] = buffer[right];
        right += 1;
        out += 1;
    }
    buffer[start..end].copy_from_slice(&scratch[start..end]);

    inversions
}

/// Computes [`DisturbanceStats`] for `order` against `current`.
#[must_use]
pub(super) fn compute(order: &LoadOrder, current: &LoadOrder) -> DisturbanceStats {
    let current_rank: BTreeMap<ModId, usize> = current
        .as_slice()
        .iter()
        .enumerate()
        .map(|(index, id)| (id.clone(), index))
        .collect();

    // The current-order rank of every mod `order` places, taken in
    // `order`'s own sequence: counting inversions in this sequence is
    // exactly counting pairs whose relative order flipped. Mods absent
    // from `current` contribute no pairs (nothing to compare their
    // relative order against) and are excluded here.
    let sequence: Vec<usize> = order
        .as_slice()
        .iter()
        .filter_map(|id| current_rank.get(id).copied())
        .collect();
    let kendall_tau_inversions = count_inversions(&sequence);

    let mut mods_displaced_over_10 = 0;
    let mut mods_displaced_over_50 = 0;
    let mut max_displacement = 0;
    let mut positions_changed = 0;
    for (new_position, id) in order.as_slice().iter().enumerate() {
        match current.position(id) {
            Some(old_position) => {
                let displacement = old_position.abs_diff(new_position);
                if displacement > 0 {
                    positions_changed += 1;
                }
                if displacement > 10 {
                    mods_displaced_over_10 += 1;
                }
                if displacement > 50 {
                    mods_displaced_over_50 += 1;
                }
                max_displacement = max_displacement.max(displacement);
            }
            None => positions_changed += 1,
        }
    }

    DisturbanceStats {
        kendall_tau_inversions,
        mods_displaced_over_10,
        mods_displaced_over_50,
        max_displacement,
        positions_changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(ids: &[&str]) -> LoadOrder {
        LoadOrder::new(ids.iter().map(|id| ModId::new(*id)).collect())
    }

    #[test]
    fn count_inversions_is_zero_for_an_already_sorted_sequence() {
        assert_eq!(count_inversions(&[0, 1, 2, 3, 4]), 0);
    }

    #[test]
    fn count_inversions_counts_every_flipped_pair() {
        // Every pair among [3,2,1,0] is inverted: C(4,2) = 6.
        assert_eq!(count_inversions(&[3, 2, 1, 0]), 6);
    }

    #[test]
    fn count_inversions_handles_a_single_swap() {
        assert_eq!(count_inversions(&[0, 2, 1, 3]), 1);
    }

    #[test]
    fn identical_orders_have_zero_disturbance() {
        let current = order(&["a", "b", "c"]);
        let stats = compute(&current, &current);
        assert_eq!(stats.kendall_tau_inversions, 0);
        assert_eq!(stats.positions_changed, 0);
        assert_eq!(stats.max_displacement, 0);
    }

    #[test]
    fn a_full_reversal_maximizes_inversions() {
        let current = order(&["a", "b", "c", "d"]);
        let suggested = order(&["d", "c", "b", "a"]);
        let stats = compute(&suggested, &current);
        assert_eq!(stats.kendall_tau_inversions, 6); // C(4,2)
        assert_eq!(stats.positions_changed, 4);
        assert_eq!(stats.max_displacement, 3);
    }

    #[test]
    fn a_single_mod_moved_to_the_front_creates_few_inversions() {
        // Moving one mod from the back to the front is a small,
        // structural change (e.g. a real dependency pulling a
        // prerequisite forward): it inverts its relation with everything
        // it jumped, not with everything else.
        let current = order(&["a", "b", "c", "d", "e"]);
        let suggested = order(&["e", "a", "b", "c", "d"]);
        let stats = compute(&suggested, &current);
        assert_eq!(stats.kendall_tau_inversions, 4);
    }

    #[test]
    fn a_mod_absent_from_current_does_not_inflate_inversions() {
        let current = order(&["a", "b", "c"]);
        let suggested = order(&["new", "a", "b", "c"]);
        let stats = compute(&suggested, &current);
        // `a`, `b`, and `c` keep their relative order — inserting `new`
        // ahead of them shifts every one of their raw positions by one
        // (so `positions_changed` is 4, the whole point `kendall_tau_inversions`
        // exists to see past), but flips no pair's relative order.
        assert_eq!(stats.kendall_tau_inversions, 0);
        assert_eq!(stats.positions_changed, 4);
    }
}
