//! [`PatchOperationBehaviours`]: the *data* half of custom patch-operation
//! replay — which third-party operation class names map onto which of
//! this crate's closed set of modelled behaviours, and which class-name
//! prefixes carry a mod-gate the engine evaluates before the operation's
//! own worker runs.
//!
//! **Why this is data and not a table in [`crate::patch_eval`]**: a
//! class name like
//! `Some.Framework.PatchOperationX` is knowledge about one specific
//! third-party mod, and mod-specific knowledge lives in the rules repo,
//! not in this workspace's source. What stays in code is the *behaviour*
//! — [`CustomBehaviour`] and [`GateBehaviour`] are closed enums, each
//! with its own handler in [`crate::patch_eval`], so a data file can
//! only ever point an unknown class at a behaviour this binary already
//! implements. A `behaviour` string this binary does not know is
//! **ignored with a warning**, never an error: a newer data file must
//! never break an older binary.
//!
//! Equally deliberately, the *structural* detections stay in code and
//! stay name-free — an `<operations>` child, a `<match>`/`<nomatch>`/
//! `<operation>` toggle with no `<xpath>` of its own, and the
//! `PatchOperation*` suffixes of the engine's own classes. Those are
//! facts about RimWorld and about XML shape, not about any one mod, and
//! they are what actually cover the long tail of sequence-shaped and
//! mod-setting-gated custom classes (see
//! [`crate::patch_eval`]'s own dispatch).
//!
//! This crate is serde-free on purpose (`crates/rim-merge/CLAUDE.md`), so
//! the JSON wire format lives with the adapter that reads files —
//! `rim_io::mod_knowledge` — and hands the parsed result here through
//! [`PatchOperationBehaviours::new`]. [`CustomBehaviour::from_wire`]/
//! [`GateBehaviour::from_wire`] are the one piece of the wire vocabulary
//! that must stay beside the enums themselves, so adding a variant
//! cannot silently leave the string spelling behind.

use std::collections::BTreeMap;

/// How a data row's `class` is matched against an operation's own
/// `Class` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClassMatch {
    /// The operation's `Class` *ends with* the row's `class` — the same
    /// match every engine class in [`crate::patch_eval`] uses, so a
    /// namespace-qualified name still matches.
    Suffix,
    /// The operation's `Class` *starts with* the row's `class` — for a
    /// whole namespace sharing one gate.
    Prefix,
}

impl ClassMatch {
    /// Whether `class` matches `pattern` under this mode.
    #[must_use]
    pub fn matches(self, class: &str, pattern: &str) -> bool {
        match self {
            Self::Suffix => class.ends_with(pattern),
            Self::Prefix => class.starts_with(pattern),
        }
    }

    /// The wire spelling (`"suffix"`/`"prefix"`), or `None`.
    #[must_use]
    pub fn from_wire(text: &str) -> Option<Self> {
        match text {
            "suffix" => Some(Self::Suffix),
            "prefix" => Some(Self::Prefix),
            _ => None,
        }
    }
}

/// One custom operation behaviour this crate models exactly. Closed: a
/// data row may only ever select one of these, and each has its own
/// handler in [`crate::patch_eval`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CustomBehaviour {
    /// Each `<value>` `li` replaces the `modExtensions` item carrying the
    /// same `Class`, or is appended when there is none.
    SetModExtension,
    /// The `<xpath>` names a *container*; each `<value>` child replaces
    /// the container's existing child of the same tag, or is appended.
    AddOrReplace,
    /// Sets `researchViewX`/`researchViewY` on every matched node,
    /// appending either child when absent.
    ReplaceResearchCoords,
}

impl CustomBehaviour {
    /// The wire spelling, or `None` for a behaviour this binary does not
    /// implement (the caller warns and ignores the row).
    #[must_use]
    pub fn from_wire(text: &str) -> Option<Self> {
        match text {
            "set_mod_extension" => Some(Self::SetModExtension),
            "add_or_replace" => Some(Self::AddOrReplace),
            "replace_research_coords" => Some(Self::ReplaceResearchCoords),
            _ => None,
        }
    }
}

/// One gate behaviour this crate models exactly — a check a framework
/// runs *before* its own operation's worker, whose `true` means "skipped
/// outright", the same "no-op, not a failure" outcome an unmet
/// `MayRequire` gate has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GateBehaviour {
    /// A gate reading "every one of these mods is loaded" /
    /// "none of these mods is loaded" out of named child elements.
    ModsLoadedGate,
}

impl GateBehaviour {
    /// The wire spelling, or `None` for a behaviour this binary does not
    /// implement.
    #[must_use]
    pub fn from_wire(text: &str) -> Option<Self> {
        match text {
            "mods_loaded_gate" => Some(Self::ModsLoadedGate),
            _ => None,
        }
    }
}

/// Which way a [`GateBehaviour::ModsLoadedGate`]'s `conditionalType`
/// reads its named mods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConditionalKind {
    /// The operation runs only when *every* named mod is active.
    AllLoaded,
    /// The operation runs only when *no* named mod is active.
    NoneLoaded,
}

impl ConditionalKind {
    /// The wire spelling, or `None`.
    #[must_use]
    pub fn from_wire(text: &str) -> Option<Self> {
        match text {
            "all_loaded" => Some(Self::AllLoaded),
            "none_loaded" => Some(Self::NoneLoaded),
            _ => None,
        }
    }
}

/// Which child element of the operation carries each of a
/// [`GateBehaviour::ModsLoadedGate`]'s three inputs. Element names are
/// data because they are the framework's own vocabulary, not RimWorld's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateFields {
    /// Child element naming a comma-separated mod list that must *all*
    /// be active, or the operation is skipped.
    pub requires_all: String,
    /// Child element naming which conditional test to apply.
    pub conditional_type: String,
    /// Child element naming that test's comma-separated mod list.
    pub conditional_param: String,
}

/// One class-name pattern carrying a gate, with the element names and
/// conditional vocabulary that gate reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassGate {
    /// The class-name pattern.
    pub class: String,
    /// How [`Self::class`] is matched.
    pub match_kind: ClassMatch,
    /// Which modelled gate this is.
    pub behaviour: GateBehaviour,
    /// Where the gate's inputs live on the operation element.
    pub fields: GateFields,
    /// The framework's own `conditionalType` values, mapped onto the
    /// modelled [`ConditionalKind`]s. Matched by suffix, so a
    /// namespace-qualified value still resolves. A value absent from this
    /// map leaves the operation `Unsupported` rather than guessing — the
    /// gate might read the user's own mod settings, which a replay cannot
    /// see.
    pub conditional_types: BTreeMap<String, ConditionalKind>,
}

/// One class-name pattern mapped onto a [`CustomBehaviour`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassBehaviour {
    /// How [`PatchOperationBehaviours::classes`]' key is matched.
    pub match_kind: ClassMatch,
    /// Which modelled behaviour this class has.
    pub behaviour: CustomBehaviour,
}

/// Every custom class and gate this replay currently knows, as loaded
/// from data. [`Self::none`] is the empty set — every custom class then
/// falls back to the structural detections, and anything those don't
/// recognize stays `Unsupported`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PatchOperationBehaviours {
    /// Class pattern -> behaviour. A `BTreeMap` so lookup order (and so
    /// the winner when two patterns both match) is deterministic — the
    /// workspace's own determinism contract.
    classes: BTreeMap<String, ClassBehaviour>,
    /// Gates, in data order.
    gates: Vec<ClassGate>,
}

/// The empty behaviour set — see [`PatchOperationBehaviours::none`].
static NONE: PatchOperationBehaviours = PatchOperationBehaviours {
    classes: BTreeMap::new(),
    gates: Vec::new(),
};

impl PatchOperationBehaviours {
    /// Builds the set from already-parsed rows.
    #[must_use]
    pub fn new(classes: BTreeMap<String, ClassBehaviour>, gates: Vec<ClassGate>) -> Self {
        Self { classes, gates }
    }

    /// The empty set: no custom class is known by name. Borrowed from a
    /// `static` so a caller with no data in hand (a unit test, or a
    /// [`crate::patch_eval::ReplayContext`] built before any project is
    /// loaded) can point at it without allocating.
    #[must_use]
    pub fn none() -> &'static Self {
        &NONE
    }

    /// Whether this set knows no class at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.classes.is_empty() && self.gates.is_empty()
    }

    /// The behaviour `class` maps onto, if any.
    #[must_use]
    pub fn behaviour_for(&self, class: &str) -> Option<CustomBehaviour> {
        self.classes
            .iter()
            .find(|(pattern, row)| row.match_kind.matches(class, pattern))
            .map(|(_, row)| row.behaviour)
    }

    /// The gate `class` carries, if any.
    #[must_use]
    pub fn gate_for(&self, class: &str) -> Option<&ClassGate> {
        self.gates
            .iter()
            .find(|gate| gate.match_kind.matches(class, &gate.class))
    }

    /// Every class pattern, in key order — for a caller that wants to
    /// report what it loaded.
    pub fn class_patterns(&self) -> impl Iterator<Item = &str> {
        self.classes.keys().map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn behaviours() -> PatchOperationBehaviours {
        PatchOperationBehaviours::new(
            BTreeMap::from([
                (
                    "Example.PatchOperationAddOrReplace".to_string(),
                    ClassBehaviour {
                        match_kind: ClassMatch::Suffix,
                        behaviour: CustomBehaviour::AddOrReplace,
                    },
                ),
                (
                    "Example.PatchOperationSetModExtension".to_string(),
                    ClassBehaviour {
                        match_kind: ClassMatch::Suffix,
                        behaviour: CustomBehaviour::SetModExtension,
                    },
                ),
            ]),
            vec![ClassGate {
                class: "Example.".to_string(),
                match_kind: ClassMatch::Prefix,
                behaviour: GateBehaviour::ModsLoadedGate,
                fields: GateFields {
                    requires_all: "doesRequire".to_string(),
                    conditional_type: "conditionalType".to_string(),
                    conditional_param: "conditionalParam".to_string(),
                },
                conditional_types: BTreeMap::from([(
                    "Cond_ModsLoaded".to_string(),
                    ConditionalKind::AllLoaded,
                )]),
            }],
        )
    }

    #[test]
    fn a_suffix_row_matches_a_namespace_qualified_class() {
        assert_eq!(
            behaviours().behaviour_for("Prefixed.Example.PatchOperationAddOrReplace"),
            Some(CustomBehaviour::AddOrReplace)
        );
    }

    #[test]
    fn an_unknown_class_has_no_behaviour() {
        assert_eq!(
            behaviours().behaviour_for("Other.PatchOperationThing"),
            None
        );
    }

    #[test]
    fn a_prefix_gate_matches_every_class_in_its_namespace() {
        let loaded = behaviours();
        let gate = loaded
            .gate_for("Example.PatchOperationAddOrReplace")
            .expect("the prefix gate must match");
        assert_eq!(gate.behaviour, GateBehaviour::ModsLoadedGate);
        assert!(loaded.gate_for("Other.PatchOperationThing").is_none());
    }

    #[test]
    fn the_empty_set_knows_nothing() {
        let none = PatchOperationBehaviours::none();
        assert!(none.is_empty());
        assert_eq!(
            none.behaviour_for("Example.PatchOperationAddOrReplace"),
            None
        );
        assert!(
            none.gate_for("Example.PatchOperationAddOrReplace")
                .is_none()
        );
    }

    #[test]
    fn every_wire_spelling_round_trips_and_an_unknown_one_is_none() {
        assert_eq!(
            CustomBehaviour::from_wire("set_mod_extension"),
            Some(CustomBehaviour::SetModExtension)
        );
        assert_eq!(
            CustomBehaviour::from_wire("add_or_replace"),
            Some(CustomBehaviour::AddOrReplace)
        );
        assert_eq!(
            CustomBehaviour::from_wire("replace_research_coords"),
            Some(CustomBehaviour::ReplaceResearchCoords)
        );
        assert_eq!(CustomBehaviour::from_wire("teleport_the_pawn"), None);
        assert_eq!(
            GateBehaviour::from_wire("mods_loaded_gate"),
            Some(GateBehaviour::ModsLoadedGate)
        );
        assert_eq!(GateBehaviour::from_wire("cosmic_ray_gate"), None);
        assert_eq!(ClassMatch::from_wire("prefix"), Some(ClassMatch::Prefix));
        assert_eq!(ClassMatch::from_wire("suffix"), Some(ClassMatch::Suffix));
        assert_eq!(ClassMatch::from_wire("regex"), None);
        assert_eq!(
            ConditionalKind::from_wire("all_loaded"),
            Some(ConditionalKind::AllLoaded)
        );
        assert_eq!(
            ConditionalKind::from_wire("none_loaded"),
            Some(ConditionalKind::NoneLoaded)
        );
        assert_eq!(ConditionalKind::from_wire("sometimes"), None);
    }
}
