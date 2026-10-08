# Quickstart

The same four steps, whether you use the CLI or the desktop app: scan,
review, sort, apply. The desktop app also walks you through the last
part (use the suggested order, Apply, confirm) on its Dashboard; see
[the guided path](#on-the-desktop-the-guided-path). Every command below was run against a scratch
profile before this page was written — copy them as-is.

## 1. Scan

Rimmerge reads your install, your active mod list, and every active
mod's own content (defs, patches, assemblies, textures) into one
report, written into a profile directory. Nothing here writes to your
install.

```
rimmerge load --profile-dir <profile>
```

`--game-dir`/`--workshop-dir`/`--mods-config`/`--profile-dir` are all
optional — see [install.md](install.md) for what each one defaults to
when omitted. `load` prints the profile directory it used and the path
of the report it wrote (`<profile>/report.json`); the commands below
read that file by path, not by profile.

The desktop app does this automatically when you open or create a
profile, and again whenever you click Rescan. A project opens with the
**suggested** order selected, so the steps below start from it.

## 2. Review the inbox

The scan's report feeds a **ledger**: one finding per thing the sorter
or the merge evaluator noticed — two mods editing the same def, a
missing dependency, an ambiguous template, and so on. Each finding
carries a suggested action and a confidence score; see
[concepts/ledger.md](concepts/ledger.md) for the full model.

```
rimmerge ledger --report <profile>/report.json --status needs-input
```

On the desktop app this is the Inbox page. You don't have to clear
every finding before sorting — an undecided finding just means the
sorter falls back to its own default handling for that case, same as it
always would.

## 3. Sort

`sort` previews an order built from the report alone — its own edges
and tags — without reading your saved decisions or rules; it's a quick
look, not what `apply` will actually use.

```
rimmerge sort --report <profile>/report.json
```

The desktop app's Order page shows the *real* suggested order — built
from your profile's actual decisions and rules, the same way `apply`
does — with a why-panel explaining any given mod's position.

## 4. Apply

This is the one step that can write to `ModsConfig.xml`. Everything
above it is read-only by construction.

```
rimmerge apply --profile-dir <profile> --dry-run   # preview only, writes nothing
rimmerge apply --profile-dir <profile>              # writes ModsConfig.xml
```

`apply` writes the suggested order by default (`--source current` writes
the order already in your file). It re-derives that order itself, from the
profile's own report and rules, rather than reading anything `sort`
printed — the two flags above are the whole command, no report path
needed. Before writing or diffing, it lists any **hard problems** in the
order it is about to write (a required mod that is not active, two active
mods declared incompatible, a mod in `ModsConfig.xml` that is not
installed, and so on) under a `hard problems in the Suggested order`
header. The list is information only: it does not change the exit code or
ask anything.

### On the desktop: the guided path

The Dashboard shows the same flow as four steps, read from what the app
already knows (nothing about your progress is saved, except that you
skipped step one):

1. **Get the recommended rules.** One click downloads the community
   rules and the Steam Workshop database (about 49 MB) and imports them,
   so the suggested order already uses them. It needs internet access
   unless only an import is left, and never runs without the click;
   **Skip** moves on, and **Get them now** stays available, still listing
   what it will do and the download size.
2. **Use the suggested order.** It is already selected when a project
   opens. If you switched to Current, one button switches back; a link
   takes you to the Load order page to look first.
3. **Apply.** Opens the Apply dialog, which shows the diff before it
   writes anything and a stale-order warning if your install has changed
   since the order was built (the strip offers Rescan instead of Apply
   until you do).
   Findings that still need your input are shown as a count with a link
   to the Inbox; they do not block Apply.
4. **Confirm.** If the order has hard problems you have not already
   decided in the Inbox, an "Apply anyway?" panel lists them first, with
   **Go back** focused. Nothing is asked when there are none, or when you
   already decided each one. If RimWorld looks like it is running, a
   separate "Write anyway" prompt comes after this.

The strip reads **Done** once `ModsConfig.xml` matches the suggested
order, as of the last scan or apply. See
[desktop.md](desktop.md#dashboard) for the details.

## The safety rules, stated plainly

- Scanning, sorting, the ledger, and verify are read-only. Only these
  write, and only when you run them:
  - `apply`, `mods activate`/`deactivate` and `order import` (without
    `--dry-run`) write
    your real `ModsConfig.xml`.
  - `apply --write-merge-mod` and `patch export`/`assign export` with
    `--install` write a generated mod folder under your RimWorld
    install's `Mods` folder and add it to `ModsConfig.xml`.
  - `order export --out` (and the desktop app's Export menu) writes the
    one `.rml` file you name, and never touches `ModsConfig.xml`.

  Every `ModsConfig.xml` write takes a timestamped backup first, and
  none of these run while RimWorld looks like it is running unless you
  pass `--force`. See [SECURITY.md](../SECURITY.md) for the full list.
- Rimmerge never touches your RimSort install or its own config/rule
  files — it only ever *reads* a RimSort rule database you explicitly
  import (see [concepts/rules-databases.md](concepts/rules-databases.md)).
- Rimmerge contacts exactly two GitHub hosts: a rule-database refresh
  and a check for a newer Rimmerge version. The desktop app runs both
  automatically (the version check at each launch, the rules refresh at
  most once a day), but never before its first-run notice explains this
  and offers to turn it off; the largest database
  (the Steam Workshop one, about 49 MB) is on by default but never
  fetched automatically, only by a manual Refresh click, `rimmerge db
  refresh`, the recommended-databases notice's "Turn on and download"
  button, or the Dashboard's "Get the recommended rules" step. The CLI
  never contacts either host on its own. See
  [privacy-and-network.md](privacy-and-network.md).

## Going further

- Want Rimmerge to also predict which patches RimWorld would fail to
  apply, before you launch the game? See
  [concepts/verify.md](concepts/verify.md).
- Two mods fighting over the same def, and you want a merged def that
  keeps both sets of changes? See [concepts/merge.md](concepts/merge.md).
- Want to check every CLI flag? See [cli.md](cli.md).
