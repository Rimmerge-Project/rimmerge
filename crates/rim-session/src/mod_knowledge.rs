//! [`ModKnowledge`]: the mod-specific knowledge a loaded project carries
//! that is *data*, not code — see `docs/concepts/rules-databases.md`.
//!
//! Four sections live here, and one deliberately does not:
//!
//! - **precedence** (which of several matching assignment instances the
//!   game actually picks, per def type),
//! - **patch-operation behaviours** (which third-party operation class
//!   names map onto which of `rim_merge`'s closed set of modelled
//!   behaviours),
//! - **def-cache carriers** (which def-cache plugin DLL to look for
//!   under a mod's own `Plugins/`, and which `Player.log` prefix it
//!   writes),
//! - **log shapes** (the `Player.log` formats that mods, not the game,
//!   print: a patch-reporting mod's stack-trace block, a texture loader's
//!   fallback lines, a patching library's back-reference stubs),
//!
//! all four read once at [`crate::use_cases::LoadProject`] time through
//! the [`crate::ports::ModKnowledgeStore`] port, as "the fetched cache if
//! present, else the vendored defaults". None of them can change the
//! sorter's output: precedence affects `assign coverage`, the behaviour
//! table affects `verify`/`merge` replay, and the carrier list affects a
//! startup diagnostic and an apply-dialog note, and the log shapes affect
//! how a `Player.log` import segments and reads its entries.
//!
//! - **tag rules are not here, on purpose.** Tags feed cluster/placement
//!   rules, so a tag rule *can* move a mod. Reading one straight out of a
//!   fetched cache would break the guarantee that no sort path touches
//!   the network or the cache, so `rim_io::mod_knowledge` never folds
//!   one into `ModKnowledge` at all — it only parses and validates the
//!   section (a malformed one is still reported), then discards it.
//!   Unlike the community/Steam sources, there is currently **no import
//!   path** from this rimmerge-rules section into a profile's
//!   `rules.json` either — a tag rule that should apply today still has
//!   to be written by hand or inferred, exactly as before this section
//!   existed.
//!
//! A [`ModKnowledge::default`] knows nothing at all: every def type is
//! `PrecedenceRule::Unverified`, every custom operation class falls back
//! to `rim_merge`'s own structural detections, and no def-cache carrier
//! is recognized and no mod-produced log format is known. That is the honest state for a [`crate::Session`] built
//! without a store (most tests), and it is exactly the degradation the
//! vendored defaults exist to avoid for a real run.

use std::collections::BTreeMap;

use rim_merge::patch_behaviours::PatchOperationBehaviours;
use rim_resolve::domain::PrecedenceRule;

use crate::ports::{DefCacheCarrier, LogFormats, LogShapes};

/// See this module's own doc comment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModKnowledge {
    precedence: BTreeMap<String, PrecedenceRule>,
    patch_operations: PatchOperationBehaviours,
    def_cache_carriers: Vec<DefCacheCarrier>,
    log_shapes: LogShapes,
}

impl ModKnowledge {
    /// Builds the knowledge from already-parsed sections.
    #[must_use]
    pub fn new(
        precedence: BTreeMap<String, PrecedenceRule>,
        patch_operations: PatchOperationBehaviours,
        def_cache_carriers: Vec<DefCacheCarrier>,
        log_shapes: LogShapes,
    ) -> Self {
        Self {
            precedence,
            patch_operations,
            def_cache_carriers,
            log_shapes,
        }
    }

    /// `def_type`'s own verified precedence rule, or
    /// [`PrecedenceRule::Unverified`] when there is none — the honest,
    /// documented state for every def type without a verified rule
    /// (`coverage` then lists every match and names no winner).
    #[must_use]
    pub fn precedence_for(&self, def_type: &str) -> PrecedenceRule {
        self.precedence
            .get(def_type)
            .cloned()
            .unwrap_or(PrecedenceRule::Unverified)
    }

    /// Every precedence rule, by def type.
    #[must_use]
    pub fn precedence(&self) -> &BTreeMap<String, PrecedenceRule> {
        &self.precedence
    }

    /// The custom patch-operation class table replay consults — hand this
    /// to `rim_merge::patch_eval::ReplayContext::behaviours`.
    #[must_use]
    pub fn patch_operations(&self) -> &PatchOperationBehaviours {
        &self.patch_operations
    }

    /// Every recognized def-cache plugin.
    #[must_use]
    pub fn def_cache_carriers(&self) -> &[DefCacheCarrier] {
        &self.def_cache_carriers
    }

    /// Every recognized mod-produced log format.
    #[must_use]
    pub fn log_shapes(&self) -> &LogShapes {
        &self.log_shapes
    }

    /// The formats a `Player.log` read recognises: the def-cache
    /// carriers' line prefixes and the log shapes.
    #[must_use]
    pub fn log_formats(&self) -> LogFormats<'_> {
        LogFormats {
            def_cache_carriers: &self.def_cache_carriers,
            shapes: &self.log_shapes,
        }
    }
}

#[cfg(test)]
mod tests {
    use rim_analyzer::domain::ModId;
    use rim_resolve::domain::{FieldPath, PathSegment};

    use super::*;

    #[test]
    fn an_empty_knowledge_reports_every_def_type_unverified() {
        let knowledge = ModKnowledge::default();
        assert_eq!(
            knowledge.precedence_for("example.PartAssignmentDef"),
            PrecedenceRule::Unverified
        );
        assert!(knowledge.patch_operations().is_empty());
        assert!(knowledge.def_cache_carriers().is_empty());
        assert!(knowledge.log_shapes().is_empty());
    }

    #[test]
    fn a_loaded_rule_is_returned_for_its_own_def_type_only() {
        let rule = PrecedenceRule::PreferOutsideFramework {
            framework: ModId::new("example.framework"),
            key_priority: vec![FieldPath::new(vec![PathSegment::Child(
                "kindNames".to_string(),
            )])],
        };
        let knowledge = ModKnowledge::new(
            BTreeMap::from([("example.PartAssignmentDef".to_string(), rule.clone())]),
            PatchOperationBehaviours::default(),
            Vec::new(),
            LogShapes::default(),
        );

        assert_eq!(knowledge.precedence_for("example.PartAssignmentDef"), rule);
        assert_eq!(
            knowledge.precedence_for("example.PartDef"),
            PrecedenceRule::Unverified
        );
    }
}
