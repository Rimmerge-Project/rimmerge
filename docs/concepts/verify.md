# Verify: predicting patch failures before you launch

The sorter (see [sorting.md](sorting.md)) only ever orders mods by
facts it can point at — a declared dependency, a shipped assembly
reference, and so on. Not every possible patch failure has one of those
facts behind it: an xpath can simply fail to match anything, for
reasons the sort graph has no edge for. `verify` closes that gap by
actually **replaying** your patches instead of only reasoning about
edges.

## What it does

For every def with at least one active patch operation aimed at it,
`verify` runs the same patch-application logic RimWorld itself runs — each active
mod's patch operations, in load order, against the combined def
document — and records which top-level operations would fail. This
mirrors the engine's own two-phase load exactly: every mod's `Defs/`
loads first, then every mod's patches apply in sequence, each xpath
evaluated against whatever the previous operations left behind.

An OR-ed head such as `ThingDef[defName="A" or defName="B"]/comps` is
one query, not two: the game evaluates it once over the whole document
and the operation succeeds when either def has the node. `verify` checks
every def the head names, including a def that only its own mod patches,
and drops the prediction when any of them matches.

A def that only its own mod patches is replayed like any other: when its
own operation fails, the game logs that too (an author bug, not a load
order problem, so no reorder is ever offered for it).

An operation nested inside an unsatisfied `PatchOperationFindMod` is
correctly never replayed at all — it would never run in the real game
either, so predicting it as a failure would be a false positive. The
same holds for `MayRequire`/`MayRequireAnyOf`, but only on the one node
shape the game actually reads it on — a `<li>` list item inside a
`PatchOperationSequence`'s `<operations>`, or inside a `<match>`/
`<nomatch>` branch holding a list. A top-level `<Operation>`'s own
`MayRequire` (or a `<match>`/`<nomatch>` node that is itself one
operation, not a list) is never read by the game at all, so `verify`
replays it regardless of whether the named mod is active — gating it
there would itself be a false negative.

`verify` is explicit and on-demand: it never runs as part of a cached
sort or ledger build (a full replay is comparatively expensive), only
when you ask for it — the CLI's `verify` command or the desktop app's
Apply dialog.

A query naming defs by a child value — `ThingDef[thingClass="Pawn"]/comps`,
say, rather than an explicit `defName`/`@Name` — is a **whole-document**
query in the real game: it can match any def whose content happens to
qualify, not only the one def `verify` is currently replaying.
`verify` never predicts a failure from that shape's empty match on one
def — an empty selection there says nothing about whether the query
matched somewhere else — the same conservatism it already applies to an
xpath filter head like `[@ParentName="X"]`. It still applies the
operation when the query *does* match the def being replayed, and a
`PatchOperationConditional`/`Test` using this shape still reads a
non-empty match as decisive. The suppressed count (operations this
conservatism declined to turn into a prediction) is reported as
`suppressed_filter_head_ops`.

## Checking a prediction against reality

Because `verify` is a prediction, not an observation, you can check it
against what RimWorld itself actually logged: `log import` reads a
`Player.log` or an in-game console snapshot, attributing every
failure/warning/timing line to the active mods responsible, so a
predicted failure can be lined up against a real one from a session you
actually played.

Check what the file covers before comparing:

- A `Player.log` that ended truncated or crashed may not contain the
  failures you are looking for, above all one that stopped before any
  patch failure was logged: the patch phase may never have run.
- A console snapshot holds at most the 1,000 entries the game's console
  kept (the oldest are already gone when it holds exactly 1,000), and
  proves nothing about stages it does not show, start-up included, where
  patch failures are logged. A failure missing from a snapshot is not
  evidence it did not happen.
- Where the game stopped logging for a stretch (a logging gap), nothing
  it would have written there exists to compare.

`log import` reports each of these next to the result (see
[Importing game logs](../cli.md#importing-game-logs)), and the desktop
app's apply dialog adds a note for each limit that applies, so a partial
file never reads as a whole session.

## The counterfactual: what would fix it

For a predicted failure, Rimmerge can go one step further and ask "what
single load-order move would make this succeed?" — it takes the mod
whose operation failed, tries moving its whole contiguous block of
content earlier or later relative to the mod it seems to conflict with,
and re-replays. When a move fixes it, that becomes a concrete,
testable suggestion ("load X after Y") you can turn directly into a
pair rule (`rule set-pair`) rather than a vague "these two mods
conflict."

This only ever proposes moving a *contiguous* block — a mod whose own
content isn't contiguous in the current order (interleaved with
something else by a prior move) is refused rather than guessed at, so
a suggestion is never built on an input shape the check itself can't
actually reason about.

### Cosmetic fixes: same def, different log line

A counterfactual fix can also come back **cosmetic**: the load-order
move makes the failing operation succeed, but the def RimWorld ends up
with is identical either way — only which mod's own operation logs
as failed changes (the same fact
[`sorting.md`](sorting.md#cycle-breaking)'s cosmetic
`PatchRemovedNode` edges name statically, here confirmed by actually
replaying both orders). Two mods independently removing the same node,
or one removing it and another replacing it outright, are the common
shapes: whichever runs last, the node ends up gone.

`verify` never offers a `set-pair` rule for a cosmetic fix — moving a
mod's whole load position to change nothing a player would ever notice
isn't worth the disturbance. When the two mods also collide on the same
target — the common case, since a cosmetic pair almost always means
they're mutating the identical node — the ledger already has a
`PatchCollision` finding for it, and `verify` points at that instead
(`rimmerge merge plan --key <key>`): merging keeps the losing mod's own
intent, which a reorder that changes nothing never could.
