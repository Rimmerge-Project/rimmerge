# How RimWorld decides load order

Rimmerge's sorter exists because RimWorld's own loading model has real,
observable ordering dependencies — this page states them as engine
facts, not as anything specific to a mod.

## Load order is one flat list

RimWorld keeps one ordered list of active mods (`Verse.ModsConfig`,
backed by `ModsConfig.xml`'s `<activeMods>`). `Verse.LoadedModManager`
loads every mod's content in that order: first every mod's `Defs/`
(and assemblies, and `LoadFolders.xml`-gated content folders) into one
combined document, then — once every mod's defs exist — every mod's
patch operations run, in load order, against that combined document.

Within one mod, a file's relative path can be duplicated across its own
loaded folders (a `1.6/` version folder and the mod root both shipping
`Defs/X.xml`, say). RimWorld only ever reads one copy: `Verse
.DirectXmlLoader.XmlAssetsInModFolder` collects every loaded folder's
files into a dictionary keyed by that relative path, and only the first
folder (in the mod's own folder-priority order) to offer a given path is
ever opened — a lower-priority duplicate is never read, so it never
produces a log line either. Folder priority here is not always
`LoadFolders.xml`'s own document order: the engine's own
`ModContentPack.InitLoadFolders` walks a matched version block's `<li>`
list from its last entry to its first when building its internal
priority list, so the **last-listed** folder in the XML wins a
same-path conflict, not the first-listed one. (No `LoadFolders.xml`, or
no entry for the running version, needs no such reversal: the fallback
rule's own construction order — version folder, then `Common`, then the
mod root — already is priority order.)

Within one loaded folder, files aren't read in plain alphabetical order
either: the engine's own directory walk (`DirectoryInfo.GetFiles`,
`AllDirectories`) is breadth-first — every matching file directly in a
directory first, then each subdirectory in turn — with siblings ordered
by NTFS's own case-insensitive collation, not a byte-wise sort. So the
within-mod order that decides both `Defs/` load order and `Patches/`
apply order is: folder priority first (as above), then breadth-first,
collation-ordered file order within each folder, then document order
within one file. A `.`-prefixed file name (including a macOS `._`
resource-fork leftover) is never read at all.

**Duplicates within one mod: the first one read wins, not the last.**
When a mod's own `Defs/` defines the same `(type, defName)` twice —
across two files, or across two loaded folders neither shadowing the
other — the game keeps the first copy in the order above and logs `Mod
X has multiple <T>s named Y. Skipping.` for every later one. The same
applies to a `Name`-attributed template: a second registration of an
already-used `Name` within one mod is refused outright, so the first
registration is the only one that can ever be inherited from.

Two consequences that shape everything downstream:

- **A def a later-loaded mod's `Defs/` folder defines with the same
  `defName` as an earlier one overrides it outright** — the later load
  wins, no merge. This is why two mods editing the "same" def is a
  load-order-sensitive conflict, not automatically a bug.
- **Every patch operation sees the state every earlier operation left
  behind.** An xpath that selects a node another mod's own patch
  injected only resolves if that mod's patch already ran — the one XML
  mechanism that genuinely needs order, independent of def-load order
  entirely (see `Verse.PatchOperationAdd`/`PatchOperationInsert` and
  friends).

## Where the ordering facts come from

Rimmerge derives an ordering-relevant edge between two mods only when
it can point at a specific engine mechanism, never from "these two mods
are often installed together":

- **A `<modDependencies>` entry** is an author's own declared
  requirement. RimWorld itself never reorders by it — it only warns at
  startup when the named mod is missing — but a sorter can and should
  still treat it as an ordering statement, the same way community
  tooling (RimSort) already does.
- **`<loadAfter>`/`<loadBefore>`/`<forceLoadAfter>`/`<forceLoadBefore>`**
  are the author's own direct order statements, read from `About.xml`.
- **`MayRequire`/`MayRequireAnyOf`** gates content on another mod being
  active — evidence the two mods interact, without necessarily requiring
  an order. The game only actually reads the attribute on a def or
  template node directly under `<Defs>`, on a def-typed field, and on a
  `<li>` list item (a `PatchOperationSequence`'s `<operations>` child, or
  a `<match>`/`<nomatch>` branch holding a list). A top-level
  `<Operation>` and a `<match>`/`<nomatch>` node that is itself one
  operation both carry the attribute in the XML but the engine never
  reads it there, so it never gates — the op runs regardless of whether
  the named mod is active.
- **`PatchOperationFindMod`** names another mod by display name inside
  a patch's own conditional logic — matched exactly and case-sensitively
  against the active mod's own display name (`ModLister.HasActiveModWithName`),
  unlike `MayRequire`'s case-insensitive package-id match above.
- **`IfModActive`** in `LoadFolders.xml` gates an entire content folder
  on another mod.
- **A patch's xpath selects a node another active mod's own patch
  injects** (not something either mod ships inline in `Defs/`) — the
  injecting mod's patch must run first. This includes a node a
  `PatchOperationReplace` creates, and a predicate-keyed `<li>` item
  (`li[@Class="…"]` or a bare-text `li[text()="…"]`) another mod's
  `Add` or node-keeping `Replace` injects into a list — not just a
  plainly-addressable element path.
  A list item that some active mod's own `Defs/` already ships with the
  same identity is not a dependency: the selecting op finds it in either
  order, so that pair is left to the other evidence. This holds only when
  the selecting op's **last** step is that `li[...]` and carries exactly
  one predicate, under the same def and container the `Defs/` item sits
  in. A step with two predicates (`li[@Class="X"][thingDef="Y"]`), or a
  `li[...]` the op merely reads through on the way to a deeper node
  (`li[@Class="X"]/things/li[text()="Y"]`), is never demoted by an inline
  item: that item proves neither the second predicate nor the deeper node.
  Text identity is compared as written, as the game's `text()="X"` does, so
  an inline `<li> X </li>` does not count as `li[text()="X"]`. Known gap: a
  `<li>` nested inside another inline `<li>` is not indexed under the
  predicate key a patch uses for it, so such a pair keeps its `Hard` edge.
  (A `MayRequire`-gated inline item counts as shipped even when its mod is
  inactive; that can only lose an ordering, never invent one.)
- **A later `PatchOperationReplace` discards an earlier active mod's
  own addition into the same node, silently.** `PatchOperationReplace`
  inserts each child of its own `<value>` before the matched node, then
  removes the node — subtree and all, including anything an earlier op
  added into it — so whichever of the two operations runs *second*
  decides the outcome: the replace running last wipes the earlier
  addition out; the addition running last lands inside the replacement
  and survives. Neither operation logs anything either way, so this is
  invisible without inferring it directly from patch content.
- **A node-keeping `PatchOperationReplace` also removes everything
  strictly below the node it keeps, so a `Remove`/`Replace`/`Insert`
  reading or rewriting content that only existed in the old subtree
  must run first, not the replace.** Unlike the addition case above,
  the failure here is visible (`Remove`/`Replace`'s own xpath resolves
  to nothing, `Insert`'s own anchor doesn't exist), the same class of
  failure an outright `PatchOperationRemove` already produces against a
  later op that reads or rewrites content inside the node it removes
  (see the "removed node" edge in [sorting.md](sorting.md#cycle-breaking))
  — this is that same rule, with a node-keeping `Replace` standing in
  for the remover. When the replace's own new value recreates the exact
  node the other op needs (the same predicate-keyed `<li>` identity
  match as the injected-node bullet above), no order is required at
  all.
- **`ParentName` inheritance**: a def's `ParentName` resolves against
  every currently-registered `Name`-attributed template with that name,
  picking the one belonging to the highest-loaded mod at or before this
  one (`Verse.XmlInheritance.GetBestParentFor`) — if the only
  registration belongs to one other, later-loading mod, the def (and
  everything inheriting from it) **loads without any of that parent's
  inherited fields** (an `XML error` in the log, not a dropped def: the
  child's own XML still registers and its type still comes from its own
  element or `Class`). This is a load-time failure mode exactly like a
  missing assembly reference, not merely an inheritance convention.
- **A shipped assembly references a type another mod's own assembly
  owns**, or two mods ship the same-named assembly at different
  versions (the first-loaded copy is the one the runtime actually
  binds to).
- **A texture-only mod overriding another mod's texture path** only
  wins if it loads after the mod whose texture it's replacing.
- **A `texPath` resolves against more than an active mod's own loose
  files.** RimWorld looks up a texture in this order: loose files across
  every loaded mod (last-loaded mod first, a `.dds` shadowing any
  same-key file across the whole mod regardless of folder), then Core's
  own built-in `Resources.Load` textures, then every running mod's own
  compiled asset bundle (DLCs included), last-loaded mod first. A `.dds`
  file with a valid magic and header size but a compressed pixel format
  whose width or height isn't a multiple of 4 fails to decode — the
  engine shows the bad-texture placeholder for it, the same outcome as a
  missing path.

None of this is inferred from a mod's popularity, its author's other
mods, or a community convention with no engine mechanism behind it —
that kind of knowledge, where it exists at all, lives in an optional,
explicitly-imported rule database (see
[rules-databases.md](rules-databases.md)), never baked into the
sorter's own logic.

**One fact above is deliberately not an ordering fact.** RimWorld
resolves every cross-reference — a list item, a scalar field, a keyed
element name — only once, after every active mod's defs and every
active mod's patches have finished loading
(`DirectXmlCrossRefLoader.ResolveAllWantedCrossReferences`). A name
that never resolves to any active def (see
[ledger.md](ledger.md)'s "dangling def reference") is therefore a fact
about the *set* of active mods, never about their order: no reorder
changes whether it resolves. A reorder can still add or remove the error
line itself: when the def holding the reference is itself added by a patch
that only runs in some orders (a later step of a sequence that stops early
in the other order), the order decides whether the reference exists at all,
never whether the name resolves.

## What Rimmerge adds beyond what RimWorld enforces

RimWorld itself never picks an order for you — it just loads whatever
order `ModsConfig.xml` gives it and fails loudly (or silently drops
content) when that order is wrong. Rimmerge's own job, described in
[sorting.md](sorting.md), is turning every fact above into a suggested
order, ranked by how sure each fact actually is.

## Sharing a load order

RimWorld's own mod manager can save the active list to a file and load it
back: "Save list" writes a `.rml` file (a `<savedModList>` holding the
package ids, the mods' names, their Steam ids, and the game version) into a
`ModLists` folder beside the `Config` folder that holds `ModsConfig.xml`,
and "Load list" reads from there. Rimmerge reads and writes that same
format, so a list exported into `ModLists` shows up in the game, and a list
saved in the game can be imported. It also reads a `ModsConfig.xml`-shaped
list and a plain text list (see
[Sharing a load order](../desktop.md#sharing-a-load-order) for the
buttons, and [`order export`/`order import`](../cli.md) for the CLI).

**Export shares the order in `ModsConfig.xml`**, re-read at export time —
what RimWorld loads now, not the Suggested order. Package ids are written
without `_steam`, and your own generated merge mod is left out, since only
your machine has it.

**An imported list is the whole active set.** In the desktop app, *Use this
order* turns it into the **Current** order like this:

- **It rescans with the list.** The listed mods you have installed, in the
  list's order, become the active set of a fresh scan, so newly activated
  mods have their defs, patches and ordering edges in the report before
  the ledger, the preflight, or the Suggested order say anything about
  them. The scan's result is selected as Current. If the scan fails,
  nothing changes.
- **A mod you already have active is kept as is.** When the list names a
  package (with or without `_steam`) whose copy is already active here,
  that copy stays, so re-importing your own export changes nothing.
  Otherwise the exact id is activated, or another installed copy of the
  same package when the exact one isn't installed.
- **Your own merge mod stays at the end.** A list never carries it, and
  the import doesn't drop it when `ModsConfig.xml` has it. A merge mod
  named by the sender's list was made on their machine and is never
  activated.
- **Core comes first if the list lacks it.** Core is never deactivated.
  A list that names Core keeps Core where the list puts it.
- **Everything else active is deactivated**, and a listed mod you don't
  have stays out of the order. Rimmerge never downloads a mod; the preview
  names each missing one, with its Workshop link when the list has one.
- **Nothing reaches `ModsConfig.xml` until you Apply.** The page shows a
  note that the order isn't in the file yet, and the ordinary Apply
  (hard-problem confirmation, running-game check, backup) writes it. The
  CLI's `order import` has no scan or Current to stage: it writes the
  planned order to `ModsConfig.xml` itself, with a backup first.

An import is refused, with the reason shown in the preview, when:

- **no Core is installed**, so the order would have none;
- **nothing is installed**: no listed mod besides Core and Rimmerge's
  generated mods is installed, so the import would only deactivate your
  mods;
- **the list is too long**: more than 5,000 entries.
