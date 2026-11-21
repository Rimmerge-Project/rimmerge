# apps/desktop (`rimmerge-desktop`)

A Tauri v2 desktop shell over the same engine the CLI uses: `src-tauri`
(Rust) exposes one command per use case, `src` (Vue 3 + TypeScript)
renders it. Nothing here computes anything the CLI doesn't already
compute — see [`docs/desktop.md`](../../docs/desktop.md) for what each
page shows and [`docs/testing.md`](../../docs/testing.md) for the
real-install test tier this app shares with the CLI.

Tooling is `bun` only. Vue is pinned to 3.5.x; PrimeVue is pinned to
4.5.5 (`@primevue/themes` 4.5.4) — the last MIT release before 5.x
became commercial. Never upgrade PrimeVue past 4.x without checking its
license first.

## Layer

`src-tauri` is a composition root: every `#[tauri::command]` maps a DTO
to a `rim-session` call and maps the result back. Filtering, paging,
and aggregation live in `rim-session` (`FindingIndex`, `Session::mods`,
`working_mod_list`/`inactive_mod_list`, `order_rows`, `mod_detail`), never in a command handler or in Vue. The adapters a
command needs for the loaded project (scanner, decision/rule/patch
stores, def reader, asset locator, release feed, ...) come from
`AppState::adapters`, never constructed inline. The narrow exception is
app-global state that has no session: the `<base>` stores
(`JsonAppSettingsStore`, `JsonProfileNotificationStateStore`, ...) and
the rule-database fetcher are built per call from `profile_base()` /
`cache_dir()`, each command splitting an `_at(base)` (or `_with(...)`)
function out so its tests run against a temp base. `src` is a thin client
of that command surface — no direct filesystem or process access outside
`src-tauri`.

## Invariants

- **Every session command is `async fn`, returns `Result<T, CommandError>`,
  and does its real work inside `with_session`** — a `spawn_blocking`
  closure holding the session lock; the guard never crosses an `await`.
  **The exception is app-global commands**, which touch no session and so
  never call `with_session`: `get_default_paths`/`save_app_config`,
  `get_app_settings`/`update_app_settings`/`reset_network_policy`/
  `enable_recommended_rule_databases`, the
  notification mutations (`mute_notification_kind`, `complete_welcome`,
  `dismiss_notification` for every kind but `GameVersionChanged`, ...),
  `open_app_link`, and `refresh_rule_databases`/`check_for_update`. They
  work on `<base>` files or the state's adapters directly; the two
  infallible ones (`get_default_settings`, `get_app_version`) return their
  value without a `Result`, and `get_app_version` is not even `async`.
  `load_project`/`rescan_project` are the other shape: they hold
  `state.load_lock` and swap the session in with `replace_session`, which
  (like the rescan swap) re-reads the organization under the session write
  lock just before the swap, so an organization edit that landed during the
  scan is not lost. Both swaps go through `state::reload_persisted_state`,
  which re-reads the organization and (`Session::reload_persisted`) the
  rules, decisions, patches and assignments the scan read outside the lock;
  the rescan's swap is `active_set::swap_scanned`. The sort and ledger are
  recomputed once, only when rules or decisions changed, and a file that
  cannot be re-read keeps the scanned copy and adds a
  `persistedReloadFailed` rule warning (never fails the swap). The summary's
  `ruleWarnings` are read from the swapped-in session (`replace_session`
  and `swap_scanned` return them). Organization, rule and decision commands
  take only the session lock, never `load_lock` (an edit must not wait on a
  scan).
  `import_order` (`commands/order_share.rs`) is the rescan shape too: it is
  `rescan_with(ScanRequest::Import(order))`, so the order is validated
  (`ImportOrder::validate`, including its size bound), scanned and swapped in
  all under the one `load_lock`, never staged in the working set.
  `get_recommended_rules_step`, `get_recommended_rules` and
  `skip_recommended_rules_step` (`commands/recommended_rules.rs`) are
  **session commands**: each needs a loaded project (the step reads the
  profile's import records and skip flag, the skip writes
  `<profile>/notifications.json`, the click imports into the session).
  The one exception inside them is the click's download phase, a bare
  `spawn_blocking` that never holds the session lock across the network;
  its import phase and the other two run in `with_session`.
  `with_session` catches a panic with `catch_unwind`: a caught panic or
  a poisoned lock clears the session and returns `session_lost`.
  `no_project_loaded` is checked before any probe read, so it always
  wins over `rimworld_running`.
- **A default-gate test must never consult the real
  `SysinfoGameProcessProbe`.** Every probe-gated command
  (`apply` when it writes `ModsConfig.xml`, `export_assignment`/
  `export_patch` with `install: true`) injects a fake probe in its own
  tests. `activate_mods`/`deactivate_mods` are **not** probe-gated: they
  only edit the in-memory working set, and the running-game check happens
  when `apply` writes it. The established pattern is a local `struct FixedProbe(bool)`
  implementing `crate::state::GameProcessProbe`, then
  `AppState { game_process_probe: Arc::new(FixedProbe(false)),
  ..AppState::default() }` (or reassigning `state.game_process_probe`
  on an already-built `mut` state). `commands/apply.rs`,
  `commands/patch.rs`, and `commands/assignments.rs` all follow it.
- **A default-gate test must never call the real launcher's `launch`.**
  `AppState::game_launcher: Arc<dyn GameLauncher + Send + Sync>` defaults to
  `rim_io::SystemGameLauncher` (which opens `steam://run/294100` or spawns
  `RimWorldWin64.exe`); every `get_game_launch_status`/`launch_game` test
  injects `test_support::RecordingGameLauncher` (scripted route, records
  launches, optional scripted failure) and a `FixedProbe`. Calling the real
  `route` against a temp dir is fine; the real `launch` never is.
- **`get_game_launch_status`/`launch_game` (`commands/game_launch.rs`) are
  session commands with a documented exception to "real work inside
  `with_session`".** They snapshot `(game_dir, order_on_disk())` under
  `with_session`, release the lock, then a bare `spawn_blocking` reads the
  probe and runs `LaunchGame::status`/`execute`: the lock is held only for the snapshot,
  never across the process-list scan or a spawn, so a poll or launch never
  makes another command wait on it (the snapshot itself can still wait for
  a long command such as `verify`). `no_project_loaded` still wins. `launch_game` takes
  no `AppHandle` and emits no `session://changed` (nothing changes); the
  request is one enum (`ifNotApplied`) and never carries a path or URL.
  `LaunchGame` is generic over its launcher, so the command wraps the shared
  `Arc<dyn GameLauncher>` in a private newtype rather than widening
  `rim-session`. `GameRunning` reuses `rimworld_running`; `order_not_applied`,
  `game_executable_missing`, `steam_launch_failed` and `game_start_failed` are
  the new codes (every one needs wording in `utils/errors.ts`).
- **The Launch RimWorld button polls once, shares its in-flight state, and never
  stacks requests.** `components/launch/LaunchGameButton.vue` (sidebar under Apply,
  and the strip's done line) owns its Apply-first prompt and its own `ApplyDialog`;
  `composables/useGameLaunch.ts` holds the logic over the shared
  `useGameLaunchStatusQuery()` (`["gameLaunch", "status"]`). Only the sidebar
  instance (`pollsStatus`, always mounted while the strip is) runs
  `composables/useGameLaunchPolling.ts`: every 5 s (`useIntervalFn`) while
  `useDocumentVisibility()` is visible, plus on window focus, so two placements cost
  one poller. The in-flight click and the "Starting RimWorld…" window (until the
  status reads `gameRunning`, or 30 s) live in `stores/gameLaunch.ts`, shared by
  every button, so a click on one disables the other and a second `steam://run`
  cannot be sent. The status snapshot takes the session lock and so can wait
  behind a long command (`verify`); a tick is **skipped while a request is
  pending** (Colada's `refetch` aborts the previous call on the frontend only; the
  backend call would still queue). Until the first answer the button is disabled
  with no note; an errored query is the view's `unknown` case (disabled, with
  `gameLaunch.statusFailed`) even when an older answer is held. Colada keeps `data`
  during a refetch, so after the first answer the button shows the last answer, and
  a click reads a **fresh** status first (which can wait behind a verify), and when an
  invalidation starts a newer fetch meanwhile (Colada aborts the click's and resolves it
  with the old state) it waits for the newest call and reads the entry, so the click never
  acts on a superseded answer. The button is disabled while that read or `launch_game`
  is in flight. `launch_game` is called directly, not through a mutation: the app-wide
  `mutationOptions.onError` would toast every failure with the generic sentence, wrong
  for `rimworld_running` (own sentence) and `order_not_applied` (reopens the prompt);
  `useGameLaunch` shows each failure once. `ApplyDialog.vue` has one extra emit,
  `applied: [{ wroteModsConfig }]`, fired from `useApplyDialog`'s `finishSuccess`
  (once per successful apply, even when the dialog stays open for the merge-mod
  summary); the two older hosts ignore it, the launch button's own dialog launches
  on it only while its `isLaunchPendingApply` flag (set by "Apply first", cleared by
  `applied` or by the dialog closing) is set. The mock (`gameLaunchStatus`/`launchGameMock`
  in `scenario.ts`) derives the status from the selected order against `fileOrder`, the
  unscanned set and `window.__GAME_RUNNING__`, and records `__LAUNCH_GAME_CALLS__`.
- **Sharing a load order lives on the Load order page, and the backend decides
  everything it shows.** `components/order/OrderShareMenus.vue` (the header's
  Export and Import menus, which own `PasteOrderDialog` and `ImportPreviewDialog`)
  runs `composables/useOrderShare.ts`. Export, preview, import and Workshop calls go
  straight to the services, not through mutations: the app-wide mutation
  `onError` would word an export failure with the generic sentence, while
  here it is "couldn't save/copy" plus the code's own sentence, shown once.
  Its toasts go to their own group (`ORDER_SHARE_TOAST_GROUP`, a second `<Toast>`
  in `App.vue` at the bottom right) because the default top-right toasts sit over
  the page header's Export and Import buttons for their whole lifetime; a clipboard
  failure is worded the same way for both copies.
  The import is not a mutation either (`importOrder` is called directly): the
  app-wide `onError` would add a second, default-group toast on top of the one
  shown here. Its success path is `useAdoptImportedSession` (`queries/orderShare.ts`):
  adopt the returned `selected` and invalidate every query **without awaiting it**
  (through `startInvalidation`, `queries/invalidation.ts`), as `useRescanMutation` does:
  Colada's `invalidateQueries` resolves only when every active query has refetched, and a
  slow one would hold the preview's spinner long after the swap landed (the dialog closes
  when the swap does). A failure posts exactly one order-share toast (the code's title and
  sentence) and keeps the preview open, except `invalid_input` (the install changed
  since the preview, so the same order would be refused again): that closes it and
  toasts "preview again" with the code's sentence. `isBusy` makes every entry point a
  no-op while a dialog, export or preview is in flight. The save dialog starts at
  `suggested_mod_list_path` (null when RimWorld's `ModLists` folder is absent)
  and offers `.rml` only; `dialog:allow-save` is the capability it needs.
  "Import from a save…" (`SaveImportDialog.vue`, owned by `OrderShareMenus` with
  `isSavesOpen`/`openSaves`/`previewSave`/`browseSave` in `useOrderShare`) lists RimWorld's
  `Saves` folder through `list_saves` (`rim_io::list_saves`, called inside `with_session`:
  `<user-data root>/Saves`, the sibling of the `Config` folder holding `ModsConfig.xml`, found by
  the same `user_data_folder` helper `suggested_mod_list_path` uses). The query
  (`useSavesQuery`, `queryKeys.saves()`, `staleTime: 0`) is enabled only while the dialog is
  open. The backend orders and bounds the list (newest first, at most 200, `omitted` counts
  the rest) and the page never re-sorts it; a missing folder is `folder: null` (the dialog says
  so, Browse still works), an unlistable one is `saves_folder_unreadable` shown inside the
  dialog in place of the list (it is a state of the list, not a toast). A failed preview of a
  listed save is a toast: the order-share group renders above the modal mask and stays
  clickable (`order-share-save.spec.ts`), so the dialog stays open under it. Directory metadata only: no save is
  opened for the list; "Preview" and "Browse…" (`pickSaveFile`, starting in the `Saves` folder)
  both go through `preview_order_import_file`, and the list closes when the preview opens
  (`showPreview`). A save's name is the player's text and renders as text. The mock
  (`list_saves` in `scenario.ts`) mirrors the ordering and cap over `window.__SAVES__` (any
  order in), `__LIST_SAVES_ERROR__` makes it fail, `__LIST_SAVES_CALLS__` counts calls and
  `__OPEN_DIALOG_OPTIONS__` records every file picker's options. The
  preview's "Use this order" is enabled exactly when `importBlocked` is `null`
  (never re-derived from the entries). `importBlocked` is the backend's reason
  (`ImportBlockedDto`, from `ImportOrder::import_blocker`, the one rule behind
  `validate_order`: `coreMissing`, `nothingInstalled` (no mod besides Core and
  generated ones, the common "Core plus only missing mods" case), `tooMany`,
  `unknown`, `duplicate`), worded by `importBlockedMessage`; `scanDidNotMatch`
  cannot arise from a preview and has no variant. `order` is sent back unchanged, and a Workshop
  button sends the id as digits, never a URL. Every `ImportedEntryDto`,
  `MissingKindDto`, `SkippedEntryDto`, `VersionCheckDto`, `CorePlacementDto` and
  `ImportRejectionDto`, `ListFormatDto` and `ListAlignmentDto` is worded by an
  `assertNever` switch in `utils/orderShare.ts` (the preview shows the listed
  game version verbatim whenever it has one, "From a save made with..." for a
  save, and the "you have RimWorld X" note only on a major.minor difference;
  a misaligned names or Steam-id list gets a note of its own); a sender's name and a skipped excerpt are untrusted
  and render as text only. The two replace warnings are separate facts:
  `replacesPendingChanges` (from the preview: unrescanned activation edits) and
  `PendingActiveChangesDto.orderUnapplied` (the Current order is not in the
  file yet, e.g. an earlier import not applied), which the page passes down as
  `replacesUnwritten`. The "This isn't your active order yet" note
  (`OrderNotInFileNote.vue`) shows only while Current is selected and
  `orderUnapplied` is true, with its own `ApplyDialog`. **Own facts vs live
  facts:** `PendingActiveChangesDto` (`unapplied`, `orderUnapplied`) compares with what
  Rimmerge last loaded or wrote, never the live file, so an outside edit of
  `ModsConfig.xml` alone trips none of the close and project-switch guard, the
  pending banner, this note, the export panels' note or `replacesUnwritten`;
  `FileChangeNote` is the one place that explains an outside edit. The live
  comparison (`DashboardDto.fileMatchesCurrent`/`fileMatchesSuggested`, the launch
  status, the Added/Removed marks) follows the file; no "unsaved work" copy reads it. The mock
  (`planOrderImport`/`importOrderMock` in `scenario.ts`) computes the diff
  against the working list (`ImportTarget::from_session` passes `working`, so pending edits
  show as changes; the CLI keeps the file's list), the `import_blocker` rule and the rejections, accepts a `.rml`
  export path only, swaps Current on import and leaves `fileOrder` alone. It mirrors
  the planner's details: the first active on-disk copy per base id wins, the list's
  `# RimWorld X` header is parsed and compared by numeric major.minor (`unknown` when
  unparsable), and the profile's own merge mod is kept at the end when the file has it
  (`__PUT_MERGE_MOD_IN_FILE__` puts it there);
  `__EXPORT_ORDER_FILE_CALLS__`, `__PREVIEW_ORDER_IMPORT_*_CALLS__`,
  `__IMPORT_ORDER_CALLS__`, `__OPEN_WORKSHOP_PAGE_CALLS__` and
  `__SAVE_DIALOG_OPTIONS__` record the calls. Biome rewrites a value import used
  only as a type in a `.vue` script to `import type`, which silently drops the
  component from the template: type a template ref with `MenuMethods`, never
  `InstanceType<typeof Menu>`.
- **A default-gate test must never let the real feed be called.**
  `AppState.adapters.release_feed: Arc<dyn ReleaseFeed + Send + Sync>`
  defaults to the real `GithubReleaseFeed` for production (merely
  constructing it opens nothing; calling it opens a socket); a test exercising
  `check_for_update`/`run_launch_network_checks` injects a fake
  implementing `rim_session::ports::ReleaseFeed` instead:
  `AppState { adapters: Adapters { release_feed: Arc::new(FakeReleaseFeed(...)),
  ..Adapters::default() }, ..AppState::default() }` (see
  `commands/notifications.rs`'s tests). The rule-database fetcher has no equivalent seam on
  `AppState` at all — `refresh_rule_databases`/`run_launch_network_checks`
  build `rim_io::GithubRuleDatabaseFetcher` inline per call, the same
  way most commands construct their `rim-io` adapters directly — so a
  test covering that path stays hermetic by pointing it at a scratch
  `--cache-dir`/`cache_dir` with every source's own fetch toggle off
  (`apps/cli/tests/db_cli.rs`'s `seed_settings_every_source_disabled`
  is the established pattern this mirrors), never by faking the port.
- **The working active-mod set has three tiers.** `working`
  (uncommitted activate/deactivate/move edits), `scanned` (what the last
  `Rescan`/`load` saw — `list_mods` reads this tier, for patch scopes and
  rule pickers; the Mods page's `list_working_mods`/`list_inactive_mods`
  read `working`, so an activated mod is in Active at once as a
  `pendingScan` row and a deactivated one is in Inactive at once), and `file` (what's actually on disk
  in `ModsConfig.xml` — what a real, non-dry-run `apply` writes; since 1.2.4 a **live
  baseline**, re-read by the poller below, not only what the file held at load).
  `activate_mods`/`deactivate_mods` re-plan against the session's
  current report inside the same call rather than trusting whatever
  plan the frontend displayed a moment earlier, so the two can never
  disagree. `rescan_project` holds `state.load_lock` for the whole scan
  (a deliberate, occasional action, not a fast query — safe to block a
  concurrent `load_project`/`rescan_project` for) but takes the
  session's own lock only for two short `with_session` calls around it:
  one snapshots `(paths, working ids)` and releases immediately, the
  real scan then runs in a bare `spawn_blocking` touching no live
  `Session` at all (so a scan panic can only fail this call, never
  poison anything already loaded), and a second short `with_session`
  call swaps the new session in only if `session.working().ids()` still
  matches the first snapshot — otherwise something else
  (`activate_mods`/`deactivate_mods`) changed the working set while the
  scan was running, and it refuses rather than silently discarding that
  change: `"the working active-mod set changed during the rescan;
  rescan again"`. A `move_mods` during a scan changes that sequence, so it
  makes the rescan refuse too. The tiers settle in two directions
  (`crates/rim-session/CLAUDE.md`): an Apply (and a revert) makes `working`
  follow Current, while an edit from the Mods page (`move_mods`,
  `activate_mods`, `deactivate_mods`) that leaves the same ids makes Current
  follow `working`, writing nothing; the file tier is untouched, so
  `orderUnapplied` turns true (and the live `fileMatchesCurrent` false) until an Apply. The commands
  `move_mods`, `discard_pending_changes` and `revert_order_to_file` all emit
  `session://changed` with `activeSetChanged`; `move_mods` also returns the
  selection it left (`MoveModsResultDto`), because the first reorder while
  Suggested is selected switches to Current, and the caller adopts it and
  says so. `activate_mods` (every placement) returns `ActivateModsResultDto { pending,
  activated, alreadyActive, selected, didSwitchToCurrent }`; an *anchored* one (an explicit drop
  position) switches Suggested to Current the same way, an un-anchored one (the
  button) never does. `deactivate_mods` returns `DeactivateModsResultDto { pending,
  removed, notActive }`. These lists come from the commit's own plan (it re-plans, so
  they can differ from the dialog's plan; a refused Core is in neither).
  `PendingActiveChangesDto.orderUnapplied` is Current differing from the list Rimmerge
  last loaded or wrote (the acknowledged list), not from the live file: a
  settled reorder changes no membership, so `unscanned`/`unapplied` are empty
  for it and only this field (read by the close/project-switch guard, the
  `OrderNotInFileNote` on the Mods page, and the export panels' install note)
  shows it. `ModListDto.reorderable` is the backend's answer to "may this
  list be reordered" (Active in load order, filters allowed); the frontend
  never restates it from the sort. `ActivateRequestDto.anchor` is `null` for
  the default placement. The mock (`scenario.ts`) mirrors all of it, except the `PreserveCurrent` re-sort of Suggested and a
  mid-scan refusal, and is pinned by `e2e/specs/mods-reorder-mock.spec.ts`.
- **The file tier is live, and one poller keeps it so.** `refresh_mods_config_baseline`
  (`commands/file_baseline.rs`) has the `game_launch` lock exception: a short
  `with_session` for `Session::file_probe()`, the read and parse in a bare
  `spawn_blocking` (generic over the `ModsConfigStore`, so a test can slow it down),
  then a short `with_session` to `FileObservation::commit`. It only reads, and emits
  `session://changed` with `fileBaselineChanged` **only when `BaselineChange::Changed`**
  (the list or the status differs), so a quiet poll invalidates nothing; the reason is
  deliberately not in `HANDLED_BY_MERGE_MUTATION`, so it invalidates everything like any
  other change. `get_mods_config_baseline` is the pure read behind
  `useFileBaselineQuery` (`FileBaselineDto`: `state` fresh or stale with a reason code,
  and `externalChange`, a `kind`-tagged union: `none`, `membership` with `added`/`removed`
  counts, or `reordered`). `composables/useFileBaselinePolling.ts`, called once
  in `TheShell.vue`, polls every 5 s while the document is visible and on window focus
  (`useVisiblePolling`, shared with the launch poller) and **skips a tick while its call is
  pending** (the call can wait behind a long command such as verify, and a tick per
  interval would queue). A failed poll is ignored. Each `ModListRowDto` carries
  `membership` (`none`/`added`/`removed`, by base id), drawn by `MembershipBadge.vue`
  after the mod name (icon, word and tooltip, never colour alone). `FileChangeNote.vue`
  on the Mods page shows two notes: the outside change (plain words, no file name; a
  membership change is the `(+N, -M)` sentence, a `reordered` change its own; each has a
  whole-sentence key with and without "Your pending changes are kept.", chosen by
  `hasOwnPendingChanges`, never concatenated;
  its action runs `useOrderShare().previewModsConfig`, the existing
  `preview_order_import_file` preview on the session's own file path, so the existing
  import confirm and replace warnings apply) and the stale-baseline failure (this one may
  name ModsConfig.xml, per reason code through `staleBaselineMessage`). The mock mirrors it:
  `window.__MODS_CONFIG_FILE__` (`{ status: "ok", activeMods }` or a failure status) is what
  the next refresh, rescan or import reads (a failure status makes the rescan or import fail
  with `mods_config_io_failed`, as the real load does; an Apply clears it, as the
  real write replaces the file), `__REFRESH_BASELINE_CALLS__` counts calls,
  `__REFRESH_BASELINE_DONE__` finished ones and `__BASELINE_CHANGED_EMITS__` the change
  events it emitted (the end-of-handler signal a "quiet poll" spec waits for), and
  `e2e/specs/mods-file-baseline.spec.ts` pins the behaviour.
- **The Apply dialog reads the merge mod only while it is open, gated by mounting.** Every page
  mounts an `ApplyDialog`, and on a freshly swapped session the `get_merge_mod` render replays
  every ledger entry (seconds) under the session lock. The query lives in children of the
  dialog's body (`ApplyMergeModSection.vue`: the checkbox, the loading line and the summary;
  `ApplyResult.vue`: the skipped-merge reasons), which PrimeVue mounts only while the dialog is
  open; `useApplyDialog` holds no merge-mod query. `useMergeModQuery` has **no `enabled`
  option**: observers share one Colada entry and `invalidateQueries` honours only the last
  observer's options, so one hidden dialog's gate would stop an open dialog or `MergeModPage`
  from refreshing. An entry with no mounted observer is skipped by an invalidation (it is marked
  stale and refetches when next observed). The query has `staleTime: Infinity`, as
  `useDefGraphicQuery`: reopening the dialog or refocusing the window does not re-render under the
  session lock; only an invalidation (a swap, a decision, `session://changed`) does. The cost: a
  merge-mod folder added or removed on disk outside the app is not noticed until the next
  invalidation. The section treats "no answer yet **or** a fetch in flight" as unknown
  (`status === "pending" || isLoading`, an exception to the `isPending` rule above: after a
  session swap the entry keeps the replaced session's answer with `status: "success"`; only that
  line and the checkbox wait, nothing unmounts). One always-mounted `role="status"` element
  carries the text (`common.loading`, then the summary, then nothing) so a screen reader
  announces it reliably; the checkbox's `aria-describedby` points at it while unknown, and the box
  cannot be toggled meanwhile (a tick made before a swap stays ticked). Data is read **only on
  `status === "success"`** (here and in `ApplyResult.vue`): Colada keeps the previous `data` on a
  failed refetch (`status: "error"`), which is the replaced session's answer. A *failed*
  `get_merge_mod` therefore reads as no entries (no summary, box enabled, plain skipped-merge
  wording); there is no error UI for it.
- **The Apply dialog confirms hard problems; it has no needs-input gate.**
  `useApplyPreflightQuery(source)` (enabled while the dialog is visible)
  holds `get_apply_preflight`; `submit()` asks `requiresConfirmation` (computed
  in Rust, never re-derived in TypeScript), always awaiting a **fresh**
  preflight first (`refreshPreflight()`: refetches when stale or errored, joins
  an in-flight call) because Colada keeps stale data after an invalidation, so
  held data may predate an undone decision or a rescan. A fetch error shows
  and writes nothing; resolving with neither data nor an error is an
  invariant breach and throws — never read as "nothing to confirm". When it is true and `writeModsConfig` is on, `awaitingProblemConfirm`
  swaps the action row for `ApplyConfirm.vue` ("Go back" focused, "Apply
  anyway" never default); only "Apply anyway" calls `performApply`. The
  running-game refusal (`awaitingForceConfirm`, "Write anyway") is a separate,
  later step, never merged into the same button. The needs-input count is
  information only (a count and an Inbox link); the old `applyAnyway` /
  `canSubmit` checkboxes in the dialog and on the Dashboard are gone.
- **The Mods page's selection is a client-side view of the backend's lists.**
  `useModSelection` works over the rows of the current `list_working_mods`/
  `list_inactive_mods` response, which the backend already filtered, so "Select
  all N shown" never re-implements a filter in Vue. A selected id a later filter
  hides stays selected and is counted ("N not shown"); Clear removes all. An
  *unfiltered* response is the whole list, so a selected id missing from it is
  pruned (`ModListPane`'s watcher); under a filter it is only hidden. After a commit,
  `useModActivation`/`useModDeactivation` drop from that list's selection every id the
  backend's commit-time plan names, never just the requested ids: `activated` plus
  `alreadyActive` (requested mods found active by base id) for Inactive, `removed` plus
  `notActive` (requested mods no longer active) for Active. So a dependency pulled in under a
  filter that hides it leaves the selection, and a refused Core stays. The "(N not
  shown)" brackets are part of the `mods.selection.notShown` message, never the
  template, but the gap before them is the span's `ms-1` margin, not whitespace:
  vue-i18n trims whitespace around each plural form, so a leading space in the
  message would vanish from form 2 on. A CJK locale writes full-width brackets and
  can ignore the small margin. Selecting
  writes nothing: the activate/deactivate plan and confirm dialogs are unchanged,
  and a failed plan opens the dialog with the error and no plan (Confirm
  disabled). The always-visible "Rescan mods" button (`ModsRescanButton.vue`) is
  `useRescanMutation`, so the `rescan_project` lock protocol and its refusal
  ("working set changed during the rescan") are unchanged.
- **The Mods page is two side-by-side `ModListPane`s, and every position change
  ends in an anchor.** Inactive left, Active right (`lg:` and up; stacked
  below), each pane with its own filter (`useModListFilters`, one per list), sort
  and selection, in a `grid` whose cells are `min-h-0` so each `ModTable` scroller
  keeps the definite height the virtualizer needs. The list-level `data-testid`s
  (`mod-table`, `mods-total`, `mods-search`, `mods-sort-<key>`, ...) now exist twice, so
  tests scope them through the pane's `mods-pane-active|inactive` (`modsPane(page,
  kind)` in `e2e/specs/support.ts`). The sort bar is one toggle button per criterion (`ModSortBar.vue`): the
  active one flips its direction, another starts ascending (`nextSort` in
  `utils/modListSort.ts`). It has no `aria-pressed` (it never un-presses and its
  name changes); the active state is in its accessible name and title plus an
  arrow and a thicker border. *Anchors name a neighbour by id, never an index*
  (`AnchorDto`, `utils/modMove.ts`): a drop gap, a one-step move and a "Move
  after…" choice are all turned into `before`/`after` a **shown** row that is not
  itself moving (`anchorForSlot`, `anchorForStep`), so a filtered view stays exact
  and the backend refuses a stale neighbour (`move_anchor_not_active`: the page
  toasts it and refreshes the lists). Never send a row index or a sorted-view
  position. A reorder is gated on `ModListDto.reorderable` alone (the sort is never
  re-checked in TypeScript): when it is false, drag over Active is refused, the Move
  menu items are disabled with the reason in the menu, Alt+Arrow announces the
  reason, and a note above the list says why. A mod dragged in from Inactive
  still activates, with `anchor: null`.
- **Row polish on the Mods page (#54).** The open row is `ModsPage`'s route `modId`, passed down as
  `openModId` (never the last focused row): it gets `aria-current`, the accent ring and the
  `bg-accent-soft` tint; selection keeps only the ring. A plain double click emits `toggleActive`
  and the page runs `openActivateFor`/`openDeactivateFor` with that one id, the same plan and
  dialog as the buttons (Ctrl/Shift double clicks are selection and do nothing, and a double click
  on the row's checkbox, menu button or chip is ignored). The row menu's colour swatches sit in
  the `Menu`'s `#start` slot (pointer-only real buttons, one `set_mod_colors` call, no dialog);
  keyboard users have the "Colour..." entry. A group chip or dot on a row is a pointer-only
  `<button tabindex="-1" aria-hidden>` (the grid keeps one tab stop per row; its `mousedown` is
  prevented so focus stays on the row) that sets the page's group scope; the keyboard route to the
  same scope is the groups panel. There, the groups the open mod belongs to carry a small accent
  dot (not the scope tint) and every group a folder icon. The row stripe is the mod's own colour,
  else its first coloured group (`ModListRowDto.groups` arrives in group-tree order from
  `rim-session`, never re-sorted in Vue), faded, with screen-reader text that names the group.
  "Create group" in the Add to group dialogs is `create_group` with `modIds`: one backend edit and
  one save, and the outcome's `members` is the count the toast reports (never counted in Vue); the
  dialogs offer it only when no group has that exact name (case-insensitive), and the backend still
  decides sibling clashes and limits.
- **Drag and drop is native HTML5, with no dependency, and the page owns the
  drag.** `useModReorder` holds the one `DragState` (source list, moving ids,
  whether Core is among them) and answers `dropModeFor(list)`: `position` (a line
  between rows, accepts), `pane` (accepts anywhere), `refused` (no `preventDefault`
  on `dragover`, so the not-allowed cursor shows, plus the reason over the list) or
  `off`. `ModTable` only reports `dragStart`/`dragEnd`/`drop(slot)` and draws the line
  (a gap is `round(offsetY / 40)`, rows being a fixed 40px); it never decides what a
  drop means. SortableJS-style libraries move DOM nodes the virtualizer owns, which is
  why none is used. Edge auto-scroll is `useDragAutoScroll` (a frame loop fed by
  `dragover`, `utils/dragScroll.ts` for the speed). The dragged set is the whole
  selection when the dragged row is selected (`moveSetFor`), else that row. A drop
  commits without a dialog only when the plan is exactly the dropped mods with no
  warning (`isPlainActivation`/`isPlainDeactivation`); otherwise the same
  Activate/Deactivate dialog opens and its confirm commits the same anchored
  request (`useModActivation.activateAt`, `useModDeactivation.deactivateDropped`).
  The buttons always open the dialog. Core is refused for Inactive by `source ===
  "core"` of a shown dragged row; the backend plan refuses it too.
- **Every move is announced through one always-mounted `role="status"` region**
  (`mods-move-status` in `ModsPage.vue`), never a toast: "Moved X to position 14 of
  312", read from the refreshed Active list after the mutation's invalidation has
  landed (the position is the row's unfiltered `position`; the total is left out
  under a filter, where `total` counts only the matches). Edge and
  reorder-blocked answers use the same region. Keyboard: `Alt+ArrowUp/Down` on an
  Active row goes through `ModTable.onMoveKey`, which checks `isInert` first
  (`shortcutTargets.ts`: dialogs and text fields keep their keys) and runs *before* the
  roving arrow handling, so a plain arrow still moves focus and an Alt+arrow never
  does; after a keyboard or menu move the page puts focus back on the row
  (`ModTable.focusRow`, exposed through `ModListPane`). The row's Move button
  and the position picker (`ModPositionDialog`: "Move after…" and "Activate at…")
  search the **working** list (`useWorkingModsQuery`), not `useModsQuery`
  (`ModPicker`'s scanned tier), so a just-activated mod can be picked.
- **A move adopts the backend's selection, and the page says so.** `useMoveModsMutation`
  and `useActivateModsMutation` call `session.setSelected(result.selected)` the way
  `useAdoptImportedSession` does, and `didSwitchToCurrent` raises
  `OrderSwitchedNote` ("Now using your own order. The suggested order is unchanged.
  [View suggested]", gone once Suggested is selected again). `OrderNotInFileNote
  allow-revert` adds "Revert to your saved active order" (ModsConfig.xml named only in its tooltip) on the Mods page (only for a
  pure reorder: `orderUnapplied` with nothing unscanned or unapplied by
  membership, and no outside change of the file: `RevertOrderToFile` compares with the live
  file, so beside an outside edit it would be refused or would adopt the outside order, and
  `FileChangeNote`'s own action covers that case); `PendingChangesBanner` carries "Discard pending changes". Their
  refusals go through `utils/errors.ts` like every other command error.
- **The mod info panel is a drawer unless the window is very wide.**
  `ModsPage.vue` reads `useMediaQuery("(min-width: 1800px)")`: below it
  `ModInfoPanel` is `fixed` over the right edge (`isOverlay`; Escape closes it from
  anywhere except dialogs and text fields) and `followFocus` is off, since an arrow
  key through a list must not pop a drawer over it; at or above it the panel is a
  third column and follows focus as before. `SourceBadge`'s visible label is a
  container query (`@lg:`) on the table, not a viewport breakpoint, because two lists
  share the window.
- **The Dashboard strip is derived, never stored.** `GuidedFlowStrip.vue`
  reads `utils/guidedFlow.ts`'s `guidedStep()` over five facts (the
  recommended-rules step's `rules` state, `selected`,
  `DashboardDto.fileMatchesSuggested`, the stale active set, and its own
  dialog-open flag) and owns the Dashboard's one `ApplyDialog`. It has four
  steps, the first being "Get the recommended rules". `rules` is the
  `RecommendedRulesStepDto` derived in Rust (`rim_session::recommended_rules_step`;
  never re-derived in TypeScript) and is `null` until its query answers, so
  the Dashboard never blocks on it and the strip falls back to the other
  facts. Precedence: an open Apply dialog and the `done` file match are
  unchanged, so an open Apply dialog outranks an offered step 1;
  `needsAction`/`inProgress` hold the flow on step 1 and outrank `done`
  (importing changes the suggested order); `skipped`/`unavailable` never hold
  it. `skipped` carries the same `sources` as `needsAction` (owner decision
  Q11), so its row lists them and its "Get them now" button discloses the size
  like the offered one; an import failure crosses as a code
  (`ImportFailureCodeDto`) worded per code, with the backend's English text
  only as the toast's technical-details line (Q12). Step 1's state, its click, Skip and the progress tick live in
  `composables/useGuidedRulesStep.ts`, its text in `GuidedRulesBody.vue` and
  its controls in `GuidedRulesActions.vue`. Its live regions speak on a
  transition into done and as each phase of a running click begins.
- **The recommended-rules step's server state.** `queryKeys.recommendedRulesStep()`
  is `["rules", "ruleDatabases", "recommendedStep"]`, nested under the
  `ruleDatabases` prefix on purpose: Pinia Colada's `invalidateQueries({ key })`
  matches a key as a prefix (`isSubsetOf`, verified against 1.4.5), so every
  `ruleDatabases()` invalidation (refresh, enable) refreshes it too. The mutations
  that change its inputs without touching that prefix invalidate it explicitly:
  `useUpdateAppSettingsMutation`, `useResetNetworkPolicyMutation` and the Welcome
  answer/dismiss mutations. `useGetRecommendedRulesMutation` invalidates in
  `onSettled` (not only `onSuccess`) and awaits the refetch: a `NotNeeded`/`Failed`/
  `ProfileChanged` result or a refused run emits no `session://changed`, yet phase 1
  may already have written the settings, the cache or a last-failure record. A
  mutation's `isLoading` (not `isPending`, which is true before it ever runs)
  is its in-flight flag.
- **A running "Get the recommended rules" click is guarded in the backend and
  reported by an event.** `AppState::recommended_rules_running` (an RAII guard)
  makes a second concurrent call fail with `already_running` and makes
  `get_recommended_rules_step` answer `inProgress`, so a second window or a
  remount shows the running state. The command emits
  `rules://recommended-progress` (`RecommendedRulesProgressEventDto`: `downloading`
  per source, then `importing`); the strip keeps only the latest tick (read through
  `useTauriEvent`) and shows the slow-connection note while Steam downloads. No
  Cancel in 1.1.0. `recommended_rules_unavailable` carries
  `detail.reason` (`networkOff` | `awaitingFirstRun`), which
  `utils/errors.ts` turns into a different sentence; `app_settings_damaged`
  points at saving settings in Settings, the repair. The Playwright mock
  (`recommendedRulesStepFor`/`getRecommendedRulesMock` in `scenario.ts`) derives the
  step and its outcomes from the scenario's own state, really reorders the
  suggested order on import, keeps the skip flag across a reload (`sessionStorage`,
  standing in for the profile file) and drops the two rule-database notices' offered
  sources, as the backend does; `__GET_RECOMMENDED_RULES_CALLS__`,
  `__SKIP_RECOMMENDED_RULES_CALLS__` and `__RECOMMENDED_RULES_PROGRESS__` record
  its calls.
- **`ApplyDialog.vue` has a positive "Write your active mod list" checkbox
  (`writeModsConfig`, checked by default; a negative "export only" box was
  misread as the ModsConfig toggle). It forces it off and disables it
  while the active set is stale** (the backend refuses a real write
  anyway; this states why before the refusal), restoring whatever the
  checkbox held before staleness once it clears rather than resetting to
  a fixed default. The primary button's label comes from
  `utils/applyButtonLabel.ts` over both boxes (Apply / Write merge mod /
  Save decisions and rules). Its "Write merge mod" checkbox starts **unchecked**
  even when complete merge entries exist (writing into `Mods/` is a
  deliberate act, like the CLI's opt-in `apply --write-merge-mod`); the
  user's choice holds for the dialog's lifetime and resets on reopen.
  `PatchExportPanel.vue`/`AssignmentExportPanel.vue`
  both warn, once `install` is checked and the working set is
  unscanned-stale, that pending activation changes are not included: an
  install-time export still writes the session's *Current* order (the
  analysed one, not the working one) plus the new package id, silently
  omitting pending activation changes rather than refusing. They also warn,
  with their own `unappliedOrderNote`, when `PendingActiveChangesDto.orderUnapplied`
  is true: Current is not in the file yet (a settled move, an import; an outside edit
  alone does not count), and
  install writes it, so an unapplied reorder reaches `ModsConfig.xml` with the
  export. That is kept on purpose, as for an unapplied import; the warning is
  the mitigation. A third note (`outsideChangeNote`, from `useFileBaselineQuery`'s
  `externalChange.kind !== "none"`) says install writes Current over an outside change of
  the list.
- **The selected order is backend-led.** `load_project_inner` selects
  `INITIAL_ORDER_SOURCE` (Suggested, the desktop's own presentation
  default; `Session::new` stays on Current for the CLI) before the
  session is swapped in, and `ProjectSummaryDto.selected` reports it:
  `SetupPage.vue` passes it to `session.setLoaded(paths, selected)`, and
  `useRescanMutation` adopts the rescan's `selected` the same way, so the
  Pinia store never holds a second default of its own.
  `rescan_project_inner` (`rescan_with` with `ScanRequest::Rescan`)
  reads the live session's selection at the swap
  (not a snapshot taken before the scan, which a `select_order` made
  mid-scan would outrun) and puts it on the new session.
  `import_order` selects Current instead, through `ImportOrder::finish`, which
  also checks that the scan produced the validated order.
  `ApplyRequestDto.source` is required: `useApplyDialog` sends
  `session.selected`, and `apply` writes that order.
- **A finding's winner is computed in Rust for the selected order.**
  `FindingDto::TextureOverride.owners` is in scan order and carries a separate
  `winner` (the owner loaded last in the order the ledger was built for);
  `TexturePair.vue` badges `finding.winner` and never reads `owners.at(-1)`.
  The mock's `findingFor` recomputes it from the selected order, mirroring
  `ledger::findings::conflicts::last_loaded`. `DefOverride.winner` still comes
  from the analyzer's scan order (not yet order-aware).
- **A background invalidation goes through `startInvalidation(queryCache, filters?)`
  (`queries/invalidation.ts`), never `void queryCache.invalidateQueries(...)`.** Colada's
  `invalidateQueries` is a `Promise.all` over the refetches and each refetch rethrows its
  query's error (1.4.5 `fetch`), so a failing query under a bare `void` is an unhandled
  rejection. The helper swallows it on purpose: the failed query already carries the error
  in its own state. A caller that needs the refetch to land first (a mutation's `onSuccess`)
  uses `awaitInvalidation(queryCache, ...filters)` from the same module, which resolves when
  every refetch has settled and never rejects (no filters means every query). Never return or
  await a raw `invalidateQueries` from `onSuccess`/`onSettled`: Colada awaits those inside
  `mutate`, so one unrelated query's failed refetch would fail a mutation whose write already
  committed (status `error`, the app-wide `onError` toast for the wrong operation, a rejected
  `mutateAsync`).
- **`session://changed` fires after any mutating command and invalidates
  every query by default.** `useSessionEvents`'s
  `HANDLED_BY_MERGE_MUTATION` is a skip-list of reasons already handled
  by a mutation's own targeted invalidation (`mergeChanged`,
  `patchDecided` — `useSetMergeChoicesMutation`'s own `onSuccess`), not
  an exhaustive switch: a new `SessionChangeReasonDto` variant falls
  through to "invalidate everything" with no code change required.
  **One reason has its own targeted refetch instead:** `organizationChanged`
  (every organization command: groups and mod colours) refetches only
  `ORGANIZATION_QUERY_KEYS` (the `organization` query and the two Mods
  lists), because organization touches no order, sort, ledger or active
  list. A new query that shows organization must share one of those key
  prefixes, or be added to the list. The prefixes come from `queryKeys`, not
  from copies. A command that fails with `group_not_found` emits the same
  event (the frontend's tree is stale), so the error text's promise that
  "the groups have been refreshed" is true.
- **ts-rs bindings are generated, never hand-edited.** After any DTO
  change: `cargo test -p rimmerge-desktop` regenerates
  `src/types/generated/`, then `bun run types:check`.
- **The real-install test tier follows the identical
  `RIMMERGE_GAME_DIR`-driven, three-state, 200-active-mod-floor rule as
  the CLI's** (`real_install_support.rs`'s own
  `require_game_dir`/`require_profile_dir`). A real-install ignored test
  asserts shape/band/determinism (non-empty, every member active, at
  most a small ceiling, identical across two runs of the same session),
  never a hardcoded mod list; an optional `RIMMERGE_EXPECTED_*` env var
  is a maintainer's own opt-in exact pin. See
  [`docs/testing.md`](../../docs/testing.md) for the general mechanism.
- An agent whose cwd is `apps/desktop` must not create
  `apps/desktop/.claude` — subagent memory lives at the workspace root
  (`.claude/agent-memory/`, gitignored).
- **Any window or plugin API called from JavaScript needs its permission
  listed in `capabilities/default.json`.** `core:default` only covers each
  plugin's own `*:default` set (e.g. `core:window:default`, which excludes
  `allow-destroy`/`allow-close`) — a missing permission fails silently: the
  IPC call is denied, the rejection is easy to swallow, and nothing in the
  UI signals why (see `PendingChangesCloseGuard.vue`'s `destroy()` call and
  `src-tauri/src/capabilities.rs`'s regression test). The same file pins
  that no capability grants `opener:*` or `shell:*`: every program or URL
  this app starts (links, RimWorld) is derived in Rust, never named by the
  webview. Exporting a load order needs `dialog:allow-save` (the native save
  dialog; `dialog:allow-open` does not cover it), and no capability may grant
  an `fs:*` permission: `export_order_file` writes the chosen path from Rust.
  That command takes only a `.rml` target (ASCII case-insensitive; `.xml`, no
  extension and `.rml.xml` are refused as `invalid_input`), an allowlist because
  `canonicalize` cannot see every alias of `ModsConfig.xml` (a `\\localhost\C$`
  share, a mapped drive); the same-file check stays as a second guard.

## Gates

```sh
cargo fmt --all -- --check
cargo clippy -p rimmerge-desktop --all-targets --all-features -- -D warnings
cargo nextest run -p rimmerge-desktop --all-features
bun run types:check   # cargo test -p rimmerge-desktop, regenerates src/types/generated/
bun run typecheck && bun run lint && bun run test
bun run e2e            # Playwright, mock IPC; must pass twice in a row with zero local retries
bun run e2e:smoke      # Playwright/CDP against `tauri dev`; local only, see docs/testing.md
bun run e2e:smoke:i18n # Playwright/CDP, its own `tauri dev` + WebView2 `--lang=zh-CN`; local only
```

Full workspace gate block: root `CLAUDE.md`.

## Tauri configuration (`src-tauri/tauri.conf.json`)

JSON has no comments, so the rationale for two otherwise-unexplained
values lives here instead:

- **`app.security.csp` vs. `devCsp`**: the production `csp` never
  includes `ws://localhost:5173`/`http://localhost:5173` — those exist
  only for Vite's dev-server HMR websocket and asset requests, and have
  no reason to be reachable from a built, shipped binary. `devCsp`
  carries the identical policy plus those two dev-only sources; per
  Tauri's own schema, `devCsp` (when set) is what's injected during
  `tauri dev`/`bun run tauri dev`, and `csp` alone is what ships in a
  release build. Changing either string, keep the two in sync except
  for that one difference.
- **`bundle.active` is `false`: Rimmerge ships as a portable zip, not an
  installer.** `bun run tauri build` emits only
  `target/release/rimmerge-desktop.exe` (frontend and the rules bundle
  are embedded in the exe; `tauri.conf.json` declares no `resources`, so
  nothing else is needed at runtime). `bundle.icon` stays because
  `tauri-build` embeds `icons/icon.ico` into the exe. There is no WiX
  `upgradeCode`, MSI, or NSIS config; do not add one without a design
  decision. `scripts/package-portable.ps1` (shared with
  `.github/workflows/release.yml`) zips the exe as `Rimmerge.exe`
  beside the CLI and the licenses; nothing depends on the exe's file
  name. Data stays out of the exe's folder: `%LOCALAPPDATA%\rimmerge`
  (the app's own data) and `%LOCALAPPDATA%\dev.rimmerge.app` (the
  WebView2 profile, named by `identifier`: EBWebView cache plus the
  `localStorage` display prefs). Both belong in the uninstall docs.

## Localization (i18n)

The UI is localized in `en` plus twelve translated locales (`zh-CN`,
`pt-BR`, `ru`, `uk`, `pl`, `de`, `fr`, `es-ES`, `tr`, `ja`, `ko`,
`zh-TW`; `SUPPORTED_LOCALES` in `src/i18n/locales.ts`) via `vue-i18n` v11
(Composition API, `legacy: false`); the CLI stays English. Foundation
(`src/i18n/`) and structure:

- **Every user-visible string in a `.vue` template goes through `t()`**
  — never a bare string, a module-level constant table of labels (see
  `TheShell.vue`'s `NAV_ITEMS`: a `labelKey`, resolved by `t()` at
  render time, not a `label` baked in once), or a raw enum value. The
  `vue/no-bare-strings-in-template` ESLint rule enforces this across
  every `.vue` file (`eslint.config.js`) — it started as a ratchet whose
  `files` list grew component by component as each one migrated; that
  migration is done, so any new component is covered from its first
  commit, with no file to add to a list.
- **Helpers in `utils/`/`composables/` return a `{@link MessageDescriptor}`
  (`{ key, params? }`, `src/i18n/messageDescriptor.ts`), never a
  rendered string** — the component calls `t()` on it. This keeps
  helpers pure (no `useI18n()` outside `setup`), keeps every
  `assertNever`-terminated exhaustive switch intact (each arm returns a
  literal key), and keeps a locale switch correct with no reload
  (nothing translated is ever cached at module scope — see
  `resolveStoredLocale`/`setAppLocale`, `src/i18n/i18n.ts`).
- **Backend text reaching the UI arrives as codes/enums, the frontend
  renders the sentence.** `CommandError` is rendered by `code`
  (`utils/errors.ts`'s `describeCommandError`, keyed by
  `CommandErrorCode` through a `satisfies Record<...>` table, not a
  template-literal key — a new code is a TypeScript error, not a silent
  gap); its `message` stays English, shown only as an expandable
  "technical details" line (`CommandErrorDescriptor.technicalDetail`).
  `DefaultPathsDto.warningCode`/`.errorCode` (`utils/defaultPaths.ts`)
  are the same pattern for `get_default_paths`'s own prefill
  warning/error, with no plain-string companion (the English `warning`/`error`
  fields were removed; the checked locations cross as paths in `errorCode`).
  The same rule
  applies to any future DTO field carrying prose built in `src-tauri` —
  add a code/enum, not a translated string field, unless nothing but
  that one hard-coded English sentence exists to send (rare, and a
  signal to add the enum instead). `rim_resolve::domain::Rationale`
  (`SuggestionDto`/`AlternativeDto`/`VerifyReorderDto.rationaleCode`,
  rendered by `utils/rationale.ts`) is this same pattern's largest
  instance — but its own plain-string `rationale` field, unlike
  the removed `DefaultPathsDto` warning/error strings, is never a transitional
  fallback due to be dropped: `useApplyDialog.ts`'s "Create pair rule"
  action copies it verbatim into a new rule's own `comment`, which is
  **persisted** in `rules.json` and can be shared or published to a
  rules database, where English is the common language — a translated
  comment would be unreadable to whoever pulls that rule in.
  `VerifyReorderActions.vue` still renders the localized `rationaleCode`
  as the row's own context text next to the button, so the user reads a
  translated reason before clicking; only the rule's own stored
  `comment` stays English.
- **`useTranslateMessage()` (`composables/useTranslateMessage.ts`)
  renders a `MessageDescriptor` a helper already returned** — `const tm
  = useTranslateMessage();` then `{{ tm(errorMessage.title) }}` in the
  template, instead of repeating `t(d.key, d.params ?? {})` at every
  call site. Reach for it whenever a value in hand is already a
  `MessageDescriptor` (an exhaustive-switch helper's return, a
  `CommandErrorDescriptor` field, …); call `t()` directly for a literal
  key. `MessageDescriptor.count`, when a helper sets it, is the plural
  selector for `t(key, named, count)` — but only as a fallback:
  `@intlify/core-base`'s own `getPluralIndex` reads `named.count`/
  `named.n` first if either is set, and only falls back to this explicit
  third argument when neither is. `params.count` (set for the message's
  own `{count}` placeholder) already drives plural selection on its own
  in practice — a plural-producing helper still sets both, for the rare
  message with a count but no `{count}` placeholder of its own.
- **Keys are literal strings in source**, nested by feature
  (`shell.*`, `settings.*`, …), camelCase segments, an enum family's
  wire value verbatim as the last segment. No dynamic key
  construction — an enum-driven family is mapped through an exhaustive
  switch or a `satisfies Record<Dto, MessageKey>` table. This is what
  `src/i18n/locales.test.ts`'s unused-key check relies on (every `en`
  key must appear as a source literal) and what makes a new DTO variant
  a compile error instead of a silent gap.
- **`primevue.*` is the one namespace never looked up through `t()`.**
  It is vendored (PrimeVue's own English defaults, ~15 keys) and
  bulk-copied onto `usePrimeVue().config.locale` by `setAppLocale` —
  excluded from the unused-key check for exactly that reason.
- **Every locale must have no missing keys.** `locales.test.ts`'s
  `STRICT_TRANSLATION_PARITY` was flipped to `true` (2026-09-26, once
  `zh-CN`/`pt-BR`'s own translator agents finished) and stays `true` for
  good — a missing key fails the gate for each locale, every run (a
  locale file that is still `{}` runs in English, via `fallbackLocale`, but
  fails its own row until its translation is done),
  alongside the always-on extra/malformed-key, placeholder-parity and
  plural-form-count checks. A single locale can still be checked with
  `I18N_STRICT_LOCALE=zh-CN bun run test` (or `pt-BR`), now a redundant
  superset of the same check — see `docs/translating.md`. A locale
  file's `_pending` key (or any `_`-prefixed leaf) is metadata, not a
  translatable string, in both modes.
- **Plurals are pipe-separated forms** (`"{count} mod | {count} mods"`),
  picked by an `Intl.PluralRules`-backed selector per locale
  (`src/i18n/format.ts`), not vue-i18n's own (nonexistent) ICU support.
  The form count and order per locale live in one table,
  `src/i18n/pluralForms.ts` (also read by `locales.test.ts` and
  `scripts/i18n-status.mjs`, and documented in `docs/translating.md`):
  `en`/`pt-BR`/`de`/`fr`/`es-ES`/`tr` are 2-form (`one | other`; pt-BR and fr
  treat 0 as `one`), `ru`/`uk`/`pl` are 3-form (`one | few | many`),
  `zh-CN`/`zh-TW`/`ja`/`ko` are 1-form. vue-i18n applies the *current*
  locale's rule even to an English message reached through the fallback, so
  the selector reads a message whose form count is not the locale's own
  with English's one/other rule.
- **A system language maps to one of our locales in `detectLocale`**
  (`src/i18n/locales.ts`): Traditional-Chinese tags to `zh-TW`, other
  Chinese to `zh-CN`, any `es-*` to `es-ES`, any `pt-*` to `pt-BR`, other
  regional variants to their language; a language with no catalogue is
  skipped. `:lang(...)` rules in `style.css` pick the CJK font per locale.
- A new feature (the mod info panel, notifications) adds its own keys
  under its own top-level feature key from its first commit — never
  hardcoded English pending a later "localize this" pass. Its own DTOs
  follow the codes-not-prose rule above from the start.
- **A sentence never forces English word order by concatenating
  translated fragments around a `<code>`/`<strong>` element in the
  template** (`{{ t("x.intro") }}<code>...</code>{{ t("x.outro") }}`) —
  a translator can only ever place the rich content where English does.
  Write one whole-sentence key instead and render it with vue-i18n's
  `<i18n-t keypath="..." tag="p">`, giving each placeholder its own named
  `<template #name>` slot (plain text or `<code>`/`<strong>` alike) —
  see `SuggestionPanel.vue`'s `edgeDropped`/`danglingDefReference`/
  `brokenInheritance` blocks. A branch that needs a structurally
  different sentence (not just a different word) gets its own whole key,
  selected by a literal-string ternary or a small exhaustive-switch
  helper returning the key (never a template-literal/dynamic key).

## Conventions

- **DTOs**: `rename_all = "camelCase"`, tagged enums carry `kind`
  (`#[serde(tag = "kind")]`; the one documented exception is `type`,
  used when the enum is itself the value of a field named `kind` -- the
  three existing ones, e.g. `LoadEventKindDto`), struct variants need
  their own `rename_all`, `FindingKey`/`FieldPath` cross as strings, `u64` fields carry `#[ts(type = "number")]`, request
  DTOs use `deny_unknown_fields`. Mod lookups compare `ModId::base()` so
  a `_steam` copy resolves the same as the primary one.
- **Rust module layout**: `commands/assignments.rs` keeps every
  `#[tauri::command]` function and delegates to `*_inner` functions in
  `assignments/{projects,sections,rows,coverage,export}.rs`.
  `generate_handler!` resolves each command's path together with the
  `__cmd__*` macros the attribute generates next to it, and re-exporting
  the function by name alone does not carry those macros. `error.rs`
  keeps `CommandError`/`CommandErrorCode` and holds the `From` mappings
  in `error/{project,findings,merge,mods,patches,assignments}.rs`.
  `dto/finding.rs` and `dto/assignment.rs` are facades too. A facade's
  former inline tests live in one sibling `<facade>_tests.rs`, declared
  with `#[cfg(test)] #[path = "…"] mod tests;`.
- **`ts-rs`'s `#[ts(type = "Record<string, V>")]` override cannot be
  used when `V` is a type this crate exports separately** — confirmed
  against `ts-rs`'s own `type_override_struct`, which always builds an
  empty `Dependencies` set regardless of the override string's own
  contents, so the generated file never gets the import a non-primitive
  `V` needs. Such a map (e.g. `FieldSpecMapDto`, the active-set plan's
  `unresolvableDependencies`/`dependentsStillActive`) takes the plain
  derive instead, exporting as `{ [key in string]?: V }`; the frontend
  reads it through `types/dtoMaps.ts`'s `asRecord()` once at each
  component boundary rather than threading `| undefined` through every
  access.
- **`services/ipc/core.ts` is the only module calling `invoke`** (its
  `call` wrapper); `services/ipc.ts` is the named re-export facade every
  caller imports, over
  `services/ipc/{project,findings,rules,verify,merge,patches,
  assignments,gameLaunch,orderShare}.ts` (active-set calls live in `project.ts` alongside the
  rest of the project lifecycle). `session_lost`/`no_project_loaded` route
  to setup through `setSessionLostHandler`.
- **A shell component `provide`s its own state through a typed
  `InjectionKey`, never a plain string key.** `ApplyDialog.vue` is a
  shell owning state via `composables/useApplyDialog.ts` and
  `provide`s it (`APPLY_DIALOG_KEY`) to `ApplyPreflight.vue`/
  `ApplyProgress.vue` (nests `ApplyOrderDiff.vue`)/`ApplyResult.vue`,
  each reading it back with `useApplyDialogState()`.
  `components/assignments/RowEditor.vue` likewise provides
  `composables/rowEditorContext.ts`'s `ROW_EDITOR_KEY` to
  `RowEditorSlots.vue`/`RowEditorMatches.vue`.
- **An assignment command addressing one section falls back to the
  project's only section, and errors naming every section once there is
  more than one.** `commands/assignments.rs`'s `resolve_section` (an
  optional `section` field on `set_assignment_row`/`clear_assignment_row`/
  `get_assignment_coverage`/`copy_assignment_row_from`'s request DTOs)
  mirrors `apps/cli`'s own `resolve_section`/`resolve_section_type`
  exactly. `add_assignment_section`/`remove_assignment_section` wrap the
  matching `rim-session` use cases; a `SectionError::SectionInUse`
  refusal gets a dedicated `CommandErrorCode::AssignmentSectionInUse`
  (not generic `InvalidInput`) carrying a structured
  `CommandErrorDetail::AssignmentSectionInUse { referenced_by }` so the
  frontend can render every referencing row rather than re-parsing a
  message string.
- **`CoverageDto.applicable` is `false` for a standalone ("new def")
  project** (no `TargetKey` field to match a target through) — `rows` is
  always empty then, and a consumer must never render that as an empty,
  seemingly-fully-covered queue. `AssignmentEditorPage.vue` decides this
  from the section's own `isStandalone` before ever firing the coverage
  query, so a free-standing section's pane never requests coverage at
  all.
- **`EditReferencesTargetsDialog.vue` renders every section's own
  `schemaChanges`/`strandedValues`/`droppedRows` entry by name after an
  R/T (references/targets) edit** — a data-loss notice, not a nicety,
  the same per-field disclosure convention `RowEditor.vue` uses for a
  dropped or clamped value — or an explicit "nothing was dropped or
  stranded" confirmation when there's genuinely nothing to report.
- `copy_assignment_row_from`'s own "existing instance" picker is scoped
  to the selected coverage row's own `matches`, not a search across
  every active instance of the assignment type — the row editor's own
  "Copy from" chips are exactly the target's `CoverageRowDto.matches`.
- **Server state is Pinia Colada queries; stores hold UI state only.**
  Use `isPending` for placeholders, never `isLoading` (it flips on
  every background refetch and unmounts content). Tauri events go
  through `useTauriEvent` (handles unlisten races).
- **Every keyboard-shortcut composable (`useInboxKeys`, `useMergeKeys`,
  `useAssignmentEditorKeys`) shares one inert predicate**
  (`composables/shortcutTargets.ts`'s `isInert`): a shortcut never fires
  inside a `role="dialog"`, a text-entry control (`input`/`textarea`/
  `select`), `contentEditable`, or a `role="combobox"` — kept in one
  place so the inert rules can't drift between composables. A
  `role="radiogroup"` (the shell's Current/Suggested and names/ids
  switches, the texture viewer's facing control) owns only its **arrow
  keys**: `shortcutTargets.ts`'s `ownsArrowKeys` is checked for `Arrow*`
  keys alone, so every other list shortcut still works after a switch
  click.
- Biome's `useLiteralKeys` is off because `noPropertyAccessFromIndexSignature`
  forces bracket access on `route.params`. Vitest files are type-checked
  through `tsconfig.vitest.json`.
- A Vitest guard that only scans sources with `node:fs` declares
  `// @vitest-environment node` (no happy-dom window), so it stays inside the 5 s
  default timeout under load.
- **An imported game log's facts follow its coverage.** `GameLogSummaryDto.coverage`
  is a `kind`-tagged union whose variant is the log's kind (`playerLog` /
  `consoleSnapshot`); a consumer of the summary checks it
  (`utils/gameLogCoverage.ts`'s `isConsoleSnapshot`) before using a fact only
  a `Player.log` holds (timers, def-cache build lines, startup passes): a
  snapshot shows "—" (with the reason) for it, never `0`, and its counts are
  lower bounds; so are a `Player.log`'s with a logging gap or cut off before
  any patch result was logged (`areCountsLowerBounds`, deliberately not every
  non-clean end). `ImportedLogSummary.vue` renders kind, coverage, logging gaps and
  read losses wherever the import button is; every number in it (lower-bound
  share included) is derived in `dto/game_log_coverage.rs`, never recomputed
  in TypeScript. The shared test fixture is `utils/gameLogSummary.test-support.ts`.
- **Every raw mod id renders through `useModLabel().label()`** — the
  footer's display-mode toggle (id vs. name) affects every surface
  uniformly; a component reading a mod's own `name`/`id` field directly
  is a bug.
- **Never mount PrimeVue's `ProgressBar` directly — use
  `components/base/BaseProgressBar.vue`** (enforced by
  `progressbar.guard.test.ts`, run against a probe file, not just
  written). PrimeVue renders its label as unrounded `value + '%'`, and
  its own stylesheet puts a 1s width transition on the fill, so a fast
  tick stream visibly desyncs the label from the fill. `BaseProgressBar`
  takes one `progress` prop (`{done,total} | {percent}`, a
  discriminated union so the two shapes can't be mixed), a required
  `label` (its only accessible name), floors rather than rounds the
  percentage (so 99.96% never claims "100%"), and overrides the
  transition to 120ms via an inline style. A `{done, total}` of exactly `0` of `1` (a stage
  that has begun but cannot count, such as the scan's `Analyzing` bracket around the analysis)
  renders indeterminate, never as an empty or full bar; `percent` is always determinate.
- **A list page fills the shell, it does not pick a fixed height.**
  `TheShell`'s root is `h-screen`; a list page's root is `flex h-full
  min-h-0 flex-col`, its header/filters are `shrink-0`, its scroll
  container is `min-h-0 flex-1 overflow-auto` (the definite height
  `@tanstack/vue-virtual` needs). Never `h-[calc(100vh-...)]` on a
  full-bleed page — the viewport is not the shell, and that formula
  overflows by exactly the progress bar's height whenever a scan is
  running. A `ToggleSwitch`/`Checkbox`/`Button` that's a flex sibling of
  free text needs `shrink-0` or it shrinks to a sliver; a `sticky
  left-0` cell needs an opaque background, which then hides its own
  row's `hover:` tint — pair it with `group`/`group-hover:`.
- **PrimeVue 4's severities use `"warn"`, not `"warning"`.** A typo here
  fails silently (no theme match, no visible error) rather than
  throwing.
- **`get_default_rimsort_paths` and `apps/cli`'s own `import` share one
  implementation** (`rim_session::{resolve_from_rimsort_dir,
  resolve_from_cache, should_import_from_cache}`) rather than each
  interface layer reimplementing the cache-vs-local-RimSort precedence.
- **`commands/rules_databases.rs`'s `ALL_DATABASES` is an exhaustive,
  fixed-length array of every `RuleDatabase` source** — adding a fourth
  source fails to compile (a fixed-size-array type mismatch) until it's
  listed there, rather than silently reading as disabled.
- **`refresh_rule_databases` takes an optional
  `RefreshRuleDatabasesRequestDto { sources }`**: absent means every
  enabled source (the Databases card's Refresh, and "Turn on and
  download"), present means exactly those sources, each once, and an empty
  list is `invalid_input`. A notice's action passes what the notice lists
  (`useNotificationActions` for `refreshRuleDatabases`), so one click never
  downloads a source the notice did not name (the 49 MB Steam Workshop
  database in particular). The mutation's variable is the request or
  `null`; the Playwright mock mirrors all of it and records the requests
  in `__REFRESH_RULE_DATABASES_CALLS__`.
- **`get_default_paths` delegates to `rim_io::resolve_each_path`**
  (`resolve_project_paths` is its all-or-error form), the same precedence
  ladder (explicit field > `RIMMERGE_GAME_DIR` > `<base>/config.json` >
  detection > a typed error) `apps/cli`'s own `resolve_paths` uses, so the
  two interfaces can never disagree about where an install is. It takes an
  optional `DefaultPathsRequestDto { gameDir }`, the Setup field as rung 1
  (`get_default_paths_at(base, request)` is the testable half), and each
  path resolves on its own: `modsConfig`/`profileDir` are filled even with
  `errorCode: gameDirNotFound`. The command never fails outright — a
  machine with no install still has to reach the setup form — so the
  structured `errorCode`/`warningCode` come back on the DTO (no English
  prose) and `SetupPage.vue` words them in a persistent `Message`, never a
  toast. The page re-queries (`queryKeys.defaultPaths(gameDir)`) when the
  game folder is committed (Browse or blur, never per keystroke), refills
  each other field only when it is empty or still equals the previous
  default, and ties the banner to the answer for the committed folder.
  `save_app_config` (the desktop half of `rimmerge config set`)
  deliberately never derives or pins `profileDir` from a
  `ModsConfig.xml` path it might not match — that is exactly how two
  installs would end up sharing one profile's decisions.
- **No test in `src-tauri/**` may name a real plugin or mod.** The
  def-cache carrier-detection commands take their carrier list from
  `session.mod_knowledge()` (data, not a hardcoded id), never an
  invented one here. The embedded bundle's own real carrier row is
  covered by a dedicated contract test in `rim-io`'s own
  `mod_knowledge.rs` — that test checks the bundle parses cleanly and
  every section is present, deliberately **not** any row's specific
  content (a carrier id, a class name), since that content is the
  `rules` repo's own to change.
- **`resolve_def_graphic`/`read_def_texture` (`commands/def_graphics.rs`,
  `dto/def_graphic.rs`) are read-only and answer in keys, never paths.**
  `read_def_texture` re-resolves the def and refuses (`invalid_input`) any
  `textureKey` its graphic did not produce, so a forged key reads nothing; a
  malformed key is refused before the def is looked at. `DefTextureDto`
  carries a `data:` URL and the owner mod id, no file path. A DDS the
  texture-override view cannot show is the `texture_unsupported_format` error
  code (a translated sentence in the view), not the backend's English message.
  The mock handlers in `e2e/fixtures/scenario.ts` mirror all of this and are
  pinned by `e2e/specs/def-graphics-mock.spec.ts`.
- **A def's texture is shown by `components/graphics/`.** `DefGraphicViewer`
  (160 px, with facing/variant/source controls; in the row editor header for a
  target-keyed row only) and `DefThumbnail` (decorative: 40 px in the coverage
  queue, 24 px in `ItemPicker.vue` for every row except the project's own new
  `own` rows, which are not on disk; the caller sizes it by class) both read `queries/defGraphics.ts`. A thumbnail mounts its queries
  (`DefThumbnailBody`) only while near the viewport, so a row scrolled away
  releases them and `useDefTextureQuery`'s 30 s `gcTime` drops the image.
  Every `DefTextureDto`/`DefGraphicDto` outcome is worded by
  `utils/defGraphics.ts`'s exhaustive switches (no blank box); the viewer's
  selection resets on a changed def or graphic shape (`useGraphicViewer`), never
  on a refetch that returns an equal graphic. The provider of the shown file is
  named, and flagged when it is not the def's owner.
- **Two DTOs may not share a Rust type name, even in different modules.**
  ts-rs writes one file per name, so the second silently overwrites the first
  and the other consumer then fails `typecheck` in an unrelated place. The
  face availability DTO is `FaceAvailabilityDto` because the apply
  preflight already owns `AvailabilityDto`.
- **A mock-IPC handler must mirror the real backend's own filter/set
  semantics exactly, not just its DTO shape.** A mismatch (for example a
  merge-mod-entry filter that doesn't mirror the backend's own
  decision-status exclusion, or a three-tier active-set model that diffs
  deltas instead of comparing resulting sets) leaves every existing
  Playwright spec green, since none of them modeled the real behavior to
  begin with. When a mock and its real counterpart diverge, the mock is
  wrong, not the spec.
- **`e2e`'s Vite dev server always runs on port 5183 with
  `--strictPort`** (fails fast instead of silently picking another
  port) **and `reuseExistingServer: false` unconditionally** — never
  reused, only one runner occupies that port at a time in this
  workspace. Specs assert the exact IPC payloads
  `e2e/fixtures/scenario.ts` records (its `__X_CALLS__` recorders), not
  presence.
- **`e2e/fixtures/scenario.ts`'s `installScenario` is serialized with
  `Function.prototype.toString()`** for Playwright's `addInitScript`, so
  it cannot import anything from `src/` at runtime — every helper it
  uses must be defined inside that same closure, or a real page load
  gets `undefined`.
- **Every smoke spec must open with its own explicit
  `page.goto(`${APP_URL}/setup`)`** — never assume initial or leftover
  router state, even from being first alphabetically; a spec ordered
  earlier can leave the one shared window on a different route.
- The mock-IPC tier and the CDP smoke tier answer different questions:
  a mock-IPC spec (`e2e/specs/`) exercises frontend wiring against a
  fixed fixture; a smoke spec (`e2e/smoke/`) runs the real backend
  (real inference, real classification, a scratch or the real install)
  and is what actually verifies the two agree — a wizard/schema table
  test belongs in the mock tier only for wiring, never as the sole
  proof the classification itself is correct.
- **The smoke tier uses `RIMMERGE_PROFILE_DIR` and a scratch game tree,
  never the real install** — `e2e/smoke/def-conflict-real.spec.ts` is
  the one deliberate exception: it points the setup form at the
  machine's real, read-only RimWorld/Workshop/`ModsConfig.xml` (never
  written to) to open two real contested defs no synthetic fixture
  reproduces, still against a fresh scratch profile directory.
- **The smoke tier's own `playwright.config.ts` writes a scratch
  `app-settings.json` with `allowNetwork: false` under `RIMMERGE_PROFILE_DIR`
  before the shared `bun run tauri dev` process ever starts**
  (`e2e/smoke/scratch-game.ts`'s `writeScratchAppSettingsNetworkOff`) —
  belt and braces alongside the first-run gate, which already keeps
  every automatic network feature off until the Welcome notice is
  answered and no smoke spec ever answers it: a missing
  `app-settings.json` defaults every network switch **on**, and this
  keeps the tier hermetic even if some future spec did answer that
  notice.

## Traps

- **`CommandErrorDetail`'s `#[serde(tag = "kind", rename_all =
  "camelCase")]` renames variants, not a struct variant's own fields.**
  A struct-variant field (e.g. `referenced_by`) still needs its own
  explicit `#[serde(rename = "...")]` or it ships snake_case on the wire
  despite every other DTO in this crate being camelCase.
- **`upsert_rule` is not idempotent for a rule's own fields.** Writing a
  pair or placement rule from anywhere but `RulesPage.vue`'s own form
  (the verify dialog's "Create pair rule" button, for instance) without
  first reading what's stored would silently clear
  `overridesDeclared` and overwrite a hand-written comment — always read
  the existing `userDecision` rule at that key first and carry those
  two fields forward when one exists.
- **`useApplyDialog.ts`'s `existingUserPairKeys` is a snapshot, never a
  live read.** It's built once when a verify report resolves, from the
  rules query at that moment — a live read would be wrong twice over:
  creating a pair rule invalidates that query, so the row that just
  created one would relabel itself "already exists" moments later, and
  the honest question this answers is what was true of the order the
  prediction was computed against, not what's true right now.
- **A reorder's fix direction is computed once in Rust and never
  re-derived in TypeScript.** `RemovedBy(remover)` orients the remover
  after the failing mod; `NotYetInjected(injector)` orients the failing
  mod after the injector — the two render almost identically, so an
  inverted direction would pass every component test that only checks
  presence.
- **A reorder's `conflicts` line only fires for an edge the sorter
  actually recorded giving up** (in the sort outcome's own dropped-edge
  list), never merely violated — an `Awareness`-strength edge is
  violated by the hundreds on any real, cycle-free order. The "Create
  pair rule" button is never disabled or hidden for a conflict, only
  relabeled "Create pair rule anyway" — the conflict is disclosed, the
  user decides.
- **The window-close/project-switch guard fails open.**
  `useStaleActiveSetGuard`'s `isStaleOrUnapplied()` reads a
  module-scoped value kept current by the one live
  `PendingChangesCloseGuard.vue` instance (mounted once in
  `TheShell.vue`) via `usePendingActiveChangesQuery`; before that query
  first resolves, it reads `false` (not stale), so a close or
  project-switch in that window during the first render is allowed
  without a confirmation prompt.
- A field's role can only be reclassified in the assignment wizard,
  before `create_assignment` ever runs — there is no session-layer use
  case for reclassifying a field on an already-created project.
- `get_assignment_coverage`/`list_assignment_items` take no server-side
  filter or paging beyond `ListItemsFilter` — a coverage result's own
  search/owner filtering happens client-side, since nothing else in
  this codebase filters a `Coverage` result server-side either.
- A missing mod (active per `ModsConfig.xml`, never found on disk) has
  no `report.mods` row at all; it's recovered by `rim-session` (`mod_index::query`
  for `list_mods`, `mod_lists` for the Mods page's lists), not reconstructed in
  Vue — `ModSummaryDto.missing`/`.source: SourceDto | null` (`list_mods`) and a
  `ModListRowDto` with `state.kind === "missing"` (`list_working_mods`) carry it,
  and its `source` is always unknowable (shown only under "any source").
