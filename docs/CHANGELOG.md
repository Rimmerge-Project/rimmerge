# Changelog

This changelog starts at the first public release. Development history
before that point is not summarized here — see `.journal/` in a
maintainer's own checkout for the full record, if you have it.

## [Unreleased]

- **Faster Verify, merge mod and loading on large installs.** Verify (in the
  Apply dialog, and `rimmerge verify`) no longer rebuilds the same list of
  defs for each def it checks, and checks defs on four threads: on a test
  install of about 1,000 active mods, verifying one load order (including
  the check for orders that would fix a failure) went from about 78 s to about
  12 s (release build). Working out the merge mod is about three times
  faster (about 3.4 s to about 1.2 s), and the analysis after the per-mod
  scan is about a quarter faster (about 4.4 s to about 3.2 s). Both use a
  little more memory while they run (about 100 MB at most). Results are
  unchanged.
- **Faster loading, Rescan and load-order import on large installs.** The
  analysis that runs after the per-mod scan is about four times faster: on a
  test install of about 1,000 active mods it went from about 27 s to about
  6 s (release build). Its results are unchanged.
- **Fix: a change no longer reports failure when it succeeded.** Applying,
  activating or deactivating mods, deciding a finding, or changing merge
  choices could show an error toast (and keep a dialog open) after the change
  had been saved, when an unrelated page's data failed to refresh. The change
  now finishes as a success, and the page that failed shows its own error.
- **Fix: Launch RimWorld acts on the latest game status.** A click that
  overlapped a background refresh could act on the status from before it: an
  extra "not applied" error, a click that did nothing, or the Apply-first
  prompt giving the wrong reason.

## [1.2.0] - 2026-10-08

- **Fix: importing a load order no longer keeps spinning after the progress
  bar finishes, and progress no longer stops early.** "Use this order" now
  closes the preview as soon as the rescan lands instead of waiting for
  every page to refresh. The Apply dialog now works out its merge mod when
  it opens rather than on every page, with a "Loading…" line and the
  *Write merge mod* box disabled until it answers. The progress bar now
  covers the whole analysis after the per-mod scan, and shows as
  indeterminate (instead of a full or empty bar) while that single step runs.
- **New: Launch RimWorld.** A button in the sidebar, under Apply, and at
  the end of the Dashboard's Get started strip once its steps are done
  starts RimWorld: through Steam for a Steam copy, or by running
  `RimWorldWin64.exe` from the install folder for any other copy. If
  `ModsConfig.xml` doesn't hold the selected order, or activation changes
  are waiting for a rescan, it offers to apply first (Apply first, Launch
  anyway, or Cancel). It says when the game is
  already running and won't start a second copy. Launching writes
  nothing on its own; Rimmerge stays open.
- **New: share a load order.** The Load order page's **Export** menu saves
  the order in `ModsConfig.xml` as a RimWorld mod list (`.rml`), which
  opens in RimWorld's own mod manager when saved in its `ModLists` folder
  (the save dialog starts there), or copies it as a numbered text list
  for a chat message. **Import** takes a `.rml`, a `ModsConfig.xml`-shaped
  list or a text list, from a file or a paste, and previews what would
  change first: the mods activated and deactivated, the ones you don't
  have installed (with an *Open on Steam Workshop* button where the list
  has a Workshop link; Rimmerge never downloads mods), and lines it
  couldn't read. *Use this order* rescans with the list and shows it as
  Current; nothing is written to `ModsConfig.xml` until you Apply. The CLI
  gains `rimmerge order export` (text on stdout, or `--out` for a `.rml`)
  and `rimmerge order import <file | ->`, which prints the same preview
  and then writes `ModsConfig.xml` with a backup first (`--dry-run` writes
  nothing).

## [1.1.1] - 2026-10-06

- **Change: the automatic update check now runs every time the app
  starts**, so a new release shows up on your next launch instead of
  waiting for a 24-hour interval to pass. Everything else about it is
  unchanged: it still waits for the first-run notice, honours the internet
  and update switches and GitHub's rate limit, runs once per launch, and
  usually costs one small request that answers "nothing new". The rule
  database refresh stays at most once a day, and the Welcome notice's
  wording now says so.
  If you answered the first-run notice before 1.1.1, you agreed to wording
  that said once a day; the release check now runs each launch, with the
  same host, the same data, and the same "Check for updates" switch in
  Settings to turn it off.
- **Fix: the def search box on the Mods page no longer makes the page
  scroll sideways.** Its result list was wider than the box and hung past
  the window's right edge while open, so the page gained a horizontal
  scrollbar. The list now matches the box's width, and long def names wrap
  inside it.
- **Fix: the Startup cost table's columns no longer change width while you
  scroll.** This was already fixed in 1.1.0 but was missing from that
  release's notes.

## [1.1.0] - 2026-10-02

- **New: the Dashboard's Get started strip begins with Get the recommended
  rules.** One click turns on, downloads (about 49 MB with the Steam
  Workshop database), and imports the recommended rule databases, so the
  suggested order uses them. It is skippable and needs internet access
  unless only an import is left; nothing happens without the click. While
  the step is offered, the two rule-database notices leave out the sources
  it covers.

## [1.0.0] - 2026-10-01

- **Distribution: Rimmerge ships as a portable zip, not an installer.**
  Download `Rimmerge-1.0.0-windows-x64-portable.zip`, extract it anywhere,
  and run `Rimmerge.exe` (the desktop app); `cli\rimmerge.exe` (the CLI) and
  the licenses are in the same folder. Your data stays in
  `%LOCALAPPDATA%\rimmerge` (settings, profiles, rules) and
  `%LOCALAPPDATA%\dev.rimmerge.app` (WebView2 cache and display
  preferences). To uninstall, delete the folder, and those two data
  folders if you want them gone. The desktop app needs the WebView2
  runtime, usually preinstalled on Windows 10 and 11 (get it from
  Microsoft if a build lacks it). The binaries are unsigned,
  so SmartScreen may warn; each release carries a `SHA256SUMS.txt`.
- **New: the patch maker shows each target's texture.** The coverage queue
  has a small preview per row, the row editor a larger viewer with facing,
  variant and source controls (it names the mod whose file is shown and
  flags a retexture of another mod's def), and the item picker a preview
  per def. Textures the app cannot read (asset bundles, built-in art,
  `.dds` without an image copy) are labelled instead of left blank. Nothing
  is cached on disk and no path leaves the backend.
- **Change: a texture the override view cannot display (a `.dds`) now
  says so in a translated sentence** instead of the backend's English
  message.
- **Fix: a removal colliding with a replacement is no longer auto-accepted
  as "nothing to merge".** When one mod removes a field and another
  replaces it, the replacing mod's own operation fails in every order it
  runs after the removal. It used to be read as agreeing with the
  remover; its candidate now comes from replaying its own operations
  against the def alone, so the field asks which one to keep.
- **Fix: the "replace discards an addition" load-order edge no longer
  fires when the replacement already contains the added content.** The
  duplicate check compared two differently built digests and never
  matched on real patches, so it kept an edge it was written to drop.
- **Fix: "the winner declared its order" is judged for the order being
  shown.** Whether the mod that wins a def override or patch collision
  declares `loadAfter` on every other contributor (and whether it shadows
  a framework) used to be fixed from the order at scan time, so the
  Suggested ledger scored a def by the wrong winner. The winner and these
  checks are now worked out per ledger, and a collision is no longer
  called intentional when another mod removes the field or one of its
  ancestors. The report no longer stores the winner or the two flags, and
  records which mods remove the patched node instead. The report schema
  version is now 24.
- **Fix: a mod's failed operation elsewhere in a def no longer changes
  the candidate it offers for a contested field.**
- **Fix: Apply no longer lists a lazily resolved any-of assembly
  constraint as a hard problem.** Only load-time ones can break loading.
- **Fix: dangling-reference check.** A terrain template subclass
  generates carpets like the base type, and a gene or terrain template
  (or colour) removed by a patch no longer counts as generating names.
- **New: a patch collision won by a mod that declares it loads after
  every other patcher scores 80.** The change of the mod whose operation
  runs last is kept when it declares `loadAfter` or a dependency on every
  other mod patching the field and none of them removes it.
- **Fix: `verify` no longer predicts a failure for an operation whose
  OR-ed head names a def that exists.** The game evaluates
  `[defName="A" or defName="B"]` once and succeeds when either def has the
  node, but `verify` never looked at a def that only its own mod patches.
  It now replays those defs too. Side effect: a def a mod patches only
  itself is checked, so a failing self-patch (an author bug) is now
  predicted as well.
- **Fix: dangling-reference check.** A name made of two def names only
  counts as a generated gene when the first half is a gene template (and
  as a generated carpet when it is a terrain template followed by a
  colour), which had hidden real dangling names. Generated `Psytrainer_`
  and `Neurotrainer_` names are no longer reported, and a
  `descriptionHyperlinks` entry is attributed to the def that contains it
  instead of the def type it points at.
- **New: `log import` attributes more `[Tag]` lines.** A tag that is not
  a mod's exact name is now tried as its package id, then as its name
  reduced to ASCII letters and digits (`[ExampleMod]` for "Example Mod"). The
  match has to be unique: when two active mods share the compacted name,
  nothing is attributed and a family is reported as ambiguous.
- **New: `sort --dropped [--mod <id>]` lists the edges the sorter dropped,**
  each with the cycle it would have closed and, for a direct two-mod
  contradiction, the edge that overruled it.
- **Docs: a dangling cross-reference can appear or disappear with load
  order** when the def holding it is itself added by a patch that only
  completes in one order. The name still resolves the same way in every
  order.

- **New: a guided path on the Dashboard, and a confirmation before Apply
  writes a broken order.** The Dashboard shows three steps (use the
  suggested order, Apply, confirm) derived from what Rimmerge already
  knows. Apply now lists the "hard problems" in the order it is about to
  write (a required mod that is not active, two active mods declared
  incompatible, a mod in `ModsConfig.xml` that is not on disk, a violated
  hard load requirement, or an unmet any-of) and asks "Apply anyway?"
  while any is still undecided. Exporting without writing `ModsConfig.xml`
  skips the question, and the separate "RimWorld is running" prompt still
  follows it. `rimmerge apply` prints the same list; its exit code and
  flags are unchanged.
- **Change: the Steam Workshop rule database is now recommended and on by
  default.** It is still never downloaded automatically (only a Refresh
  click or `rimmerge db refresh`, about 49 MB). Consequences: **Restore
  internet access defaults** and `rimmerge network reset` turn it on too
  (the Settings button now says so), and a plain `rimmerge db refresh` on
  a machine that has never saved settings downloads it. A saved
  `fetch_steam_workshop: false` is left as it is, and a damaged settings
  file still loads with every switch off. Answering the first-run notice
  (any button, or closing it) now saves `app-settings.json` when it does
  not exist yet, so a later change to a default never alters what you
  answered.
- **New: a "recommended rule databases" notice, and a Steam Workshop
  switch on the first-run card.** The first-run card gains a fourth
  switch for the Steam Workshop database (about 49 MB, downloaded only
  when you click Refresh); answering still fetches nothing. After you
  answer it, a low-priority notice lists any recommended database that is
  off, or on but never downloaded where nothing automatic will fetch it
  (always the Steam database at first). **Turn on and download** turns the
  recommended databases on and refreshes them in one click; it never
  changes the internet-access switch and never imports. The notice is
  hidden while internet access is off, can be dismissed or muted, and the
  first-run text no longer says the Steam database is "never downloaded
  unless you ask" (it is never downloaded automatically).
- **Fix: the Apply confirmation no longer says the game "will report"
  every problem it lists**; some items (such as a mod that is not
  installed and is dropped from the active list) are not game errors.
- **Change: the needs-input "Apply anyway" checkboxes are gone** (the
  Apply dialog's, which started checked, and the Dashboard's, which
  started unchecked). The count of findings that need input is shown as
  information with a link to the Inbox; it no longer gates Apply.
- **Change: the Apply dialog's "Write merge mod" checkbox starts
  unchecked.** It used to start checked whenever a complete merge existed.
  Deciding a merge no longer puts anything in `Mods/` until you tick the
  box (the CLI's `apply --write-merge-mod` was always opt-in); your choice
  holds until the dialog closes. An apply without it still saves decisions
  and writes `ModsConfig.xml`, but leaves an existing merge mod as it was.
- **Change: "Allow network access" is now "Allow internet access"**
  (Settings and the first-run notice, "Turn off internet access"; the
  `rimmerge network` command, its `allow_network` key and the JSON file
  are unchanged). The setting only ever covers the two GitHub hosts, never
  your local network; the wording says so.
- **Fix: the notification bell's count badge was cut off** by the button it
  sat inside. The scan progress captions ("Scanning mods", "Analyzing",
  ...) and the text of update-check, rule-database, rules-file and
  import-skip failures are now translated instead of arriving as English
  from the backend (the original English stays as a technical detail
  where it helps).
- **New: notifications, and internet access moved app-global.** Network
  policy (the master switch, the per-source fetch toggles, and two new
  automatic-feature toggles) moved out of the per-profile `Settings`
  (`rules.json`) into a new app-global `<base>/app-settings.json` — one
  per machine, shared by every profile and by both the desktop app and
  the CLI, since a privacy switch can't sensibly differ between two
  installs opened on the same machine. **Breaking, deliberately not
  migrated**: existing per-profile network values in `rules.json` are
  no longer read for policy (though the file's own legacy keys still
  parse harmlessly); a fresh `app-settings.json` starts at the defaults,
  and the desktop app's first-run notice offers to turn internet access
  off before anything automatic ever runs. The setting itself is
  renamed `allow_network_refresh` → `allow_network`, since it now also
  gates the update check.
- **New: an automatic, once-a-day check for a newer Rimmerge release**
  (`api.github.com`, a second allowed host alongside
  `raw.githubusercontent.com`), and **automatic, once-a-day refresh of
  the community rules and `rimmerge-rules` databases** (never the Steam
  Workshop database, which stays manual). Both are on by default but
  never run before the desktop app's first-run notice is answered, and
  never run from the CLI — `rimmerge check-update` and `rimmerge db
  refresh` are the CLI's own manual equivalents. A corrupt or unreadable
  `app-settings.json` fails **closed** (every network switch off) until
  settings are saved again. See
  [privacy-and-network.md](privacy-and-network.md).
- **New: a notification bell and a Welcome card** on the desktop app's
  Dashboard, surfacing the first-run notice, a new-version notice, a
  rule-database staleness reminder, and an "imported rules outdated"
  badge — each dismissible, and most mutable ("don't remind me again").
  See [desktop.md](desktop.md)'s Notifications section.
- **New: `rimmerge network status|on|off`** (the app-global network
  policy, from the command line — no more hand-editing JSON for a
  privacy switch) **and `rimmerge check-update`** (a manual, explicit
  release check). `rimmerge db status` now names each source
  `automatic`/`manual` and prints a staleness reminder line. See
  [cli.md](cli.md).
- **New: a mod info panel on the Mods page** (`/mods/:modId`, also
  available as a full page at `/mods/:modId/details`). Opening any mod —
  active or inactive — shows its preview image and icon when it ships
  one, its `About.xml` description (rendered as plain text; BBCode and
  any other markup shows up literally, exactly as RimWorld itself
  displays an unrecognized tag), workshop and homepage links that open
  in your system browser, and, for an active mod, its placement, tier,
  tags, dependents, and live findings. The panel follows your keyboard
  focus as you move through the list. A matching `rimmerge mods show
  <id>` CLI command prints the same information from the command line.
- **New: an About section in Settings**, showing the installed version
  and links to Rimmerge's GitHub repository, its issues page, and a
  support page; the sidebar also carries a small "Support Rimmerge"
  text link to the same support page. Every one of these links, plus
  the mod info panel's workshop/homepage links, opens through one
  backend-owned opener — the frontend never builds or sends a URL, only
  names which fixed link to open. See
  [privacy-and-network.md](privacy-and-network.md)'s "Links you click"
  section.
- **New: game-log import reads console snapshots and accounts for
  every line.** `rimmerge log import` and the desktop app's **Import game
  log** button accept an in-game console snapshot as well as a
  `Player.log`, told apart by content (`--kind` overrides it), and the
  CLI takes several paths at once. Every result says what the file
  covers: how a `Player.log` ends, whether a snapshot reached the
  console's 1,000-entry cap, and where the game stopped logging; counts
  that may be short are shown as lower bounds. Every line now lands in
  a class of counted, attributed message families (`unclassified` for
  anything unknown), save-game loads are reported alongside new games,
  a log of any size is read with no size limit, and a patch failure
  pairs with its stack trace by mod and file rather than by position.
  The line formats that mods print are data: the new `log-shapes`
  section of `rimmerge-rules`. **Breaking for `log import --json`:**
  `logged_load_order` is replaced by `load_events` (the last event's
  `mods` is the old list). See [cli.md](cli.md#importing-game-logs).
- **`PreserveCurrent` now resolves a violated heuristic (`Inferred`)
  edge by moving whichever side disturbs less**, instead of always
  moving the dependent and its whole downstream chain. `Rebuild` is
  unchanged.
- **Fix: a generated patch's `MayRequire` dependency gate now actually
  takes effect in-game.** The merge mod, a compatibility patch, and a
  patch-maker export previously placed the gate on the wrapping
  `PatchOperationSequence` itself — RimWorld never reads `MayRequire` on
  a top-level `<Operation>`, only on a `<li>` list item inside one, so
  that gate was silently inert: the whole sequence ran even when none of
  its source mods were active. The gate now sits on each `<li>`
  individually, naming exactly the mod(s) that operation's own value
  came from. **A patch exported before this fix needs re-exporting** for
  its dependency gate to take effect — re-running `patch export`/`assign
  export`, or letting the merge mod regenerate on its next apply, is
  enough; nothing about the fields it merges changes, only whether the
  gate that protects them actually works.
- **Breaking: building from source now requires the `rimmerge-rules`
  git submodule.** `rim-io`'s build script embeds
  `rules/rimmerge-rules.json` at compile time and fails immediately if
  it's missing; clone with `git clone --recursive`, or run
  `git submodule update --init` in an existing clone. See
  [install.md](install.md).
- **Breaking: the "Harmony patch" concept is renamed to "runtime
  patch"** throughout the report, decisions, and compat-patch-project
  formats — a def-cache/patch-collision finding's key changes from
  `harmony_patch_collision:...` to `runtime_patch_collision:...`
  everywhere that text is persisted (`report.json`'s `Conflict`/
  `FindingKey` JSON, `decisions.json`'s finding keys, and a compat
  patch project's own `patches/<id>.json`, which stores its decisions
  the same way). A `report.json` regenerates cleanly on the next scan;
  an existing `decisions.json` or `patches/<id>.json` entry keyed on the
  old text does not migrate automatically and reads as an unrecognized
  finding key (the error itself now says how to recover) — delete that
  one entry, or the whole file, and redo it. `report.json`'s schema
  version moves 15 → 16, `decisions.json`'s moves 6 → 7,
  `patches/<id>.json`'s moves 1 → 2.
- Repository prepared for open-source release: public documentation
  (this `docs/` tree), development history and local scratch data moved
  under a gitignored `.journal/`, real-install-specific literals moved
  behind environment variables, CI added.
- Dependency update pass: every Rust and JS dependency brought current
  within its manifest range, plus a manual bump of `ts-rs` (10 → 12)
  and `base64` (0.22 → 0.23) on the Rust side and `vue` (3.5.42 →
  3.5.43) on the JS side. See
  [dependency-versions.md](dependency-versions.md) for the full table
  and what was deliberately left un-bumped (PrimeVue, TypeScript 7,
  `@types/node`, `@vueuse/core`).
