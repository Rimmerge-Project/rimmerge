//! [`find_placement`]: the sorter's placement explanation for one mod id,
//! used by [`crate::Session::explain_placement`]. Pure over the placements.

use std::collections::BTreeMap;

use rim_analyzer::domain::ModId;
use rim_resolve::sort::PlacementExplanation;

/// The sorter's explanation for `id`: the placement keyed by exactly that
/// id first, else the first one sharing its [`ModId::base`]. Placements are
/// keyed by whichever exact id is active, while findings and other callers
/// carry base ids, so the base pass keeps those working; the exact pass
/// keeps a plain and a `_steam` copy of one mod apart.
pub(crate) fn find_placement<'placements>(
    placements: &'placements BTreeMap<ModId, PlacementExplanation>,
    id: &ModId,
) -> Option<&'placements PlacementExplanation> {
    let target = id.base();
    placements.get(id).or_else(|| {
        placements
            .values()
            .find(|explanation| explanation.mod_id.base() == target)
    })
}
