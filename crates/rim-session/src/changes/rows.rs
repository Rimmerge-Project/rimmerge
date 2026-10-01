//! The change inventory's filter, rows, and page.

use std::collections::{BTreeMap, BTreeSet};

use rim_resolve::domain::{DefRef, FindingKey};

/// Which kind of asset a [`ChangeKind::OverridesAsset`] row names — the
/// three [`Conflict`](rim_analyzer::domain::Conflict) shapes that count as "assets" (a texture, a
/// sound, or a `Languages/*/Keyed` translation key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AssetKind {
    /// See [`rim_analyzer::domain::TextureOverride`].
    Texture,
    /// See [`rim_analyzer::domain::SoundOverride`].
    Sound,
    /// See [`rim_analyzer::domain::KeyedTranslationCollision`].
    KeyedTranslation,
}

/// What kind of "change" one [`ChangeRow`] describes
///
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChangeKind {
    /// The mod defines this def ([`SourceIndex::defs`](rim_analyzer::analysis::SourceIndex::defs)).
    OwnsDef,
    /// The mod registers this `Name`-attributed template.
    OwnsTemplate,
    /// The mod's patch ops target a def or template it does not own.
    PatchesDef,
    /// The mod ships a texture/sound/keyed-translation path another
    /// active mod also ships.
    OverridesAsset(AssetKind),
}

/// Filters and paging for [`crate::Session::changes`]. `limit` is capped
/// at [`MAX_PAGE_SIZE`](crate::MAX_PAGE_SIZE) regardless of the requested value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeFilter {
    /// Only rows of one of these kinds.
    pub kinds: Option<BTreeSet<ChangeKind>>,
    /// Only rows whose def/template name, type, or asset path contains
    /// this substring (case-insensitive).
    pub search: Option<String>,
    /// How many matching rows to skip before collecting the page.
    pub offset: usize,
    /// How many rows to collect, capped at [`MAX_PAGE_SIZE`](crate::MAX_PAGE_SIZE).
    pub limit: usize,
}

/// One thing a mod changes, plus how contested it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRow {
    /// What kind of change this is.
    pub kind: ChangeKind,
    /// The def/template this row is about. `None` for
    /// [`ChangeKind::OverridesAsset`] (an asset path, not a def).
    pub def_ref: Option<DefRef>,
    /// The shared asset path/key. `None` for every def-shaped kind.
    pub asset_path: Option<String>,
    /// How many *other* active mods also touch this target: for
    /// [`ChangeKind::OwnsDef`]/[`ChangeKind::OwnsTemplate`], other owners
    /// plus other patchers plus (for a template) other children; for
    /// [`ChangeKind::PatchesDef`], the def's owners plus other patchers;
    /// for an asset, the other shippers. Never counts the mod itself.
    pub other_touchers: usize,
    /// This mod's own top-level patch-op count on the target — 0 for an
    /// owner that never patches its own def/template, and for every
    /// asset row.
    pub op_count: usize,
    /// Every finding this mod is a genuine party to on this target
    /// (BTree-ordered, deduplicated): the target's own ownership conflict
    /// (`DefOverride`/`DuplicateTemplateName`) when this mod is one of its
    /// owners, plus one `PatchCollision` per contested `sub_path` this mod
    /// is a patcher in, plus (for [`ChangeKind::OverridesAsset`]) one
    /// `TextureOverride`/`SoundOverride` or one `KeyedTranslationCollision`
    /// per pair naming this mod. Empty when the target isn't actually
    /// contested (a single owner with no patchers, for instance). Never
    /// names a finding this mod merely shares a target with but doesn't
    /// itself appear in — membership is the same base-compared predicate
    /// [`crate::finding_index::FindingFilter::mod_id`] filters by — and
    /// never one a generated mod's own declared scope would have the
    /// ledger hide (`rim_resolve::ledger::findings::hidden_by_generated`,
    /// re-derived locally in [`owner_set_hidden`](crate::changes::conflicts::owner_set_hidden)/[`pair_hidden`](crate::changes::conflicts::pair_hidden) since
    /// that function is private to `rim-resolve`).
    ///
    /// `OwnsTemplate`'s own `DuplicateTemplateName` link has a quirk this
    /// field's construction (`contested_finding_keys`) works around:
    /// `rim_analyzer`'s `Indices::template_owners` — and so this crate's
    /// own conflict index built from it — is keyed by template `Name`
    /// *alone*, not `(def_type, Name)`, so two same-named templates under
    /// different `def_type`s collapse into one `Conflict`. A row for a
    /// template this mod owns *alone* under its own `def_type` must not
    /// borrow that conflict's `owners` just because the Name matches; the
    /// key is only attached when another owner (base-compared) actually
    /// exists in the conflict.
    pub finding_keys: Vec<FindingKey>,
}

/// One page of [`crate::Session::changes`], plus the total matching the
/// filter (before paging) and a count per kind over the *whole* matching
/// set (not just this page) — enough for a UI's kind-chip counters
/// without a second, unfiltered call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangePage {
    /// Total rows matching the filter, before `offset`/`limit`.
    pub total: usize,
    /// This page's rows, in the index's stable sort order.
    pub items: Vec<ChangeRow>,
    /// How many rows fall under each kind, counted after
    /// [`ChangeFilter::search`] narrows the rows but *before*
    /// [`ChangeFilter::kinds`] narrows them further — so a UI's kind-chip
    /// row always shows what each chip would total if clicked next,
    /// under the search box's own current text, rather than freezing at
    /// whatever kind is already selected (which a count computed after
    /// both filters would do: every unselected chip would read 0).
    pub kind_counts: BTreeMap<ChangeKind, usize>,
}
