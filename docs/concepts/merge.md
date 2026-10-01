# Merging

When two active mods both change the same def, RimWorld's own rule is
simple and total: the later-loaded mod's version wins, in full — see
[load-order.md](load-order.md). That's often not what you actually
want; Rimmerge's merge tooling exists to produce a def that keeps both
mods' changes instead of picking one winner outright.

## Field-level merge

For a def two or more mods touch, Rimmerge computes each mod's own
field-level diff against the def's base state and, where the diffs
touch different fields (or the same field the same way), folds them
together automatically — a clean merge. Where two mods genuinely
disagree on the same field, that's a finding
(see [ledger.md](ledger.md)) asking you to pick a winner for that field
specifically, not for the whole def.

A merge is never silent about what it can't safely do: a **structural
guard** withholds auto-merging on a field where merging field-by-field
could itself produce an invalid def (e.g. a type-identity field like
`thingClass`) — that case always asks you to confirm a winner instead.

A mod whose own operation fails when its changes are moved last,
because another mod removes the node it targets first, is never counted
as agreeing with the remover. Its candidate value comes from replaying
its own operations against the def alone, so a removal colliding with a
replacement always asks you which one to keep. Only a failure at or
beneath the contested field triggers this; an operation of the same mod
that fails on some other part of the def does not. An operation that
also fails when replayed alone is a dead target in every order, and the
reordered result stands.

## The merge mod

Every finding you've resolved with a merge decision (an accepted merge,
a chosen field winner) is rendered into one generated mod: a small,
synthetic `Defs/`-only mod placed after every mod it draws from, whose
own defs are the merged results. It's regenerated whenever you rescan
or change a decision — never hand-edited.

## Compatibility patches

A compatibility patch narrows the same merge engine to a **scope** you
choose: two or more specific mods, with their own decisions, exported
as an independent, publishable mod (its own `About.xml`, its own
dependency list, its own patch-formatted output naming what it merges
and why). Where the merge mod covers everything you've resolved across
your whole install, a compatibility patch covers exactly the mods you
name — useful when you want to publish a fix for one specific pair
without shipping your entire personal merge state.

Every generated patch operation — in the merge mod and in a
compatibility patch alike — is gated on the mod(s) it actually draws its
value from: each `<li>` inside the generated `PatchOperationSequence`'s
own `<operations>` carries its own `MayRequire`, naming whichever mod's
field value it copied. That's deliberately per-operation, not one gate
on the sequence as a whole — RimWorld only reads `MayRequire` on a list
item inside a patch sequence, never on the sequence's own top-level
node, so a whole-sequence gate would silently never take effect. An
operation whose value traces only to Core (vanilla) carries no gate at
all, since Core is always present.

## The patch maker (assignment projects)

A different kind of gap: sometimes what's missing isn't a merge between
two mods' own changes, but a set of fields a *reference* mod's own
convention expects every eligible def to carry, and a *target* mod's
defs simply don't have yet — for example, a compatibility-framework mod
that reads a set of fields on every race def, and a new race mod that
was never updated to add them.

The patch maker infers the field shape a reference mod expects
(cardinality, whether a field looks like a picker over known
defs/items or free text, whether an existing def already resolves a
sensible default) from every instance the reference mod already
handles, then gives you one row per target instance to fill in — a
structured editor instead of hand-writing patch XML. The result exports
as its own small `Defs/`-only mod, the same shape a compatibility patch
does.

In the desktop app, each target shows its texture, so you can recognise
the race or item behind a name. The texture comes from the def's
*effective* data under the selected order (patches applied, inherited
`graphicClass` and `texPath` followed), expanded the way the game expands
it: a directional graphic has four faces (a missing side is the opposite
one, mirrored), an apparel has one per body type, a pawn kind one per life
stage. A race has no graphic of its own: it shows the graphics of the pawn
kinds that use it. A kind whose `race` was patched away by a later mod no
longer counts toward the original race. The file shown is the one the last
mod in the selected order ships at that path, which is not always the def
owner's. Switching between the current and the suggested order can change
it.

What cannot be shown is said so: textures served by an asset bundle or the
game's own resources are never read, a `.dds` is shown only through a PNG
or JPEG copy beside it, and a humanlike race the game builds from body,
head and hair parts has no single texture. A field outside the game's own
graphic rules whose text names texture files is shown as an approximation
labelled "found by path". See [desktop.md](../desktop.md).

## Where decisions come from

Every merge decision here starts life as a ledger finding
(see [ledger.md](ledger.md)) — `merge`/`patch`/`assign` are how you act
on one, not a separate decision-making system.
