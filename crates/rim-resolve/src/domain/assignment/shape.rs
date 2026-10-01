//! [`TargetShape`]: which defs of a target key's `def_type` are real
//! candidate targets.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

/// A [`TargetShape`]'s required children are the ones present on at
/// least this share of the already-referenced targets — a
/// share, not a strict intersection, since the real install's own
/// intersection over 600 referenced races was empty while `race` alone
/// was present on 599 of them.
pub const SHAPE_MIN: f64 = 0.95;

/// Top-level child tags every def of any type carries, excluded from a
/// learned [`TargetShape::required_children`] — they say nothing about
/// which defs of the type are real targets.
const UNIVERSAL_CHILDREN: [&str; 3] = ["defName", "label", "description"];

/// Which defs of a `FieldRole::TargetKey`'s `def_type` are real
/// candidate targets — `raceNames` resolves to `ThingDef`, but only a
/// `ThingDef` with a `<race>` block is actually a race.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TargetShape {
    /// The target granularity this shape describes.
    pub def_type: String,
    /// Top-level child tags present on at least [`SHAPE_MIN`] of the
    /// already-referenced targets, minus the universal ones (`defName`,
    /// `label`, `description`). User-editable.
    pub required_children: BTreeSet<String>,
}

impl TargetShape {
    /// Learns a shape from the top-level children of every already-
    /// referenced target of `def_type` (one [`BTreeSet`] per target,
    /// already read and extracted by the caller — see
    /// `crate::domain::assignment`'s own module doc comment for why the
    /// read itself isn't done here). Empty input (no instance references
    /// anything yet) yields an empty shape: every def of the type is a
    /// candidate (the "empty framework" case).
    #[must_use]
    pub fn infer(def_type: impl Into<String>, referenced_children: &[BTreeSet<String>]) -> Self {
        let def_type = def_type.into();
        if referenced_children.is_empty() {
            return Self {
                def_type,
                required_children: BTreeSet::new(),
            };
        }

        let total = referenced_children.len();
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for children in referenced_children {
            for tag in children {
                *counts.entry(tag.as_str()).or_default() += 1;
            }
        }

        let required_children = counts
            .into_iter()
            .filter(|(tag, count)| {
                !UNIVERSAL_CHILDREN.contains(tag) && (*count as f64 / total as f64) >= SHAPE_MIN
            })
            .map(|(tag, _)| tag.to_string())
            .collect();

        Self {
            def_type,
            required_children,
        }
    }

    /// Whether a candidate target's own top-level children satisfy every
    /// required tag.
    #[must_use]
    pub fn matches(&self, children: &BTreeSet<String>) -> bool {
        self.required_children
            .iter()
            .all(|tag| children.contains(tag))
    }

    /// Whether this shape imposes no requirement at all — every def of
    /// [`Self::def_type`] is a candidate ("unfiltered").
    #[must_use]
    pub fn is_unfiltered(&self) -> bool {
        self.required_children.is_empty()
    }
}
