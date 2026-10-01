//! XPath resolution against the replayed def: head predicates, cross-def existence, and node
//! selection.

use rim_analyzer::domain::{DefTarget, ModId, Selector};
use rim_analyzer::extract::xpath_expr::{self, Predicate, Step, XPathExpr};
use roxmltree::Node;

use super::dispatch::unsupported;
use super::identity::child_text;
use super::{ReplayContext, ReplayError, ReplayLog};
use crate::plan::Caveat;
use crate::tree::{Content, FieldNode, FieldPath, FieldTree, PathSegment, identify_all_li};

/// One operation's xpath, parsed and checked against `context`'s def
/// identity.
pub(super) struct ResolvedXPath {
    /// Every def the head names (more than one for a
    /// `[defName="A" or defName="B"]` disjunction).
    targets: Vec<DefTarget>,
    /// Predicates on the def node itself.
    root_predicates: Vec<Predicate>,
    /// The steps under the def root.
    steps: Vec<Step>,
    /// Whether the xpath ends in `text()`.
    selects_text: bool,
    /// Whether one of `targets` is the def being replayed.
    pub(super) is_this_def: bool,
    /// `true` for both [`resolve_head_filter`]'s own resolutions and
    /// [`resolve_head_content_predicate`]'s (Group B): both are *global*
    /// queries over the whole document, so an empty selection here proves
    /// nothing about the op's real outcome and must never become a
    /// prediction. [`resolve_selection`] reports [`Selected::Done`]`(true)`
    /// with no [`Caveat::FailedOp`] for one of these, counting the
    /// suppression in [`ReplayOutcome::suppressed_filter_head_ops`];
    /// [`condition_matches`] reports [`ReplayError::Unsupported`] instead
    /// of a decisive `false`.
    non_predicting: bool,
}

/// What resolving one operation's `<xpath>` against this replay's context
/// produced: an ordinary def-targeting head, or the bare document root
/// — no def name at all, so there is no [`ResolvedXPath`]
/// to build. Callers handle the two shapes separately: a def-targeting
/// xpath is resolved against the current tree, while a document-root one
/// is dispatched per operation class (see `document_root_selection` and
/// `condition_matches`) since "add a new top-level def" and "this can't be
/// expressed without the whole `<Defs>` document" need different answers.
pub(super) enum XPathResolution {
    Def(ResolvedXPath),
    DocumentRoot,
    /// The strict grammar rejected the xpath, but the analyzer's own
    /// loose `xpath_target::parse_all` head names one or more defs, none
    /// of them this replay's own def — the same "succeeded elsewhere"
    /// shape as a strictly-parsed cross-def head, just reached through
    /// the looser scan collision detection already relies on.
    Elsewhere,
}

pub(super) fn resolve_xpath(
    xpath_text: &str,
    tree: &FieldTree,
    context: &ReplayContext<'_>,
) -> Result<XPathResolution, String> {
    match xpath_expr::parse(xpath_text) {
        XPathExpr::Supported {
            targets,
            root_predicates,
            steps,
            selects_text,
        } => {
            let is_this_def = targets.iter().any(|target| {
                target.def_type == context.def_type
                    && target.def_name == context.def_name
                    && target.selector == context.selector
            });
            Ok(XPathResolution::Def(ResolvedXPath {
                targets,
                root_predicates,
                steps,
                selects_text,
                is_this_def,
                non_predicting: false,
            }))
        }
        XPathExpr::DocumentRoot => Ok(XPathResolution::DocumentRoot),
        XPathExpr::Unsupported { reason } => {
            // Group B: the
            // strict grammar already parses a head-position child-value
            // predicate (`Predicate::ChildText`/`NestedChildText`) fine —
            // it just can't *enumerate* which defs it names, since that
            // needs a cross-mod content index this module has no access
            // to at all (by design — see `xpath_expr::head_content_predicate`'s
            // own doc comment). But a replay already has the one thing
            // that resolves the question for *this* def: its own tree.
            // Evaluated with the identical `Predicate` machinery a root
            // predicate already uses (`HeadContentQuery::as_predicate`),
            // never by trusting `SourceIndex`'s own (already-computed,
            // but independent) answer — a wrong index entry must never
            // become a wrong replay result with no check of its own.
            if let Some(query) = xpath_expr::head_content_predicate(xpath_text) {
                return resolve_head_content_predicate(query, tree, context);
            }
            // The filter-head fallback, third and last of the head
            // fallbacks and deliberately *after* Group B's: the two
            // overlap (Group B's child-value head is a strict subset of
            // what `head_filter_predicate` admits), and keeping the
            // shipped, measured one first means no existing behaviour is
            // silently re-routed through newer code.
            if let Some(query) = xpath_expr::head_filter_predicate(xpath_text) {
                return resolve_head_filter(query, tree, context);
            }
            let heads = rim_analyzer::extract::xpath_target::parse_all(xpath_text);
            let names_only_other_defs = !heads.is_empty()
                && heads.iter().all(|target| {
                    !(target.def_type == context.def_type
                        && target.def_name == context.def_name
                        && target.selector == context.selector)
                });
            if names_only_other_defs {
                return Ok(XPathResolution::Elsewhere);
            }
            Err(reason)
        }
    }
}

/// [`resolve_xpath`]'s Group B fallback: a head-position child-value
/// predicate names no def by construction (the strict grammar can't
/// enumerate them), so `targets` stays empty — `is_this_def` is instead
/// answered by evaluating the predicate directly against `tree`, the def
/// actually being replayed, with no index consulted at all. `targets`
/// being empty here (impossible for the ordinary name-identifying path,
/// where a head always names at least one def) is exactly what tells
/// [`cross_def_existence`] to refuse rather than guess when a
/// `PatchOperationConditional`/`PatchOperationTest` using this shape
/// doesn't match the current def either — this module still cannot say
/// whether it matches some *other* def without the index it deliberately
/// never reaches for.
///
/// **`non_predicting: true` — Group B takes the filter-head outcome
/// path, ground-truthed against a real-install false positive:**
/// RimWorld itself evaluates this head once, over the whole `<Defs>`
/// document, exactly like `[@ParentName="X"]` or a bare-type head.
/// `is_this_def` being `true` only says the content predicate matches
/// *this* def; the narrower steps under it (`/comps`, say) can still
/// select nothing on this one def while the same head selects something
/// on another — that empty selection is no more evidence of failure here
/// than any other filter head's is. Before this was `false`, a
/// `PatchOperationConditional` whose steps matched nothing on this def
/// decisively answered `false` and ran its `nomatch` branch, when the
/// game's own whole-document query could easily be `true` elsewhere —
/// exactly the shape a real install's play-test log showed (a compat
/// patch's `nomatch` branch never fires in the game, but was predicted
/// to inject a node here). See [`resolve_head_filter`]'s own doc
/// comment for the shared reasoning.
fn resolve_head_content_predicate(
    query: xpath_expr::HeadContentQuery,
    tree: &FieldTree,
    context: &ReplayContext<'_>,
) -> Result<XPathResolution, String> {
    // `as_predicate` returning `None` is unreachable in practice (see its
    // own doc comment) — treated as "doesn't match this def" rather than
    // propagating a panic, the same safe-direction refusal this whole
    // fallback is built on.
    //
    // Evaluated through `root_predicate_matches`, not `predicate_matches`:
    // the head applies to the def *node*, where `Name`/`ParentName` live
    // on the tree rather than in `root.attrs` and a `Position` cannot be
    // answered at all. Strictly more correct than `predicate_matches`,
    // and identical for everything Group B actually produces — its only
    // leaves are `ChildText`/`NestedChildText`, which
    // `root_predicate_matches` forwards to `predicate_matches` unchanged.
    let is_this_def = match query.as_predicate() {
        Some(predicate) if query.def_type == context.def_type => {
            root_predicate_matches(tree, &predicate)?
        }
        _ => false,
    };
    Ok(XPathResolution::Def(ResolvedXPath {
        targets: Vec::new(),
        root_predicates: query.root_predicates,
        steps: query.steps,
        selects_text: query.selects_text,
        is_this_def,
        non_predicting: true,
    }))
}

/// [`resolve_xpath`]'s filter-head fallback: a head this module's grammar
/// parses but can never *enumerate* — `[@ParentName="X"]`, an
/// `and`-composed filter, a `not(...)`, a nested child filter, a
/// `contains(text(), ...)`, or a bare-type head with no predicate at all.
/// Exactly like Group B above, `targets` stays empty and `is_this_def` is
/// answered by evaluating the head against the one def already in hand,
/// with no index consulted.
///
/// **`root_predicate_matches`, never `predicate_matches`** — load-bearing
/// rather than stylistic: `crate::xml::parse` lifts `Name` and
/// `ParentName` *off* the root element onto the [`FieldTree`], so a
/// `[@ParentName="X"]` head evaluated with `predicate_matches` would read
/// an empty `root.attrs` and silently answer `false` for every def on the
/// install — turning the single largest skip shape into a silent no-op
/// instead of a fix. Its `Err` (an `@Abstract`/`@Inherit` head, a
/// `Position` on the def node) stays an `Unsupported` refusal, never a
/// "doesn't match".
///
/// **A bare-type head is gated on [`ReplayContext::this_def_present`]**:
/// `Defs/ThingDef/race/...` names no def at all, so without the
/// gate it would match `verify_order`'s zero-owner synthetic placeholder,
/// whose whole premise is that the def does *not* exist.
///
/// **Shares its outcome path with Group B (`resolve_head_content_predicate`),
/// deliberately**: a filter head resolved here never predicts a failure,
/// however empty its selection, because a global query that selects
/// nothing *on this def* says nothing about whether it selected something
/// elsewhere -- and even for the shapes that are indexed (`@ParentName`
/// via `analysis::edges::parent_name_targets`), over-prediction is
/// already verify's main precision problem, so opening a new prediction
/// source here would make that precision impossible to measure. The
/// suppressed count is reported ([`ReplayOutcome::suppressed_filter_head_ops`])
/// and is the number that decides whether to lift this. Group B used to
/// keep a separate, *predicting* behaviour instead — that produced a
/// real-install false positive, since Group B's own head is exactly as
/// much a whole-document query as any of these, so it now takes this
/// same path.
fn resolve_head_filter(
    query: xpath_expr::HeadFilterQuery,
    tree: &FieldTree,
    context: &ReplayContext<'_>,
) -> Result<XPathResolution, String> {
    let is_this_def = query.def_type == context.def_type
        && match &query.predicate {
            None => context.this_def_present,
            Some(predicate) => root_predicate_matches(tree, predicate)?,
        };
    Ok(XPathResolution::Def(ResolvedXPath {
        targets: Vec::new(),
        root_predicates: query.root_predicates,
        steps: query.steps,
        selects_text: query.selects_text,
        is_this_def,
        non_predicting: true,
    }))
}

/// Whether every predicate on the def node itself holds. `Err` when one
/// of them can't be answered from a single def's tree at all — refusing
/// beats guessing, since a wrong answer silently applies (or silently
/// skips) a real mutation.
fn root_predicates_match(tree: &FieldTree, predicates: &[Predicate]) -> Result<bool, String> {
    for predicate in predicates {
        if !root_predicate_matches(tree, predicate)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn root_predicate_matches(tree: &FieldTree, predicate: &Predicate) -> Result<bool, String> {
    match predicate {
        Predicate::And(left, right) => {
            Ok(root_predicate_matches(tree, left)? && root_predicate_matches(tree, right)?)
        }
        Predicate::Or(left, right) => {
            Ok(root_predicate_matches(tree, left)? || root_predicate_matches(tree, right)?)
        }
        Predicate::Attr(name, value) => match name.as_str() {
            // `crate::xml::parse` lifts these two off the root element
            // into the tree's own fields (see [`FieldTree`]'s doc
            // comment), so they aren't in `root.attrs` to look up.
            "Name" => Ok(tree.name.as_deref() == Some(value.as_str())),
            "ParentName" => Ok(tree.parent_name.as_deref() == Some(value.as_str())),
            // These two it discards outright — nothing here can answer a
            // predicate on them.
            "Abstract" | "Inherit" => Err(format!(
                "a root predicate on '@{name}' can't be evaluated: the attribute isn't kept"
            )),
            _ => Ok(tree
                .root
                .attrs
                .get(name)
                .is_some_and(|actual| actual == value)),
        },
        // "the Nth <ThingDef> of the document" — a replay only ever sees
        // one def, never the document it came from.
        Predicate::Position(_) => {
            Err("a position predicate on the def node needs the whole document".to_string())
        }
        // A negation still applies to the **def node**, so it has to stay in
        // this evaluator rather than fall through to `predicate_matches` with
        // the rest. Delegated, `not(@ParentName="X")` would read an empty
        // `root.attrs`, answer "the attribute isn't there", and so **match
        // every def on the install** — the exact inversion of the shape it is
        // meant to exclude, and a common shape on real installs. Recursing
        // also keeps `@Abstract`/`@Inherit`'s `Err` an `Unsupported` refusal
        // when it is wrapped in a `not(...)`, instead of quietly becoming
        // `true`.
        Predicate::Not(inner) => Ok(!root_predicate_matches(tree, inner)?),
        // Everything left applies to the root node's own children or
        // text, where `root.attrs` is not consulted at all and
        // `predicate_matches` is exactly right: `ChildText`,
        // `NestedChildText`, `Text`, `Contains`, `Has`, and `Child`
        // (whose inner predicate is evaluated against a *child*, which
        // never had a `Name`/`ParentName` lifted off it).
        Predicate::ChildText(..)
        | Predicate::NestedChildText(..)
        | Predicate::Text(_)
        | Predicate::Contains(_)
        | Predicate::Has(_)
        | Predicate::Child(..) => Ok(predicate_matches(&tree.root, 0, predicate)),
    }
}

/// Every node `resolved` matches in `tree`: empty when a root predicate
/// rules the def out (RimWorld's own "this op matched nothing" outcome),
/// and — for a `text()` xpath — narrowed to the elements that actually
/// have text content, since `text()` selects an empty node set otherwise.
///
/// **A bare def-head xpath (`resolved.steps` empty — no sub-path at all)
/// against `!context.this_def_present` is forced empty here, before `select`
/// ever runs**: `select`'s own base case is `vec![FieldPath::new(vec![])]` —
/// the root path itself — so with zero steps to narrow it, it always
/// "matches" the tree's own always-present root node. That's correct for
/// a real def (a bare `Defs/ThingDef[defName="X"]` existence test
/// against a real, owned tree genuinely should match), but wrong for the
/// zero-owner synthetic placeholder `verify_order`'s own fast path
/// replays against: that tree's root exists as a data structure, but the
/// whole point of the placeholder is that the def it stands in for does
/// not. Left unguarded, `condition_matches`/`resolve_selection` would
/// answer "this def exists" for exactly the case whose entire premise is
/// that it doesn't — inverting every `Conditional`/`Test` on a bare def
/// head (a `<match>` add would be wrongly predicted `DeadTarget`; a bare
/// `Test` would wrongly report success with no finding at all; a
/// `Sequence` wrapping one of either would have its own
/// `lastFailedOperation` identity name the wrong child, breaking the
/// log-grep contract `operation` exists for).
fn matched_paths(
    tree: &FieldTree,
    resolved: &ResolvedXPath,
    context: &ReplayContext<'_>,
) -> Result<Vec<FieldPath>, String> {
    if !root_predicates_match(tree, &resolved.root_predicates)? {
        return Ok(Vec::new());
    }
    if resolved.steps.is_empty() && !context.this_def_present {
        return Ok(Vec::new());
    }
    let mut matched = select(tree, &resolved.steps);
    if resolved.selects_text {
        matched.retain(|path| {
            matches!(
                tree.get(path).map(|node| &node.content),
                Some(Content::Text(_))
            )
        });
    }
    Ok(matched)
}

/// A `PatchOperationConditional`/`Test` whose xpath is a bare existence
/// test on *another* def (`Defs/ThingDef[defName="X"]` — no steps, no
/// root predicates, no `text()`): answerable from the caller's own def
/// index through [`ReplayContext::def_exists`]. `None` for every other
/// shape, and whenever the callback doesn't know.
///
/// `resolved.targets.is_empty()` refuses too (Group B) — impossible for
/// an ordinary name-identifying
/// head (`defName=`/`@Name=` always names at least one def), so it only
/// ever fires for [`resolve_head_content_predicate`]'s own fallback: a
/// content predicate that didn't match *this* def either, about which
/// this module genuinely knows nothing regarding any other def (no
/// cross-mod content index reaches here by design). Answering `false`
/// there would be a guess dressed up as a fact — this must stay
/// [`None`] (forcing `Unsupported`, or the conservative branch walk)
/// rather than silently asserting the condition doesn't hold.
fn cross_def_existence(resolved: &ResolvedXPath, context: &ReplayContext<'_>) -> Option<bool> {
    if !resolved.steps.is_empty()
        || !resolved.root_predicates.is_empty()
        || resolved.selects_text
        || resolved.targets.is_empty()
    {
        return None;
    }
    let mut any_exists = false;
    for target in &resolved.targets {
        // The callback answers about concrete defs; a `[@Name="X"]`
        // template lives in a different index entirely.
        if target.selector != Selector::DefName {
            return None;
        }
        if (context.def_exists)(&target.def_type, &target.def_name)? {
            any_exists = true;
        }
    }
    Some(any_exists)
}

/// Evaluates a `PatchOperationConditional`/`PatchOperationTest`'s
/// `<xpath>` against the current tree — or, when it names another def
/// entirely, against the caller's def index. The bare document root
/// (`/Defs`) always exists, so a Test/Conditional on it always matches.
pub(super) fn condition_matches(
    node: Node,
    tree: &FieldTree,
    context: &ReplayContext<'_>,
    class: &str,
) -> Result<bool, ReplayError> {
    let xpath_text = child_text(node, "xpath")
        .ok_or_else(|| unsupported("", format!("{class} missing <xpath>")))?;
    let resolved = match resolve_xpath(&xpath_text, tree, context)
        .map_err(|reason| unsupported(&xpath_text, reason))?
    {
        XPathResolution::DocumentRoot => return Ok(true),
        XPathResolution::Elsewhere => {
            return Err(unsupported(
                &xpath_text,
                format!("{class}'s xpath targets a different def"),
            ));
        }
        XPathResolution::Def(resolved) => resolved,
    };
    if resolved.is_this_def {
        let matched = matched_paths(tree, &resolved, context)
            .map_err(|reason| unsupported(&xpath_text, reason))?;
        // The one place the filter-head rule cannot be expressed as
        // "succeeded elsewhere": a
        // condition is a claim about the **whole document**, so an empty
        // selection under a global filter head is not `false` — it is
        // unanswerable from one def's tree, exactly like
        // `cross_def_existence`'s own refusal below. Answering `false`
        // would take the `<nomatch>` branch (real, silently wrong merge
        // output) and would make a `PatchOperationTest` report
        // `succeeded: false`, i.e. a prediction — which is precisely what
        // `resolve_selection` refuses to emit for the same head.
        // A non-empty selection *is* decisive: the node exists, so the
        // condition holds however many other defs the head also names.
        if matched.is_empty() && resolved.non_predicting {
            return Err(unsupported(
                &xpath_text,
                format!("{class}'s xpath is a global filter head that selects nothing on this def"),
            ));
        }
        return Ok(!matched.is_empty());
    }
    cross_def_existence(&resolved, context).ok_or_else(|| {
        unsupported(
            &xpath_text,
            format!("{class}'s xpath targets a different def"),
        )
    })
}

/// Every matched path, ordered so that mutating them **in this order**
/// (i.e. iterating `.rev()`) never invalidates a not-yet-processed
/// match's own [`ItemId::Position`] identity: removing or inserting
/// siblings at a higher index never shifts a lower one.
pub(super) fn matched_highest_index_first(mut matched: Vec<FieldPath>) -> Vec<FieldPath> {
    matched.reverse();
    matched
}

/// What resolving one mutating operation's `<xpath>` produced.
pub(super) enum Selected {
    /// Nothing to apply: the operation's result is this boolean. `true`
    /// when the xpath aims at another def (not this replay's to run —
    /// "succeeded elsewhere"), `false` when it matched no node here (a
    /// [`Caveat::FailedOp`] has already been recorded).
    Done(bool),
    /// The element nodes it matched.
    Elements(Vec<FieldPath>),
    /// The nodes whose *text content* it matched (`.../texPath/text()`).
    TextOf(Vec<FieldPath>),
}

/// Operation classes that only ever *add new children* to whatever their
/// xpath matches (suffix-matched, like every other class in this module).
/// Aimed at the bare document root (`/Defs`), one of these can only be
/// adding a whole new top-level def — it can never touch an *existing*
/// def's own node, so it is simply not this replay's to run: the same
/// "succeeded elsewhere" outcome as an xpath naming a different def
/// outright.
pub(super) const DOCUMENT_ROOT_ADDITIONS: [&str; 3] = [
    "PatchOperationAdd",
    "PatchOperationInsert",
    "PatchOperationAddModExtension",
];

/// [`resolve_selection`]'s handling for a document-root (`/Defs`) xpath:
/// an addition "succeeds elsewhere" (see [`DOCUMENT_ROOT_ADDITIONS`]);
/// anything else (`Replace`/`Remove`/`SetName`/an attribute op) would need
/// to rewrite the whole `<Defs>` document to express and stays
/// `Unsupported`, naming the class, rather than silently no-op'ing.
fn document_root_selection(class: &str, xpath_text: &str) -> Result<Selected, ReplayError> {
    if DOCUMENT_ROOT_ADDITIONS
        .iter()
        .any(|suffix| class.ends_with(suffix))
    {
        return Ok(Selected::Done(true));
    }
    Err(unsupported(
        xpath_text,
        format!("{class} on the document root ('/Defs') needs the whole <Defs> document"),
    ))
}

/// The shared prelude of every mutating operation: parse the `<xpath>`,
/// skip it when it aims at another def (or, for an addition, at the bare
/// document root — see [`document_root_selection`]), and record a
/// [`Caveat::FailedOp`] when it matches nothing (which RimWorld treats as
/// a failed op too).
pub(super) fn resolve_selection(
    tree: &FieldTree,
    context: &ReplayContext<'_>,
    xpath_text: &str,
    class: &str,
    log: &mut ReplayLog,
    mod_id: &ModId,
) -> Result<Selected, ReplayError> {
    let resolved = match resolve_xpath(xpath_text, tree, context)
        .map_err(|reason| unsupported(xpath_text, reason))?
    {
        XPathResolution::DocumentRoot => return document_root_selection(class, xpath_text),
        XPathResolution::Elsewhere => return Ok(Selected::Done(true)),
        XPathResolution::Def(resolved) => resolved,
    };
    if !resolved.is_this_def {
        return Ok(Selected::Done(true));
    }
    let matched = matched_paths(tree, &resolved, context)
        .map_err(|reason| unsupported(xpath_text, reason))?;
    if matched.is_empty() {
        // A *filter* head is a global query, so
        // "selected nothing here" is not evidence the operation failed.
        // Reported as "succeeded elsewhere", with no caveat, and counted.
        if resolved.non_predicting {
            log.suppressed_filter_head_ops += 1;
            return Ok(Selected::Done(true));
        }
        log.caveats.push(Caveat::FailedOp {
            mod_id: mod_id.clone(),
            xpath: xpath_text.to_string(),
        });
        return Ok(Selected::Done(false));
    }
    Ok(if resolved.selects_text {
        Selected::TextOf(matched)
    } else {
        Selected::Elements(matched)
    })
}

/// [`resolve_selection`] for the operations that only model *element*
/// targets — everything but `Replace`/`Remove`: a `text()` target on any
/// of them is `Unsupported`, never silently applied to the element instead.
pub(super) fn resolve_element_selection(
    tree: &FieldTree,
    context: &ReplayContext<'_>,
    xpath_text: &str,
    class: &str,
    log: &mut ReplayLog,
    mod_id: &ModId,
) -> Result<Selected, ReplayError> {
    match resolve_selection(tree, context, xpath_text, class, log, mod_id)? {
        Selected::TextOf(_) => Err(unsupported(
            xpath_text,
            format!("{class} on a text() node is not modelled"),
        )),
        other => Ok(other),
    }
}

/// Whether `matched` contains the def root itself (a matched path with no
/// segments) — the trigger both [`reject_root_target`] and
/// [`apply_remove`]'s own whole-def-removal branch key off.
pub(super) fn matches_def_root(matched: &[FieldPath]) -> bool {
    matched.iter().any(|path| path.segments().is_empty())
}

/// Refuses a matched set containing the def root itself, for the
/// operations that can't express what RimWorld would do to it: replacing,
/// inserting a sibling of, or renaming the whole def needs the enclosing
/// `<Defs>` document, which a replay never has. Silently no-op'ing (what
/// the path-addressed mutators below would otherwise do) would report a
/// wrong outcome as a real one. **`PatchOperationRemove` is the one
/// exception** — see [`apply_remove`]'s own whole-def branch, which handles a
/// def-root match before this is ever reached for that class.
pub(super) fn reject_root_target(
    matched: &[FieldPath],
    xpath_text: &str,
    class: &str,
) -> Result<(), ReplayError> {
    if matches_def_root(matched) {
        return Err(unsupported(
            xpath_text,
            format!("{class} on the def node itself needs the whole <Defs> document"),
        ));
    }
    Ok(())
}

fn predicate_matches(node: &FieldNode, position: u32, predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Attr(name, value) => node.attrs.get(name).is_some_and(|actual| actual == value),
        Predicate::ChildText(name, value) => child_text_matches(node, name, value),
        Predicate::NestedChildText(outer, inner, value) => {
            nested_child_text_matches(node, outer, inner, value)
        }
        Predicate::Text(value) => matches!(&node.content, Content::Text(text) if text == value),
        Predicate::Not(inner) => !predicate_matches(node, position, inner),
        // `name[pred]` -- some child *named* `name`
        // satisfies `pred`. `position` restarts among the same-tag
        // siblings, so a nested `li[2]` keeps XPath's own "second `li`"
        // meaning rather than counting every sibling of every tag.
        Predicate::Child(name, inner) => match &node.content {
            Content::Children(children) => children
                .iter()
                .filter(|child| child.tag == *name)
                .enumerate()
                .any(|(index, child)| {
                    predicate_matches(child, u32::try_from(index).unwrap_or(u32::MAX), inner)
                }),
            Content::Empty | Content::Text(_) => false,
        },
        Predicate::Contains(value) => {
            matches!(&node.content, Content::Text(text) if text.contains(value.as_str()))
        }
        Predicate::Has(name) => has_named_child(node, name),
        Predicate::Position(target) => position + 1 == *target,
        Predicate::And(a, b) => {
            predicate_matches(node, position, a) && predicate_matches(node, position, b)
        }
        Predicate::Or(a, b) => {
            predicate_matches(node, position, a) || predicate_matches(node, position, b)
        }
    }
}

fn child_text_matches(node: &FieldNode, name: &str, value: &str) -> bool {
    let Content::Children(children) = &node.content else {
        return false;
    };
    children.iter().any(|child| {
        child.tag == name && matches!(&child.content, Content::Text(text) if text == value)
    })
}

/// Whether any `outer` child of `node` has an `inner` child whose text equals
/// `value` — RimWorld's own list-membership idiom (`things/li="Column"`),
/// tested against every matching `outer` (a def can carry more than one
/// same-tag child in principle; `child_text_matches`' own `.any` over
/// siblings is the precedent for not assuming exactly one).
fn nested_child_text_matches(node: &FieldNode, outer: &str, inner: &str, value: &str) -> bool {
    let Content::Children(children) = &node.content else {
        return false;
    };
    children
        .iter()
        .filter(|child| child.tag == outer)
        .any(|child| child_text_matches(child, inner, value))
}

fn has_named_child(node: &FieldNode, name: &str) -> bool {
    matches!(&node.content, Content::Children(children) if children.iter().any(|child| child.tag == name))
}

/// Selects every node reachable from `tree.root` matching `steps`
/// (the xpath grammar, already parsed), returning their addresses.
/// Addresses (not references) so the caller can mutate the tree by path
/// afterward without fighting the borrow checker. `li` steps use
/// [`identify_all_li`] — the same duplicate-identity-aware heuristic
/// [`crate::tree`] itself uses — so a path this produces always resolves
/// back to the same node via [`FieldTree::get`].
fn select(tree: &FieldTree, steps: &[Step]) -> Vec<FieldPath> {
    let mut candidates = vec![FieldPath::new(vec![])];
    for step in steps {
        let mut next = Vec::new();
        for candidate in &candidates {
            let Some(node) = tree.get(candidate) else {
                continue;
            };
            let Content::Children(children) = &node.content else {
                continue;
            };
            if step.name == "li" {
                let identities = identify_all_li(children);
                for (position, (index, item_id)) in identities.iter().enumerate() {
                    let child = &children[*index];
                    if step
                        .predicates
                        .iter()
                        .all(|predicate| predicate_matches(child, position as u32, predicate))
                    {
                        let mut segments = candidate.segments().to_vec();
                        segments.push(PathSegment::Item(item_id.clone()));
                        next.push(FieldPath::new(segments));
                    }
                }
            } else {
                let mut position = 0u32;
                for child in children {
                    if child.tag != step.name {
                        continue;
                    }
                    if step
                        .predicates
                        .iter()
                        .all(|predicate| predicate_matches(child, position, predicate))
                    {
                        let mut segments = candidate.segments().to_vec();
                        segments.push(PathSegment::Child(step.name.clone()));
                        next.push(FieldPath::new(segments));
                    }
                    position += 1;
                }
            }
        }
        candidates = next;
    }
    candidates
}
