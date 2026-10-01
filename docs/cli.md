# CLI reference

`rimmerge <command> [subcommand] [flags]`. Every command supports
`--help` for its own exact flags; this page is the map of what exists
and why you'd reach for it — regenerate the flag-level detail with
`rimmerge <command> --help` / `rimmerge <command> <subcommand> --help`,
since flags are more likely to drift than the command list itself.

Most commands take `--profile-dir <path>` (defaulting to a path derived
from your `ModsConfig.xml`, so it's rarely needed by hand) and honor the
same `--game-dir`/`--workshop-dir`/`--mods-config` overrides described
in [install.md](install.md). `sort` and `ledger` are the exception:
they take `--report <path>` — a JSON report file, either one `load`
already wrote into a profile (`<profile>/report.json`) or a standalone
file with no live install or profile involved at all.

## Report-only (no live install needed)

- **`sort`** — sorts a JSON report and prints the suggested order.
  `--dropped` adds, after the summary, one block per edge the sorter had
  to drop: `after <- before  [layer] kind: detail` (`after` loads after
  `before`), the `cycle:` that edge would have closed, and, for a direct
  two-mod contradiction, the `overruled by:` edge that won. `--mod <id>`
  (with `--dropped`) keeps edges with that mod at either end. The list
  is sorted by edge, so it is the same on every run. It sorts the report
  alone, like the rest of `sort`, so an edge dropped because of a rule in
  your profile is not shown; `ledger` and `apply --dry-run` use the profile.
- **`ledger`** — builds and prints the resolution ledger for a JSON
  report.
- **`fixture trim`** — trims a full report into a smaller golden
  fixture (used by this project's own tests).
- **`fixture gen`** — generates a complete synthetic RimWorld install
  from a spec file: mods, defs, patches, assemblies, invented from
  scratch. See [testing.md](testing.md) for what this is for.

## Install and profile setup

- **`config show` / `config set` / `config clear`** — the app-global
  path config described in [install.md](install.md): where this
  machine's RimWorld install, workshop folder, and `ModsConfig.xml`
  are, so `--game-dir` and friends stop being needed.
- **`load`** — scans a RimWorld install and writes the report into the
  profile.
- **`mods list` / `mods activate` / `mods deactivate`** — lists,
  activates, and deactivates mods directly in `ModsConfig.xml`'s own
  `<activeMods>` list. `activate --with-dependencies` pulls in a mod's
  declared dependencies first.
- **`mods show <id>`** — one mod's full information: identity,
  `About.xml` description and version, and — for an active mod — its
  placement, tier, tags, dependents, and live findings count. Unlike
  `list`/`activate`/`deactivate` (discovery only), `show` runs a full
  scan, the same as `load`/`defs inspect`, since placement and findings
  only exist under a selected order.

## Ordering

- **`rule set-placement`** — pins a mod to the top or bottom of the
  load order as your own decision, persisted so it survives re-sorts
  and import toggles.
- **`rule set-pair` / `rule remove-pair`** — states (or removes) "X
  must load after Y" as your own decision.
- **`promote`** — promotes an imported pair or placement rule (from a
  RimSort database) to a user-owned copy that survives both import
  toggles and a re-import.
- **`import`** — imports RimSort's rule databases into the current
  profile. See [concepts/rules-databases.md](concepts/rules-databases.md).
- **`db status` / `db refresh`** — rule-database cache maintenance:
  `status` reads the cache with no network and names each source
  `automatic`/`manual` (the app-global auto-refresh toggle, never a
  per-command flag); `refresh` fetches every requested, *enabled*
  source unconditionally — a manual refresh has no "due" check at all
  (that cadence only gates the automatic, launch-time refresh); a click
  is its own consent. With no `--source`, that includes the Steam
  Workshop database (about 49 MB) while its switch is on, which it is by
  default, so a machine that has never saved settings downloads it on the
  first plain `db refresh` (`--source community` or
  `rimmerge network set --steam-workshop off` avoids that). `status` also prints a trailing reminder line once
  an *enabled* source is stale and internet access is on (silenced by
  the same "don't remind me again" mute the desktop bell honours), and a
  one-line notice if `app-settings.json` itself couldn't be read. See
  [privacy-and-network.md](privacy-and-network.md).
- **`network status` / `network on` / `network off` / `network set` /
  `network reset`** — the app-global internet-access policy (only the two GitHub
  hosts, never your local network)
  (`<base>/app-settings.json`): `status` prints the master switch and
  every feature toggle; `set` changes only the switches you name
  (`--updates`, `--auto-refresh`, `--community-rules`, `--steam-workshop`,
  `--rimmerge-rules`) and never the master switch, so it can turn one
  source off without allowing internet access; `on`/`off` flip the
  master switch (optionally,
  in the same call: `--updates on|off`/`--auto-refresh on|off` for the
  two automatic-feature toggles, `--community-rules on|off`/
  `--steam-workshop on|off`/`--rimmerge-rules on|off` for each source's
  own fetch toggle); `reset` restores every one of those switches to its
  default (all on, including Steam Workshop's own fetch toggle: about
  49 MB, downloaded only by `db refresh`) while
  leaving the stale-reminder threshold untouched — the way to recover
  from a corrupt `app-settings.json`, which fails closed rather than
  crashing, without hand-editing JSON. A privacy switch shouldn't
  require hand-editing JSON.
- **`check-update`** — a manual, explicit check for a newer Rimmerge
  release. Honours `allow_network` and GitHub's rate limit; never runs
  on its own. Its result is recorded the same way the desktop's own
  "Check now" is, so the desktop's notice reflects a check run from
  here.
- **`apply`** — writes the order named by `--source` (`suggested` by
  default, or `current`) to `ModsConfig.xml` (or previews it with
  `--dry-run`). Before writing or diffing, it prints the order's hard
  problems (see [concepts/ledger.md](concepts/ledger.md#hard-problems-at-apply)),
  one line each, under a header such as
  `hard problems in the Suggested order (2, 1 already decided):`; a
  problem you already decided in the Inbox is marked `[decided]`.
  Nothing is printed when there are none. The list is informational: it
  never changes the exit code and adds no flag or prompt, so scripts keep
  working (grep for the header line to refuse on one).

## Verification

- **`verify`** — replays every active patch operation against a
  chosen order and predicts which top-level operations RimWorld itself
  would log as failed. Explicit, on demand only. See
  [concepts/verify.md](concepts/verify.md). `--json`'s per-def rows
  carry a `reorder_kind` field (`"content"`, `"cosmetic"`, or `null`
  when the cause offers no reorder at all) and, for a `"cosmetic"` row,
  a `cosmetic_merge_key` naming the existing `PatchCollision` finding to
  merge instead, if the ledger has one — both additive to the existing
  shape.
- **`log import <path>... [--kind auto|player-log|console-snapshot]
  [--json]`** — imports one or more game logs, each a `Player.log` or an
  in-game console snapshot (a copy of the debug console, saved under any
  name), attributes every failure, warning and timer to the active mods,
  and says what each file covers, so a predicted `verify` failure can be
  checked against what RimWorld actually logged. Read-only: a log is
  never modified, moved or copied. See
  [Importing game logs](#importing-game-logs) below.
- **`startup`** — prints the per-mod startup-cost table (patch op
  counts, slow-shape xpaths, texture/DLL counts and bytes) from a
  report. Static analysis only, no timings.

### Importing game logs

`rimmerge log import` reads each path on its own: the install is scanned
once, and the files are never merged into one session. Every result
says what the file is and what it covers (the text output leads with
it), then lists what it holds. Only positive evidence is reported: a
file never proves that something did not happen.

```sh
rimmerge log import Player.log
rimmerge log import Player.log midgame1.txt --json
```

#### Kind: `Player.log` or console snapshot

With `--kind auto` (the default), each file's kind comes from its
content. The file name is never consulted: real names such as
`Player.log` and `midgame1.txt` carry no kind. Detection peeks at the
first 200 lines (at most 1 MiB) and decides:

1. A console-copy trace start anywhere in the peek means a console
   snapshot. That is the line
   `UnityEngine.StackTraceUtility:ExtractStackTrace ()` or the exact line
   `No stack trace.`, one of which every entry of a console copy carries.
2. Otherwise, the exact `RimWorld <version> rev<n>` banner means a
   `Player.log`. A real `Player.log` starts with Unity's `Mono path[0] = `
   line and holds no copy trace start.
3. Neither means a console snapshot, the reading that can only
   under-claim.

The banner alone cannot decide, because the game also logs it into the
console. An uncleared console copy holds it near its top, and as its very
first entry when the game was launched without options (`Command line
arguments:` is logged only when there are some).

Stated limits: a `Player.log` whose banner lies beyond the peek reads as
a console snapshot, and a snapshot whose first trace start lies beyond
the peek while its banner lies inside it reads as a `Player.log` (real
copies hold their first trace start by line 71). The peek stays small
because the file is then parsed in one pass, and a wrong snapshot reading
only loses claims.

`--kind player-log` or `--kind console-snapshot` overrides detection for
every path, and the text `coverage:` line then ends with `[kind forced by
--kind]`. Forcing the wrong kind still accounts for every line, but the
foreign framing reads the entries differently (a whole `Player.log` read
as a snapshot is one giant message), so its classes are not meaningful.
Detection re-reads the start of the file, so on a path that cannot be
re-read (not seekable) it fails; pass `--kind` to skip it.

#### Several files

With one path, `--json` prints one object. With several, it prints an
array of those objects, each with a leading `path`, and the text output
prints a `==> <path> <==` line before each file's block. The import
fails, printing nothing, if any path cannot be read. A file's size, its
kind and bytes that are not valid UTF-8 are never a reason to refuse it.

#### Coverage

Every result carries `kind` (`player_log` or `console_snapshot`) and a
`coverage` object tagged with the same `kind`; the text output prints a
`coverage:` line.

- **A `Player.log`** reports:
  - `passes` (startup passes, see [below](#startup-passes-and-the-typed-lists))
    and `banner_lines` (the line of each `RimWorld <version>` banner, at
    most 16 listed; `passes` is authoritative past that).
  - `patch_phase`: `patch_failure_logged` when an engine `Patch
    operation ... failed` entry exists, else `none_logged`, which is not
    proof the phase did not run.
  - `end_state`, an object tagged by `state`: `clean_exit` (Unity's
    `Memory Statistics:` footer), `crashed` (Unity's crash handler
    message, with the `report_path` it prints on the next line, or
    `null`; a crash outranks a footer), or `truncated` (neither: the log
    stops where the game stopped writing, as after a freeze, a
    force-close, or a copy taken while the game ran). RimWorld itself
    logs nothing on a normal quit, so both markers are Unity's.

  There is no claim about a config check: the game logs no marker for
  one.
- **A console snapshot** reports `entries`, `console_fill` and
  `head_truncated`. The game's console keeps at most 1,000 entries and
  drops the oldest first, so `console_fill` is:
  - `below_cap`: nothing says anything was dropped (a Clear before the
    copy also leaves a short one);
  - `at_cap`: exactly 1,000, so the game dropped older entries before the
    copy was saved;
  - `over_cap`: more entries than one console holds, so several copies
    were pasted together or the entries were split wrongly; what the
    game dropped cannot be said.

  `head_truncated` is true when the copy's first non-blank line is a
  Unity frame or `No stack trace.`, which only ever follows an entry's
  text: the copy was cut by hand and its first entry's text is missing.

  A snapshot proves nothing about stages it does not show, and the
  console merges repeats of one message into a single entry, so its
  counts are lower bounds. It is never a whole session, so it states no
  startup passes (`totals` has no `passes`), and its text title states
  the scope, for example `Console snapshot import (only the 392 entries
  the console held): ...`.

#### Logging gaps and lower bounds

After 10,000 messages the game logs `Reached max messages limit.
Stopping logging to avoid spam.` and writes nothing more until it logs
`Message logging is now once again on.` What it never wrote cannot be
recovered. `logging_gaps` lists each gap as a `stop_line` and an `end`
tagged by `kind`: `resumed` (with its `line`) or `never_resumed` (no
resume follows, so everything after the stop line is unreliable). A
second stop while a gap is still open leaves the first gap
`never_resumed`, and a resume with no stop before it is not a gap. At
most 1,000 gaps are listed; `read_stats.logging_gaps_dropped` counts the
rest.

A gap is only a few file lines wide, so the one signal left in the file
is a family whose line span straddles it. Such a family may have lost
occurrences, so its count is a lower bound: `lower_bound_families` lists
each one by `class` and `key` (the two gap messages themselves are
exempt, since their counts are exact). At least these families' counts
are lower bounds; any count may be short.

The text output for a file with gaps opens with a warning naming the gap
count and how many never resumed, and lists the first ten gaps (`--json`
lists all); after the class list, it says how many families span a gap
and what share of the entries they hold.

The game also never writes a message's 100th and later consecutive
identical repeats, which no log shows, so a storm's count can be short
even with no gap.

#### Startup passes and the typed lists

A real `Player.log` holds two startup passes: a pre-patching loader
restarts the game in-process, so the log has two `RimWorld <version>`
banners, and the game prints its startup warnings again in each pass.
The typed lists handle that as follows:

- **`patch_failures`**: each terse `Patch operation ... failed` line with
  its `operation`, its `source_file` (the `file:` line after it) and,
  when found, its `stack_trace` (the leaf operation and its enclosing
  chain). A failure's stack trace is the stack-trace block of the same
  mod, in the same pass, that names the same file (paths compared
  ignoring case and slash direction); the mod alone decides only when the
  failure or the block names no file, and two different files never
  pair. A failure with no block has no stack trace.
- **`extra_stack_traces`**: every block no failure claimed, kept, never
  dropped.
- **`cross_references`**: each `Could not resolve cross-reference ...`
  line, `kind` `wanter` (with `wanter_field`) or `wanting_def` (with
  `wanting_def` and an optional `note`).
- **`dds_failures`**: each texture a texture loader could not load, with
  its `path`, `width`, `height` and `format`.
- **`dependency_warnings`**: each warning (mod name and `dependency_id`)
  as the last pass that logged it has it (once per time that pass logged
  it), in file order. Identical passes give the second pass's list, and
  a second pass cut off partway keeps what only the first pass logged
  (listed before the second pass's).
- **`timers`**: every recognized timer line from every pass, as a
  `label`, whole `milliseconds` and the real `pass` (1 for the first
  banner, 2 for the second, 0 before any banner). No timer repeats
  across passes, and the first pass's own timers (the pre-patching
  loader's load time) are real cost. A family's `count_by_pass` folds
  every pass above 16 into 16, so a timer's `pass` can exceed any key
  there.
- **`def_cache_lines`**: every def-cache plugin line, verbatim.
- **`load_events`**: each `Initializing new game with mods:` or `Loading
  game from file <save> with mods:` block, in file order, with its
  `line`, `kind` (`new_game` or `save_load`), `save_name` (`null` for a
  new game) and `mods`. `load_events_disagree` is true when two events'
  mod lists differ; the text output prints one line per event and says
  when they differ. The load-order comparison uses the last event.

The typed lists hold one record per matching occurrence, so a log full of
repeated errors makes a correspondingly long list.

Which lines form a stack-trace block, a texture fallback or a
back-reference stub is data, not code: the `log-shapes` section of the
[rules data](concepts/rules-databases.md) (the bundled snapshot, or a
fetched copy). With none loaded, the import still accounts for every line
and reports no leak, but those lines are read as ordinary messages and no
stack trace, texture size or stub folding is built from them.

#### Every line accounted for

The log is split into entries (a message plus its stack frames and detail
lines) and blank separator lines. Each entry lands in exactly one class,
and entries with the same normalized message form a family.

- **`totals`**: `lines_read`, `entry_lines`, `blank_separator_lines`,
  `entries`, `passes` (a `Player.log` only), and `conserved`, true when
  `lines_read == entry_lines + blank_separator_lines`.
- **`classes`**: one object per class present, with its `class`,
  `entries`, `lines`, an `overflow` (`entries` and `lines`, or `null`)
  once the family bounds are reached (5,000 families per class, 50,000
  overall), and its `families`, most frequent first. Each family has its
  `key`, `count`, `lines`, `first_line`, `last_line`, `count_by_pass`,
  `severity` (`message`, `warning` or `error`: the static severity of
  the engine call that writes it, or `null`), `attribution` (see
  [below](#attribution)), and a `sample` of its first entry with
  `sample_truncated` (`sample` is `null` once the 16 MiB sample budget
  is spent).

  The classes are `load_event`; the engine's own information
  (`engine_info_banner`, `engine_info_logging_stopped`,
  `engine_info_logging_resumed`, `engine_info_session_marker`,
  `engine_info_unity_runtime`); `patch_failure`, `config_error`,
  `patch_error`, `patch_stack_trace`; `cross_reference`,
  `missing_parent`, `xml_error`, `duplicate_def`; `texture_fallback`,
  `texture_load_failure`, `type_load_error`; `save_load_reference_save`,
  `save_load_reference_load`; `mod_metadata_warning`,
  `base_gen_rule_missing`, `runtime_exception`, `unity_runtime_error`,
  `def_cache_line`, `timer`, `mod_message`; and `unclassified`, which
  keeps anything the classifier does not know, counted and visible.
- **`sentinels`**: `total` counts sentinel hits, lines that match a
  known class's loose pattern but sit in another class's entry (a
  classifier gap; a line that matches two sentinels counts twice), and
  `leaks` lists the first 20 with their line numbers.

The text output adds a `totals:` line, warns first when the accounting
does not reconcile or a sentinel leaked (it says "hit(s)", and that only
the first 20 are listed when there are more), and ends with a
`classes:` section: one line per class, the three most frequent families
under each class that has attributions, and the ten most frequent
unclassified families.

#### Attribution

The typed lists carry an `attribution` object with `mod_id` and
`mod_name`, or `unattributed_raw` (the text the log gave) when nothing
matches; the others are `null`. A patch failure or stack-trace block is
attributed by its file (the active mod whose folder is the longest
prefix of the path), then by its `[Tag]` name; a texture by its path; a
dependency warning by its mod name; a timer by its `[Tag]`, else it is
unattributed with its label. In the text output an unattributed entry
reads `(unattributed: <raw>)`.

A name (a `[Tag]`, a timer's tag, a metadata warning's mod name) is
looked up three ways, in this order, and the first that finds any mod
decides: the mod's display name, case-insensitively; its package id
(`[Example.Mod]`); then its display name reduced to ASCII letters and
digits (`[ExampleMod]` for "Example Mod"; a translated prefix in the name
is ignored). The second and third must
match exactly one active mod. When several share a package id or a
compacted name, nothing is attributed: a family is `ambiguous`, a typed
entry `unattributed`.

A family's `attribution` says which active mod its first entry points
to, or is `null` when that entry carries nothing to attribute (the
engine's own messages, cross-references, and a stack with only engine
frames or an unresolved back-reference). Otherwise it is an object whose
`kind` is one of:

- `mod`, with `mod_id` and `mod_name`;
- `unattributed`, with `raw`, the text the log gave, kept as-is;
- `ambiguous`, with `raw` and `candidates` (the `mod_id` and `mod_name`
  of every active mod that ships what the log named, when that is a DLL
  several mods ship, or that a tag matches after compaction), listed in
  `mod_id` order, not load order.

What counts as evidence depends on the class:

- a patch failure or stack-trace block: its file, then its `[Tag]` name;
- a texture fallback: its texture path;
- a `[Tag]` line, a timer, or a `Mod <name> ...` metadata warning: its
  display name;
- an exception: the type of its innermost non-engine stack frame (a
  nested or compiler-generated type counts as its outer type);
- a type-load error: the type or assembly its first line names.

A type is attributed by its namespace: the longest dotted prefix that is
the name of an assembly some active mod ships decides, and only that
prefix (one mod shipping it is the answer; several make the family
`ambiguous`, never the first of them). The rule looks at that one type
and never walks outward to the frames of whatever called it, so a family
is never blamed on a mod that merely called the failing code. A frame in
an engine namespace is skipped, and a type with no namespace is never
attributed. Known gap: a mod whose namespace differs from the name of its
DLL stays `unattributed`; matching a namespace to the assembly that
defines it needs a type index of each DLL, a future analyzer feature.

Attribution is per family, not per entry, so a message repeated a
thousand times is attributed once. Because a family is attributed from
its first entry, two messages that fall into one family share that
entry's answer. That can happen when the family's key blurs them: a
type-load key shows digits as `#`, and a key longer than 256 bytes is
cut. In the text output the mod is named after the `->`, or
`(unattributed: <raw>)`, or `(ambiguous: <raw>; N mods: <first three
names>)`.

#### Reading at any size

The log is read line by line with bounded working memory, so there is no
file-size or line-count limit; only the typed lists grow, one record per
occurrence. Every bound that has to apply is counted in `read_stats`
instead of refusing the file:

- `lines_read`: the total the other counters are read against;
- `lines_truncated`: lines longer than 64 KiB, classified from their
  first 64 KiB (real logs hold engine dumps of several hundred KB on one
  line);
- `lines_with_invalid_utf8`: lines decoded with replacement characters;
- `stack_block_lines_dropped`: lines of a stack-trace block past its
  5,000-line bound, not examined;
- `stack_joins_abandoned`: wrapped multi-line operations that outgrew
  their bound;
- `passes_folded`: entries from startup passes past the 16 a family
  tracks (hostile input only);
- `stack_refs_dropped`: back-reference originals not indexed because the
  index was full;
- `logging_gaps_dropped`: gaps past the list bound of 1,000;
- `crash_report_paths_truncated`: crash-report locations cut to their
  1 KiB bound.

The text output adds one `read N lines; bounded: ...` line only when one
of those losses is non-zero.

#### JSON shape

One object per file, with these keys in order: `path` (several paths
only), `patch_failures`, `extra_stack_traces`, `cross_references`,
`dds_failures`, `dependency_warnings`, `timers`, `def_cache_lines`,
`load_events`, `load_events_disagree`, `read_stats`, `totals`, `classes`,
`sentinels`, `kind`, `coverage`, `logging_gaps`, `lower_bound_families`.
The coverage and gap objects look like this:

```json
{
  "kind": "player_log",
  "coverage": {
    "kind": "player_log",
    "passes": 2,
    "banner_lines": [21, 1480],
    "patch_phase": "patch_failure_logged",
    "end_state": { "state": "crashed", "report_path": null }
  },
  "logging_gaps": [
    { "stop_line": 52010, "end": { "kind": "resumed", "line": 52014 } },
    { "stop_line": 90112, "end": { "kind": "never_resumed" } }
  ],
  "lower_bound_families": [
    { "class": "runtime_exception", "key": "..." }
  ]
}
```

```json
{
  "kind": "console_snapshot",
  "coverage": {
    "kind": "console_snapshot",
    "entries": 1000,
    "console_fill": "at_cap",
    "head_truncated": false
  }
}
```

## Def inspection

- **`defs changes`** — lists what one mod changes: defs/templates it
  owns, foreign defs it patches, assets it overrides.
- **`defs inspect`** — everything about one def or template under a
  selected order: owners, patchers, template chain, and the effective
  (in-game) merged def.
- **`defs search`** — finds a def or template by name.

## Merge and compatibility patches

- **`merge plan`** — prints one finding's field-level diff and the
  merge plan it currently folds to.
- **`merge coverage`** — tallies contested patch collisions by whether
  their xpaths parse under Rimmerge's own grammar, and — for the ones
  that do — by what an actual replay makes of them.
- **`patch new` / `list` / `show` / `plan` / `decide` / `revert` /
  `import` / `prune` / `export` / `delete`** — a compatibility patch is
  a user-chosen scope of two or more mods with its own decisions,
  exported as a publishable mod. See
  [concepts/merge.md](concepts/merge.md).

## The patch maker (assignment projects)

`assign` infers a generic reference-mod-to-target-mod assignment
schema, then lets you edit and export one `Defs/`-only mod per project
— e.g. wiring a race-support mod's own fields onto every active race,
without a human writing that XML by hand.

- **`assign propose`** — infers every assignment-def candidate for a
  reference/target selection, without persisting anything.
- **`assign create`** — re-proposes candidates and persists the one
  matching `--def-type`.
- **`assign list` / `show` / `update` / `delete`** — project
  management.
- **`assign set-row` / `clear-row` / `copy-from`** — edits one row,
  either target-keyed (`--target`/`--key-field`) or free-standing.
- **`assign add-section` / `remove-section`** — a project can cover
  more than one def type at once; a section is one def type's own rows.
- **`assign items`** — lists defs of one item type across the active
  list (what a row's own value could point at).
- **`assign coverage`** — the work queue: every candidate target, what
  already references it, and (under a known rule) who would win.
- **`assign export`** — renders and writes the assignment into a
  user-chosen folder, optionally installing it into the game's `Mods/`
  folder.

See [concepts/merge.md](concepts/merge.md) for how this relates to
compatibility patches.
