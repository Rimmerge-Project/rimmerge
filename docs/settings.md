# Settings

Settings split into two scopes, stored in two different places:

- **App settings** (`<base>/app-settings.json`) — internet access, the
  automatic-refresh feature toggles, and the staleness-reminder
  threshold. One per machine, shared by every profile and by both the
  desktop app and the CLI. Editable from the desktop app's Settings →
  Internet access section, or with `rimmerge network status|on|off|reset`.
- **Profile settings** (the profile's `rules.json`) — the sort and
  ledger settings. One per profile (per `ModsConfig.xml`). Editable
  from the desktop app's Settings page or by hand-editing that
  `rules.json`.

Both tables list each field's default and — the question that matters
most — whether changing it can change what the sorter or ledger
produce. Every app-setting row is **No**: none of them can change a
sort, only whether/when a network request happens.

## App settings (this machine)

| Setting | Default | Changes sort output? |
|---|---|---|
| `network.allow_network` | on | No — the master switch, shown as **Allow internet access** (only the two GitHub hosts, never your local network). Off, every request (automatic or manual: an automatic check/refresh, `db refresh`/`check-update`/Refresh/Check now) is skipped before any address is built or connection opened. If `app-settings.json` itself is unreadable or corrupt, this loads as **off** until you save settings again — see [privacy-and-network.md](privacy-and-network.md). |
| `network.check_for_updates` | on | No — whether the desktop app checks for a newer Rimmerge release automatically, once per launch. `rimmerge check-update` (a manual, explicit check) ignores this toggle. |
| `network.auto_refresh_rule_databases` | on | No — whether the desktop app automatically, once a day, at launch, refreshes the enabled, auto-refresh-eligible rule databases (community rules and `rimmerge-rules` — never the Steam Workshop database, which stays manual regardless). A manual `db refresh`/Refresh click ignores this toggle. |
| `network.fetch_community_rules` | on | No — whether a refresh (automatic or manual) is allowed to fetch the community rules database into the cache at all. Fetching never touches the sort by itself; only `import` (a separate, explicit step) does. |
| `network.fetch_steam_workshop` | on | No, same reasoning — the source is recommended, so it is on by default, but it is a large download (about 49 MB) that must never happen without you asking: it is never fetched automatically, only by a manual *Refresh* click, `rimmerge db refresh`, the recommended-databases notice's *Turn on and download* button, or the Dashboard's *Get the recommended rules* step. Turn it off to keep those from fetching it (`rimmerge network set --steam-workshop off` changes only this switch). |
| `network.fetch_rimmerge_rules` | on | No, same reasoning — this project's own small rules database (precedence rules, patch-operation behaviors, def-cache-carrier detection). Turning it off leaves the defaults built into the binary in effect, never nothing. |
| `reminders.rule_databases_stale_after_days` | 30 (range 1–365) | No — the threshold `rimmerge db status`'s `[stale]` label, the Databases card, and the staleness reminder notice all share. See [concepts/rules-databases.md](concepts/rules-databases.md#reminders). |

Restore every internet-access switch back on with **Settings → Internet
access → Restore internet access defaults**, or `rimmerge network reset` — `rimmerge
network on` alone only flips the master switch (plus whichever of
`--updates`/`--auto-refresh`/`--community-rules`/`--steam-workshop`/
`--rimmerge-rules` you pass); `reset` is the one command that restores
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
| `threshold` | 80 | Yes — the confidence line separating an auto-applied suggestion from one that needs your input. |
| `enforce.soft` | off | Yes — whether a `Soft`-strength edge (a DLL-load-time relationship RimWorld doesn't actually enforce) constrains the order or is merely advisory. |
| `enforce.awareness` | off | Yes — same, for `Awareness`-strength edges (evidence two mods interact, with no explicit ordering promise). |
| `enforce.inferred` | on | Yes — whether a heuristic edge the analyzer infers itself (not read off an author's own declaration) constrains the order. On by default: a heuristic edge is the analyzer's own conclusion, not a mere presence signal. |
| `suggest_merge_when_clean` | on | No, changes *ledger suggestions* only — when a def-override or patch-collision finding's merge preview turns out clean, its suggested action leads with Merge instead of leaving it as a plain accept/prefer-winner choice. Never changes the load order. |
| `tie_break` | `Rebuild` | Yes — which base position an otherwise-unconstrained mod starts from. `Rebuild` ranks by normalized mod name; `PreserveCurrent` starts from the mod's current position in `ModsConfig.xml`, for minimal disturbance from an order you already trust. |
| `use_imported_pairs` | on | Yes, if you've imported a RimSort rule database — whether an imported pair rule (`X after Y`) feeds the sorter. A rule you set yourself (`rule set-pair`, or a promoted import) is never affected by this toggle. |
| `use_imported_placements` | on | Yes, same condition — whether an imported placement rule (top/bottom pin) feeds the sorter. |
| `show_dangling_def_references` | off | No, changes *ledger findings* only — whether a "dangling def reference" finding (a name written at a recognized reference site that no active def of any type actually has) is surfaced at all. Off by default: measured against a real, large install, the false-positive rate was high enough (self-identifying fields such as `defName`/`identifier` mistaken for cross-references, among other causes — see [concepts/ledger.md](concepts/ledger.md)) that this finding needs an opt-in rather than showing by default. Never changes the load order — the finding carries no ordering edge. |

Refill the form from these defaults with **Settings → Reset to
defaults** (review and Save to apply — nothing is saved until you do),
or start a fresh profile from them with the Welcome notice's **Use
recommended settings** (saves immediately).

## The Settings page also has a language picker

The desktop app's Settings page hosts one more control neither table
above covers: a Language picker (English/`zh-CN`/`pt-BR`). That's a
display preference for the UI itself, stored separately from both
scopes above — it can never change what the sorter or ledger produce,
the same "no" every purely cosmetic row in either table already gets.
See [desktop.md](desktop.md#language).

## Why fetch/import are two separate steps

Every `network.fetch_*`/`network.allow_network` setting only governs
whether a refresh (automatic or manual) is allowed to *reach the
cache*. None of them can change what a sort produces — no sort path
ever touches the network or the on-disk cache. Whether an
already-fetched rule actually *feeds the sorter* is entirely
`use_imported_pairs`/`use_imported_placements`, applied identically no
matter how the rule reached your profile's `rules.json`
(fetched-then-imported or hand-typed, the sorter can't tell the
difference). See [concepts/rules-databases.md](concepts/rules-databases.md)
for the full fetch → cache → import → sort pipeline. The Dashboard's
"Get the recommended rules" step does both on one click, still as two
steps (download, then import), and adds no setting of its own.
