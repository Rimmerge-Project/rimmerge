# Changelog

This changelog starts at the first public release. Development history
before that point is not summarized here.

## [Unreleased]

- **Fix: searching in Greek missed names with a capital sigma.** A search that
  ended in a capital "Σ" (such as "ΑΣ") did not find a name that continued after
  it ("ΑΣΑ"), and a search typed with a final "ς" (such as "κοσμος") did not find
  "ΚΟΣΜΟΣ", because the letter was lowercased differently at the end of the
  search than in the middle of a name. Every search box now folds the letter the
  same way: the two Mods lists (where the matching letters are highlighted), the
  mod pickers, the Findings and Changes lists, the merge fields, the def
  search, the group tree and group picker, and the assignments coverage list.
- **Fix: several views showed the wrong copy of a mod installed twice.** When
  both a plain and a Steam Workshop copy of the same mod are active, opening one
  copy's full details, the explanation of its place in the load order, one of
  its textures or its scan cost could show the other copy. The def texture
  viewer could also call one copy's image undecodable because the other copy's
  was. Each now shows the copy you opened.
- **New: a Groups view in both Mods lists.** Each list has a **List | Groups**
  switch under its sort buttons. **Groups** shows the same mods as folders: your
  group tree, each folder with its colour icon, its name and how many mods it
  holds, its subfolders first and then its mods in the list's sort order, with
  **Not in a group** last. A mod in two groups appears under each, with an
  "also in" mark, and selecting either copy selects the mod. Folders fold with a
  click or the arrow keys, **Collapse all** and **Expand all** sit beside the
  switch, and each list remembers its folded folders per profile. A folder's ⋮
  menu has **Select all in folder**, **Activate all…** and **Deactivate all…**,
  and dropping rows on a folder adds them to that group without activating or
  moving them. The Active list keeps its load-order numbers but cannot be
  reordered in Groups view; switch to List to move mods. Empty folders show
  with 0 unless a search, tag or source filter is on.
- **New: group colours on rows, an open-row mark and quicker group edits.**
  Each Active row now always shows its load-order number, whatever the sort. A
  mod with no colour of its own shows its first coloured group's colour as a
  fainter stripe. On a narrow row the group chips become coloured dots, and
  clicking a chip or dot narrows both lists to that group. The mod whose info
  panel is open is marked on its row with a ring and a tint, and in the Groups
  panel its groups have a small dot and every group a folder icon in its
  colour. The row's ⋮ menu starts with the twelve colours as swatches. The Add
  to group dialogs have a search box and offer **Create group** with the name
  you typed, which creates the group and adds the mods in one save. A double
  click on a row opens the Activate or Deactivate dialog for that mod.
- **Fix: the Mods page controls stay readable at 1,024 pixels in every language.** In
  German, Russian, Polish and French the search and tag placeholders were cut
  off, the sort bar wrapped onto a second line, a freshly added row's date could
  shrink to a single character, and a Russian date overflowed its cell. The
  search box now has its own line in a narrow list, a long placeholder is cut
  with an ellipsis and shown in full as a tooltip, the sort bar collapses into a
  dropdown with a direction button when its buttons do not fit, the date cell
  keeps a readable minimum width, the Added and Removed marks show only their icon
  in a narrow list, and the buttons above a selection never wrap their
  labels.
- **New: search matches are highlighted in the Mods lists.** The text a search
  matched is marked in the mod's shown label, in both the List and the Groups
  view, whether the list shows names or package ids. A mod can be listed because
  of its id while the list shows its name: that label then has no mark, and
  switching to package ids marks the match at once. The marks come from the same
  matching that filters the list, so they never disagree with it, and every
  match is marked, not just the first. The mark is a highlighted background with an underline
  and adds nothing for a screen reader to announce.
- **New: Show in load order.** From the Groups view or from any other sort, the
  Active list's row menu and the mod info panel have a **Show in load order**
  action. It switches the Active list to List view in load order, scrolls to the
  mod and focuses its row. A filter that still shows the mod stays; one that
  hides it (search, tag, source or the group scope) is cleared, and a note above
  the list says so.
- **New: each Mods list remembers its sort and its view.** The Active and
  Inactive lists each come back after a restart with the sort (key and
  direction) and the view (List or Groups) they had, and keep them when you load
  another project. Before, both started over every time. A remembered value the
  app no longer knows falls back to the default. A remembered sort other than
  load order still turns reordering off and still shows the load-order numbers.
- **New: copy groups and colours from another profile.** The Mods page's
  groups panel has a **Copy from another profile…** button. Choose the other
  profile's folder (the one holding its `organization.json`) and Rimmerge shows
  what the copy would do before it does anything, for either mode.
  **Merge** combines the other profile's groups with yours: a group with the
  same name path (the same names from the top level, ignoring capitals) takes
  the other profile's mods, new groups are added under the same parent, and
  where both profiles set a colour yours stays (a differing group colour is
  listed as a clash). A colour
  you have not set is filled in from the other profile. **Replace** removes all
  your groups and colours and puts the other profile's in their place, after a
  confirmation with **Cancel** focused. A copy that would pass a limit (groups,
  mods in groups, coloured mods, nesting), a folder with no groups file, a
  damaged or too large file, a folder holding exactly what you already have, or
  your own folder is refused with the reason, and nothing is changed. Mods that are not installed keep their places in the
  groups, as they do today. The copy is one save, only reads from the other
  profile's folder, and never changes your load order, your active mods or your
  rules.
- **Fix: English text Rimmerge generates no longer uses em dashes.** Sort
  explanations, finding text, error messages, command-line output and
  `--help` pages, the description in a generated merge mod's `About.xml` and
  the comments in generated patches now use plain punctuation (colons, commas,
  full stops or " - "). The plain-text explanations (command line, saved rule
  comments) now use the app's own wording. The meaning is otherwise unchanged,
  and so is the order Rimmerge suggests. A new check,
  `scripts/check-em-dashes.ps1`, and a test over the command-line help keep
  em dashes out of those strings. Rimmerge's own source comments and its
  empty-value placeholder in the app are not covered.
- **New: Apply asks first when the active mod list was changed outside
  Rimmerge.** When you press Apply, Rimmerge now reads `ModsConfig.xml`
  again, right then, instead of trusting the last check (it re-reads every
  5 seconds). If someone else changed the list since Rimmerge last read or
  wrote it, and your order would overwrite that change, the "Apply anyway?"
  panel says so, with how many mods were added and removed, or that the
  order was changed, next to any problems you still have to confirm. It is
  one panel, with **Go back** focused and **Apply anyway** never the default.
  Applying anyway still saves a timestamped backup of the changed file
  first, and the success message names it. Nothing is asked when the file
  already holds the order you are about to write.
- **New: a note on every page when the active mod list changed outside
  Rimmerge.** Under the pending-changes banner, a short note says the list
  was changed elsewhere and links to the Mods page, where the existing note
  explains it. It goes away after a rescan, an import or an Apply. The Mods
  page keeps its own note and does not show this one.
- **Fix: a rule or decision saved while a rescan, a load or an import was
  running could be lost.** Rimmerge reads your rules, decisions, patches and
  patch maker projects from disk while it scans, and then swaps the scanned
  copy in. A rule, decision, patch or patch maker edit saved in between was
  in the file but missing from the new copy, and the next save wrote the
  stale copy over it. The swap now reads those files again just before it
  replaces the old copy, so the edit is kept and the order is sorted again
  only when something really changed. If one of the files cannot be read
  again, Rimmerge keeps what the scan read and shows a notice saying that an
  edit made during the scan may be missing.
- **Fix: a mod you activated or deactivated could stay selected, hidden
  behind a filter.** Activating a mod together with its dependencies took
  only the mod you asked for out of the Inactive selection, so a dependency
  that a filter was hiding stayed selected, and the next "Activate selected"
  quietly included it. Deactivating also took Core out of the selection,
  although Core is never deactivated. Both lists now drop the mods the change
  really moved, and the ones that were already where you sent them, and the
  "N selected (M not shown)" count stays right.

## [1.2.6] - 2026-11-20

- **New: import a load order from a RimWorld save.** The Load order page's
  Import menu has **Import from a save…**, which lists the saves in
  RimWorld's `Saves` folder (the one beside the `Config` folder that holds
  `ModsConfig.xml`), newest first, with each save's name and the date it was
  last saved, at most 200 of them. **Preview** on a save opens the usual
  import preview, and **Browse…** opens the file dialog in that folder for a
  save kept elsewhere. *Open a mod list file…* and `rimmerge order import`
  accept a save (`.rws`) too, on stdin as well. Only the mod list in the
  save's header is read: Rimmerge stops within 64 KiB after the header,
  never decodes or changes the rest of the file, and refuses a header larger
  than 4 MiB, a save that ends before its header does, and a file whose
  header has no mod list, each with its own reason. Listing the folder reads
  only file names and dates. The preview (and the CLI's `version:` line)
  says which RimWorld version the list was made with, and adds a note when
  this install is a different release (1.5 against 1.6, for example). When
  the list's names or Workshop ids do not line up with its package ids, that
  list is left out instead of being paired by guesswork, and the preview and
  the CLI say so; this applies to `.rml` files too. A missing mod's Workshop
  link uses the id written in the file. For a save that is the id the mod's
  own `About.xml` declares, which few mods have.
- **New: select several mods at once.** On the Mods page, Ctrl+click
  (Cmd+click on a Mac) adds or removes one row, and Shift+click selects a
  range the way a file manager does. From the keyboard, Shift+Space and
  Shift+Up or Shift+Down extend the range from the focused row, Ctrl+A
  (Cmd+A) selects every shown row and Escape clears the selection.
  Reversing a range shrinks it again, and rows you selected before the
  range, or that a filter hides, stay selected. Screen readers announce each
  list as a multi-select grid.
- **New: groups and colours on the Mods page.** Sort your mods into groups,
  nested up to 8 levels, from a Groups panel: a column on a very wide window
  and a drawer opened by the **Groups** button otherwise. It lists **All
  mods**, **Ungrouped** and your group tree, with how many mods each group
  holds. Choosing a group narrows both lists to its mods and its subgroups'
  mods. This is a filter: the Active list stays in load order and can still
  be reordered. A group or a mod can have one of twelve named colours. Every
  row, Inactive included, shows its groups as chips, its colour as a stripe
  that is also named in text, and the tags you added by hand, and the tag
  filter now works on both lists. A group's menu creates subgroups, renames,
  colours, moves or deletes it, and can remove the mods in it that are not
  installed. Groups are a way to find mods and nothing more: they never move
  a file, activate or deactivate a mod, or change the sort or your load
  order. They are saved per profile in `organization.json`, keyed by package
  id so they survive updates and rescans. If that file cannot be used, a
  notice says so and groups stay read-only; **Start over…** keeps a copy of
  the file and begins with no groups.
- **New: act on many mods at once from the Mods page.** While a list has a
  selection, a row of buttons above it works on that selection (each list
  keeps its own, and mods a filter hides are included; every dialog says how
  many were not shown). **Add to group…** puts the mods in a group, **Remove
  from group…** takes them out of the groups that hold them, and **Move to
  group…** replaces one group with another. **Colour** sets or clears their
  colour, and **Tags** adds or removes one tag on all of them. A group's menu
  also has **Activate** and **Deactivate** for the whole group, subgroups
  included, in the same dialogs as the selection buttons, with a line that
  says how many mods it covers. Each action is one save and one message that
  says what changed and what was left alone. None of them changes the load
  order, a pending change or `ModsConfig.xml`; tags never change the sorted
  order.
- **New: drag mods and groups onto the Groups panel.** Drag a row, or every
  selected row, onto a group to add the mods to it; mods stay in their other
  groups. Drag a group onto another group to put it inside, or onto **All
  mods** to make it top level. The group under the pointer is outlined and a
  sentence under the tree says what the drop will do, or why it cannot. A
  collapsed group opens when you hold a drag over it. A drop never activates
  or moves a mod, and the Active list's own drag works as before. On a
  narrower window, open the Groups drawer first. Without a mouse, a group's
  menu **Move group to…** and the bulk **Add to group…** do the same.

## [1.2.4] - 2026-11-19

- **New: see what Apply will add or remove.** On the Mods page, an Active mod
  that your saved active mod list does not have is marked **Added**, and an
  Inactive mod that the saved list still has is marked **Removed**, so a
  pending change is visible before you Apply. A mod counts the same with or
  without its `_steam` suffix, and a reorder alone makes no mark. The saved
  list is re-read every few seconds while the window is visible and when you
  switch back to it, so a change made in the game or another mod manager shows
  up without reloading. A note then tells you what changed outside Rimmerge
  (and, when you have pending changes, that they are kept), and offers to use
  the file's list through the usual import preview. An outside edit alone is
  never counted as your own unsaved work: it does not trip the close and
  project-switch warning, the pending-changes banner, or the "This isn't your
  active order yet." note. Installing a patch or assignment export while the
  list was changed outside Rimmerge warns that it writes your order over those
  changes. If the file is missing or cannot be read,
  a note says so and the marks keep using the last list that could be read.
  A change that only reorders the file has its own note. The import preview
  now compares the list you import with the active list you have in Rimmerge
  now, pending changes included, so a mod you just deactivated shows as coming
  back. Rimmerge only reads the file for this and never writes it. Reading
  `ModsConfig.xml` is now also capped at 4 MiB.
- **Fix: one colon in the "fixable by order" heading.** The Apply dialog's
  heading read "Order-fixable: reordering the named mod may resolve these:".
  It now reads "Fixable by order (reordering the named mod may resolve these):".
- **New: Inactive and Active side by side, with drag and drop.** The Mods page
  shows the Inactive list on the left and the Active list on the right
  (stacked on a narrow window), each with its own search, filters, sort and
  selection. Drag a row (or every selected row) within Active to reorder it,
  from Inactive into Active to activate it at that place, or onto Inactive to
  deactivate it. A line shows where it will land. A drop that would pull in a
  dependency or leave a dependent active asks first, as the buttons do.
  Without a mouse, every Active row has a Move menu (up, down, top, bottom,
  after another mod), every Inactive row has an Activate at button, and
  Alt+Up and Alt+Down move the focused row. Screen readers announce each
  move. Moving works while Active is sorted by load order (a search or filter
  is fine); under another sort it is disabled, with the reason shown. A
  reorder over the same set of mods changes your own order at once, without a
  rescan, and reaches `ModsConfig.xml` when you Apply. The first reorder while
  the suggested order is selected switches to your own order and says so.
  Discard pending changes (in the pending-changes banner) and Revert to your
  saved active order (in the "This isn't your active order yet." note) undo
  what you staged. Activating a mod without choosing a place now puts it back where it
  was in your order, and never ahead of a mod it depends on. The mod info
  panel is a drawer over the lists on all but very wide windows. The source
  label beside a row's icon appears when the list is wide enough, not the
  window.
- **Fix: the brackets around "N not shown" follow each language.** They are
  now part of the translated text, so Chinese and Japanese use their own
  full-width brackets.
- **New: sort the Mods lists.** Each list on the Mods page has one sort
  button per criterion: Load order (Active only), Name, Package ID or Last
  updated. Click a button to sort by it, and click it again to flip the
  direction. Each button has a tooltip and an accessible name that say the
  current direction and what the next click does. Inactive now opens sorted by name instead of by package id. Sorting
  works with search and the filters, never changes the load order or any
  pending change, and is not saved. "Last updated" is, for a Workshop mod,
  when the installed version was published on the Workshop, as Steam records
  it; for a local mod it is the `About.xml` modification time. Core and the
  DLCs have none, and mods without a time sort last in either direction (see
  the Mods page in `docs/desktop.md`).
- **A Steam installed outside Program Files is now found.** Detection only
  looked under `%ProgramFiles(x86)%\Steam` and `%ProgramFiles%\Steam`, so a
  game in a library of a Steam installed elsewhere (for example `Q:\Steam`)
  was reported as "no RimWorld install found", and Launch started such a copy
  directly instead of through Steam. Detection and Launch now also use the
  Steam folders Steam records in the Windows registry (a local, read-only
  lookup), and read `libraryfolders.vdf` with a size bound.
- **The Setup page recovers once you point it at your game folder.** Entering
  or browsing to the folder now checks it again: the "no install found"
  message clears, and the workshop folder, `ModsConfig.xml` and profile
  folder fill in (a field you changed is kept). `ModsConfig.xml` and the
  profile folder are shown even while no install is found. Save as defaults
  refreshes the page's answer.
- **The "no install found" message is in your language.** It used to show
  the backend's English command-line advice ("see the technical details
  below"). It now says what to do in the app's language and lists each
  checked location. The "technical details" label on a load error is
  translated too.
- **Mods that move now follows the orders.** After an Apply made the current
  order the suggested one, the Dashboard, the getting-started strip and the
  Apply dialog kept showing the number measured before the Apply until
  something sorted again. It is now measured against the orders as they are,
  so it reads 0 right after such an Apply.
- **Rescan after Apply no longer reverts the current order.** The active-mod
  list the app rescans kept the order from before the Apply. A rescan then
  scanned the old order while `ModsConfig.xml` held the new one, and a later
  Apply of Current would have written the old order back. The list now
  follows the written order. When the current order changes over the same
  mods and the tie-break preserves the current order, the suggested order is
  sorted again.
- **The Mods page shows every mod, and has Select all, Clear selection and
  Rescan mods.** The Active and Inactive lists no longer stop at 200 rows (a
  list is bounded at 5,000, with the full count shown). They show your edits
  at once: an activated mod is in Active ("not scanned yet" until a rescan),
  a deactivated one in Inactive. Each list has a "Select all N shown"
  checkbox for the rows matching the search and filters, a note for selected
  rows a filter hides, and Clear selection. The plan and confirmation dialogs
  are unchanged. Rescan mods is always available in the page header.
- **A refused activate or deactivate plan is no longer silent.** When the
  plan for Activate selected or Deactivate selected failed, nothing visible
  happened. The dialog now opens with the reason and no way to confirm.
- **Faster mod scan.** Loading, rescanning and importing a load order start
  by scanning every active mod's files. That scan now makes far fewer
  file-system calls and allocations:
  - each loaded folder is listed once instead of a dozen times, and checking
    whether an entry is a folder no longer opens it;
  - each folder's `Defs`, `Patches`, `Textures`, `Assemblies` and
    `AssetBundles` trees are walked once instead of up to four times;
  - the def reader no longer builds a new string for every field it visits;
  - files are read into a buffer sized for them;
  - each mod's folder is walked once for both the side-loaded assembly check
    and the later search for definitions in folders that aren't loaded.

  On a test install of about 1,000 active mods (release build, warm file
  cache, median of three old and new runs taken alternately), a full
  analysis went from about 19 s to about 14 s and used about a third less
  processor time (98 s to 64 s). Peak memory is about the same (+10 MB) and
  results are unchanged. A cold file cache is still dominated by reading the
  `.dds` texture headers.
## [1.2.3] - 2026-10-09

- **Verify checks each mod file for changes once per run.** Reading a def or
  patch operation back from its mod's file checked the file's size and
  modification time on disk before every read, about 46,500 checks per verify
  on a test install of about 1,000 active mods. Each file is now checked the
  first time a verify or a def inspection reads it (and at most twice while
  building the merge mod), and that check holds until the run ends: a file
  edited on disk while one of these runs may be seen only by the next run, and
  a later run that reads the file again sees an edit made in between. On that
  install, verifying one load order after it was loaded went from about 6.0 s
  to about 5.1 s and used about 3 s less processor time (release build,
  median of six base and new runs taken alternately in one session). Those
  figures are not comparable with the 5.5 s of 1.2.2, which was measured in an
  earlier session. Merge mod and def inspection times, peak memory and results
  are unchanged.

## [1.2.2] - 2026-10-09

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
- **Verify and the merge mod no longer re-parse the same files.** Reading a
  def or patch operation back from its mod's file parsed the whole file again
  for every read; each file is now parsed once and kept in a cache of at most
  16 MiB, which still notices a file edited on disk. On the same test install,
  verifying one load order after the install was loaded went from about
  12.5 s to about 5.5 s, and working out the merge mod from about 1.2 s to
  about 0.6 s (release build). Peak memory and results are unchanged.

## [1.2.1] - 2026-10-08

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
