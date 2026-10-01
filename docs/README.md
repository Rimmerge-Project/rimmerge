# Rimmerge docs

Rimmerge is a load-order sorter, conflict ledger, and merge-patch
generator for RimWorld. It reads your install and `ModsConfig.xml`,
builds a graph of every ordering-relevant relationship between your
active mods, and turns that graph into a suggested load order plus a
ledger of findings you can accept, override, or leave for later.

Start here:

- **[install.md](install.md)** — where Rimmerge finds your RimWorld
  install, workshop folder, and `ModsConfig.xml`, and how to point it
  somewhere else.
- **[quickstart.md](quickstart.md)** — scan, review the inbox, sort,
  apply, in that order, plus the safety rules that keep this tool from
  ever touching your install without asking.
- **[cli.md](cli.md)** — every `rimmerge` subcommand.
- **[desktop.md](desktop.md)** — the desktop app, page by page.
- **[settings.md](settings.md)** — every setting: its default, what it
  changes, and whether it can change the sorter's output.

How it works:

- **[concepts/load-order.md](concepts/load-order.md)** — how RimWorld
  itself decides load order, and what that means for a sorter.
- **[concepts/sorting.md](concepts/sorting.md)** — tiers, edge
  strengths, tie-breaks, and how Rimmerge turns evidence into an order.
- **[concepts/ledger.md](concepts/ledger.md)** — findings, the
  confidence table, decisions, and how they persist.
- **[concepts/verify.md](concepts/verify.md)** — the verify pass:
  replaying patches against a chosen order to predict what RimWorld's
  own log would say.
- **[concepts/merge.md](concepts/merge.md)** — the merge mod,
  compatibility patches, and assignment projects (the patch maker).
- **[concepts/rules-databases.md](concepts/rules-databases.md)** —
  RimSort import, `rimmerge-rules`, and the network policy.

Reference:

- **[architecture.md](architecture.md)** — the crate graph, the
  layering rule, and where new code belongs.
- **[privacy-and-network.md](privacy-and-network.md)** — the single
  host this tool is ever allowed to talk to, and the offline switch.
- **[testing.md](testing.md)** — for contributors: the real-install
  test tier, the synthetic fixture, and how to regenerate it.
- **[translating.md](translating.md)** — for translators: the locale
  files, placeholders and plurals, the glossary, and the checks.
- **[dependency-versions.md](dependency-versions.md)** — pinned
  toolchain and library versions.
- **[troubleshooting.md](troubleshooting.md)** — common problems.
- **[CHANGELOG.md](CHANGELOG.md)**

## A note on history

This project has a longer development history than these pages
describe — design decisions, real-install measurements, and the
reasoning behind them, recorded as they happened. That history lives in
`.journal/` in a maintainer's own checkout (gitignored, never
published); these public pages describe the *current* behavior only,
grounded in the code and in RimWorld's own engine semantics, never in a
specific mod or a past decision's own name.
