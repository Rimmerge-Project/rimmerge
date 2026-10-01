# The ledger

Every thing Rimmerge notices — a sort conflict, two mods editing the
same def, a dropped dependency edge, an ambiguous template — becomes
one **finding**. The ledger is the whole set of findings for a report,
each carrying a suggested action and a confidence score.

## Findings

A finding is keyed by what it's about (a def, a pair of mods, a
specific edge) plus its own kind — a `DefOverride`, a
`PatchCollision`, an `EdgeDropped`, and many more, one kind per
distinct thing worth surfacing. Each kind has its own logic for what a
reasonable suggestion looks like, but every finding shares the same
shape: a subject, a suggested action, alternatives, and a confidence
score.

Most evidence is the same whichever order you are looking at. A few
facts are not: when two mods override the same def, patch the same node,
or ship the same texture file, the owners are listed in the order the
scan saw them, and the **winner** (the owner loaded last, whose def,
operation or file the game uses) is worked out for the order the ledger
is built for. Under the suggested order it can be a different mod than
under the current one, and the Inbox badges the winner it is given
rather than the last name in the list.

What the winner declares is judged the same way. "The winner declares
`loadAfter` on every other owner" (and its patch-collision twin, which
also needs that no other mod removes the patched field or an ancestor of
it) is asked of the report's per-mod declarations with the ledger's own
winner, so a def whose declaring mod only wins under the suggested order
scores as intentional there and as unexplained under the current one.

## Confidence

Confidence is a 0–100 score reflecting how sure Rimmerge is that its
own suggestion is correct — not how important the finding is. A few
examples of what drives it up or down:

- A `Hard`-strength engine fact overruling something weaker: very high
  confidence (the game itself would break otherwise).
- The same author owning both sides of a conflict: high confidence
  (one author's own intent rarely contradicts itself).
- Two independent facts of equal strength contradicting each other with
  nothing to break the tie except an arbitrary rule: low confidence —
  this is exactly the shape that should ask you, not guess.
- A dropped rule-origin edge (a rule losing to something stronger) is
  scored a notch more cautiously than a dropped bare engine edge, since
  overruling something you (or a database) explicitly asserted is a
  bigger claim than overruling an inferred fact.
- A dropped "removed node" edge that's *cosmetic* (see
  [sorting.md](sorting.md)'s cycle-breaking section — the final defs are
  identical whichever of the two mods loads last) auto-accepts at 95
  with no reorder to offer: there's nothing to weigh, since the only
  difference between the two orders is which mod's own op logs the
  failure.
- A replacer that already declares it loads after the mod whose
  addition it discards (`DiscardedAddition`) scores 80, exactly meeting
  the default `Auto` threshold: the replacer's own author already chose
  this outcome on purpose, so it auto-accepts by default as disclosure
  rather than a conflict — there's no reorder to offer against a
  declaration the replacer's own author wrote, only the record of what
  was chosen away.
- A patch collision where the mod whose operation runs last declares, in
  its own `About.xml`, that it loads after (or depends on) every other
  mod patching the same field scores 80, exactly meeting the default
  `Auto` threshold: that author ordered their change after the others on
  purpose, so the game's own result is kept. It doesn't apply when
  another of those mods removes the field, since the declared mod's own
  operation then fails and the result is not the change its author wrote.

Your own **threshold** setting (see [settings.md](../settings.md)) is the
line: a suggestion at or above it is applied automatically; below it,
it waits for you.

Some findings are **informational**: a plain `Accept` with no
alternatives, because there's no action this engine can offer that
fixes the underlying fact. A `.dds` file that fails to decode
(`UndecodableTexture`) is one — the fix is re-encoding the file outside
this workspace, so the suggestion just discloses that the base game
shows the bad-texture placeholder in its place.

**"Content lost at load"** is one such family: a def's `ParentName`
resolving to nothing, or to a template of a *confirmed*-unrelated
element type (`BrokenInheritance`), and `UndecodableTexture` above —
both are proven load-time facts (ground-truthed against the decompiled
engine), scored around 90, with no reorder or merge that changes the
outcome. The fix in every case is the referencing mod's own authoring.
A resolved parent whose own element type merely differs in letter case,
belongs to a vanilla/DLC child, or is a genuine subclass of the asking
child's own type (a mod framework's `SomeDef : ThingDef`-style
specialization) is never flagged — none of those actually lose content
at load. When neither a mismatch nor a subclass relationship can be
confirmed at all, it surfaces as a plain informational note instead of a
ledger finding, rather than guessing.

A **dangling def reference** (`DanglingDefReference`) joins the same
family: a name written at a recognized reference site — a list item, a
scalar field, a keyed-dictionary element name, or a
`descriptionHyperlinks` entry — that no active def of any type actually
has, and that isn't one of the engine's own generated names either
(a blueprint, a frame, a corpse, a psytrainer or neurotrainer, a gene
generated from a gene template, a carpet, and similar). Reference sites are
inferred from the data itself, never a hand-written field table: a
field only counts once enough of its own distinct values resolve to
some real def. This is diagnostic only, never an ordering fact —
cross-references resolve once, after every mod's defs and patches have
loaded, so the result never depends on load order. Scored at 85, with
the cause stated in words: removed by another mod's patch, defined only
in a folder this install doesn't load, defined only in an installed but
inactive mod, defined nowhere at all, or — when the byte-bounded search
for the first three causes ran out of budget — simply undetermined. A
name whose resolved sibling values are mostly `SoundDef`s carries a
note that a missing sound falls back to an undefined one with a
warning, rather than failing to load. Type-agnostic by design: a name
that exists under the *wrong* def type reads as resolved, since typing
a field from the game's own compiled metadata is a larger, later
undertaking.

A **"possible typo"** finding is a lower-confidence sibling of that
family: a `PatchOperationFindMod` name or a `MayRequire`/
`MayRequireAnyOf` id that resolves to no active or installed mod, but
closely resembles exactly one active mod's own name or package id
(`NearMissModReference`). This is a guess, not a proven fact — the
confidence tracks how strong the resemblance is (an exact match apart
from letter case scores highest; a fuzzy edit-distance or a leading
extra word scores lowest), and it's always below the default `Auto`
threshold so it never silently auto-accepts. A name that matches an
installed-but-inactive mod, that looks like a sequel (a trailing
number) or a renamed fork (`(Continued)` and similar), is never flagged
at all — those are real, deliberate references, not typos.

A later `PatchOperationReplace` silently discarding an earlier active
mod's own addition inside the replaced node (see
[load-order.md](load-order.md)) is usually surfaced as an ordinary
`Inferred` edge, ordering the addition after the replace so it lands
inside the fresh replacement instead — dropping that edge in a cycle
goes through the same `EdgeDropped` path every other edge kind uses.
When the replacer's own `About.xml` already declares it loads after
the mod it discards from, there's nothing to reorder: the author chose
this outcome on purpose, so it surfaces only as a `DiscardedAddition`
finding, an informational disclosure of what was chosen away, with a
`Merge` alternative offered whenever a `PatchCollision` already exists
for the same def (letting you keep the discarded content field by
field without contradicting the replacer's own declared order).

## Deciding a finding

Every finding's suggested action has one or more alternatives —
accept the suggestion, prefer a specific mod's version, merge the
contested fields (see [merge.md](merge.md)), or reorder two mods. A
decision you make is remembered per-profile and re-applied on every
later scan against the same finding key, the same way a version
control system's conflict-resolution cache (`git rerere`) replays a
resolution you've already made — a decision survives a rescan as long
as the finding key it was made against still resolves the same way; if
the underlying facts change enough that the key no longer matches
cleanly, the finding reopens rather than silently keeping a stale
decision.

## Hard problems at Apply

Confidence says how sure Rimmerge is of a suggestion. It says nothing
about whether the order about to be written is broken. A separate,
pure check (`rim_resolve::preflight`) lists the **hard problems** in
that exact order, whatever any finding's confidence:

- a required dependency that is not in the order (installed but
  inactive, or not installed);
- two mods in the order that are declared incompatible;
- a mod listed in `ModsConfig.xml` that is not on disk (the order either
  drops it or keeps it);
- a violated `Hard`-strength edge: a type missing at load time, a
  `forceLoadAfter`/`forceLoadBefore` declaration, a `ParentTemplate`
  parent registered after its child, or a `PatchInjectedNode` target not
  yet created when the patch runs;
- an any-of assembly constraint with no candidate loading first, when the
  reference is resolved at load time (a lazily resolved one is `Soft`
  and stays in the Inbox, like a lazy single-owner edge).

Violated `Declared`, `Soft`, `Awareness` and inferred edges, unsupported
versions, and every needs-input finding about defs, textures and patches
are not hard problems; they stay in the Inbox. The list is sorted by kind
and then by id, so it is identical across runs.

A hard problem is **acknowledged** when you already decided its own
finding (missing dependency, incompatible pair, missing mod, or the
dropped edge for a violated one). A violated edge in the current order
has no finding to decide, and an unsatisfied any-of is never
acknowledged. An interface asks for confirmation only while some problem
is unacknowledged.

## Where decisions feed back in

A decision doesn't just sit in the ledger — it becomes real input
elsewhere: a "prefer this mod's def" decision changes what
[merge.md](merge.md)'s merge mod contains; an accepted reorder becomes
a user-owned rule the sorter (see [sorting.md](sorting.md)) honors on
every future sort, exactly as if you'd typed it yourself with
`rule set-pair`.
