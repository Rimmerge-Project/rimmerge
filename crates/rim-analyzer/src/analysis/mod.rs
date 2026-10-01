//! Builds edges, detects conflicts, checks the load order, and produces
//! the final [`Report`](crate::domain::Report) from a completed scan.

use crate::domain::{DefTarget, PatchOp};
use crate::extract::xpath_target;

mod checks;
mod conflicts;
/// Public so a downstream crate's test can build a real
/// [`Edge`](crate::domain::Edge) through [`edges::uses_type_edges`] rather
/// than a hand-built literal — the `Edge.subject` field is easiest to verify
/// against the function that actually sets it.
pub mod edges;
mod framework_score;
/// Public alongside [`edges`]: [`indices::Indices`]/[`indices::ActiveMods`]
/// are `edges::uses_type_edges`'s own parameters, so a caller building one
/// needs these reachable too.
pub mod indices;
/// Public for the same reason [`indices`] is:
/// [`inheritance::TemplateRegistrations`] is `edges::parent_template_edges`'s
/// own parameter, so anything calling that producer directly needs this type
/// reachable.
pub mod inheritance;
mod mod_cost;
/// Pure string-similarity rules for `checks::near_miss_mod_references` —
/// normalization, tokenization, and the sequel/fork suppression checks.
/// No dependency on any domain type, so it's straightforward to unit test
/// in isolation from a whole `ScannedMod` fixture.
mod name_similarity;
mod order_check;
/// Public so a downstream crate's test can call
/// [`references::dangling_def_references`] directly against a hand-built
/// scan, the same reason [`edges`] is.
pub mod references;
mod report_builder;
pub mod source_index;
pub mod texture_index;

pub use report_builder::{RunContext, build, build_ref};
pub use source_index::{IndexedPatchOp, SourceIndex};
pub use texture_index::TextureIndex;

/// Every def one patch operation targets, re-parsed from the op's own
/// xpath so a `[defName="A" or defName="B"]` head yields both (RimWorld
/// applies such an op to each named def, so collision detection and
/// replay indexing must see all of them). Falls back to the single
/// [`PatchOp::target`] the scan already resolved when there is no xpath
/// text to re-read; empty when the xpath names no def at all.
pub(crate) fn patch_op_targets(op: &PatchOp) -> Vec<DefTarget> {
    match op.xpath.as_deref() {
        Some(xpath) => {
            let targets = xpath_target::parse_all(xpath);
            if targets.is_empty() {
                op.target.clone().into_iter().collect()
            } else {
                targets
            }
        }
        None => op.target.clone().into_iter().collect(),
    }
}
