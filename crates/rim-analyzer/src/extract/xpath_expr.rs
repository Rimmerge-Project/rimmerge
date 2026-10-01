//! An XPath subset expressive enough to *replay* a patch operation
//! against a def's field tree — not merely to find which def a patch
//! targets (that's `xpath_target`, whose head location and
//! [`DefTarget`] this module reuses; the *names* in the head, though,
//! this module re-derives from its own strictly parsed predicate tree, so
//! a replay never rests on that module's deliberately loose regex scan).
//!
//! Grammar (whitespace-tolerant, either quote style):
//!
//! ```text
//! expr     := head root_pred* ('/' step)* ('/' 'text()')?
//! head     := one or more `defName=str` / `@Name=str` equalities joined
//!             by `or` — nothing else, and no `and` (see
//!             xpath_target::locate_head for the accepted surrounding
//!             forms: `/Defs/`, `*/`, `//`)
//! root_pred := '[' pred ']'                 // applies to the def node itself
//! step     := name ('[' pred ']')*          // name := [A-Za-z_][\w.-]*, or 'li'
//! pred     := '@' name '=' str              // attribute equality
//!           | name '=' str                  // child text equality
//!           | name '/' name '=' str         // grandchild text equality
//!           | 'text()' '=' str
//!           | 'contains(' 'text()' ',' str ')'  // text contains literal
//!           | 'not(' pred ')'               // negation (`not(comps)`, `not(li[@Class="X"])`)
//!           | name ('[' pred ']')+          // some child named `name` satisfies the predicates
//!           | name                          // child present
//!           | integer                       // 1-based position (`li[2]`)
//!           | 'position()' '=' integer      // explicit 1-based position (`li[position()=2]`)
//!           | pred ' and ' pred | pred ' or ' pred | '(' pred ')'
//! ```
//!
//! `and`/`or` may also follow a closing quote with no space
//! (`defName="A"or defName="B"`), which XPath 1.0 allows because a string
//! literal is self-delimiting; the *trailing* side still requires
//! whitespace, so an identifier merely ending in the operator (`nor`)
//! never splits.
//!
//! `and` binds tighter than `or` (standard XPath precedence): `A or B and
//! C` parses as `Or(A, And(B, C))`, not `And(Or(A, B), C)`.
//!
//! A head naming several defs (`[defName="A" or defName="B"]`) is
//! [`XPathExpr::Supported`] with one [`DefTarget`] per name: RimWorld
//! applies such an op to every matching def, so a replay of any one of
//! them must run it.
//!
//! Anything outside this grammar parses to [`XPathExpr::Unsupported`]
//! naming the offending token, never a partial, silently-wrong replay:
//! `..`, `//`/`*` after the head, `|` (union), any function other than
//! `not`/`text`/`position`/`contains(text(), …)` (`last()` included, and
//! every other `contains` form), a head predicate mixing
//! def names with anything else (an `and`-joined term, a `not(...)`, a
//! non-identifying equality — `xpath_target`'s looser head match drops
//! those silently, which is fine for collision detection but wrong for
//! replay), attribute predicates other than equality, `text()` anywhere
//! but as the final step, a `li[0]` or `li[position()=0]` position (XPath
//! positions are 1-based), any `position()` comparison other than a bare
//! `=` against an unquoted integer (`position()>1`, `position()!=2`,
//! `position()=last()`), and a predicate tree deeper than
//! `MAX_PREDICATE_DEPTH` (a pathological input must fail cleanly, not
//! overflow the stack).
//!
//! Structural tokens (`..`, `//`, `|`, `and`, `or`) are recognized only
//! outside quoted string literals, so `label="Fish and Chips"`,
//! `label="a|b"`, `label="http://x"`, and `@Class="A..B"` all parse as
//! ordinary equality predicates rather than tripping the corresponding
//! rejection.

use std::collections::BTreeSet;

use crate::domain::DefTarget;

use crate::extract::xpath_target;
use lex::contains_outside_quotes;
use parse::{locate_bare_type_head, parse_after_head, parse_supported};
use predicates::parse_predicate;

mod lex;
mod parse;
mod predicates;

// Sibling-file tests; `#[path]` keeps the module named `tests`.
#[cfg(test)]
#[path = "xpath_expr/xpath_expr_tests.rs"]
mod tests;

/// Budget for [`parse_boolean`]'s own recursion — a predicate nested or
/// chained past this fails with a clean [`XPathExpr::Unsupported`]
/// instead of a stack overflow.
///
/// **Set by measurement rather than a guess.** [`split_boolean`] splits a
/// flat `A or B or C …` chain at its first top-level operator, so one level
/// is spent per *term*, and the real install carries a `defName="…" or …`
/// head with **103** of them (a ship-building mod) plus a 72-term one (a
/// genetics-style mod).
///
/// **The budget is not one unit per level, because the levels do not cost the
/// same** (measured, not reasoned about: each shape was bisected to
/// destruction on a thread with a **1 MiB** stack in a **debug** build, the
/// most pessimistic configuration this code runs in). Deepest surviving
/// nesting:
///
/// | shape | survives to |
/// | --- | --- |
/// | flat `or` / `and` chain | 477 / 475 |
/// | nested parens | 473 |
/// | nested `not(not(…))` | 156 |
/// | nested child filters `a[a[a[…]]]` | **96** |
/// | relative path `a/b/c/…` | 6,343 |
///
/// A bracket filter re-enters `parse_boolean` through
/// [`parse_path_segment`] and [`extract_brackets`], so it burns ~5x the
/// stack of an `or` split; `not(…)` ~3x. Charging each construct its own
/// [`FILTER_DEPTH_COST`]/[`NOT_DEPTH_COST`] lets one budget bound every
/// shape at once instead of forcing the cap down to the worst one (which
/// would be below the 103 real terms the grammar has to accept).
/// [`a_pathological_predicate_of_every_shape_fails_cleanly_on_a_small_stack`]
/// is the proof, and it runs on that same 1 MiB stack rather than
/// trusting the arithmetic.
const MAX_PREDICATE_DEPTH: u32 = 256;

/// What one level of bracket filter (`a[…]`) costs against
/// [`MAX_PREDICATE_DEPTH`] — 32 levels, against a measured ceiling of 96.
/// The deepest real one on this install is 2 (`comps[li[@Class="X"]]`).
const FILTER_DEPTH_COST: u32 = 8;

/// What one level of `not(…)` costs against [`MAX_PREDICATE_DEPTH`] — 64
/// levels, against a measured ceiling of 156. Real heads nest it once.
const NOT_DEPTH_COST: u32 = 4;

/// One patch xpath, parsed either into a replayable target-plus-steps
/// shape or into the reason it can't be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XPathExpr {
    /// The xpath's head named one or more defs, and everything after it
    /// (if anything) parsed into replayable predicates and `steps`.
    Supported {
        /// Every def this xpath targets — more than one when the head is
        /// a `defName="A" or defName="B"` disjunction. Never empty.
        targets: Vec<DefTarget>,
        /// Extra `[...]` predicates applying to the def node itself
        /// (`[defName="X"][not(comps)]`), evaluated against the def root
        /// before any step is walked.
        root_predicates: Vec<Predicate>,
        /// Every `/`-separated step after the head, in source order.
        steps: Vec<Step>,
        /// Whether the xpath ended in a `text()` step — the target is the
        /// selected element's *text content*, not the element itself.
        selects_text: bool,
    },
    /// The bare document root (`/Defs`, `Defs`, `/Defs/`, whitespace
    /// tolerant) — no def name at all, so there is nothing for
    /// `super::xpath_target::locate_head` to find. Distinct from
    /// [`XPathExpr::Unsupported`] on purpose: a mod adds a whole new
    /// top-level def with `<xpath>/Defs</xpath>`, which can never affect any
    /// *existing* def's node, so a per-def replay (`rim_merge::patch_eval`)
    /// needs to tell that shape apart from a head it genuinely can't parse.
    DocumentRoot,
    /// Something in the xpath falls outside the grammar this module
    /// supports — `reason` names the offending token or shape.
    Unsupported {
        /// Human-readable, naming the specific unsupported token/shape.
        reason: String,
    },
}

/// One `/`-separated path segment after the head: a name, with zero or
/// more bracketed predicates narrowing which node(s) it matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The element name this step matches (or `li` for a list item).
    pub name: String,
    /// Every `[...]` predicate narrowing this step, in source order.
    pub predicates: Vec<Predicate>,
}

/// One `[...]` predicate on a [`Step`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Predicate {
    /// `@name="value"`.
    Attr(String, String),
    /// `name="value"` — a child element's text.
    ChildText(String, String),
    /// `outer/inner="value"` — a *grandchild's* text: `things/li="Column"`
    /// tests whether any `<li>` inside `<things>` has the text `Column` —
    /// RimWorld's own list-membership idiom.
    ///
    /// **Scoped to exactly this two-step shape, but not because anything
    /// deeper is refused**: `parse_relative_path` models a longer chain
    /// (`a/b/c="x"`) exactly, as nested [`Predicate::Child`]s. This variant
    /// is kept, and its arm in `parse_atom` still runs *first*, because
    /// `analysis::edges::child_value_targets` and `extract::defs`'
    /// `child_value_hashes` pattern-match it to build an indexed, scan-time
    /// fact — re-shaping that for tidiness would be churn, not progress. So
    /// the two-segment shape parses to this and everything deeper parses to
    /// `Child`.
    NestedChildText(String, String, String),
    /// `text()="value"`.
    Text(String),
    /// `not(pred)` — the inner predicate does **not** hold. `not(comps)` is
    /// `Not(Has("comps"))`. The argument is any predicate this grammar can
    /// express, not only a bare child name, which is what makes
    /// `comps[not(li[@Class="X"])]` — 31 real rows, and the operation
    /// standing between `example.progression.kitchen` and
    /// `example.progression.production` — replayable at all.
    Not(Box<Predicate>),
    /// `name[pred]` — some child element named `name` satisfies `pred`.
    /// Models `li[text()="X"]`, `comps[li[@Class="X"]]` and
    /// `thingClass[contains(text(), "X")]`, at arbitrary depth. Evaluation
    /// numbers `position()` **among the same-tag siblings**, so `li[2]` keeps
    /// XPath's own 1-based "second `li`" meaning.
    ///
    /// [`Predicate::ChildText`]/[`Predicate::NestedChildText`] are *not*
    /// re-expressed through this variant even though it subsumes them:
    /// `analysis::edges::child_value_targets` and `extract::defs`'
    /// `child_value_hashes` pattern-match those two shapes to build a
    /// scan-time, indexed fact, and re-shaping a shipped fact for tidiness
    /// would be churn, not progress.
    Child(String, Box<Predicate>),
    /// `contains(text(), "value")` — the node's own text contains
    /// `value`. Deliberately the *only* `contains` shape in this grammar:
    /// `contains(@attr, …)`, `contains(name, …)` and every other function
    /// stay [`XPathExpr::Unsupported`] (see `parse_atom`'s own
    /// rejection loop, which this is checked before).
    Contains(String),
    /// A bare `name` — the named child is present.
    Has(String),
    /// A bare integer, or the explicit `position()=N` form — 1-based
    /// position among matched siblings (`li[2]` and `li[position()=2]`
    /// both mean the *second* item, matching XPath's own convention;
    /// `li[0]`/`li[position()=0]` are rejected rather than parsing to this
    /// variant, and any other `position()`/`last()` comparison stays
    /// `Unsupported`).
    Position(u32),
    /// `left and right` — binds tighter than [`Predicate::Or`].
    And(Box<Predicate>, Box<Predicate>),
    /// `left or right` — binds looser than [`Predicate::And`].
    Or(Box<Predicate>, Box<Predicate>),
}

/// Parses one patch `<xpath>` value into the module-level grammar.
#[must_use]
pub fn parse(xpath: &str) -> XPathExpr {
    match parse_supported(xpath) {
        Ok(expr) => expr,
        Err(reason) => XPathExpr::Unsupported { reason },
    }
}

/// Substrings whose presence anywhere outside a quoted literal marks
/// `xpath`'s shape as slow to evaluate — the vocabulary real slow-shape
/// xpaths were measured against: descendant-or-self (`//`), and the
/// `contains`/`starts-with`/`count`/`last` functions. `descendant::` and a
/// bare `*` step are checked separately below, not substring-matched here,
/// since neither is a fixed function-call token.
const SLOW_SHAPE_SUBSTRINGS: [&str; 5] = ["//", "contains(", "starts-with(", "count(", "last("];

/// Whether `xpath`'s literal shape is one RimWorld evaluates slowly:
/// `//` (descendant-or-self), `contains()`/`starts-with()`/`count()`/
/// `last()`, an explicit `descendant::` axis, or a bare `*` step (`/*`,
/// or a leading `*` step). A cheap string-shape test over the raw xpath
/// text, deliberately not a parse: most of these shapes are exactly the
/// ones [`parse`] rejects as [`XPathExpr::Unsupported`], so classifying
/// off the parsed [`XPathExpr`] would miss most of what this is meant to
/// find. [`contains_outside_quotes`] keeps a literal attribute/text value
/// that merely contains one of these substrings (`label="a//b"`) from
/// false-positiving.
#[must_use]
pub fn is_slow_shape(xpath: &str) -> bool {
    SLOW_SHAPE_SUBSTRINGS
        .iter()
        .any(|needle| contains_outside_quotes(xpath, needle))
        || contains_outside_quotes(xpath, "descendant::")
        || contains_outside_quotes(xpath, "/*")
        || xpath.trim_start().starts_with('*')
}

/// Every `[@Class="T"]`/`[name="T"]` predicate value naming a
/// namespace-qualified type (`T` contains a `.`) anywhere in `xpath`'s root
/// predicates or step predicates: the classes this xpath's selector actually
/// chooses among, read off the same [`Predicate`] tree [`parse`] already
/// builds for replay rather than a second, looser scan. Empty when `xpath`
/// doesn't parse into [`XPathExpr::Supported`] at all — a selector this
/// grammar can't model exactly contributes no evidence rather than a guess.
///
/// **The dot-requirement is deliberate**: a bare, non-namespace-qualified
/// value (`RoomRequirement_ExampleAnyOfCount`, the real Example Doors/Example
/// Core shape) is not accepted, since widening to it does not make that
/// case's edge appear.
#[must_use]
pub fn selected_classes(xpath: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    if let XPathExpr::Supported {
        root_predicates,
        steps,
        ..
    } = parse(xpath)
    {
        for predicate in &root_predicates {
            collect_selected_classes(predicate, &mut out);
        }
        for step in &steps {
            for predicate in &step.predicates {
                collect_selected_classes(predicate, &mut out);
            }
        }
    }
    out
}

fn collect_selected_classes(predicate: &Predicate, out: &mut BTreeSet<String>) {
    match predicate {
        Predicate::Attr(name, value) if name == "Class" && value.contains('.') => {
            out.insert(value.clone());
        }
        Predicate::ChildText(name, value) if name == "name" && value.contains('.') => {
            out.insert(value.clone());
        }
        Predicate::And(left, right) | Predicate::Or(left, right) => {
            collect_selected_classes(left, out);
            collect_selected_classes(right, out);
        }
        // A positive test on a descendant still *selects* by that class.
        Predicate::Child(_, inner) => collect_selected_classes(inner, out),
        // `not(...)` deliberately does **not** recurse: the classes under
        // a negation are the ones the selector excludes, not the ones it
        // chooses among, and this function feeds `PatchInjectedNode`
        // edges ("this op selects a node some other mod injects"), where
        // an excluded class is evidence of the opposite.
        Predicate::Not(_)
        | Predicate::Attr(..)
        | Predicate::ChildText(..)
        | Predicate::NestedChildText(..)
        | Predicate::Text(_)
        | Predicate::Contains(_)
        | Predicate::Has(_)
        | Predicate::Position(_) => {}
    }
}

/// Whether `xpath` is nothing but the document root: `Defs`, optionally
/// wrapped in one leading and/or one trailing `/`, surrounding whitespace
/// tolerated. Checked before [`xpath_target::locate_head`] — `/Defs` has no
/// `[...]` predicate for that head scan to find, so without this check it
/// would fall all the way through to "no recognized head", indistinguishable
/// from a genuinely malformed xpath.
fn is_document_root(xpath: &str) -> bool {
    matches!(xpath.trim(), "Defs" | "/Defs" | "/Defs/")
}

/// One `(path segments, literal value)` alternative
/// [`head_content_predicate`] `or`-combines into a [`HeadContentQuery`] —
/// `(["race", "intelligence"], "Humanlike")` for the two-step shape,
/// `(["thingDef"], "Column")` for the single-step one.
pub type ContentAlternative = (Vec<String>, String);

/// [`head_content_predicate`]'s own result: the def type a
/// child-element-value head names, every `or`-combined `(path, value)`
/// alternative it tests, the raw sub-path (if any) following the head, and —
/// since the patch replay needs them too, not just an indexer — the parsed
/// root predicates/steps/`text()` flag exactly as [`XPathExpr::Supported`]
/// carries them. Not yet resolved against any content:
/// `analysis::edges::child_value_targets` answers "which defs" from
/// [`Self::alternatives`] alone (indexing, cross-mod); [`Self::as_predicate`]
/// plus a real def's own tree answers "does *this* def match" (replay,
/// single-def, no index needed at all — see that method's own doc comment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadContentQuery {
    pub def_type: String,
    pub alternatives: Vec<ContentAlternative>,
    pub sub_path: Option<String>,
    pub root_predicates: Vec<Predicate>,
    pub steps: Vec<Step>,
    pub selects_text: bool,
}

impl HeadContentQuery {
    /// Folds [`Self::alternatives`] into the one `Or`-chained [`Predicate`]
    /// tree `parse_predicate` would have built had the *name*-identifying
    /// grammar recognized this shape — the same `ChildText`/
    /// `NestedChildText` leaves a root/step predicate already evaluates
    /// against a real tree via `rim_merge::patch_eval`'s own
    /// `predicate_matches`. Lets a replay caller with an actual def tree
    /// in hand evaluate a head-position content predicate with the
    /// *identical* machinery already used for root predicates, rather
    /// than a second, parallel evaluator.
    ///
    /// `None` only for a [`HeadContentQuery`] with zero alternatives, or
    /// one whose path isn't 1 or 2 segments — [`head_content_predicate`]
    /// never actually produces either (`collect_head_content_predicates`
    /// pushes at least one leaf, always 1 or 2 segments, before any `Ok`
    /// return), but this returns `Option` rather than panicking on that
    /// unreachable case (`unwrap`/`expect` are denied outside tests in
    /// this workspace) — a caller treats `None` the same safe-direction
    /// "cannot evaluate" refusal as everywhere else in this module.
    #[must_use]
    pub fn as_predicate(&self) -> Option<Predicate> {
        self.alternatives
            .iter()
            .map(|(path, value)| Self::leaf_predicate(path, value))
            .collect::<Option<Vec<_>>>()?
            .into_iter()
            .reduce(|acc, leaf| Predicate::Or(Box::new(acc), Box::new(leaf)))
    }

    fn leaf_predicate(path: &[String], value: &str) -> Option<Predicate> {
        match path {
            [name] => Some(Predicate::ChildText(name.clone(), value.to_string())),
            [outer, inner] => Some(Predicate::NestedChildText(
                outer.clone(),
                inner.clone(),
                value.to_string(),
            )),
            _ => None,
        }
    }
}

/// A head predicate naming defs by a **child-element-value** equality —
/// `[race/intelligence="Humanlike"]` ([`Predicate::NestedChildText`]) or the
/// single-step `[thingDef="Column"]` shape ([`Predicate::ChildText`]) —
/// optionally `or`-combined among themselves, in source order. `None` for any
/// other shape: mixed with a `defName`/`@Name` leaf (that's `head_targets`'s
/// own job, never this one's — `ChildText("defName", _)` is explicitly
/// excluded here so the two stay disjoint regardless of call order), an `and`
/// term, or anything richer — this grammar's existing "equality against a
/// literal only" scope, applied in **head** position (a real case:
/// `ExampleRace.ThingDef_ExampleRace[race/intelligence="Humanlike"]`).
///
/// This module has no cross-mod content index at all (see the crate's
/// own top-of-file doc comment) — exactly like
/// [`super::xpath_target::parent_name_predicate`], it only ever locates
/// and reports the query itself; `analysis::edges::child_value_targets`
/// is where it's actually answered, against a real per-mod content
/// index. [`parse`]'s own `targets` field is unaffected by this
/// function's existence — a bare `xpath_expr::parse` call still (and
/// must still) return [`XPathExpr::Unsupported`] for this head shape,
/// since this module alone can never enumerate which defs it names.
#[must_use]
pub fn head_content_predicate(xpath: &str) -> Option<HeadContentQuery> {
    let head = xpath_target::locate_head(xpath)?;
    let predicate = parse_predicate(head.predicate).ok()?;
    let mut alternatives = Vec::new();
    collect_head_content_predicates(&predicate, &mut alternatives).ok()?;

    let after_head = parse_after_head(xpath, head.end).ok()?;

    Some(HeadContentQuery {
        def_type: head.def_type.to_string(),
        alternatives,
        sub_path: after_head.sub_path,
        root_predicates: after_head.root_predicates,
        steps: after_head.steps,
        selects_text: after_head.selects_text,
    })
}

/// Flattens an `or`-only predicate tree into its `(path segments, value)`
/// leaves — [`head_content_predicate`]'s own worker, the content-query
/// mirror of [`collect_head_names`]. `Err(())` for any other shape,
/// including a bare `defName=` leaf (that belongs to [`collect_head_names`]
/// alone, never this one).
fn collect_head_content_predicates(
    predicate: &Predicate,
    out: &mut Vec<ContentAlternative>,
) -> Result<(), ()> {
    match predicate {
        Predicate::Or(left, right) => {
            collect_head_content_predicates(left, out)?;
            collect_head_content_predicates(right, out)
        }
        Predicate::ChildText(name, value) if name != "defName" => {
            out.push((vec![name.clone()], value.clone()));
            Ok(())
        }
        Predicate::NestedChildText(outer, inner, value) => {
            out.push((vec![outer.clone(), inner.clone()], value.clone()));
            Ok(())
        }
        _ => Err(()),
    }
}

/// [`head_filter_predicate`]'s own result: a head this module can parse
/// but can never *enumerate* — `[@ParentName="X"]`, an `and`-composed
/// head, a `not(...)`, a nested child filter, a `contains(text(), …)`,
/// or a **bare-type head** with no `[...]` at all — together with
/// everything after it, exactly as [`XPathExpr::Supported`] carries it.
///
/// The division of labour is the same one [`HeadContentQuery`] already draws,
/// generalized: enumerating which defs a head names needs a cross-mod index
/// this module deliberately never reaches for, but *evaluating* the head
/// against one def already in hand needs nothing at all.
/// `rim_merge::patch_eval` answers [`Self::predicate`] against that def's own
/// tree with the identical `Predicate` machinery a root predicate already
/// uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadFilterQuery {
    /// The def type the head names.
    pub def_type: String,
    /// The head's own `[...]` predicate — `None` for a bare-type head
    /// (`Defs/ThingDef/race/…`, no bracket at all), which selects across
    /// **every** def of that type.
    pub predicate: Option<Predicate>,
    /// Extra `[...]` predicates on the def node itself, after the head's.
    pub root_predicates: Vec<Predicate>,
    /// Every `/`-separated step after the head, in source order.
    pub steps: Vec<Step>,
    /// Whether the xpath ended in a `text()` step.
    pub selects_text: bool,
}

/// A head predicate this grammar parses but cannot turn into a list of
/// def names — everything `head_targets` and
/// [`head_content_predicate`] both reject — plus the bare-type head with
/// no predicate at all. `None` when the xpath has no locatable head, when
/// the head predicate itself is outside the grammar, or when anything
/// after the head is (a caller must never see a half-understood
/// selector: that is the "refuse rather than guess" rule this whole
/// module is built on).
///
/// Deliberately *additive*: [`parse`] still returns
/// [`XPathExpr::Unsupported`] for every shape this accepts, because `parse`'s
/// contract is "which defs does this name", which no amount of head-predicate
/// parsing can answer here. [`head_content_predicate`] is likewise untouched
/// — its [`HeadContentQuery::alternatives`] shape is consumed by
/// `analysis::edges::child_value_targets` for *indexing*, and a caller is
/// expected to try it **first**, so a shipped, measured behaviour is never
/// silently re-routed through this newer, more general path.
#[must_use]
pub fn head_filter_predicate(xpath: &str) -> Option<HeadFilterQuery> {
    if is_document_root(xpath) {
        return None;
    }
    let (def_type, predicate, head_end) = match xpath_target::locate_head(xpath) {
        Some(head) => (
            head.def_type.to_string(),
            Some(parse_predicate(head.predicate).ok()?),
            head.end,
        ),
        None => {
            let (def_type, end) = locate_bare_type_head(xpath)?;
            (def_type.to_string(), None, end)
        }
    };
    let after_head = parse_after_head(xpath, head_end).ok()?;
    Some(HeadFilterQuery {
        def_type,
        predicate,
        root_predicates: after_head.root_predicates,
        steps: after_head.steps,
        selects_text: after_head.selects_text,
    })
}
