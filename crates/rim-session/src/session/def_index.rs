//! [`Session::def_exists`]: the scan's own def index, answering a
//! `PatchOperationConditional`/`Test` whose xpath is a bare existence test
//! on another def.

use std::collections::BTreeSet;

use rim_analyzer::analysis::SourceIndex;

use super::Session;

/// Every concrete def key the scan indexed, plus every def *type* it saw —
/// see [`DefIndex::contains`] for why both halves are needed.
#[derive(Debug, Default)]
pub(super) struct DefIndex {
    keys: BTreeSet<(String, String)>,
    types: BTreeSet<String>,
}

impl DefIndex {
    fn build(sources: &SourceIndex) -> Self {
        let mut keys = BTreeSet::new();
        let mut types = BTreeSet::new();
        for (_, (def_type, def_name)) in sources.defs.keys() {
            types.insert(def_type.clone());
            keys.insert((def_type.clone(), def_name.clone()));
        }
        Self { keys, types }
    }

    fn contains(&self, def_type: &str, def_name: &str) -> Option<bool> {
        if self
            .keys
            .contains(&(def_type.to_string(), def_name.to_string()))
        {
            return Some(true);
        }
        // "Absent" is only a real answer for a def type the scan actually
        // indexed: one it never saw at all could be registered from C# at
        // runtime, so the honest answer there is "unknown" (which keeps
        // the op `Unsupported`).
        self.types.contains(def_type).then_some(false)
    }
}

impl Session {
    /// Whether the scan indexed a concrete def `def_type`/`def_name`:
    /// `Some(true)`/`Some(false)` when the scan can answer for certain,
    /// `None` when `def_type` itself was never indexed at all.
    ///
    /// The index is built on the first call and kept for the session's
    /// lifetime: `sources` never changes after construction, most sessions
    /// never ask (only a replay reaching an existence test does), and a
    /// pass that does ask — `VerifyOrder` replays thousands of defs —
    /// would otherwise fold the same ~200k keys into a fresh set per def.
    pub(crate) fn def_exists(&self, def_type: &str, def_name: &str) -> Option<bool> {
        self.def_index
            .get_or_init(|| DefIndex::build(&self.sources))
            .contains(def_type, def_name)
    }
}
