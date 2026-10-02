# Rule databases

Not every ordering fact has an engine mechanism behind it (see
[load-order.md](load-order.md)) — sometimes a load-order requirement is
genuinely community knowledge with no file Rimmerge can read to derive
it. Rule databases are how that knowledge reaches the sorter, always as
an explicit, reviewable import, never silently.

## Three sources, one pipeline

Fetch → cache → import → sort, and each arrow is a separate, explicit
step:

1. **Fetch** pulls a database into a local cache. For the community
   rules and `rimmerge-rules` sources, this now happens **automatically,
   once a day, at desktop launch** — gated by the master network switch
   and the automatic-refresh toggle (see
   [settings.md](../settings.md) and
   [privacy-and-network.md](../privacy-and-network.md)) and by each
   source's own enabled/disabled flag. The **Steam Workshop database
   stays manual only**, even though it is enabled by default — it's about
   49 MB, and an automatic daily download of that size is exactly what
   being enabled is not meant to imply. A manual `rimmerge db refresh`/Refresh click
   still works for every source, automatic or not, and the CLI never
   fetches anything on its own outside that explicit command.
2. **Import** stays entirely manual for every source, automatic-fetch
   or not. Nothing is ever imported automatically — you choose when,
   from the Rules page's Databases card, `rimmerge import`, or the
   Dashboard's Get started step "Get the recommended rules": one click
   that downloads what is missing and imports it, still never silently.
   With fetches now happening on their own, a fresh cached copy can
   arrive with no user action; the Databases card's own "needs reimport"
   marker is how you learn an import is available.
3. **Sort** uses whatever rules are in your profile, regardless of
   where they came from — a rule you typed by hand and an imported one
   look identical to the sorter once imported.

A refreshed `rimmerge-rules.json` takes effect from the **next time a
profile loads** (the next `load`/rescan, or the next time the desktop
app opens that profile) — it improves how `verify` and the findings
list model specific mods' own patch operations and precedence. It never
changes an already-sorted load order on its own, and it is never
consumed by the sorter itself (see "An imported rule never outranks a
fact" below).

The three sources:

- **RimSort community rules** — the same community-maintained pair/
  load-order database RimSort itself uses.
- **Steam Workshop database** — a much larger, Workshop-derived
  database (about 49 MB). It is recommended and on by default, but never
  downloaded automatically: only a manual action (the Databases card's
  Refresh, `rimmerge db refresh`, the recommended-databases notice's
  "Turn on and download" button, or the Dashboard's "Get the recommended
  rules" step) fetches it, for exactly that reason.
- **`rimmerge-rules`** — this project's own small database: verified
  precedence rules for specific def-ownership conflicts, custom
  patch-operation class behaviors `verify` needs to model correctly,
  and def-cache detection data. On by default — it's small,
  it's this project's own, and a stale copy is the difference between
  `verify` correctly modeling a framework's custom patch operations and
  reporting them as unsupported.

## An imported rule never outranks a fact

An imported pair or placement rule sits below every `Declared`/`Hard`
engine-derived edge in the sorter's own layering (see
[sorting.md](sorting.md)) — it can settle a case the engine facts leave
ambiguous, but it can never overrule something RimWorld itself would
enforce. A rule you set yourself, or promote from an import (so it
survives a re-import or the import being toggled off), always outranks
a merely-imported one.

## The `rimmerge-rules` format

A fetched `rimmerge-rules.json` is one envelope over independent
sections, each with its own schema so a malformed row in one section
never invalidates the others:

- **precedence** — which def type has a verified rule for resolving a
  multi-owner conflict (e.g. "prefer the def outside the framework
  mod").
- **patch-operations** — which custom (non-vanilla) patch-operation
  classes map onto which behavior, so `verify`'s replay can model them
  instead of reporting `Unsupported`.
- **def-cache-carriers** — which mods/plugins act as a def-cache plugin,
  so their own startup timing is attributed correctly.
- **log-shapes** — the game-log line formats that mods (not the game)
  print, in a `Player.log` and a console snapshot alike: a
  patch-reporting mod's stack-trace block, a texture loader's fallback
  lines, and a patching library's back-reference stubs. Each is a
  *format template* (literal text plus typed placeholders such as
  `{path:text}`), never a regex, so a data file can't inject regex
  syntax; a template is capped at 512 bytes and 8 placeholders, needs
  at least 4 bytes of literal text, and a role holds at most 8 rows. A
  row with an unknown `match` mode or placeholder type, a template over
  a bound, or a missing required capture is ignored with a warning, and
  so is a role this binary does not know. `log import` uses these to keep a
  stack-trace block together, pair it with its terse failure, read a
  texture fallback's size, and fold back-reference stubs; with none
  loaded the same log still parses and every line is still accounted
  for, but those formats' lines land in the generic classes and no typed
  record is built from them.
- **tag-rules** — parsed and validated (a malformed row is still
  reported), but **not yet consumed anywhere**: never read into a
  running session, and, unlike the community and Steam Workshop
  databases above, there is no import path from this source into your
  profile's `rules.json` today either. A row here currently has no
  effect on anything Rimmerge does; a tag rule you want applied still
  has to be written by hand or inferred.

An unknown top-level key is reported as a warning, never silently
ignored and never a hard failure — a newer data file must not break an
older binary, but a typo in a section name should be visible, not
silently read as "this section is simply absent." The envelope's own
`schema` field is required for exactly this reason: it's what
distinguishes a real rules file from an arbitrary JSON object.

If nothing has ever been fetched, or a refresh is disabled, Rimmerge
falls back to a snapshot of `rimmerge-rules.json` bundled into the
binary at build time — there is never a state with no
precedence/patch-operation/def-cache/log-format knowledge at all, only a
potentially stale one. See
[`rimmerge-rules`'s own README](https://github.com/Rimmerge-Project/rimmerge-rules#how-rimmerge-uses-this-file)
for how that bundling works. `rimmerge db status` and the desktop's
Databases card both show that bundled snapshot's own sha256 next to the
`rimmerge` row (`[bundled: ...]` on the CLI, `· Bundled ...` on the
card) — it never changes at runtime, only at the next build.

## Reminders

Automatic refresh doesn't cover every case — you might be offline for
weeks, or GitHub might keep failing — so a **reminder** notice can still
appear, using an app-global staleness threshold (`reminders
.rule_databases_stale_after_days` in [settings.md](../settings.md),
default 30 days, the same threshold `rimmerge db status`'s own
`[stale]` label and the Databases card use).

- **Internet access off**: no reminder — you chose offline, and a
  reminder would only nag. Settings shows a persistent line instead.
- **Automatic refresh on, source eligible, still fresh**: no reminder.
- **Automatic refresh on, source eligible, but the last success is
  older than the threshold**: a reminder, naming the last successful
  date and the last failure reason if there is one. Actions: refresh
  now, open Settings.
- **Automatic refresh off, source stale**: a reminder naming how many
  days it's been.
- **The Steam Workshop database, enabled and stale**: the same reminder
  as the row above — it's manual only regardless of the automatic
  toggle.
- **Never fetched, and the first-run notice hasn't been answered yet**:
  no reminder — the first-run notice covers it.
- **Never fetched, first-run notice answered, automatic refresh on**:
  no reminder for the first 24 hours (giving the first automatic
  attempt time to run); after that, treated the same as a failed
  automatic refresh.

The reminder can be muted ("don't remind me again"), and it never
changes behavior on its own: staleness never triggers a fetch, never
blocks an import, and never changes a sort.

### Recommended databases not set up

A second, separate notice covers what the reminder above deliberately
skips: a recommended source that is **off**, or **on but never
downloaded** where no automatic refresh will ever fetch it (the Steam
Workshop database, or any source while automatic refresh is off). The two
notices partition the sources — a source is never in both: the stale
reminder covers "fetched, then went stale" and "automatic and failing",
this one covers "off" and "manual and never fetched". A never-fetched
source that automatic refresh does cover is left to the reminder, after
its 24-hour grace.

- It is informational, can be dismissed or muted, and sorts last. A
  dismissal hides exactly the listed set: it comes back when a source is
  added to or removed from the list, or a source's state changes (turned
  on but still not downloaded).
- It never appears before the first-run notice is answered, and never
  while internet access is off — so a damaged settings file, which loads
  with everything off, never prompts anyone to switch a source back on.
- **Turn on and download** turns every recommended source on (the list of
  which sources are recommended lives in the backend), then runs the same
  manual refresh as the Databases card's Refresh button. The click is the
  consent for the download, including the Steam Workshop database (about
  49 MB) when it is in the list. It never changes the internet-access
  switch, and it never imports anything: importing stays its own step.
- **Hand-off to the Dashboard step.** While the loaded profile's "Get the
  recommended rules" step is offered, this notice and the "New rule
  content to import" notice leave out the sources that step covers, so
  one call to action is shown at a time. Once the step is done, skipped
  or not available, the notices list those sources again.

See [privacy-and-network.md](../privacy-and-network.md) for exactly which
host any of this is ever allowed to reach.
