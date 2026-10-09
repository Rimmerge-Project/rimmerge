//! Shared def/template/patch-source loading, used by
//! [`super::plan_merge::PlanMerge`] and `InspectDef`: reading one
//! def/template's
//! raw XML back by locator, walking a `ParentName` chain into a
//! [`TemplateSet`], finding the mod whose raw node a def/template
//! xpath addresses under a selected order, and reading a def's top-level
//! patch operations' own XML text back.
//!
//! Both use cases share this one implementation instead of two drifting
//! copies.

use std::collections::BTreeMap;

use rim_analyzer::analysis::IndexedPatchOp;
use rim_analyzer::domain::{LoadOrder, ModId, PatchOp, Selector, XmlLocator};
use rim_analyzer::extract::xpath_expr::{self, Predicate, Step, XPathExpr};
use rim_merge::inherit::TemplateSet;
use rim_merge::tree::{FieldPath, FieldTree, ItemId, PathSegment};

use crate::Session;
use crate::ports::{DefSourceError, DefSourceReader, ElementExpectation};

/// A def/template/patch-op text failed to be read back or parsed — the
/// subset of [`super::plan_merge::PlanMergeError`] this module's own
/// functions can raise. Converted 1:1 into the caller's own error type
/// (see `impl From<DefSourceLookupError> for PlanMergeError` in
/// `plan_merge.rs`) rather than shared directly: `PlanMergeError` also
/// carries `Inherit`/`UnsupportedFinding` variants no function here ever
/// produces.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum DefSourceLookupError {
    /// The element couldn't be read back — a stale scan, or a read
    /// failure.
    #[error(transparent)]
    Source(#[from] DefSourceError),
    /// The read-back XML text failed to parse.
    #[error("parsing merge XML: {0}")]
    Xml(String),
    /// The source index has no record of an owner/template a caller
    /// named — a stale or inconsistent scan.
    #[error("{0}")]
    MissingSource(String),
}

/// Parses `text` as a [`FieldTree`], wrapping a parse failure as
/// [`DefSourceLookupError::Xml`].
pub(crate) fn parse_tree(text: &str) -> Result<FieldTree, DefSourceLookupError> {
    rim_merge::xml::parse(text).map_err(|e| DefSourceLookupError::Xml(e.to_string()))
}

/// What one element the merge editor/inspector needs to read back as a
/// top-level `<Operation>` is expected to be.
pub(crate) fn operation_expectation() -> ElementExpectation {
    ElementExpectation {
        tag: "Operation".to_string(),
        def_name: None,
        name_attr: None,
    }
}

/// Whether `id` is a vanilla (`Core`/DLC) mod — the proxy this module
/// uses throughout for `Verse.XmlInheritance`'s own `mod == null`
/// sentinel.
/// A linear scan over `session.report().mods` — fine here: every call
/// site asks this for a small handful of registrants/askers at a time,
/// never the whole install.
pub(crate) fn is_vanilla_owner(session: &Session, id: &ModId) -> bool {
    session
        .report()
        .mods
        .iter()
        .find(|m| &m.id == id)
        .is_some_and(|m| m.source.is_vanilla())
}

/// Who is asking for a template `Name`'s resolution — the pivot
/// [`nearest_owner`]'s own `Verse.XmlInheritance.GetBestParentFor`
/// simulation resolves against.
pub(crate) enum Asking<'a> {
    /// A specific mod's own `ParentName` reference is being resolved —
    /// the ordinary case for *every* real def or template in the game,
    /// concrete or abstract: `GetBestParentFor` always has a `node.mod`,
    /// even for vanilla content, where it's the sentinel `null`
    /// `is_vanilla` stands in for (see [`is_vanilla_owner`]).
    Mod { id: &'a ModId, is_vanilla: bool },
    /// No specific child is asking at all — [`super::inspect_def::InspectDef`]
    /// is showing a duplicated template `Name` in the abstract, with no
    /// concrete def's own `ParentName` reference driving the question.
    /// The real engine has no such case (`GetBestParentFor` is only ever
    /// called *for* a node with a `ParentName`) — see
    /// [`nearest_owner`]'s own doc comment for what this crate reports
    /// instead of fabricating a winner.
    Nobody,
}

/// Ground-truthed against the decompiled `Verse.XmlInheritance.GetBestParentFor`
/// (a decompiler was needed to read it; no game assembly is normally
/// reachable from this workspace): among `owners` (every mod that
/// registered a template
/// `Name`), finds the one `asking`'s own mod actually inherits from.
///
/// - [`Asking::Mod`] with `is_vanilla: false`: the registrant nearest at
///   or before the asking mod's own position in `order`, falling back to
///   a vanilla registrant when none qualifies (mirrors `GetBestParentFor`'s
///   `for (k = ...) if (value[k].mod.loadOrder <= node.mod.loadOrder ...)`
///   loop, then its `for (l = ...) if (value[l].mod == null)` fallback).
/// - [`Asking::Mod`] with `is_vanilla: true`: **inverted** — a vanilla
///   registrant first if one exists, else the lowest-positioned
///   registrant (`GetBestParentFor`'s own `node.mod == null` branch).
/// - [`Asking::Nobody`]: the real rule has no answer at all — this
///   crate's own explicit, disclosed choice is the *highest*-positioned
///   (last-loaded) registrant, since it is the one every real child
///   loading at or after it would actually resolve to — the widest
///   single group any one representative can honestly stand in for.
///   Callers showing this to a user must say so, never present it as
///   *the* resolution: see [`super::inspect_def::TemplateAmbiguity`],
///   which reports the real per-known-child answers alongside it.
///
/// `None` only when `owners` is empty, or (for a `Mod` asker with no
/// position of its own, or an [`Asking::Nobody`] query) every owner is
/// missing from `order` entirely — an id the scan once saw that the
/// current active list no longer carries.
pub(crate) fn nearest_owner(
    owners: &[ModId],
    order: &LoadOrder,
    session: &Session,
    asking: &Asking<'_>,
) -> Option<ModId> {
    let vanilla_registrant = || {
        owners
            .iter()
            .find(|o| is_vanilla_owner(session, o))
            .cloned()
    };
    let lowest_positioned = || {
        owners
            .iter()
            .filter(|o| order.position(o).is_some())
            .min_by_key(|o| order.position(o))
            .cloned()
    };
    let highest_positioned = || {
        owners
            .iter()
            .filter(|o| order.position(o).is_some())
            .max_by_key(|o| order.position(o))
            .cloned()
    };

    match asking {
        Asking::Mod {
            is_vanilla: true, ..
        } => vanilla_registrant().or_else(lowest_positioned),
        Asking::Mod {
            id,
            is_vanilla: false,
        } => match order.position(id) {
            Some(asking_position) => owners
                .iter()
                .filter(|o| order.position(o).is_some_and(|p| p <= asking_position))
                .max_by_key(|o| order.position(o))
                .cloned()
                .or_else(vanilla_registrant),
            None => vanilla_registrant().or_else(lowest_positioned),
        },
        Asking::Nobody => highest_positioned(),
    }
}

/// [`nearest_owner`]'s own entry-aware sibling, for the two callers
/// (`def_owner_and_raw`'s own `NameAttr` branch, [`template_set`]/
/// [`template_chain`]'s per-hop walk) that need the winning registrant's
/// own raw-XML locator back, not just its id.
fn nearest_registration<'a>(
    registrants: &'a [(ModId, rim_analyzer::domain::TemplateEntry)],
    order: &LoadOrder,
    session: &Session,
    asking: &Asking<'_>,
) -> Option<&'a (ModId, rim_analyzer::domain::TemplateEntry)> {
    let owners: Vec<ModId> = registrants.iter().map(|(o, _)| o.clone()).collect();
    let winner = nearest_owner(&owners, order, session, asking)?;
    registrants.iter().find(|(o, _)| *o == winner)
}

/// Finds the mod whose raw node a def/template xpath addresses: the
/// concrete def's own owner under the selected order for
/// [`Selector::DefName`] (last-loaded wins, matching an ordinary
/// `DefOverride`), or, for [`Selector::NameAttr`], the real
/// `Verse.XmlInheritance.GetBestParentFor` resolution *when a specific
/// child is asking* — plus that node's own parsed raw text.
///
/// **This function's own `NameAttr` branch never has a child to ask
/// with** — its only caller, [`super::inspect_def::InspectDef`], reaches
/// it while inspecting a template `Name` in the abstract (a
/// `DuplicateTemplateName` finding's own name-only ref, or a bare
/// `[@Name]` navigation with no originating concrete def) — so it always
/// resolves via [`Asking::Nobody`]: the highest-positioned (last-loaded)
/// registrant, an explicit, disclosed representative, not a fabricated
/// "the" answer (see [`nearest_registration`]'s own doc comment).
/// `InspectDef` itself is responsible for also surfacing
/// [`super::inspect_def::TemplateAmbiguity`] alongside this whenever more
/// than one registrant exists, so a caller never mistakes this
/// representative for the one true resolution.
pub(crate) fn def_owner_and_raw<Reader: DefSourceReader>(
    reader: &Reader,
    session: &Session,
    order: &LoadOrder,
    def_type: &str,
    def_name: &str,
    selector: Selector,
) -> Result<(ModId, FieldTree), DefSourceLookupError> {
    match selector {
        Selector::DefName => {
            let key = (def_type.to_string(), def_name.to_string());
            let owner = order
                .as_slice()
                .iter()
                .rev()
                .find(|id| {
                    session
                        .sources()
                        .defs
                        .contains_key(&((*id).clone(), key.clone()))
                })
                .cloned()
                .ok_or_else(|| {
                    DefSourceLookupError::MissingSource(format!(
                        "{def_type}/{def_name}: no active owner in the selected order"
                    ))
                })?;
            let entries = session
                .sources()
                .defs
                .get(&(owner.clone(), key))
                .unwrap_or_else(|| unreachable!("just found by contains_key above"));
            // Within one mod, a def declared twice loads as the *first*
            // one read (`Mod X has multiple <T>s named Y. Skipping.`) —
            // `entries` is in scan order, so this is `.first()`, not
            // `.last()`.
            let entry = entries.first().ok_or_else(|| {
                DefSourceLookupError::MissingSource(format!(
                    "{def_type}/{def_name}: empty source entry list for {owner}"
                ))
            })?;
            let text = reader.read_element(
                &entry.locator,
                &ElementExpectation {
                    tag: def_type.to_string(),
                    def_name: Some(def_name.to_string()),
                    name_attr: None,
                },
            )?;
            Ok((owner, parse_tree(&text)?))
        }
        Selector::NameAttr => {
            let key = (def_type.to_string(), def_name.to_string());
            let owners = session.sources().templates.get(&key).ok_or_else(|| {
                DefSourceLookupError::MissingSource(format!(
                    "missing template {def_type}/{def_name}"
                ))
            })?;
            let (owner, entry) = nearest_registration(owners, order, session, &Asking::Nobody)
                .ok_or_else(|| {
                    DefSourceLookupError::MissingSource(format!(
                        "template {def_type}/{def_name} has no active owner in the selected order"
                    ))
                })?;
            let owner = owner.clone();
            let text = reader.read_element(
                &entry.locator,
                &ElementExpectation {
                    tag: def_type.to_string(),
                    def_name: None,
                    name_attr: Some(def_name.to_string()),
                },
            )?;
            Ok((owner, parse_tree(&text)?))
        }
    }
}

/// Reads one specific, already-known owner's own raw copy of a concrete
/// def, parsed. Unlike [`def_owner_and_raw`], which *finds* the winning
/// owner under a selected order, this takes `owner` directly — a caller
/// that already has every active owner from `Finding::DefOverride.owners`
/// and needs each one's own copy, not just whichever wins. Two callers:
/// the identical-copies content check
/// (`Session::redecide_identical_copies_at`) and
/// `plan_merge::build_owner_versions` — the wider of the two, since it
/// also runs on every real-install `merge coverage`/`merge plan` call,
/// not only the once-per-finding identical-copies pass.
///
/// Key-representability note: `owner` must be a key `session.sources().defs`
/// actually carries an entry under — true for an ordinary `Defs/`-inline
/// registration (vanilla's own `Core`/DLC ids included, scanned like any
/// other mod) but never true for a def that exists only as a patch
/// injection (indexed separately — see
/// `crates/rim-analyzer/CLAUDE.md`'s own `SourceIndex` bullets), which
/// correctly falls through as [`DefSourceLookupError::MissingSource`]
/// rather than being read at all.
pub(crate) fn read_owner_def_raw<Reader: DefSourceReader>(
    reader: &Reader,
    session: &Session,
    def_type: &str,
    def_name: &str,
    owner: &ModId,
) -> Result<FieldTree, DefSourceLookupError> {
    let key = (def_type.to_string(), def_name.to_string());
    let entries = session
        .sources()
        .defs
        .get(&(owner.clone(), key))
        .ok_or_else(|| {
            DefSourceLookupError::MissingSource(format!(
                "{def_type}/{def_name}: no source entry for owner {owner}"
            ))
        })?;
    // Within one mod, a def declared twice loads as the *first* one read
    // (`Mod X has multiple <T>s named Y. Skipping.`) — `entries` is in
    // scan order, so this is `.first()`.
    let entry = entries.first().ok_or_else(|| {
        DefSourceLookupError::MissingSource(format!(
            "{def_type}/{def_name}: empty source entry list for {owner}"
        ))
    })?;
    let text = reader.read_element(
        &entry.locator,
        &ElementExpectation {
            tag: def_type.to_string(),
            def_name: Some(def_name.to_string()),
            name_attr: None,
        },
    )?;
    parse_tree(&text)
}

/// Walks `def_type`'s template chain starting at `start_parent`,
/// collecting every template it reaches into a [`TemplateSet`]
/// `rim_merge::inherit::resolve` can walk on its own. Stops (without
/// erroring) the moment a name it has already collected recurs — a
/// genuine cycle is then reported by `resolve` itself, which tracks
/// visited names across its own walk from the def's own raw node.
///
/// **Real `GetBestParentFor` resolution, not "the earliest owner"**:
/// `start_owner` is
/// the concrete def actually asking for `start_parent` (every caller
/// already has this — it's the def whose own raw node it just read via
/// [`def_owner_and_raw`] or, inside [`super::plan_merge::PlanMerge`]'s
/// per-owner loop, the owner being replayed). Each hop up the chain then
/// asks on behalf of *that* template's own resolved owner — exactly
/// `GetBestParentFor`'s own recursive behaviour, since every node in
/// `XmlInheritance`'s graph, template or concrete def, resolves its own
/// `ParentName` the identical way. No ambiguity to disclose here: the
/// asking child is always concretely known at every step, unlike
/// [`def_owner_and_raw`]'s own `NameAttr` branch.
pub(crate) fn template_set<Reader: DefSourceReader>(
    reader: &Reader,
    session: &Session,
    order: &LoadOrder,
    def_type: &str,
    start_owner: &ModId,
    start_parent: Option<&str>,
) -> Result<TemplateSet, DefSourceLookupError> {
    let mut collected: BTreeMap<(String, String), FieldTree> = BTreeMap::new();
    let mut current = start_parent.map(str::to_string);
    let mut asking = start_owner.clone();

    while let Some(name) = current {
        let map_key = (def_type.to_string(), name.clone());
        if collected.contains_key(&map_key) {
            break;
        }
        let registrants = session.sources().templates.get(&map_key).ok_or_else(|| {
            DefSourceLookupError::MissingSource(format!(
                "missing parent template {def_type}/{name}"
            ))
        })?;
        let asking_arg = Asking::Mod {
            id: &asking,
            is_vanilla: is_vanilla_owner(session, &asking),
        };
        let (owner_id, entry) = nearest_registration(registrants, order, session, &asking_arg)
            .ok_or_else(|| {
                DefSourceLookupError::MissingSource(format!(
                    "template {def_type}/{name} has no active owner in the selected order"
                ))
            })?;
        let owner_id = owner_id.clone();
        let text = reader.read_element(
            &entry.locator,
            &ElementExpectation {
                tag: def_type.to_string(),
                def_name: None,
                name_attr: Some(name.clone()),
            },
        )?;
        let tree = parse_tree(&text)?;
        let next_parent = tree.parent_name.clone();
        asking = owner_id;
        collected.insert(map_key, tree);
        current = next_parent;
    }

    Ok(TemplateSet::new(collected))
}

/// [`template_set`]'s own walk, plus which mod registered each template
/// reached and the chain itself in visit order (nearest ancestor first) —
/// what `InspectDef` needs to
/// build `rim_merge::effective::EffectiveInput::template_owners` (which
/// `TemplateSet` itself deliberately never carries — see that field's own
/// doc comment) and its own `parents` list, without a second walk.
///
/// Kept entirely separate from [`template_set`] rather than sharing a
/// common walk: this function's own missing-template/no-active-owner
/// cases *stop the walk instead of erroring* (see below), a genuine
/// behaviour difference from `template_set`'s `DefSourceLookupError::MissingSource`
/// that must never reach [`super::plan_merge::PlanMerge`]'s own,
/// already-tested call path.
pub(crate) struct TemplateChain {
    /// Every template reached, as a [`TemplateSet`] `rim_merge::inherit::resolve`
    /// (or `rim_merge::effective::compute`) can walk on its own.
    pub set: TemplateSet,
    /// Which mod registered each template in [`Self::set`] — `TemplateSet`
    /// itself carries no ownership.
    pub owners: BTreeMap<(String, String), ModId>,
    /// The chain actually walked, nearest ancestor first.
    pub chain: Vec<(String, String)>,
}

/// [`template_set`]'s own walk, but a `ParentName` naming a template with
/// no [`rim_analyzer::analysis::SourceIndex::templates`] entry, or one
/// with no active owner in `order`, simply **stops the walk** there
/// instead of raising [`DefSourceLookupError::MissingSource`] — the
/// resulting [`TemplateSet`] is missing that name (and everything beyond
/// it) exactly the same way it would be if `template_set` had errored,
/// but `rim_merge::effective::compute`'s own internal `ParentName`
/// resolution discovers the identical gap when it walks the (necessarily
/// incomplete) set it's handed and reports it as
/// `Stopper::Inherit(InheritError::MissingParent { .. })` — a *display*
/// concern `InspectDef` surfaces inside its `EffectiveDef` ("never skip a
/// stopper and continue", not "never show a def with a broken chain at
/// all"). A
/// genuine cycle needs no special handling here either: this walk's own
/// already-collected-name guard just stops (same as `template_set`), and
/// `effective::compute`'s internal walk — which tracks its own visited
/// set fresh from the def's raw node — detects the real
/// `InheritError::Cycle` against the (already complete-enough) set this
/// function handed it.
///
/// Only a genuine reader failure (`DefSourceError::Stale`/`Io`) or an
/// unparsable template — a source-of-truth problem, not a chain-shape
/// one — still propagates as an `Err`.
///
/// **Real `GetBestParentFor` resolution**, the same rule and the same reasoning as
/// [`template_set`]'s own doc comment: `start_owner` is the concrete def
/// actually asking for `start_parent` (`InspectDef`'s own already-resolved
/// `winner_id`), and each hop up the chain asks on behalf of the
/// previous hop's own resolved owner.
pub(crate) fn template_chain<Reader: DefSourceReader>(
    reader: &Reader,
    session: &Session,
    order: &LoadOrder,
    def_type: &str,
    start_owner: &ModId,
    start_parent: Option<&str>,
) -> Result<TemplateChain, DefSourceLookupError> {
    let mut collected: BTreeMap<(String, String), FieldTree> = BTreeMap::new();
    let mut owners: BTreeMap<(String, String), ModId> = BTreeMap::new();
    let mut chain: Vec<(String, String)> = Vec::new();
    let mut current = start_parent.map(str::to_string);
    let mut asking = start_owner.clone();

    while let Some(name) = current {
        let map_key = (def_type.to_string(), name.clone());
        if collected.contains_key(&map_key) {
            break;
        }
        let Some(registrants) = session.sources().templates.get(&map_key) else {
            break;
        };
        let asking_arg = Asking::Mod {
            id: &asking,
            is_vanilla: is_vanilla_owner(session, &asking),
        };
        let Some((owner_id, entry)) =
            nearest_registration(registrants, order, session, &asking_arg)
        else {
            break;
        };
        let owner_id = owner_id.clone();
        let entry = entry.clone();
        let text = reader.read_element(
            &entry.locator,
            &ElementExpectation {
                tag: def_type.to_string(),
                def_name: None,
                name_attr: Some(name.clone()),
            },
        )?;
        let tree = parse_tree(&text)?;
        let next_parent = tree.parent_name.clone();
        asking = owner_id.clone();
        owners.insert(map_key.clone(), owner_id);
        chain.push(map_key.clone());
        collected.insert(map_key, tree);
        current = next_parent;
    }

    Ok(TemplateChain {
        set: TemplateSet::new(collected),
        owners,
        chain,
    })
}

/// Every top-level `<Operation>` a mod's patch file aims at one def,
/// deduplicated and ordered per `order`.
///
/// `session.sources().patch_ops_by_def` flattens every op *and* its
/// nested `PatchOperationSequence`/`Conditional` descendants into
/// separate entries (right for collision detection, wrong for replay —
/// see `rim_merge::patch_eval`'s own doc comment), so several entries can
/// share one top-level `<Operation>`. Its own `element_path`'s first
/// ordinal is always that operation's position under the enclosing
/// `<Patch>` (`XmlLocator`'s own doc comment: `[0, 2, 1]` is `<Patch>` ->
/// 1st `<Operation>` -> ...), so re-deriving the top-level locator from it
/// and deduplicating by `(mod_id, file, first ordinal)` recovers exactly
/// the set of top-level operations `patch_eval::replay` expects, one
/// `PatchContribution` each. Grouping stays per mod (in each mod's own
/// original relative order) so the result can be re-interleaved by *any*
/// mod ordering, not just the one the scan happened to run under — which
/// is what makes a patch-collision preview meaningful under both
/// `OrderSource::Current` and `OrderSource::Suggested`.
pub(crate) fn top_level_operations(
    indexed: &[IndexedPatchOp],
    order: &LoadOrder,
) -> Vec<(ModId, XmlLocator)> {
    let mut seen: std::collections::BTreeSet<(ModId, std::sync::Arc<std::path::Path>, u32)> =
        std::collections::BTreeSet::new();
    let mut per_mod: BTreeMap<ModId, Vec<XmlLocator>> = BTreeMap::new();
    for entry in indexed {
        let Some(&first_ordinal) = entry.op.locator.element_path.first() else {
            continue;
        };
        let dedup_key = (
            entry.mod_id.clone(),
            entry.op.locator.file.clone(),
            first_ordinal,
        );
        if !seen.insert(dedup_key) {
            continue;
        }
        let top_locator = XmlLocator::new(entry.op.locator.file.clone(), vec![first_ordinal]);
        per_mod
            .entry(entry.mod_id.clone())
            .or_default()
            .push(top_locator);
    }

    let mut result = Vec::new();
    for mod_id in order.as_slice() {
        if let Some(locators) = per_mod.get(mod_id) {
            for locator in locators {
                result.push((mod_id.clone(), locator.clone()));
            }
        }
    }
    result
}

/// Reads every `(mod_id, locator)` pair's own `<Operation>` XML text back,
/// in order — the patch-collision replay's own contribution loading loop,
/// shared so a scoped collision preview and (later) `InspectDef`'s
/// unscoped one build [`rim_merge::patch_eval::PatchContribution`]s from
/// the same source.
///
/// # Errors
///
/// Returns [`DefSourceError`] on the first locator that fails to read —
/// callers that must never fail on an out-of-scope mod's own stale file
/// filter `locators` down to the set they actually intend to replay
/// *before* calling this.
pub(crate) fn load_operation_texts<'a, Reader: DefSourceReader>(
    reader: &Reader,
    locators: impl IntoIterator<Item = &'a (ModId, XmlLocator)>,
) -> Result<Vec<(ModId, String)>, DefSourceError> {
    locators
        .into_iter()
        .map(|(mod_id, locator)| {
            reader
                .read_element(locator, &operation_expectation())
                .map(|text| (mod_id.clone(), text))
        })
        .collect()
}

/// One `/`-separated xpath step, converted to a [`PathSegment`] — the
/// inverse of `rim_merge::emit`'s own step rendering. No tree lookup is
/// needed: every predicate shape [`super::plan_merge`]/[`super::verify_order`]
/// accept already names the item's identity directly (`li[@Class="X"]`
/// names a `Class` identity outright; the position/priority heuristic
/// [`rim_merge::tree::ItemIdentity`] uses only matters when *deriving* an
/// item's identity from its content, not when a predicate already gives
/// it explicitly).
///
/// Lives here so `plan_merge.rs` and `verify_order.rs` share one copy
/// instead of two drifting ones.
pub(crate) fn segment_from_step(step: &Step) -> Result<PathSegment, String> {
    if step.name != "li" {
        if !step.predicates.is_empty() {
            return Err(format!(
                "cannot represent a predicate on step '{}' as a field path",
                step.name
            ));
        }
        return Ok(PathSegment::Child(step.name.clone()));
    }
    match step.predicates.as_slice() {
        [Predicate::Attr(name, value)] if name == "Class" => {
            Ok(PathSegment::Item(ItemId::Class(value.clone())))
        }
        [Predicate::ChildText(name, value)] => Ok(PathSegment::Item(ItemId::Key {
            child: name.clone(),
            value: value.clone(),
        })),
        [Predicate::Text(value)] => Ok(PathSegment::Item(ItemId::Text(value.clone()))),
        [Predicate::Position(position)] => Ok(PathSegment::Item(ItemId::Position(
            position.saturating_sub(1),
        ))),
        other => Err(format!(
            "cannot represent li predicate {other:?} as a field path"
        )),
    }
}

/// Converts a raw xpath-remainder text (e.g. `comps/li[@Class="X"]`, the
/// text past a def head's own `defName="..."]`/`@Name="..."]` bracket)
/// into a [`FieldPath`] — reusing `rim_analyzer`'s xpath-subset parser
/// so a sub-path outside that
/// grammar degrades to `Err`, never a wrong path. Moved here alongside
/// [`segment_from_step`] — see that function's own doc comment.
pub(crate) fn field_path_from_sub_path(
    sub_path: Option<&str>,
) -> Result<Option<FieldPath>, String> {
    let Some(sub_path) = sub_path else {
        return Ok(None);
    };
    let synthetic = format!(r#"Defs/X[defName="x"]/{sub_path}"#);
    match xpath_expr::parse(&synthetic) {
        XPathExpr::Supported { steps, .. } => {
            let segments = steps
                .iter()
                .map(segment_from_step)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Some(FieldPath::new(segments)))
        }
        // `synthetic` always wraps `sub_path` in a real `Defs/X[defName="x"]/`
        // head, so this arm is unreachable in practice — kept exhaustive
        // rather than `unreachable!()` since a parser change elsewhere
        // should never be able to panic here.
        XPathExpr::DocumentRoot => Err(format!(
            "synthetic sub_path xpath unexpectedly resolved to the document root: '{sub_path}'"
        )),
        XPathExpr::Unsupported { reason } => Err(reason),
    }
}

/// Recovers the shallowest [`IndexedPatchOp`] sharing a top-level
/// `<Operation>`'s own ancestor — exact when the top-level node is itself
/// a mutating op, but [`rim_analyzer::analysis::SourceIndex::patch_ops_by_def`]
/// never indexes a non-mutating `PatchOperationSequence`/`FindMod`/
/// `Conditional` wrapper node itself, so a wrapped top-level op resolves
/// to its first mutating descendant's own class/xpath instead of the
/// wrapper's — the best available summary without a new analyzer field to
/// name the wrapper. Lives here so `inspect_def.rs` and `verify_order.rs`
/// share it.
pub(crate) fn representative_op<'a>(
    indexed: &'a [IndexedPatchOp],
    mod_id: &ModId,
    top_locator: &XmlLocator,
) -> Option<&'a PatchOp> {
    indexed
        .iter()
        .filter(|entry| {
            &entry.mod_id == mod_id
                && entry.op.locator.file == top_locator.file
                && entry.op.locator.element_path.first() == top_locator.element_path.first()
        })
        .min_by_key(|entry| entry.op.locator.element_path.len())
        .map(|entry| &entry.op)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use rim_analyzer::analysis::SourceIndex;
    use rim_analyzer::domain::DefEntry;

    use super::*;
    use crate::test_support::{InMemoryDefSourceReader, session_with_sources_and_mods};

    fn locator(ordinal: u32) -> XmlLocator {
        XmlLocator::new(Arc::from(std::path::Path::new("Things.xml")), vec![ordinal])
    }

    /// A mod defining `(ThingDef, Wall)` twice in its own `Defs/` (two
    /// files, or the same file twice) must resolve to the **first** one
    /// read, not the last — `Mod X has multiple <T>s named Y. Skipping.`
    /// `SourceIndex.defs`' own entries are in scan order, so
    /// `entries[0]` is that first copy: this pins `def_owner_and_raw`
    /// reads it, not `entries[1]`. Regression: before this change,
    /// `def_owner_and_raw` read `entries.last()` and this test failed.
    #[test]
    fn in_mod_duplicate_def_reads_the_first_copy() {
        let owner = ModId::new("dup.mod");
        let key = ("ThingDef".to_string(), "Wall".to_string());
        let first_locator = locator(0);
        let second_locator = locator(1);

        let mut sources = SourceIndex::default();
        sources.defs.insert(
            (owner.clone(), key.clone()),
            vec![
                DefEntry {
                    def_type: "ThingDef".to_string(),
                    def_name: "Wall".to_string(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: first_locator.clone(),
                },
                DefEntry {
                    def_type: "ThingDef".to_string(),
                    def_name: "Wall".to_string(),
                    may_require: Vec::new(),
                    may_require_any_of: Vec::new(),
                    parent_name: None,
                    locator: second_locator.clone(),
                },
            ],
        );

        let mut elements = BTreeMap::new();
        elements.insert(
            first_locator,
            "<ThingDef><defName>Wall</defName><label>first</label></ThingDef>".to_string(),
        );
        elements.insert(
            second_locator,
            "<ThingDef><defName>Wall</defName><label>second</label></ThingDef>".to_string(),
        );
        let reader = InMemoryDefSourceReader::new(elements);

        let report = rim_resolve::test_support::ReportBuilder::new()
            .mod_("dup.mod")
            .build();
        let session = session_with_sources_and_mods(sources, report, &["dup.mod"]);
        let order = LoadOrder::new(vec![owner.clone()]);

        let (resolved_owner, tree) = def_owner_and_raw(
            &reader,
            &session,
            &order,
            "ThingDef",
            "Wall",
            Selector::DefName,
        )
        .expect("must resolve");

        assert_eq!(resolved_owner, owner);
        let label_path: rim_merge::tree::FieldPath = "label".parse().expect("valid field path");
        let label_node = tree.get(&label_path).expect("label field must be present");
        assert_eq!(
            label_node.content,
            rim_merge::tree::Content::Text("first".to_string()),
            "the first-read copy must win, not the last"
        );
    }
}
