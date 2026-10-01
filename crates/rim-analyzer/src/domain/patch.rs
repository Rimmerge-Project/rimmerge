//! Patch operations extracted from a mod's `Patches/**/*.xml` files.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::locator::XmlLocator;
use super::mod_id::ModId;

/// How many `<relative path, content hash>` entries a single
/// [`ValueDigest`] may hold before it starts reporting `truncated`.
pub const MAX_VALUE_DIGEST_ENTRIES: usize = 512;
/// How many nesting levels under `<value>` a [`ValueDigest`] walks
/// before it stops descending and reports `truncated`.
pub const MAX_VALUE_DIGEST_DEPTH: usize = 8;

/// A bounded structural fingerprint of a patch op's own `<value>`
/// subtree: one `(relative path, content hash)` pair per element node.
/// `relative path` is built from tag names only, never an ordinal
/// index, so sibling `<li>` elements naturally share one path segment
/// rather than needing separate "collapse the position" logic — two
/// different `<li>`s at the same nesting level simply produce two
/// entries with the same path and different hashes. Bounded at
/// [`MAX_VALUE_DIGEST_ENTRIES`] entries and [`MAX_VALUE_DIGEST_DEPTH`]
/// levels; `truncated` records whether the real subtree was larger than
/// either bound.
///
/// Feeds the discarded-addition rule's own "not a duplicate" check
/// (`analysis::edges::patches::replace_discards_addition`): comparing an
/// adder's own added subtree's content hash against a replacer's own
/// replacement content at the same relative path. A truncated digest is
/// still usable, just conservatively — that function treats "cannot
/// prove a duplicate" as "keep the edge", never as "assume a duplicate".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValueDigest {
    pub entries: BTreeSet<(String, u64)>,
    pub truncated: bool,
}

/// Which attribute a patch xpath's def predicate matched on.
///
/// `defName` and `Name` are separate XML namespaces in RimWorld defs — a
/// `[@Name="X"]` predicate targets an (often abstract) template by its
/// `Name` attribute, not a `defName`, so it must never be matched against
/// def *ownership* (which is keyed by `defName`). `Ord` lets it sit in a
/// `BTreeMap` collision key alongside the def type/name/sub_path; `Hash`
/// lets `rim-resolve`'s `FindingKey` (which embeds a `Selector`) derive
/// `Hash` in turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Selector {
    DefName,
    NameAttr,
}

/// A def targeted by a patch xpath: `Defs/<def_type>[defName="<def_name>"]<sub_path>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefTarget {
    pub def_type: String,
    pub def_name: String,
    pub selector: Selector,
    /// Whatever followed the `defName="..."]` bracket, e.g. `/statBases`.
    pub sub_path: Option<String>,
}

impl DefTarget {
    /// Display-only text: `"{def_type}/{def_name}[/{sub_path}]"` — the
    /// convention `Edge.subject`/`Edge.detail` text already uses
    /// (`patch_removed_node_edges`). Deliberately **not** selector-aware (a
    /// `[@Name="X"]` template and a `[defName="X"]` def render identically
    /// here) — never use this for matching two targets against each other;
    /// see [`Self::match_key`] for that.
    #[must_use]
    pub fn display_path(&self) -> String {
        match &self.sub_path {
            Some(sub_path) => format!("{}/{}/{sub_path}", self.def_type, self.def_name),
            None => format!("{}/{}", self.def_type, self.def_name),
        }
    }

    /// The selector-aware text used for exact-match keys (the inline-node
    /// index and injected-path matching in `analysis::edges` and
    /// `extract::patches`) — identical to [`Self::display_path`] for
    /// [`Selector::DefName`], but prefixes the name segment with `@` for
    /// [`Selector::NameAttr`] (the same convention
    /// `rim_resolve::domain::DefRef`'s own text form uses), so a
    /// `[@Name="X"]` template and a `[defName="X"]` def sharing the literal
    /// name `X` never collide in the match-key space even though
    /// `Self::display_path` deliberately doesn't distinguish them — mirrors
    /// `conflicts::patch_collisions`' own `(def_type, def_name, selector,
    /// sub_path)` collision key.
    #[must_use]
    pub fn match_key(&self) -> String {
        let name_segment = match self.selector {
            Selector::DefName => self.def_name.clone(),
            Selector::NameAttr => format!("@{}", self.def_name),
        };
        match &self.sub_path {
            Some(sub_path) => format!("{}/{name_segment}/{sub_path}", self.def_type),
            None => format!("{}/{name_segment}", self.def_type),
        }
    }
}

/// One condition, from one enclosing `PatchOperationFindMod`, gating
/// whether an operation nested under its `<match>` or `<nomatch>` branch
/// actually runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindModGate {
    /// From a `<match>` branch: open when at least one of these mod
    /// display names is active.
    AnyActive(Vec<String>),
    /// From a `<nomatch>` branch: open when *none* of these mod display
    /// names is active.
    NoneActive(Vec<String>),
}

/// Which branch of the *nearest* enclosing `PatchOperationConditional` an
/// operation is reached through — the polarity
/// [`PatchOp::conditional_xpath`] itself deliberately drops (see that
/// field's own doc comment for the gap this closes). `None` when the
/// operation sits under no enclosing Conditional at all. Like
/// `conditional_xpath`, a nested Conditional replaces the outer one for
/// its own descendants — "nearest wins", not a stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionalBranch {
    /// Reached through the Conditional's own `<match>` branch: runs when
    /// the Conditional's own xpath resolves.
    Match,
    /// Reached through the Conditional's own `<nomatch>` branch: runs
    /// when the Conditional's own xpath does *not* resolve.
    NoMatch,
}

/// One patch operation node found while walking a `Patches/**/*.xml` file,
/// flattened out of the `PatchOperationSequence` / `PatchOperationFindMod`
/// / `PatchOperationConditional` nesting that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchOp {
    /// The `Class` attribute, e.g. `PatchOperationReplace`.
    pub class: String,
    /// The raw `<xpath>` text, kept verbatim regardless of whether it
    /// parses (see `extract::xpath_target`/[`crate::extract::xpath_expr`]).
    pub xpath: Option<String>,
    /// The def this op's xpath resolved to, when it matches the
    /// recognized `Defs/<Type>[defName=...]`/`[@Name=...]` head shape.
    pub target: Option<DefTarget>,
    /// One gate per enclosing `PatchOperationFindMod` this operation is
    /// nested under (outermost first). The operation only actually runs
    /// when every gate is open.
    pub find_mod_context: Vec<FindModGate>,
    /// The raw `<xpath>` text of the *nearest* enclosing
    /// `PatchOperationConditional` this operation is nested under (under its
    /// `<match>` or `<nomatch>` branch), or `None` when there is no such
    /// enclosing Conditional. A nested Conditional replaces an outer one —
    /// this is the nearest one's own xpath only, never a stack like
    /// [`Self::find_mod_context`]. `patch_removed_node_edges` needs this to
    /// tell an "if exists, then remove" Conditional (its own xpath equals a
    /// nested `PatchOperationRemove`'s xpath — effectively unconditional)
    /// from a genuinely conditional remove, which must stay advisory.
    ///
    /// **Known caveat for `patch_removed_node_edges`**: unlike
    /// [`FindModGate`], this field drops the `<match>`/`<nomatch>` *polarity*
    /// — a `PatchOperationRemove` whose own xpath equals its enclosing
    /// Conditional's, but which sits under that Conditional's `<nomatch>`
    /// branch, actually means "if this does **not** exist, remove it" (dead
    /// code: a nonexistent node can't be removed), which the "same xpath as
    /// the enclosing Conditional means unconditional" rule would misread as a
    /// genuine unconditional remove. Measured on the real install: negligible
    /// in practice — 9,615 ops sit under a Conditional's `<nomatch>` branch,
    /// only 6 of those have an xpath equal to their own Conditional's, and
    /// none of the 6 is a `PatchOperationRemove` — a real, disclosed gap, not
    /// fixed here. Separately, raw string equality (never a normalized or
    /// trimmed comparison) is confirmed safe for the "same xpath" test: of
    /// 750 `PatchOperationRemove`s nested under a Conditional on the real
    /// install, 371 match their Conditional's xpath by raw string and 0 match
    /// only after trimming whitespace.
    pub conditional_xpath: Option<String>,
    /// The `<mods>` display names named on this op itself, when this op's
    /// `Class` is `PatchOperationFindMod` — empty for every other class.
    /// Kept here (rather than only threaded into descendants'
    /// `find_mod_context`) so a `FindMod` with no `<match>` branch, or
    /// only a `<nomatch>` branch, still carries the names it names.
    pub find_mod_names: Vec<String>,
    /// `MayRequire` packageIds (comma-separated): every one must be active
    /// for this operation to run. Compared case-insensitively via
    /// [`super::mod_id::ModId`] at match time, not lowercased here.
    pub may_require: Vec<String>,
    /// `MayRequireAnyOf` packageIds: at least one must be active for this
    /// operation to run.
    pub may_require_any_of: Vec<String>,
    /// False for control-flow-only nodes (`PatchOperationSequence`,
    /// `PatchOperationConditional`, `PatchOperationFindMod`, `PatchOperationTest`)
    /// that never themselves change the target document.
    pub is_mutating: bool,
    /// Every fully-qualified type string this operation's own `<value>` child
    /// names (an element `Class` attribute or a `*Class` element's text,
    /// anywhere inside `<value>` — see
    /// `extract::xml_util::collect_class_strings`). Empty for an operation
    /// with no `<value>` (most non-`Add`/`Insert`/`Replace` classes).
    pub injected_types: BTreeSet<String>,
    /// Every node path this operation's own `<value>` injects, generalizing
    /// [`Self::injected_types`] beyond the Class-string shape to plain
    /// injected elements and whole injected defs. Two shapes, both keyed to a
    /// `"{def_type}/{def_name}/..."` path convention — **not** the shape
    /// [`crate::analysis::edges::patch_injected_node_edges`]'s own
    /// `Edge.subject` uses (a bare injected *class* string); this is instead
    /// the same def-address text `rim_resolve`'s `DefRef` already uses for
    /// `<def_type>/<def_name>` (see that type's own `Display`, one crate
    /// over) — the closest existing convention for naming a def by path,
    /// adopted here since nothing in this crate itself already addresses a
    /// def-plus-descendant-path this way:
    /// - This op's own `<xpath>`, re-parsed via
    ///   [`crate::extract::xpath_target::parse_all`] (not the single
    ///   [`Self::target`] this op resolved to — a disjunctive head,
    ///   `[defName="A" or defName="B"]`, must record the injection under
    ///   *every* def it names, or every edge about `B` silently vanishes;
    ///   the same reason [`crate::analysis::edges::patch_removed_node_edges`]
    ///   uses `analysis::patch_op_targets` instead of the single `target`
    ///   too), plus a `<value>`, **and** this op's own `Class` has append
    ///   semantics (`PatchOperationAdd`/`PatchOperationAddModExtension` —
    ///   see `extract::patches::APPEND_SEMANTICS_SUFFIXES`; every other
    ///   class, including `Replace`/`Insert`, contributes nothing from
    ///   this shape at all): one entry per `(matched target, top-level
    ///   element name inside `<value>`)` pair, each target's own
    ///   [`Self::match_key`] with the element name appended —
    ///   `"{def_type}/{name_segment}{/sub_path}/{element}"` (e.g.
    ///   `ThingDef/Human/alienRace` for an `Add` injecting `<alienRace>`
    ///   under `ThingDef[defName="Human"]`, or
    ///   `ThingDef/@Human/alienRace` under `ThingDef[@Name="Human"]`).
    ///   Selector-aware, deliberately (an unmarked path would let a
    ///   `[@Name="X"]` template and a `[defName="X"]` def collide in the
    ///   same key space, both here and in the inline-node index
    ///   `analysis::edges::emit_path_injected_node_edge` reads against;
    ///   see [`Self::match_key`]'s own doc comment). A candidate path is
    ///   dropped outright
    ///   when it contains an xpath-predicate bracket (`[`/`]`, from a
    ///   compound `sub_path` predicate) or an `li` segment — a list item
    ///   isn't individually addressable by path (most share the literal
    ///   tag `li`), so keeping one would both false-negative and
    ///   false-positive a future prefix-matching consumer.
    /// - This op's raw `<xpath>` text is exactly `Defs` or `/Defs`
    ///   (whole-`<Defs>`-root injection, no [`Self::target`] to anchor
    ///   on, and every mutating class — not just append-semantics ones —
    ///   is eligible here): one entry per top-level def element inside
    ///   `<value>` that carries a `<defName>` child, as
    ///   `"{element_tag}/{defName_text}"` — an element with no `<defName>`
    ///   contributes nothing (there is no def identity to key it by).
    ///
    /// **The class restriction on the first shape is deliberate, not an
    /// oversight**: a class-blind rule, on a real install, has tens of
    /// thousands of `PatchOperationReplace` ops emitting a doubled,
    /// nowhere-existing path (`.../alienRace/alienRace` — `<value>`'s own
    /// top-level tag repeats the node it replaces) and leaves over a quarter
    /// of all entries ending in an unaddressable `/li` segment. What this
    /// forgoes, named explicitly rather than left implicit: a sibling
    /// `PatchOperationInsert` creates next to an existing node (never
    /// captured — Insert has no append-to-target semantics this shape can
    /// honor), and a `PatchOperationReplace` that renames a node while
    /// replacing it (also never captured, for the same doubled-path reason
    /// above). The whole-`<Defs>`-root shape needed no such narrowing —
    /// across every `<xpath>` text on a real install, only the literal
    /// strings `Defs` and `/Defs` occur, no case or slash variants.
    ///
    /// Empty for an operation with no `<value>`, one whose `Class` fails
    /// the append-semantics gate above (for the first shape only), or
    /// neither shape's own condition otherwise.
    pub injected_paths: BTreeSet<String>,
    /// Every inheritance `Name` this operation registers with
    /// `Verse.XmlInheritance`: any `Name` attribute inside its own
    /// `<value>` subtree, plus — for a `PatchOperationAttributeAdd`/
    /// `AttributeSet` whose `<attribute>` is `Name` — that op's own
    /// `<value>` text (the shape Replace Stuff uses to bolt
    /// `Name="Cooler"` onto vanilla's `Cooler`, and by far the most
    /// common real-install case).
    ///
    /// `LoadedModManager` applies every patch before `ParseAndProcessXML`
    /// walks the unified document and calls `XmlInheritance.TryRegister(node,
    /// assetlookup[node]?.mod)`; a node a patch added has no `assetlookup`
    /// entry, so it registers with `mod == null`, which is exactly
    /// `GetBestParentFor`'s own fallback — such a template satisfies
    /// **every** child regardless of load order.
    ///
    /// **Deliberately over-approximating**: only a *top-level* node of
    /// the unified document registers, so a `Name` the patch nests below
    /// the `<Defs>` root is collected here but would not really
    /// register. Both consumers
    /// ([`crate::analysis::edges::parent_template_edges`] and
    /// `analysis::checks::unresolved_parent_templates`) use this set only
    /// to *suppress* output, so over-collecting can cost a true edge or a
    /// true warning but can never fabricate a false `Hard` edge — the
    /// safe direction. Collected for mutating operations only (a
    /// `Sequence`/`FindMod`/`Conditional` wrapper has no `<value>` of its
    /// own; its children carry theirs).
    pub injected_template_names: BTreeSet<String>,
    /// Whether this operation's own node is a `<li>` list item — a direct
    /// child of a `PatchOperationSequence`'s `<operations>`, or of a
    /// `<match>`/`<nomatch>` branch that itself holds a list rather than
    /// one `Class`-attributed operation — at any nesting depth. `MayRequire`
    /// on this shape is the one patch-op site the game actually reads
    /// (`Verse.DirectXmlToObject.ListFromXml`, the same list-item rule a
    /// def under `<Defs>` gets): a top-level `<Operation>` and a
    /// `<match>`/`<nomatch>` node that is itself a single operation both
    /// read `false` here, because `ModContentPack.LoadPatches` builds a
    /// top-level op straight from `DirectXmlToObject.ObjectFromXml`, which
    /// never reads `MayRequire`, and a `<match>`/`<nomatch>` wrapper's own
    /// attribute is never read either.
    ///
    /// Consulted by [`super::super::analysis::indices::patch_op_active`]:
    /// `may_require`/`may_require_any_of` only gate an op whose own node
    /// reads them at all — `false` here means the game runs the op
    /// regardless of what its (unread) `MayRequire` attribute says.
    pub is_list_item: bool,
    /// Mod ids named on this op's own loaded folder's `IfModActive`/
    /// `IfModActiveAll` gate in `LoadFolders.xml` (empty when the op's
    /// folder carries no such gate, or came from the default folder rule,
    /// which has none). Stamped by `infra::mod_scan` after this op is
    /// parsed — `extract::patches::walk` has no folder context of its own.
    /// Not yet consulted by anything; recorded for a later compat-folder
    /// exclusion on `PatchRemovedNode`.
    pub load_folder_gate: Vec<ModId>,
    /// True when this operation has no *later sibling* in any enclosing
    /// `PatchOperationSequence`'s own `<operations>` list, checked at
    /// every Sequence level up the ancestry — seeing straight through any
    /// `PatchOperationFindMod`/`PatchOperationConditional` wrapper in
    /// between (neither aborts on a child's failure the way a Sequence
    /// does, per `Verse.PatchOperationSequence.ApplyWorker`'s own
    /// first-failure-stops behaviour, so only genuine Sequence-list
    /// position affects this field). `true` vacuously when the op sits
    /// under no Sequence's own `<operations>` list at all (nothing to be
    /// a non-tail item *of*). Used by patch-removed-node edge
    /// classification to tell a cosmetic reorder (the final document is
    /// identical either way) from one where an early failure in the
    /// remover-first order would skip real, later ops a toucher's own
    /// Sequence still owed the document.
    pub sequence_tail: bool,
    /// Which branch of the *nearest* enclosing `PatchOperationConditional`
    /// this operation is reached through, `None` when there is no such
    /// enclosing Conditional — see [`ConditionalBranch`]'s own doc
    /// comment. Threaded the same "nearest Conditional wins" way
    /// [`Self::conditional_xpath`] already is; only a
    /// `PatchOperationConditional` node's own `<match>`/`<nomatch>`
    /// branches set this (a `PatchOperationFindMod`'s own branches leave
    /// it untouched, mirroring `conditional_xpath`'s own rule).
    pub conditional_branch: Option<ConditionalBranch>,
    /// Only meaningful when [`Self::conditional_branch`] is
    /// `Some(ConditionalBranch::Match)`: whether the *same* enclosing
    /// Conditional's own `<nomatch>` branch holds at least one op with
    /// append/creation semantics (`PatchOperationAdd`,
    /// `PatchOperationInsert`, `PatchOperationAddModExtension`, or
    /// `PatchOperationReplace`, at any nesting depth inside that
    /// `<nomatch>` branch) — the "if exists, replace it / if not, add it"
    /// idiom a real compat patch ships: when the tested node is missing,
    /// the `<nomatch>` branch recreates it instead of erroring, so a
    /// same-Conditional `<match>`-branch op that only *edits* the node is
    /// tolerant of the node being gone — its author already planned for
    /// that end state. `false` when [`Self::conditional_branch`] isn't
    /// `Some(Match)`, or the op sits under no Conditional at all.
    /// Computed once per Conditional in `extract::patches::walk_operation`
    /// (a bounded scan of that Conditional's own `<nomatch>` subtree,
    /// not a full re-walk with locators) and stamped onto every op
    /// reached through that Conditional's `<match>` branch — mirrors how
    /// [`Self::conditional_xpath`] is stamped onto every descendant.
    pub conditional_nomatch_creates: bool,
    /// Whether this op's own `<xpath>` names exactly one def — `false`
    /// for a disjunctive head (`[defName="A" or defName="B"]`), which can
    /// write outside a remover's own removed region for the *other*
    /// name(s) it targets even when this op's own [`Self::target`] (a
    /// single resolved `DefTarget`) suggests otherwise. Computed the same
    /// way `analysis::patch_op_targets` re-parses a
    /// mutating op's own xpath (falling back to the single resolved
    /// [`Self::target`] when there's no xpath text to re-read), kept as
    /// its own field rather than requiring every caller to re-run that
    /// parse just to learn the count.
    pub names_single_def: bool,
    /// The top-level element names this op's own `<value>` child injects,
    /// computed only for `PatchOperationAdd`/`PatchOperationAddModExtension`/
    /// a third-party `PatchOperationAddOrReplace` (matched by suffix, same
    /// convention as every other class-suffix set in this crate) —
    /// deliberately **not** shared with [`Self::injected_paths`]'s own
    /// `APPEND_SEMANTICS_SUFFIXES` gate (`extract::patches`), which stays
    /// untouched: this field exists only to feed
    /// `analysis::edges::patches::remover_recreates_region`'s own
    /// parent-plus-child-name recreate shape, and widening the shared,
    /// widely-consumed `injected_paths` gate to `AddOrReplace` too would
    /// ripple into `PatchInjectedNode`/`UsesType`/collision-keying, which
    /// nothing asked for. Empty for every other class, or a `<value>` with
    /// no element children of its own.
    pub value_child_names: BTreeSet<String>,
    /// Whether this op is reachable at all once every enclosing
    /// mod-setting-gated custom toggle class (a third-party class wearing
    /// `PatchOperationSequence`'s or `PatchOperationConditional`'s own
    /// clothes — an `<operations>` list or a `<match>`/`<nomatch>`/
    /// `<operation>` branch gated on a declared `<enabled>`/`<defaultValue>`/
    /// `<default>` value, detected purely by structure, never by class
    /// name — see `extract::patches::ToggleShape`) is resolved at its own
    /// declared default. `true` for every op with no such enclosing
    /// toggle, or whose enclosing toggle(s) all default to the branch this
    /// op sits in.
    ///
    /// This mirrors the same knowledge `rim-merge`'s own replay already
    /// uses (`rim_merge::patch_eval::identity::toggle_default`, which
    /// re-exports `extract::patches::toggle_default` rather than keeping a
    /// second copy) — an op the analyzer treats as reachable and an op the
    /// replay actually runs must never silently disagree. Known
    /// limitation, shared with the replay: a player who flipped a toggle
    /// on in-game (away from its XML-declared default) diverges from this
    /// field, since neither the analyzer nor the replay reads runtime mod
    /// settings.
    pub toggle_active: bool,
    /// Every top-level element name this op's own `<value>` child
    /// carries, in document order — computed for *any* mutating op with
    /// a `<value>` (unlike [`Self::value_child_names`], not gated to a
    /// specific class list), since the discarded-addition rule
    /// (`analysis::edges::patches::replace_discards_addition`) needs it
    /// for a `PatchOperationReplace` specifically: whether the value
    /// keeps the replaced node's own tag (exactly one root name, equal
    /// to the target's own last path segment) decides whether the
    /// replacement is even a candidate to discard another mod's earlier
    /// addition, versus a rename the "value keeps the node" rule
    /// excludes outright. Empty for an op with no `<value>` at all.
    pub value_root_names: Vec<String>,
    /// A bounded structural digest of every element under this op's own
    /// `<value>` — see [`ValueDigest`]. `None` for an op with no
    /// `<value>` at all.
    pub value_digest: Option<ValueDigest>,
    /// Where this operation's node (the whole `<Operation>`/`<li>`
    /// subtree — `<value>`, `<order>`, `<attribute>`, `<name>`, nested
    /// `<operations>`, `<match>`/`<nomatch>` all intact) lives on disk.
    pub locator: XmlLocator,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(selector: Selector, sub_path: Option<&str>) -> DefTarget {
        DefTarget {
            def_type: "ThingDef".to_string(),
            def_name: "Human".to_string(),
            selector,
            sub_path: sub_path.map(str::to_string),
        }
    }

    #[test]
    fn display_path_never_distinguishes_selector() {
        assert_eq!(
            target(Selector::DefName, None).display_path(),
            target(Selector::NameAttr, None).display_path()
        );
        assert_eq!(
            target(Selector::DefName, None).display_path(),
            "ThingDef/Human"
        );
    }

    #[test]
    fn display_path_appends_sub_path_when_present() {
        assert_eq!(
            target(Selector::DefName, Some("comps")).display_path(),
            "ThingDef/Human/comps"
        );
    }

    #[test]
    fn match_key_disambiguates_def_name_from_name_attr() {
        let by_def_name = target(Selector::DefName, None).match_key();
        let by_name_attr = target(Selector::NameAttr, None).match_key();
        assert_ne!(by_def_name, by_name_attr);
        assert_eq!(by_def_name, "ThingDef/Human");
        assert_eq!(by_name_attr, "ThingDef/@Human");
    }

    #[test]
    fn match_key_appends_sub_path_after_the_selector_aware_name_segment() {
        assert_eq!(
            target(Selector::NameAttr, Some("comps")).match_key(),
            "ThingDef/@Human/comps"
        );
    }
}
