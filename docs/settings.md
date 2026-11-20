# Settings

Settings and saved state live in three places, each with its own file:

- **App settings** (`<base>/app-settings.json`): internet access, the
  automatic-refresh feature toggles, and the staleness-reminder
  threshold. One per machine, shared by every profile and by both the
  desktop app and the CLI. Editable from the desktop app's Settings →
  Internet access section, or with `rimmerge network status|on|off|reset`.
- **Profile settings** (the profile's `rules.json`): the sort and
  ledger settings. One per profile (per `ModsConfig.xml`). Editable
  from the desktop app's Settings page or by hand-editing that
  `rules.json`.
- **Library organization** (the profile's `organization.json`): your mod
  groups and mod colours. One per profile, a plain JSON file you can copy
  between profile folders. It is separate from the settings above on
  purpose: nothing in it can change a sort or a ledger. If the file cannot
  be read, groups become read-only until you start over (the Mods page
  shows a notice with a Start over button; if the file may only be locked
  by another program, try again first). Starting over first keeps a copy
  beside it as `organization.json.unusable-<time>`.

The two tables below list each field's default and whether changing it can
change what the sorter or ledger produce, which is the question that
matters most. Every app-setting row is **No**: none of them can change a
sort, only whether and when a network request happens.

## App settings (this machine)

| Setting | Default | Changes sort output? |
|---|---|---|
| `network.allow_network` | on | No. The master switch, shown as **Allow internet access** (only the two GitHub hosts, never your local network). Off, every request (automatic or manual: an automatic check/refresh, `db refresh`/`check-update`/Refresh/Check now) is skipped before any address is built or connection opened. If `app-settings.json` itself is unreadable or corrupt, this loads as **off** until you save settings again; see [privacy-and-network.md](privacy-and-network.md). |
| `network.check_for_updates` | on | No. Whether the desktop app checks for a newer Rimmerge release automatically, once per launch. `rimmerge check-update` (a manual, explicit check) ignores this toggle. |
| `network.auto_refresh_rule_databases` | on | No. Whether the desktop app automatically refreshes the enabled, auto-refresh-eligible rule databases at launch, once a day. Those are the community rules and `rimmerge-rules`, never the Steam Workshop database, which stays manual regardless. A manual `db refresh`/Refresh click ignores this toggle. |
| `network.fetch_community_rules` | on | No. Whether a refresh (automatic or manual) is allowed to fetch the community rules database into the cache at all. Fetching never touches the sort by itself; only `import` (a separate, explicit step) does. |
| `network.fetch_steam_workshop` | on | No, same reasoning. The source is recommended, so it is on by default, but it is a large download (about 49 MB) that must never happen without you asking. It is never fetched automatically, only by a manual *Refresh* click, `rimmerge db refresh`, the recommended-databases notice's *Turn on and download* button, or the Dashboard's *Get the recommended rules* step. Turn it off to keep those from fetching it (`rimmerge network set --steam-workshop off` changes only this switch). |
| `network.fetch_rimmerge_rules` | on | No, same reasoning. This project's own small rules database (precedence rules, patch-operation behaviors, def-cache-carrier detection). Turning it off leaves the defaults built into the binary in effect, never nothing. |
| `reminders.rule_databases_stale_after_days` | 30 (range 1–365) | No. The threshold `rimmerge db status`'s `[stale]` label, the Databases card, and the staleness reminder notice all share. See [concepts/rules-databases.md](concepts/rules-databases.md#reminders). |

Restore every internet-access switch back on with **Settings → Internet
access → Restore internet access defaults**, or `rimmerge network reset`. `rimmerge
network on` alone only flips the master switch (plus whichever of
`--updates`/`--auto-refresh`/`--community-rules`/`--steam-workshop`/
`--rimmerge-rules` you pass). `reset` is the one command that restores
every switch at once, including the three per-source fetch toggles,
without hand-editing `app-settings.json`. That includes the Steam
Workshop database, which is on by default: restoring defaults turns it
back on, and a later *Refresh* (or `rimmerge db refresh`) downloads it,
about 49 MB.

Answering the first-run notice on the desktop (any of its buttons, or
closing it) writes `app-settings.json` if it does not exist yet, so the
choices you were shown stay yours if a later version changes a default.
A damaged file is never overwritten by that.

## Profile settings (this install)

| Setting | Default | Changes sort output? |
|---|---|---|
| `threshold` | 80 | Yes. The confidence line separating an auto-applied suggestion from one that needs your input. |
| `enforce.soft` | off | Yes. Whether a `Soft`-strength edge (a DLL-load-time relationship RimWorld doesn't enforce) constrains the order or is merely advisory. |
| `enforce.awareness` | off | Yes. The same, for `Awareness`-strength edges (evidence two mods interact, with no explicit ordering promise). |
| `enforce.inferred` | on | Yes. Whether a heuristic edge the analyzer infers itself (not read off an author's own declaration) constrains the order. On by default: a heuristic edge is the analyzer's own conclusion, not a mere presence signal. |
| `suggest_merge_when_clean` | on | No, changes *ledger suggestions* only. When a def-override or patch-collision finding's merge preview turns out clean, its suggested action leads with Merge instead of leaving it as a plain accept/prefer-winner choice. Never changes the load order. |
| `tie_break` | `Rebuild` | Yes. Which base position an otherwise-unconstrained mod starts from. `Rebuild` ranks by normalized mod name; `PreserveCurrent` starts from the mod's current position in your active order (`ModsConfig.xml`), for minimal disturbance from an order you already trust. |
| `use_imported_pairs` | on | Yes, if you've imported a RimSort rule database. Whether an imported pair rule (`X after Y`) feeds the sorter. A rule you set yourself (`rule set-pair`, or a promoted import) is never affected by this toggle. |
| `use_imported_placements` | on | Yes, same condition. Whether an imported placement rule (top/bottom pin) feeds the sorter. |
| `show_dangling_def_references` | off | No, changes *ledger findings* only. Whether a "dangling def reference" finding (a name written at a recognized reference site that no active def of any type has) is surfaced at all. Off by default: measured against a real, large install, the false-positive rate was high enough that this finding needs an opt-in. Self-identifying fields such as `defName`/`identifier` mistaken for cross-references were one of several causes. See [concepts/ledger.md](concepts/ledger.md) for what the finding covers. Never changes the load order, because the finding carries no ordering edge. |

Refill the form from these defaults with **Settings → Reset to
defaults** (review and Save to apply; nothing is saved until you do),
or start a fresh profile from them with the Welcome notice's **Use
recommended settings** (saves immediately).

## The Settings page also has a language picker

The desktop app's Settings page hosts one more control neither table
above covers: a Language picker (English plus twelve translated
languages). That's a display preference for the UI itself, stored
separately from both scopes above. It can never change what the sorter
or ledger produce.
See [desktop.md](desktop.md#language).

## Why fetch/import are two separate steps

Every `network.fetch_*`/`network.allow_network` setting only governs
whether a refresh (automatic or manual) is allowed to *reach the
cache*. None of them can change what a sort produces, because no sort
path ever touches the network or the on-disk cache. Whether an
already-fetched rule *feeds the sorter* is decided only by
`use_imported_pairs`/`use_imported_placements`. They apply the same way
no matter how the rule reached your profile's `rules.json`: the sorter
can't tell a fetched-then-imported rule from a hand-typed one. See [concepts/rules-databases.md](concepts/rules-databases.md)
for the full fetch → cache → import → sort pipeline. The Dashboard's
"Get the recommended rules" step does both on one click, still as two
steps (download, then import), and adds no setting of its own.
