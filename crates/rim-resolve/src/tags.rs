//! Tag inference: turning per-mod scan evidence into a
//! [`Tagging`](crate::domain::Tagging), plus the shipped default rules.

mod evidence;
pub(crate) mod infer;

pub use evidence::{collect_evidence, evidence_from_report};
pub use infer::infer_tags;
