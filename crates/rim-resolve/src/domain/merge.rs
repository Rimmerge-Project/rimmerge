//! [`FieldPath`]: the address of one field or list item inside a def's
//! resolved XML tree, and [`MergeChoice`]: what the user wants there.
//! [`GeneratedModIdentity`] is a Rimmerge-generated mod's derived, never
//! stored, identity — shared by the findings filter here and by
//! `rim-merge`'s emitter. [`GeneratedMods`] is the
//! marker-based, scope-aware view of *every* Rimmerge-generated mod a
//! [`Report`] knows about.
//!
//! `FieldPath`/`PathSegment`/`ItemId` live in this pure domain crate
//! (lower than `rim-merge` in the crate graph) because
//! [`super::resolution::Action::Merge`] persists a
//! `BTreeMap<FieldPath, MergeChoice>` inside `decisions.json` — the type a
//! decision is made of has to live wherever decisions themselves do.
//! `rim-merge` re-exports [`FieldPath`] and owns the algorithms that walk
//! one (`FieldTree::get`, `leaves`, ...).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

use rim_analyzer::domain::{GeneratedKind, ModId, Report};
use serde::{Deserialize, Serialize};

/// One step of a [`FieldPath`]: a named child element, or one item of a
/// `li` list (identified by [`ItemId`], never by raw position alone).
///
/// Invariant (not enforced by the type — see [`ItemId::Key`]'s own doc
/// comment for why): `Child`'s tag must be non-empty. An empty tag has no
/// text-form representation distinct from `FieldPath`'s own empty-path
/// sentinel (the zero-segment path, used for "the def root"), so it can
/// never round-trip through `Display`/`FromStr`; a `Display` call on one
/// trips a `debug_assert!` in debug builds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathSegment {
    /// A named child element, e.g. `partEfficiency`. Must be non-empty —
    /// see this type's own doc comment.
    Child(String),
    /// One item of a `li` list, addressed by its identity.
    Item(ItemId),
}

/// How a `li` item is told apart from its siblings — tried in this order
/// (see `rim-merge::tree::ItemId::of`), first hit wins:
/// the `Class` attribute, a well-known key child (`defName`, `stat`, ...),
/// the item's own text content, or finally its 0-based position among
/// siblings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemId {
    /// Identified by its `Class` attribute.
    Class(String),
    /// Identified by a well-known key child, e.g. `defName`.
    Key {
        /// The key child's tag, e.g. `defName`.
        ///
        /// Invariant (not enforced by the type — `ItemId`'s fields are
        /// plain public data per this crate's own style, and `rim-merge`
        /// constructs values of this shape directly): **must be
        /// non-empty**. An empty `child` renders identically to
        /// [`ItemId::Text`] (`=value` either way), so it can never
        /// round-trip; a `Display` call on one trips a `debug_assert!` in
        /// debug builds. A non-empty `child` round-trips regardless of
        /// its first character — including `@`, `#`, or `=`, which would
        /// otherwise collide with the `Class`/`Text`/`Position` markers —
        /// because this type's `Display` impl backslash-escapes a leading
        /// `@`/`#` (a leading `=` is already escaped unconditionally).
        child: String,
        /// The key child's text.
        value: String,
    },
    /// Identified by its own (leaf) text content.
    Text(String),
    /// Identified only by its 0-based position among siblings — the last
    /// resort, and surfaced as such in the editor.
    Position(u32),
}

/// The characters this grammar treats as structural and therefore
/// backslash-escapes wherever they appear inside a value: the segment
/// separator, the item-spec delimiters, and the key/value separator.
const ESCAPED_CHARS: [char; 5] = ['\\', '/', '[', ']', '='];

fn escape(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for c in raw.chars() {
        if ESCAPED_CHARS.contains(&c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// [`escape`], plus a backslash before a *leading* `@` or `#`.
///
/// `escape` already backslash-escapes every `=` wherever it appears
/// (including a leading one), because `=` is unconditionally reserved.
/// `@` and `#` are different: they only mean something as the very first
/// character of a `Child` tag or a `Key` item's `child` — `@Class=` and
/// `#<digits>` are literal, unescaped prefixes [`parse_item_id`] checks
/// for before falling back to the `Key` interpretation — so only a
/// *leading* occurrence needs escaping; one anywhere else in the text is
/// never ambiguous with anything and is left untouched. Used by
/// [`PathSegment::Child`]'s and [`ItemId::Key`]'s `child`'s `Display`
/// impls; never needed for a `Class`/`Text` value or a `Key`'s `value`,
/// since those always sit *after* a fixed literal marker no leading
/// character of theirs could ever be mistaken for.
fn escape_leading_marker(raw: &str) -> String {
    let escaped = escape(raw);
    match escaped.chars().next() {
        Some('@' | '#') => format!("\\{escaped}"),
        _ => escaped,
    }
}

/// Reverses [`escape`]: every backslash is dropped and the character
/// after it is kept literally, regardless of which character that is —
/// safe because [`escape`] never emits a backslash except immediately
/// before one of [`ESCAPED_CHARS`].
fn unescape(raw: &str) -> String {
    let mut unescaped = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                unescaped.push(next);
            }
        } else {
            unescaped.push(c);
        }
    }
    unescaped
}

/// Splits `s` at the first *unescaped* occurrence of a character in
/// `delims`, treating a backslash as escaping whatever character follows
/// it (which is skipped over, not inspected). Returns the raw (still
/// possibly escaped) text before the delimiter, the delimiter itself, and
/// the raw remainder after it — `unescape` the pieces once their exact
/// extent is known, never before.
fn split_at_unescaped<'a>(s: &'a str, delims: &[char]) -> (&'a str, Option<char>, &'a str) {
    let mut chars = s.char_indices();
    while let Some((index, c)) = chars.next() {
        if c == '\\' {
            chars.next();
            continue;
        }
        if delims.contains(&c) {
            return (&s[..index], Some(c), &s[index + c.len_utf8()..]);
        }
    }
    (s, None, "")
}

impl fmt::Display for ItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Class(value) => write!(f, "@Class={}", escape(value)),
            Self::Key { child, value } => {
                debug_assert!(
                    !child.is_empty(),
                    "ItemId::Key's child must be non-empty — an empty child is indistinguishable \
                     from ItemId::Text in the text form (see this field's own doc comment)"
                );
                write!(f, "{}={}", escape_leading_marker(child), escape(value))
            }
            Self::Text(value) => write!(f, "={}", escape(value)),
            Self::Position(position) => write!(f, "#{position}"),
        }
    }
}

impl fmt::Display for PathSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Child(tag) => {
                debug_assert!(
                    !tag.is_empty(),
                    "PathSegment::Child's tag must be non-empty — an empty tag is \
                     indistinguishable from FieldPath's own empty-path sentinel (see this \
                     variant's own doc comment)"
                );
                write!(f, "{}", escape_leading_marker(tag))
            }
            Self::Item(item) => write!(f, "li[{item}]"),
        }
    }
}

/// [`FieldPath`]'s canonical text form failed to parse.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid field path {0:?}")]
pub struct FieldPathParseError(String);

/// Parses the inner text of a `li[...]` item spec (already known to sit
/// between an unescaped `li[` and the matching unescaped closing `]`).
fn parse_item_id(inner: &str) -> Result<ItemId, FieldPathParseError> {
    if let Some(value) = inner.strip_prefix("@Class=") {
        return Ok(ItemId::Class(unescape(value)));
    }
    if let Some(value) = inner.strip_prefix('=') {
        return Ok(ItemId::Text(unescape(value)));
    }
    if let Some(digits) = inner.strip_prefix('#') {
        let position: u32 = digits
            .parse()
            .map_err(|_| FieldPathParseError(format!("li[{inner}]")))?;
        return Ok(ItemId::Position(position));
    }
    let (child, delim, value) = split_at_unescaped(inner, &['=']);
    if delim != Some('=') {
        return Err(FieldPathParseError(format!("li[{inner}]")));
    }
    Ok(ItemId::Key {
        child: unescape(child),
        value: unescape(value),
    })
}

/// Parses one `/`-delimited raw segment (still possibly escaped).
fn parse_segment(raw: &str) -> Result<PathSegment, FieldPathParseError> {
    // A literal, unescaped `li[` prefix can only ever originate from this
    // module's own `Item` rendering: `escape` always backslash-escapes a
    // literal `[` inside a `Child` tag's text, so a `Child` segment whose
    // tag happens to start with `li[` renders as `li\[...`, which this
    // check does not match.
    if let Some(rest) = raw.strip_prefix("li[") {
        let (inner, delim, after) = split_at_unescaped(rest, &[']']);
        if delim != Some(']') || !after.is_empty() {
            return Err(FieldPathParseError(raw.to_string()));
        }
        return Ok(PathSegment::Item(parse_item_id(inner)?));
    }
    if raw.is_empty() {
        return Err(FieldPathParseError("empty path segment".to_string()));
    }
    Ok(PathSegment::Child(unescape(raw)))
}

/// The address of one leaf field or list item inside a def's resolved XML
/// tree — a `/`-separated chain of [`PathSegment`]s, e.g.
/// `comps/li[@Class=Foo]/scaleAdjustment`. Persisted as a plain string (see
/// this type's `serde` impl) so it can be used as a JSON object key inside
/// `decisions.json`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FieldPath(Vec<PathSegment>);

impl FieldPath {
    /// Builds a field path from its segments, root-first.
    ///
    /// Infallible: `segments` is trusted, not validated. A `Child` with an
    /// empty tag or an `ItemId::Key` with an empty `child` violates those
    /// types' own documented invariants and will trip a `debug_assert!`
    /// the next time this path is displayed — see [`PathSegment::Child`]
    /// and [`ItemId::Key`]'s own doc comments.
    #[must_use]
    pub fn new(segments: Vec<PathSegment>) -> Self {
        Self(segments)
    }

    /// The path's segments, root first.
    #[must_use]
    pub fn segments(&self) -> &[PathSegment] {
        &self.0
    }
}

impl fmt::Display for FieldPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, segment) in self.0.iter().enumerate() {
            if index > 0 {
                write!(f, "/")?;
            }
            write!(f, "{segment}")?;
        }
        Ok(())
    }
}

impl FromStr for FieldPath {
    type Err = FieldPathParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty() {
            return Ok(Self(Vec::new()));
        }
        let mut segments = Vec::new();
        let mut rest = text;
        loop {
            let (raw_segment, delim, after) = split_at_unescaped(rest, &['/']);
            segments.push(parse_segment(raw_segment)?);
            match delim {
                Some('/') => rest = after,
                _ => break,
            }
        }
        Ok(Self(segments))
    }
}

impl TryFrom<String> for FieldPath {
    type Error = FieldPathParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<FieldPath> for String {
    fn from(value: FieldPath) -> Self {
        value.to_string()
    }
}

/// What the user wants at one [`FieldPath`]. Absent from a
/// [`super::resolution::Action::Merge`]'s `choices` map means "automatic":
/// there's no explicit `Auto` variant, so reverting a field to automatic is
/// `choices.remove(path)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "choice", rename_all = "snake_case")]
pub enum MergeChoice {
    /// Take this owner's (resolved) value, or that mod's final patched
    /// value for a collision.
    From {
        /// The owner whose value to take.
        mod_id: ModId,
    },
    /// A free-text value: an XML fragment for a list item, plain text for
    /// a leaf.
    Value {
        /// The literal value.
        text: String,
    },
    /// Remove the field or item from the merged result.
    Drop,
}

/// The `packageId` prefix every generated merge mod carries — never a
/// prefix a real mod author could plausibly ship (RimWorld package ids are
/// reverse-DNS-style author namespaces), so
/// [`GeneratedModIdentity::is_generated`] can never misfire against a genuine
/// third-party mod.
const GENERATED_PACKAGE_PREFIX: &str = "rimmerge.merge.";

/// A Rimmerge-generated mod's identity: derived, never stored, so it can't
/// drift and two installs never collide. Shared by
/// [`crate::ledger::extract_findings`]'s self-referential findings filter
/// (via [`GeneratedMods`]), [`crate::tags::infer_tags`]/
/// [`crate::tags::collect_evidence`]'s same self-exclusion for tag
/// inference, and `rim-merge`'s emitter.
///
/// Named `GeneratedModIdentity` because the profile merge mod
/// ([`GeneratedModIdentity::for_profile`]) and a compat patch
/// (`PatchModIdentity::as_generated`, `rim-resolve`'s own `domain::patch`)
/// share this one plain-data shape — a patch's identity is user-chosen
/// rather than hash-derived, but what `rim-merge`'s emitter actually
/// renders is the same three fields either way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedModIdentity {
    /// `rimmerge.merge.<hash12>` for the profile merge mod; a user-chosen,
    /// validated package id for a compat patch.
    pub package_id: ModId,
    /// `rimmerge_merge_<hash12>` for the profile merge mod; derived from
    /// the package id for a compat patch.
    pub folder_name: String,
    /// The generated mod's display name.
    pub display_name: String,
}

impl GeneratedModIdentity {
    /// Builds the profile merge mod's identity from the profile's own
    /// 12-hex hash — the same hash `rim_io::profile_dir` derives, so the
    /// same install always gets the same id.
    #[must_use]
    pub fn for_profile(hash12: &str) -> Self {
        Self {
            package_id: ModId::new(format!("{GENERATED_PACKAGE_PREFIX}{hash12}")),
            folder_name: format!("rimmerge_merge_{hash12}"),
            display_name: "Rimmerge merge patch".to_string(),
        }
    }

    /// Whether `mod_id` (any `_steam`-suffixed variant included) names the
    /// profile merge mod, by its stable `packageId` prefix alone — no
    /// profile hash needed, since the prefix itself is shared by every
    /// install.
    ///
    /// This is a *fallback* signal only: it recognizes the profile merge
    /// mod (whose id is always this prefix), never a compat patch (whose
    /// id is user-chosen). [`GeneratedMods`] is the marker-based check that
    /// covers both; this method exists for [`GeneratedMods::from_report`]'s
    /// own markerless-report fallback and for call sites that only have a bare
    /// `ModId` with no `Report` at hand.
    #[must_use]
    pub fn is_generated(mod_id: &ModId) -> bool {
        mod_id.base().as_str().starts_with(GENERATED_PACKAGE_PREFIX)
    }
}

/// Every Rimmerge-generated mod a [`Report`] knows about, with the scope
/// each declares — derived from `Report.mods[*].generated` (the
/// `rimmerge.json` marker `rim-analyzer` reads) plus the
/// [`GeneratedModIdentity::is_generated`] prefix as a fallback
/// for reports cached before generated mods carried a marker.
/// Built once per [`crate::ledger::extract_findings`]/
/// [`crate::tags::collect_evidence`]/[`crate::tags::infer_tags`] call:
/// `O(mods)`, no signature change for those functions beyond taking (or
/// building) one of these.
///
/// Keyed by [`ModId::base`], so a `_steam` copy of a generated mod is
/// recognized the same as its local counterpart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GeneratedMods {
    /// `None` means unrestricted (the profile merge mod, or a prefix
    /// fallback match): it hides every finding it's party to. `Some(scope)`
    /// is a compat patch's or assignment project's declared scope: it hides
    /// a finding only when every *other* mod the finding names is inside
    /// that scope.
    scopes: BTreeMap<ModId, Option<BTreeSet<ModId>>>,
}

impl GeneratedMods {
    /// Builds the set from every mod in `report.mods` that either carries a
    /// `rimmerge.json` marker or matches the `rimmerge.merge.` prefix
    /// (a marker takes priority when both are somehow true, which never
    /// actually happens: the prefix is reserved and a marker-carrying
    /// profile merge mod's `kind` is always `Merge`, i.e. `None` scope
    /// either way).
    ///
    /// A [`GeneratedKind::Merge`] marker's `scope` is always taken as `None`
    /// (unrestricted) regardless of what the file says — that is the
    /// profile merge mod's own, by-design behaviour (it covers the whole
    /// load order). A [`GeneratedKind::Patch`] marker's *missing* scope is
    /// treated as an *empty* declared scope (`unwrap_or_default`), not as
    /// unrestricted: an incomplete or hand-edited `rimmerge.json` with
    /// `"kind":"patch"` and no `scope` key must not silently hide every
    /// finding the patch is party to — only the single-mod kinds
    /// [`GeneratedMods::hides`]'s vacuous-true case still covers.
    /// [`GeneratedKind::Assignment`] is treated exactly like `Patch` here:
    /// its declared scope is R ∪ T,
    /// a bounded set an assignment mod's rows can actually touch, so the
    /// same "missing scope is empty, not unrestricted" rule applies for the
    /// same reason — an assignment row conflicting with a target *outside*
    /// R ∪ T is exactly the kind of surprise this filter must not hide.
    #[must_use]
    pub fn from_report(report: &Report) -> Self {
        let mut scopes = BTreeMap::new();
        for mod_entry in &report.mods {
            let scope = match &mod_entry.generated {
                Some(marker) => match marker.kind {
                    GeneratedKind::Merge => None,
                    GeneratedKind::Patch | GeneratedKind::Assignment => {
                        Some(marker.scope.clone().unwrap_or_default())
                    }
                },
                None if GeneratedModIdentity::is_generated(&mod_entry.id) => None,
                None => continue,
            };
            scopes.insert(mod_entry.id.base(), scope);
        }
        Self { scopes }
    }

    /// Whether `id` (any `_steam`-suffixed variant included) names a
    /// Rimmerge-generated mod this set knows about.
    #[must_use]
    pub fn contains(&self, id: &ModId) -> bool {
        self.scopes.contains_key(&id.base())
    }

    /// Whether a finding naming `generated` (a generated mod) and `others`
    /// (every other mod the same finding names) is that mod's own doing
    /// and should therefore be hidden: `true` when `generated` is
    /// unrestricted, or every id `others` yields is inside its declared
    /// scope (vacuously `true` when `others` yields nothing at all — a
    /// generated mod named alone is always its own doing). `false` when
    /// `generated` isn't a known generated mod at all, or when at least
    /// one other mod falls outside its declared scope — a patch
    /// conflicting with a mod *outside* its scope, including another
    /// Rimmerge patch, stays visible.
    #[must_use]
    pub fn hides<'a>(&self, generated: &ModId, others: impl Iterator<Item = &'a ModId>) -> bool {
        match self.scopes.get(&generated.base()) {
            None => false,
            Some(None) => true,
            Some(Some(scope)) => others.map(ModId::base).all(|other| scope.contains(&other)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use super::*;

    fn path(segments: Vec<PathSegment>) -> FieldPath {
        FieldPath::new(segments)
    }

    #[test]
    fn a_plain_child_chain_renders_and_parses() {
        let p = path(vec![
            PathSegment::Child("addedPartProps".to_string()),
            PathSegment::Child("partEfficiency".to_string()),
        ]);
        assert_eq!(p.to_string(), "addedPartProps/partEfficiency");
        assert_eq!(p.to_string().parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn a_class_identified_item_renders_and_parses() {
        let p = path(vec![
            PathSegment::Child("comps".to_string()),
            PathSegment::Item(ItemId::Class(
                "ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust".to_string(),
            )),
            PathSegment::Child("scaleAdjustment".to_string()),
        ]);
        assert_eq!(
            p.to_string(),
            "comps/li[@Class=ExampleBody.Hediffs.HediffCompProperties_MaxHPAdjust]/scaleAdjustment"
        );
        assert_eq!(p.to_string().parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn a_key_identified_item_renders_and_parses() {
        let p = path(vec![
            PathSegment::Child("wildPlants".to_string()),
            PathSegment::Item(ItemId::Key {
                child: "plant".to_string(),
                value: "Plant_Grass".to_string(),
            }),
            PathSegment::Child("commonality".to_string()),
        ]);
        assert_eq!(
            p.to_string(),
            "wildPlants/li[plant=Plant_Grass]/commonality"
        );
        assert_eq!(p.to_string().parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn a_text_identified_item_renders_and_parses() {
        let p = path(vec![
            PathSegment::Child("tradeTags".to_string()),
            PathSegment::Item(ItemId::Text("ImplantEmpireCommon".to_string())),
        ]);
        assert_eq!(p.to_string(), "tradeTags/li[=ImplantEmpireCommon]");
        assert_eq!(p.to_string().parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn a_position_identified_item_renders_and_parses() {
        let p = path(vec![
            PathSegment::Child("costList".to_string()),
            PathSegment::Item(ItemId::Position(3)),
        ]);
        assert_eq!(p.to_string(), "costList/li[#3]");
        assert_eq!(p.to_string().parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn a_simple_leaf_child_renders_and_parses() {
        let p = path(vec![PathSegment::Child("Plasteel".to_string())]);
        assert_eq!(
            path(vec![
                PathSegment::Child("costList".to_string()),
                PathSegment::Child("Plasteel".to_string())
            ])
            .to_string(),
            "costList/Plasteel"
        );
        assert_eq!(p.to_string().parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn every_escaped_character_round_trips_inside_a_class_value() {
        let value = r"has\backslash/slash[open]close=equals";
        let p = path(vec![PathSegment::Item(ItemId::Class(value.to_string()))]);
        let text = p.to_string();
        assert_eq!(
            text,
            r"li[@Class=has\\backslash\/slash\[open\]close\=equals]"
        );
        assert_eq!(text.parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn a_child_tag_that_looks_like_an_item_spec_still_round_trips() {
        // A `Child` tag whose text happens to start with `li[` must still
        // be distinguished from a genuine `Item`, because `escape` always
        // backslash-escapes the `[`.
        let p = path(vec![PathSegment::Child("li[not_an_item]".to_string())]);
        let text = p.to_string();
        assert_eq!(text, r"li\[not_an_item\]");
        assert_eq!(text.parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn a_key_child_starting_with_at_class_round_trips_as_a_key_not_a_class() {
        // Without `escape_leading_marker`, this would render as
        // `li[@Class=x]` and misparse back as `ItemId::Class("x")`.
        let p = path(vec![PathSegment::Item(ItemId::Key {
            child: "@Class".to_string(),
            value: "x".to_string(),
        })]);
        let text = p.to_string();
        assert_eq!(text, r"li[\@Class=x]");
        assert_eq!(text.parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn a_key_child_starting_with_hash_round_trips_instead_of_failing_to_parse() {
        // Without `escape_leading_marker`, this would render as
        // `li[#7=value]` and fail to parse at all (`parse_item_id` would
        // try to parse `7=value` as a `Position`'s digits).
        let p = path(vec![PathSegment::Item(ItemId::Key {
            child: "#7".to_string(),
            value: "value".to_string(),
        })]);
        let text = p.to_string();
        assert_eq!(text, r"li[\#7=value]");
        assert_eq!(text.parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn a_key_child_starting_with_equals_round_trips_via_the_generic_escape() {
        // `=` is already unconditionally escaped by `escape` (it's in
        // `ESCAPED_CHARS`), so this needs no help from
        // `escape_leading_marker` — included to document that the two
        // mechanisms compose correctly rather than double-escaping.
        let p = path(vec![PathSegment::Item(ItemId::Key {
            child: "=weird".to_string(),
            value: "value".to_string(),
        })]);
        let text = p.to_string();
        assert_eq!(text, r"li[\=weird=value]");
        assert_eq!(text.parse::<FieldPath>().unwrap(), p);
    }

    #[test]
    fn a_child_tag_starting_with_at_or_hash_round_trips() {
        let at_tag = path(vec![PathSegment::Child("@foo".to_string())]);
        assert_eq!(at_tag.to_string(), r"\@foo");
        assert_eq!(at_tag.to_string().parse::<FieldPath>().unwrap(), at_tag);

        let hash_tag = path(vec![PathSegment::Child("#foo".to_string())]);
        assert_eq!(hash_tag.to_string(), r"\#foo");
        assert_eq!(hash_tag.to_string().parse::<FieldPath>().unwrap(), hash_tag);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "must be non-empty")]
    fn displaying_an_empty_child_tag_panics_in_debug_builds() {
        let _ = path(vec![PathSegment::Child(String::new())]).to_string();
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "must be non-empty")]
    fn displaying_an_empty_key_child_panics_in_debug_builds() {
        let _ = path(vec![PathSegment::Item(ItemId::Key {
            child: String::new(),
            value: "x".to_string(),
        })])
        .to_string();
    }

    #[test]
    fn malformed_item_spec_without_a_closing_bracket_is_rejected() {
        assert!("li[@Class=Foo".parse::<FieldPath>().is_err());
    }

    #[test]
    fn malformed_key_item_without_an_equals_sign_is_rejected() {
        assert!("li[justtext]".parse::<FieldPath>().is_err());
    }

    #[test]
    fn field_path_round_trips_as_a_json_map_key() {
        let mut choices = BTreeMap::new();
        choices.insert(
            path(vec![
                PathSegment::Child("comps".to_string()),
                PathSegment::Item(ItemId::Class("Foo.Bar".to_string())),
            ]),
            MergeChoice::Drop,
        );
        choices.insert(
            path(vec![PathSegment::Child("label".to_string())]),
            MergeChoice::From {
                mod_id: ModId::new("ludeon.rimworld"),
            },
        );

        let json = serde_json::to_string(&choices).expect("serializable");
        assert!(json.contains(r#""comps/li[@Class=Foo.Bar]":{"choice":"drop"}"#));
        assert!(json.contains(r#""label":{"choice":"from","mod_id":"ludeon.rimworld"}"#));

        let round_tripped: BTreeMap<FieldPath, MergeChoice> =
            serde_json::from_str(&json).expect("deserializable");
        assert_eq!(round_tripped, choices);
    }

    #[test]
    fn merge_mod_identity_derives_stable_names_from_the_profile_hash() {
        let identity = GeneratedModIdentity::for_profile("3f9a1c2b7d5e");
        assert_eq!(identity.package_id.as_str(), "rimmerge.merge.3f9a1c2b7d5e");
        assert_eq!(identity.folder_name, "rimmerge_merge_3f9a1c2b7d5e");
    }

    #[test]
    fn is_generated_matches_only_the_generated_prefix() {
        assert!(GeneratedModIdentity::is_generated(&ModId::new(
            "rimmerge.merge.3f9a1c2b7d5e"
        )));
        // A `_steam` suffix must not defeat the check — matched via
        // `ModId::base`.
        assert!(GeneratedModIdentity::is_generated(&ModId::new(
            "rimmerge.merge.3f9a1c2b7d5e_steam"
        )));
        assert!(!GeneratedModIdentity::is_generated(&ModId::new(
            "example.bionicsfork"
        )));
    }

    // -- GeneratedMods -------------------------------------------------

    mod generated_mods {
        use std::collections::BTreeSet;

        use rim_analyzer::domain::{GeneratedKind, GeneratedMarker};

        use super::*;
        use crate::test_support::ReportBuilder;

        fn marker(kind: GeneratedKind, scope: Option<BTreeSet<ModId>>) -> GeneratedMarker {
            GeneratedMarker {
                kind,
                patch_id: None,
                scope,
            }
        }

        #[test]
        fn from_report_marks_a_merge_marker_as_unrestricted() {
            let report = ReportBuilder::new()
                .mod_with("rimmerge.merge.abc123456789", |m| {
                    m.generated = Some(marker(GeneratedKind::Merge, None));
                })
                .mod_("real.mod")
                .build();

            let generated = GeneratedMods::from_report(&report);

            assert!(generated.contains(&ModId::new("rimmerge.merge.abc123456789")));
            assert!(!generated.contains(&ModId::new("real.mod")));
            // Unrestricted: hides regardless of who else the finding names.
            assert!(generated.hides(
                &ModId::new("rimmerge.merge.abc123456789"),
                [ModId::new("anyone"), ModId::new("anyone.else")].iter()
            ));
        }

        #[test]
        fn from_report_marks_a_patch_marker_with_its_declared_scope() {
            let scope: BTreeSet<ModId> = [ModId::new("a"), ModId::new("b")].into_iter().collect();
            let report = ReportBuilder::new()
                .mod_with("sample.abcompat", |m| {
                    m.generated = Some(marker(GeneratedKind::Patch, Some(scope)));
                })
                .mod_("a")
                .mod_("b")
                .mod_("c")
                .build();

            let generated = GeneratedMods::from_report(&report);
            let patch = ModId::new("sample.abcompat");

            assert!(generated.contains(&patch));
            assert!(generated.hides(&patch, [ModId::new("a"), ModId::new("b")].iter()));
            assert!(
                !generated.hides(&patch, [ModId::new("a"), ModId::new("c")].iter()),
                "a mod outside the patch's declared scope must keep the finding visible"
            );
        }

        /// A `"kind":"patch"` marker with no `scope` key at all (incomplete
        /// or hand-edited `rimmerge.json`) must not be treated as
        /// unrestricted — that would hide *every* finding this mod is party
        /// to, including one against a mod entirely outside anything the
        /// file actually declared. It hides only what an empty declared
        /// scope vacuously covers (the mod named alone).
        #[test]
        fn from_report_treats_a_scopeless_patch_marker_as_an_empty_scope_not_unrestricted() {
            let report = ReportBuilder::new()
                .mod_with("sample.abcompat", |m| {
                    m.generated = Some(marker(GeneratedKind::Patch, None));
                })
                .mod_("a")
                .mod_("b")
                .build();

            let generated = GeneratedMods::from_report(&report);
            let patch = ModId::new("sample.abcompat");

            assert!(generated.contains(&patch));
            // A pair finding against it stays visible: an empty declared
            // scope covers nothing.
            assert!(
                !generated.hides(&patch, [ModId::new("a"), ModId::new("b")].iter()),
                "a scopeless patch marker must not hide a finding naming other mods"
            );
            // The single-mod (vacuous, no `others`) case is still hidden.
            assert!(generated.hides(&patch, std::iter::empty()));
        }

        /// [`GeneratedKind::Assignment`] follows the same declared-scope
        /// rule as `Patch`:
        /// hidden against a mod inside its scope, visible against one
        /// outside it.
        #[test]
        fn from_report_marks_an_assignment_marker_with_its_declared_scope() {
            let scope: BTreeSet<ModId> = [ModId::new("a"), ModId::new("b")].into_iter().collect();
            let report = ReportBuilder::new()
                .mod_with("sample.partassign", |m| {
                    m.generated = Some(marker(GeneratedKind::Assignment, Some(scope)));
                })
                .mod_("a")
                .mod_("b")
                .mod_("c")
                .build();

            let generated = GeneratedMods::from_report(&report);
            let assignment = ModId::new("sample.partassign");

            assert!(generated.contains(&assignment));
            assert!(generated.hides(&assignment, [ModId::new("a"), ModId::new("b")].iter()));
            assert!(
                !generated.hides(&assignment, [ModId::new("a"), ModId::new("c")].iter()),
                "a mod outside the assignment project's declared scope must keep the finding visible"
            );
        }

        /// Same "missing scope is empty, not unrestricted" rule the plain
        /// `Patch` test above exercises, for `Assignment`.
        #[test]
        fn from_report_treats_a_scopeless_assignment_marker_as_an_empty_scope_not_unrestricted() {
            let report = ReportBuilder::new()
                .mod_with("sample.partassign", |m| {
                    m.generated = Some(marker(GeneratedKind::Assignment, None));
                })
                .mod_("a")
                .mod_("b")
                .build();

            let generated = GeneratedMods::from_report(&report);
            let assignment = ModId::new("sample.partassign");

            assert!(generated.contains(&assignment));
            assert!(
                !generated.hides(&assignment, [ModId::new("a"), ModId::new("b")].iter()),
                "a scopeless assignment marker must not hide a finding naming other mods"
            );
            assert!(generated.hides(&assignment, std::iter::empty()));
        }

        #[test]
        fn from_report_falls_back_to_the_prefix_for_a_markerless_legacy_mod() {
            // A report cached before the marker existed: the mod is present
            // with `generated: None`, but its id still carries the
            // `rimmerge.merge.` prefix.
            let report = ReportBuilder::new()
                .mod_("rimmerge.merge.abc123456789")
                .build();

            let generated = GeneratedMods::from_report(&report);
            let merge_mod = ModId::new("rimmerge.merge.abc123456789");

            assert!(generated.contains(&merge_mod));
            assert!(generated.hides(&merge_mod, [ModId::new("anyone")].iter()));
        }

        #[test]
        fn from_report_ignores_an_ordinary_mod_with_no_marker_and_no_prefix() {
            let report = ReportBuilder::new().mod_("example.bionicsfork").build();

            let generated = GeneratedMods::from_report(&report);

            assert!(!generated.contains(&ModId::new("example.bionicsfork")));
        }

        #[test]
        fn contains_and_hides_match_a_steam_suffixed_copy_against_the_base_id() {
            let report = ReportBuilder::new()
                .mod_with("rimmerge.merge.abc123456789", |m| {
                    m.generated = Some(marker(GeneratedKind::Merge, None));
                })
                .build();

            let generated = GeneratedMods::from_report(&report);

            assert!(generated.contains(&ModId::new("rimmerge.merge.abc123456789_steam")));
        }

        #[test]
        fn hides_is_false_for_a_mod_it_does_not_know_about() {
            let generated = GeneratedMods::default();
            assert!(!generated.hides(&ModId::new("unknown"), std::iter::empty()));
        }

        #[test]
        fn hides_is_vacuously_true_for_a_scoped_patch_named_with_no_others() {
            let scope: BTreeSet<ModId> = [ModId::new("a"), ModId::new("b")].into_iter().collect();
            let report = ReportBuilder::new()
                .mod_with("sample.abcompat", |m| {
                    m.generated = Some(marker(GeneratedKind::Patch, Some(scope)));
                })
                .build();

            let generated = GeneratedMods::from_report(&report);

            assert!(generated.hides(&ModId::new("sample.abcompat"), std::iter::empty()));
        }
    }

    // -- proptest roundtrip -------------------------------------------

    fn arb_value_text() -> impl Strategy<Value = String> {
        "[\\p{L}\\p{N}_ \\\\/\\[\\]=]{1,10}"
    }

    /// Text for a `Child` tag or a `Key` item's `child` — the two places
    /// [`escape_leading_marker`] applies. Includes every character
    /// [`arb_value_text`] does, plus a leading `@`/`#`/`=` (`=` is already
    /// escaped unconditionally by [`escape`]; `@`/`#` only matter as the
    /// very first character, per [`ItemId::Key`]'s own doc comment), so
    /// the round trip proptest actually exercises both escaping paths
    /// this type has to get right — not just the interior-character one.
    fn arb_marker_sensitive_text() -> impl Strategy<Value = String> {
        "[@#\\p{L}\\p{N}_ \\\\/\\[\\]=]{1,10}"
    }

    fn arb_child_tag() -> impl Strategy<Value = String> {
        arb_marker_sensitive_text()
    }

    fn arb_key_child() -> impl Strategy<Value = String> {
        arb_marker_sensitive_text()
    }

    fn arb_item_id() -> impl Strategy<Value = ItemId> {
        prop_oneof![
            arb_value_text().prop_map(ItemId::Class),
            (arb_key_child(), arb_value_text())
                .prop_map(|(child, value)| ItemId::Key { child, value }),
            arb_value_text().prop_map(ItemId::Text),
            any::<u32>().prop_map(ItemId::Position),
        ]
    }

    fn arb_segment() -> impl Strategy<Value = PathSegment> {
        prop_oneof![
            arb_child_tag().prop_map(PathSegment::Child),
            arb_item_id().prop_map(PathSegment::Item),
        ]
    }

    fn arb_field_path() -> impl Strategy<Value = FieldPath> {
        proptest::collection::vec(arb_segment(), 1..5).prop_map(FieldPath::new)
    }

    proptest! {
        #[test]
        fn field_path_display_from_str_roundtrips(path in arb_field_path()) {
            let text = path.to_string();
            let parsed: FieldPath = text.parse().unwrap();
            prop_assert_eq!(parsed, path);
        }
    }
}
