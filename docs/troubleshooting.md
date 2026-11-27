# Troubleshooting

**Rimmerge says "no RimWorld install found".** Rimmerge couldn't
auto-detect your install and nothing pinned it either. Pass `--game-dir <path>` once, or
run `rimmerge config set --game-dir <path>` to pin it permanently. See
[install.md](install.md) for the full detection ladder and where each
platform is searched.

In the desktop app, the Setup page lists the locations that were checked.
Enter or browse to your game folder instead (for Steam,
`steamapps\common\RimWorld` inside the library that holds the game; if
your Steam is outside Program Files, that is the folder on its own drive),
and the workshop folder and profile fill in from it. Then choose **Save as
defaults** so the next launch starts from it.

**A finding I decided keeps reappearing.** A decision is remembered
per finding *key*. If the underlying facts change enough (a mod
updated, a def restructured) that the key no longer matches cleanly,
the finding reopens rather than silently keeping a stale decision. See
[concepts/ledger.md](concepts/ledger.md).

**`apply` refuses to write.** Check the reported reason. A stale order
(your active mods changed since the order was built; rescan and re-sort
first) and a running game process are both refused on purpose, never
silently overridden.

**A refresh fails or is skipped.** Check `network.allow_network` and
the per-source fetch toggle first (see [settings.md](settings.md)). A
skip reports which of the two blocked it, or that it isn't due yet (the
automatic once-a-day cadence). Refreshing needs `raw.githubusercontent.com`
to be reachable; see [privacy-and-network.md](privacy-and-network.md).

**GitHub's rate limit was reached.** This is normal, not an error.
Rimmerge waits until GitHub's own stated reset time (at most a day)
before asking again, whether that's an automatic check/refresh or
`rimmerge check-update`/`db refresh`. Settings and `rimmerge db status`
both show when it's safe to try again.

**Internet access is off after a settings-file error.** If
`app-settings.json` is ever damaged or unreadable, Rimmerge treats
internet access as **off** until you save settings again (Settings →
Internet access → Restore internet access defaults, or `rimmerge network reset`).
This is a deliberate fail-closed default, not a bug. `rimmerge network on`
alone only flips the master switch. It leaves the automatic-feature and
per-source fetch toggles as the recovered (all-off) load left them.
`reset` is the command that restores every switch at once.
`rimmerge db status` and the desktop Settings page both name this case
when it applies.

**`verify` predicts a failure I never saw in-game.** A prediction is
a static replay, not a guarantee. Import your actual `Player.log` or a console snapshot (`rimmerge log import`) to compare a
prediction against what RimWorld really logged. See [concepts/verify.md](concepts/verify.md).
On Windows, RimWorld 1.6 writes `Player.log` (and `Player-prev.log`, the
run before) to
`%USERPROFILE%\AppData\LocalLow\Ludeon Studios\RimWorld by Ludeon Studios`.
RimWorld 1.7 writes them to the `Logs` folder inside it
(`...\RimWorld by Ludeon Studios\Logs\Player.log`), and starting 1.7
deletes the old 1.6 `Player.log` and `Player-prev.log` from the folder
above, so copy a 1.6 log you want to keep before you first start 1.7.

**Something looks wrong and none of the above covers it.** Open an
issue with your Rimmerge version, OS, and (if it doesn't involve a
specific mod's private content) a copy of the report or ledger output
showing the problem.
