# Sorting

The sorter turns every ordering fact from
[load-order.md](load-order.md), plus any rule you've set or imported,
into one suggested load order. This page describes the model; see
`crates/rim-resolve` for the implementation.

## Tiers

Every mod belongs to exactly one tier, in physical load-order sequence:

1. **Core**: vanilla RimWorld.
2. **Dlc**: an official DLC.
3. **Top**: pinned to the top by an explicit placement rule.
4. **Body**: everything else; most mods live here.
5. **Bottom**: pinned to the bottom by an explicit placement rule.

A mod's tier is a floor/ceiling on where it can land, not the whole
answer. Within a tier, every edge below still applies.

## Edge strength

Every ordering fact becomes a graph edge with a strength. From weakest
to strongest, measured by how willingly an edge yields to something
else:

- **Awareness**: evidence two mods interact, no explicit order
  promise. Off by default (advisory only).
- **Soft**: a load-time relationship RimWorld doesn't enforce
  (e.g. a lazily-resolved assembly reference). Off by default.
- **Inferred**: a heuristic the analyzer concludes itself rather than
  reads off an author's declaration (e.g. "this texture-only mod must
  load after the mod whose texture it replaces"). On by default: it's
  the analyzer's own conclusion, not a mere presence signal, so leaving
  it advisory-only would discard the evidence.
- **Declared**: the author wrote this down (`loadAfter`,
  `modDependencies`, ...).
- **Hard**: breaking this breaks the game at load time (a missing
  type, a dropped def from failed inheritance, ...). Always enforced.

Whether `Soft`, `Awareness`, and `Inferred` are each enforced (rather
than only shown as advisory) is its own setting:
`enforce.soft`/`enforce.awareness` (both off by default) and
`enforce.inferred` (on by default, for the reason above). See
[settings.md](../settings.md).

On top of engine-derived edges, a rule (yours, or imported from a
RimSort database; see [rules-databases.md](rules-databases.md)) adds
its own edge at a strength reflecting where it came from. Your own
decision outranks anything imported; an imported rule sits below every
`Declared`/`Hard` engine fact. One exception, opt-in per rule: a pair
rule you mark as explicitly overriding a declared relationship (the
author wrote `loadAfter`, but you know better for your own load order)
is added at its own layer, above every ordinary `Declared` edge. It is
a one-rule escape hatch for the rare case where an author's own
declaration is wrong for your install, not a general way to outrank
declared facts by default.

## Full precedence order

This list combines edge strength and rule provenance, strongest
first. An edge from an earlier entry is never the one dropped to
satisfy a later one:

1. **Hard** engine facts.
2. **AnyOf**: whichever candidate an any-of constraint (e.g. one of
   several mods shipping the same assembly) resolved to.
3. **Declared-override**: any pair rule you've explicitly marked
   as overriding an author's own declaration (the escape hatch above).
4. **Declared** engine facts.
5. **Your own decision**: a reorder or prefer-winner you made, or a
   pair/placement rule you typed or promoted by hand.
6. **Imported rules**, most to least trusted: RimSort's own
   `userRules.json`, then its community database, then the Steam
   Workshop database (see [rules-databases.md](rules-databases.md)).
7. **Inferred**: a heuristic the analyzer concludes itself. Weaker
   than any rule with an author or user behind it, imported or not,
   but stronger than the two engine strengths below it: it's still
   the analyzer's own conclusion, not a mere presence signal.
8. **Soft**.
9. **Awareness**.

## Cycle breaking

A cycle (two mods each requiring the other, directly or through a
chain) can't be honored literally. When one occurs, the sort does not
fail: the weakest edge in the cycle is dropped, layer by layer. A `Hard`
cycle never drops a `Hard` edge in favor of a weaker one, and ties within
a layer are broken deterministically, never by insertion order. Every drop is recorded as a finding
(see [ledger.md](ledger.md)) so it's visible, not silent.

Within the `Inferred` layer, one kind of edge gives way first: a
"removed node" edge (mod A removes, or keeps-but-replaces, a node
another mod B also patches) where the final defs turn out identical
whichever of the two loads last. That is the case for a
`Replace`/`Remove`/similar op of B's own that lies entirely inside A's
removed region, has no later sibling in an enclosing
`PatchOperationSequence`, isn't gated on an unrelated Conditional, and
names exactly one def. Only which mod's own
op logs the failure changes, never the game's own content, so this
edge yields before any other `Inferred` edge when a cycle forces a
drop. Every other `Inferred` edge, including the ordinary,
content-changing "removed node" edge and the "replace discards
addition" edge below, still ranks the same as before.

A node-keeping `Replace` is a remover too, of everything strictly
below the node it keeps. Once its own old subtree is gone, a later
`Remove`/`Replace`/`Insert` reading or rewriting something that only
existed there fails exactly the way it would against an outright
`Remove`. So the same "removed node" edge above orders that op before
the replace. This is the mirror image of the addition-discarding rule
below. An op that only *adds* new content is never affected (its own
required node is the node being kept, which always survives), so the
two rules never disagree about the same operation. When the replace's
own new value happens to recreate the exact node the other op needs
(matched the same way as the injected-node case above, by a
predicate-keyed `<li>`'s own identity), no edge is added at all: the
node the other op needs is there regardless of order.

A later `Replace` also discards whatever an earlier active mod added
inside the node it replaces, silently. Both operations report
success, so nothing is logged. When that earlier addition isn't
already present in the replacement and the replacer's own `About.xml`
doesn't declare loading after the adder, the analyzer adds an
ordinary (non-cosmetic) `Inferred` edge ordering the adder after the
replacer, so its content lands inside the fresh replacement instead of
being lost. When the replacer *does* declare it loads after the adder,
no edge is added. That's a deliberate choice by the replacer's
author, surfaced only as a ledger finding (see
[ledger.md](ledger.md)).

## Placement and tie-breaks

Within a tier, a mod with no further constraint of its own needs a base
position to start from. That is the tie-break:

- **Rebuild** (default): ranks by normalized mod name. A from-scratch
  order doesn't silently inherit whatever position your prior manual
  edits (or another tool) happened to leave a mod in.
- **PreserveCurrent**: starts from the mod's current position in
  `ModsConfig.xml`, minimizing how much an order you already trust
  moves around.

Under `PreserveCurrent`, when an enforced edge is violated by the
current order, the sorter satisfies it by moving one side: either the
prerequisite earlier, to just before its dependent, or the dependent
later, to just after its prerequisite. Only an edge with an author, a
user, a rule database, or the analyzer's own `Inferred` conclusion behind
it ever moves anything this way. `Hard`, `AnyOf`, the declared-override
escape hatch, `Declared`, your own `UserDecision`, an imported rule
(`RimSortUser`/`RimSortCommunity`/`SteamDb`), and `Inferred` all do.
`Soft`/`Awareness` never do, since neither carries any author or user
behind it, only a bare presence signal. Among the edges that do
move something, the sorter picks whichever direction moves the smaller
total distance, counting every mod each option would drag along with it,
not only how many mods move. On an exact tie, an edge carrying an
author's or your own order statement moves the prerequisite; a purely
heuristic (`Inferred`) edge moves the dependent instead. `Rebuild` never
does any of this: a prerequisite is placed the moment its own name
comes up, and a dependent waits.

An explicit `Top`/`Bottom` placement pin always sorts to the extreme
edge of its tier and pulls its own dependency closure with it. A mod
promoted into a pinned region because something it depends on is
pinned doesn't get to interleave with unrelated content in that region.

## Determinism

The same report and the same rules always produce byte-identical
output. There is no hidden state, no wall-clock dependency, and no
iteration-order dependency in how the sorter builds its result.
