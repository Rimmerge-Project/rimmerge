# rim-io

Infrastructure adapters for `rim-session`'s ports: `ModsConfig.xml` I/O,
`decisions.json`/`rules.json`/`patches/<id>.json`/`assignments/<id>.json`,
RimSort import, the analyzer-backed scanner, def-source reading, the
merge-mod folder writer, the game-process probe, mod-knowledge (rules
database) loading, and the GitHub rule-database fetcher.

## Layer

Implements `rim-session`'s port traits — it depends on `rim-session` for
those trait definitions, not the other way around, and on `rim-analyzer`
for the scanner it wraps. Everything with real IO in this workspace lives
here or in the two interface apps; `rim-resolve`/`rim-merge`/
`rim-analyzer::extract` stay pure.

## Invariants

- All file writes go through `atomic::write_atomically` (temp file next
  to the target, then rename). The one exception is
  `atomic::write_if_absent`, a `create_new` of the final path for
  `JsonAppSettingsStore::save_if_missing`: a create-if-absent cannot be a
  rename (a rename replaces), and the loser of a race must leave the
  winner's file untouched. A crash between create and write can leave a
  short file, which `app_settings.rs` loads as `Recovered` (network off),
  the safe and repairable state.
- Every store file (`decisions.json`, `rules.json`, patch/assignment
  projects, the rule-database cache) carries a `version` envelope checked
  **before** the payload is deserialized. An unknown version is a hard
  error, never silently ignored; a store's own `MIN_SUPPORTED_VERSION`
  through its current `SCHEMA_VERSION` all load. Loading never writes —
  only `save` writes, and it always writes the current `SCHEMA_VERSION`,
  so an old-version file on disk is only ever rewritten current on its
  project's next explicit save, never proactively on load. **Two
  exceptions, for opposite reasons**: `app_settings.rs`
  (`app-settings.json`) fails closed — a missing file loads as the
  default (safe: network on, but nothing automatic runs before the
  first-run notice is answered), a present-but-broken one loads with
  every network switch *off*, since that file's own default means
  network on and a corrupted privacy preference must never silently
  re-enable it; `notifications.rs` (`notifications.json`) fails open —
  any problem (missing, corrupt, unrecognized version) loads as
  `NotificationState::default`, since it's app-remembered state, not a
  preference, and losing it costs at most re-showing an already-seen
  notice.
- `organization.rs` (`organization.json`, `JsonOrganizationStore`) is the
  third store that does not follow "unknown version is an error": its
  `load` has no error type, because every outcome is a state the session
  models (`OrganizationLoad::{Missing, Loaded, Unusable(issue)}`). It
  treats the file as untrusted: at most `OrganizationLimits::MAX_FILE_BYTES`
  are read, a version above 1 is `NewerVersion` (checked before the rest of
  the shape), every value goes through a domain constructor and the whole
  document through `ModOrganization::from_parts`, and nothing is repaired:
  an unusable file stays on disk untouched until the user starts over, when
  `archive_unusable` copies it to `organization.json.unusable-<seconds>`
  (a `-n` suffix when that name is taken) before the empty save. `save`
  writes groups in id order and members sorted, so one state is one byte
  string. It also persists `next_group_id` (required in the v1 file, bounded
  by `OrganizationLimits::MAX_GROUP_ID`), so a deleted group's id is never
  reused after a reload, and it **refuses to write** a document larger than
  `MAX_FILE_BYTES` (a `StoreError`, so the edit rolls back): the app never
  writes a file its own reader would turn into `TooLarge`.
  `archive_unusable` on a missing file is a no-op success. `load` opens the file only
  after `fs::metadata` says it is a regular file (a link to one still reads): a folder,
  named pipe or device named `organization.json` is `Unreadable`, never opened, since
  `load` runs under the session lock and a copy reads another profile's folder.
- `net/` is this crate's only network dependency; its only two callers
  are `databases/cache.rs` and `release_feed.rs`. `net::http::UreqHttpGet`
  is the only thing anywhere in this workspace that opens a socket —
  nothing under `#[cfg(test)]` ever constructs it (`net/http.rs`'s
  `HttpGet` trait is the seam; the `pub(crate)` test double is
  `fake::FakeHttpGet`). Every request is built from a compiled-in
  `net::allowlist::Endpoint` — host, URL, byte cap, timeouts, and header
  profile are all `const`, never a setting — and passes
  `net::allowlist::is_allowed` (an exact, case-insensitive match against
  the *one* `AllowedHost` variant that endpoint declares, never "any
  allowed host") before `HttpGet::get` is ever called. `databases/cache.rs`
  additionally runs, in order, before any write: a `Content-Length` plus
  `Read::take` size bound, and parse-before-commit (the real
  `rimsort::rules_file`/`rimsort::steam_db` parsers, reused verbatim
  against an empty active-mod set as a pure syntactic check) — then an
  atomic replace. See the root `CLAUDE.md`'s network hard rule for the
  allowed hosts and the hermetic-gate guarantee this crate must never
  break.
- `ureq` uses `rustls-no-provider` + `_ring` + `platform-verifier` (the
  Windows system trust store), not the plain `rustls` umbrella feature —
  the umbrella's bundled root store (`webpki-roots`) carries a license
  outside `deny.toml`'s allow list. `UreqHttpGet::agent()` must configure
  `TlsConfig::root_certs(RootCerts::PlatformVerifier)` explicitly: ureq's
  own default panics at the first real request without the
  `rustls-webpki-roots` feature backing it.
- **`mod_timestamps.rs`**: `FileModTimestampReader` implements
  `rim_session::ports::ModTimestampReader`. Workshop: `timeupdated` (when the installed version was published on the
  Workshop, as Steam records it; not the download time) under `WorkshopItemsInstalled` in `<library>/steamapps/workshop/appworkshop_294100.acf`
  (two parents above the workshop `content/294100` folder), read once per call and
  only when a Workshop mod is asked about. Local: mtime of `About/About.xml`.
  Core/DLC: `ShippedWithGame`, no IO. The `.acf` is untrusted: `read_bounded_with_limit`
  at 4 MiB, UTF-8 only, a streaming tokenizer (`Tokens`, an iterator the parse loop drives,
  so no token vector is ever built) with a depth bound of 16, and a malformed
  file yields no times at all (never a partial guess). A time of zero or past
  `MAX_PLAUSIBLE_SECONDS` (9999-12-31, the same bound on the `About.xml` mtime) is no time:
  the UI's date formatting throws on anything larger. A missing file, an implausible time or
  a time under the wrong section is `Unavailable(Unknown)`. Local only, no network.
- **`saves.rs`**: `list_saves(folder)` lists the `.rws` files directly in
  RimWorld's `Saves` folder for the desktop's "Import from a save" list. A plain
  function, not a port (directory metadata, no decision: the caller picks the
  folder). Directory metadata only, never a save's contents. A regular file whose
  name ends in `.rws` (ASCII case-insensitive) counts; the entry's own metadata
  never follows a link, so a symlink or junction named like a save and a folder
  named `x.rws` are skipped (the `Saves` folder itself is opened as given, so a
  player's moved folder still lists). At most `MAX_SAVE_DIR_ENTRIES` (10,000)
  entries are examined and `MAX_LISTED_SAVES` (200) returned, `omitted` counting
  the qualifying rest (past the examination bound the listing covers the first
  entries the file system hands out). Newest modified time first, ties by file
  name ordinal, a missing or implausible time (`plausible_time.rs`, shared with
  `mod_timestamps.rs`: zero or past 9999-12-31 is no time) `None` and last. A
  single entry that cannot be inspected is skipped; an unlistable folder is an
  `io::Error`.
- Tests only ever touch `tempfile` copies of `tests/fixtures/` — never
  the real install, and never anything "near" it: every path an
  end-to-end test touches, `ModsConfig.xml` included, is a scratch copy
  under a `tempfile::tempdir`. `tests/real_network_databases.rs` and
  `tests/real_network_release_feed.rs` are the two exceptions,
  `#[ignore]`d and excluded from the default gate by design.

## Gates

```sh
cargo fmt --all -- --check
cargo clippy -p rim-io --all-targets --all-features -- -D warnings
cargo nextest run -p rim-io --all-features
```
`tests/real_network_databases.rs` (three tests fetching both live
databases and asserting a real conditional-GET `304`) and
`tests/real_network_release_feed.rs` (a real `GithubReleaseFeed` fetch,
then a conditional-GET `304`) are both `#[ignore]`d — run explicitly
with `--run-ignored ignored-only`. Before reporting done, run the full
gate block in the root `CLAUDE.md`.

## Conventions

- **`game_log.rs`** (a facade over `game_log/{lines,extract,records,pairing}.rs`
  and the entry pipeline below; its tests live in `game_log/game_log_tests.rs`,
  `game_log/entry_tests.rs`, `game_log/coverage_tests.rs` and
  `game_log/detect_tests.rs`): the facade keeps `parse`,
  the streaming core (`parse_stream`, `LogParser`; kind detection lives in `detect.rs`) and
  `FileGameLogReader`; `lines.rs` splits and bounds lines; `extract.rs`
  holds the typed-record grammar (the stack-trace op-line join in
  `OpenBlock` with its `MAX_BLOCK_LINES`/`MAX_JOIN_LINES` bounds, and the
  terse-failure, `file:`, cross-reference, DDS, dependency,
  timer and load-header line parsers); `records.rs` reads the typed
  result lists off the classified entries; `pairing.rs` pairs terse
  failures with stack-trace blocks.
  `parse(&str, &LogFormats) -> ParsedGameLog` is the pure
  `Player.log` parser — no filesystem, no `ModId`, no attribution.
  **Two engines, one pipeline.** RimWorld 1.6 (Unity) and 1.7 (the Waylith
  engine) write different `Player.log` and console-copy framings. `Framing`
  has four variants (`PlayerLog`, `ConsoleCopy`, `WaylithLog`,
  `WaylithConsole`; `kind()`, `dialect()`, `Framing::of(kind, Dialect)`) and
  `detect_framing` decides it by content; **Unity files take exactly the old
  paths** (the Waylith checks need evidence a Unity log never has, below).
  `waylith.rs` holds everything 1.7-specific, `waylith_tests.rs` its tests and
  `tests/fixtures/player_log_waylith_excerpt.log` and `console_waylith_excerpt.txt` hand-written excerpts (every
  name invented). **Confirmed by a real 1.7.5014 log and the decompiled
  writer** (`Waylith.Logging.FileLog`): the header block `=== <product> log ===`
  (the product name is data, never matched) + `Key: value` lines + a blank line;
  one entry per `yyyy-MM-dd HH:mm:ss.fff LEVEL[ [category]] message` line (LEVEL
  is a 5-wide field: `TRACE`/`DEBUG`/` INFO`/` WARN`/`ERROR`/` CRIT`/5 spaces);
  RimWorld's own `Log.*` entries carry `[Verse]` (a mod's `Debug.Log*` carries
  `[UnityDebug]`, from the decompiled stub); a message may span lines (column 0
  or indented); the stack the writer captured follows as `    at <method>
  [0x<IL>]` lines (optionally ` in <file>:<line>`), for every level unless
  `-DisableLogStackTraces`; the clean end is `<ts>  INFO Log closed.` with no
  category. **Decompiled, not yet in a real log:** a crash is a `CRIT [Crash]`
  entry whose message starts `=== CRASH ===` / `Crash report written to:
  <path>`; the pre-start backlog overflow is an uncategorized warning `N
  earlier log line(s) were dropped (pre-init backlog overflow).`.
  **Confirmed by a real 1.7.5014 console copy**
  (a maintainer's `.journal/logs/2026-11-26/console-1.7-nomods.txt`): each entry is its message
  (continuation lines at column 0), then frames in the `Player.log` writer's own
  shape (`    at Ns.Type.Method(args) [0xNN]`, optionally ` in <file>:<line>`, the
  synthesized `.InvokeMessageMethod_...` and `[0xFFFFFFFF]` frames included), then
  one blank line; no timestamp and no level field. In a vanilla run the first frame is the
  logging call, `Verse.Log.Message|Warning|Error(String text)`, and that method is the
  entry's level (`waylith::console_log_severity`, applied to the entry when its
  first frame arrives, or when a head-truncated copy opens on it). That the first
  frame is `Verse.Log.*` is **confirmed for RimWorld's own entries in a vanilla
  run only**: in a modded game the log-call frame can be absent (**observed:
  0 of 1,000 entries in a 1,021-mod copy**, whose traces open at the caller, e.g.
  a mod's `..cctor()` or `RuntimeHelpers.RunClassConstructor`; a patch on
  `Verse.Log` is the presumed cause), in which case segmentation is unaffected
  (any four-space frame starts the trace) and the severity falls back to the
  classifier's (no level is assumed). A stackless
  message prints `No stack trace.` instead (`Verse.LogMessage.StackTrace`'s
  answer for a null stack, confirmed by decompiling 1.7; e.g. under
  `-DisableLogStackTraces`): in `WaylithConsole` it is a trace start (the
  entry's trace) and gives no severity. Its detector is
  the frame **shape**, never the log call: any four-space Waylith frame
  (`is_frame`; `No stack trace.` beside one counts the same), and only
  when **no timestamped entry header is anywhere in the peek window** (a 1.7
  `Player.log` carries the same frames, and a console copy never has a
  timestamp; frame plus header answers `WaylithLog` even with the banner in the
  window), checked after Unity's trace start and the timestamped first
  line and before the banner rule. A 1.6 `Player.log`'s certain first line,
  `Mono path[0] = ` (BOM-tolerant), vetoes this console answer, so a 1.6 log
  holding a four-space frame still falls to the banner rule; without that
  first line, 1.6 immunity rests on Unity's two-space frame indent and the
  trailing space after `:0 ` (pinned in `detect_tests`). A bare `No stack trace.` (no Waylith frame, no
  Unity trace start) stays a Unity console snapshot, the reading that
  under-claims. Segmentation (`Framing::WaylithConsole`, the
  Unity copy's text, trace, blank-line machine): a Waylith frame starts the
  trace (`Rule::ConsoleTraceStart`, then `ConsoleTrace`), a blank line before
  the trace stays message text, a blank line after it ends the entry; the head
  is classified like Unity's (the whole line), and attribution reads the frames
  as in a Waylith `Player.log`. A three-space frame in an exception's text is
  message text. A 1.7 `Player.log` is decided by its **first non-blank
  line**: the header block decides at once, a timestamped entry line decides
  unless a Unity snapshot signal (which a 1.7 log never holds) appears
  anywhere in the peek window. An excerpt that starts mid-entry (on a frame or
  a body line) is found by a last-resort rule: a timestamped entry header
  anywhere in the window, tried only after the Unity snapshot signal, the
  console signal and the banner all failed, so a Unity file reaches it only if
  it holds none of them (the real-log directory tier, with its independent
  dialect oracle, is the evidence it never fires on a real Unity file). A
  leading U+FEFF on the first line is ignored by detection and stripped once
  by the Waylith parse path. Segmentation (`Segmenter::push_waylith`): a timestamped line
  always opens an entry; every other line continues it (a blank line too,
  except the header block's, which separates); a line is a frame only by its
  shape (`WAYLITH_FRAME_PATTERN`: four spaces, `at`, the bracketed IL offset),
  never by indentation, so `    [0] ...` body lines stay body. An exception
  text's own three-space `   at Ns.Type.Method(...)` frames are **Mono-shaped
  frames** (`Rule::MonoFrame`, read by the shared frame rules and attributed
  like a Unity log's), never Waylith frames. The frame shape is tested **before** the block
  contexts, so an unterminated mod block cannot swallow the frames. The
  head the classifier sees is the **message** (after timestamp, level and
  category), so every body-level recognizer, record extractor, timer tap and
  def-cache tap works unchanged; the explicit level replaces the arm's
  severity; a message in an engine category (a denylist: anything but
  `Verse`/`UnityDebug`, so a future engine category is never mod evidence)
  that no arm recognizes, or that only matches a mod convention (`[Tag] ...`
  ModMessage, a duration Timer, a def-cache prefix: `[Vulkan] [General] ...`,
  `GpuProbe: completed in 84 ms`), is `EngineInfo(UnityRuntime)` (engine
  chatter), and the
  writer's own markers are `SessionMarker`s read by `coverage.rs`
  (`Log closed.` = clean exit, `=== CRASH ===` = crash with its report path,
  the backlog warning = a gap that opens and closes on its own line).
  Frames attribute through the same outer-type rules: `ENGINE_NAMESPACE_ROOTS`
  gained `Waylith`, `Brutal` and `RimWorldWaylith` (and the analyzer's own list
  the lowercase three), then `Microsoft` and `UnityStubs` (`unitystubs` in the
  analyzer; the analyzer's list lacks `microsoft` on purpose: a mod that ships
  `Microsoft.*.dll` really owns that namespace for ownership, while a frame
  of that root is never worth blaming here, so rim-io's list is a superset,
  enforced by `every_analyzer_engine_root_is_an_engine_root_here_too`), and a
  type with an empty first segment (the engine's synthesized
  `.InvokeMessageMethod_...` frames) is the engine's. **Harmony's replacement
  frames on 1.7** print as `MonoMod.Utils.DynamicMethodDefinition.Ns.Type.Method_PatchN`
  (confirmed in a real log, in the three-space exception trace and in the four-space
  log-call frames alike); only the `.InvokeMessageMethod_...` shape has an empty
  first segment. Both are engine, the first through the `MonoMod` root, so a mod
  frame after them is the one blamed (pinned by a test), like Unity's
  `(wrapper dynamic-method)` frames.
  **Engine exception wrappers with no leading exception type** are
  `RuntimeException` arms and key wrappers (`patterns.rs`):
  `Exception getting <type, possibly empty> at <path>: <ex>`
  (`GraphicDatabase.GetInner`, the path never enters the key) and `Could not
  execute post-long-event action. Exception: <ex>` (`LongEventHandler`, also in
  1.6 logs), both confirmed in the decompiled 1.7 `Assembly-CSharp`;
  `Exception getting types in assembly` stays a `TypeLoadError`: the graphic
  wrapper's regex (`Exception getting \S* at `) cannot match it, the
  arm order does not decide it. A head where a mod imitates the graphic
  wording (`... at <path>. Exception="System.X: ...`) keys on the last
  exception type between ` at ` and the first `: `, but only when that text
  holds `Exception="` or `. Exception` (otherwise a path word such as
  `Things/Example/Exception` would become the type and the type stays empty).
  **Pawn-job wrappers** (`<pawn> threw exception in WorkGiver <def>: <ex>`,
  `... in GiverTryGiveJobTargeted on WorkGiver <def>: <ex>`, `... while
  determining job (main|constant)`, `Pawn <pawn> threw exception while
  executing a global finish action (<n>), ...: <ex>` and `... while executing
  toil <toil>'s finish action (<n>), ...: <ex>`) are `RuntimeException` too,
  all confirmed by decompiling 1.7 (`JobGiver_Work`, `Pawn_JobTracker`,
  `JobDriver`, `Toil`; the 1.6 `Player.log` wording is the same). Their key
  starts at `threw exception ...` (the WorkGiver def included), so the pawn's
  name never enters it: one family per WorkGiver, exception type and key
  frame. The pawn's name is a prefix with no `:` (`[^\[\s:][^:]*? `): the game's
  name validation (`CharacterCardUtility.ValidNameRegex`, decompiled 1.7:
  letters, digits, space, `'`, `-`, `.` and the two katakana middle dots) allows no colon, and the bar keeps the
  prefix from reaching past another wrapper (`Exception ticking X: ...: Rat threw
  exception ...`) or a mod's `Name: ` prefix. In this branch the type is only one
  that follows a `: ` and is itself followed by `: ` or the end of the head, so a
  driver, job or def name in the job text (`jobDriver=...`, `[workGiverDef: ...]`)
  that ends in `Exception` is never the type. The `determining job` wrappers carry
  their exception on a later line
  (`TryStartErrorRecoverJob`), so its type is not in the key. On a Unity log that
  line is its own `RuntimeException` entry (a column-0 line starts one), carrying
  the frames and the blame, so the head's family is never attributed; on Waylith
  the exception and its frames continue the head's entry and the key frame is
  present. The Harmony
  annotation lines inside such an entry (`    - PREFIX ...TryIssueJobPackage...`)
  are continuation lines of the entry; they used to leak against the
  `runtime-exception` sentinel only because the entry was `Unclassified`.
  The two Waylith decoder failures (`Unable to load PSD <path> due to an
  exception in PsdSharp: ...`, `<path>: the file's contents are not the format its
  extension names; ...`, `[Vulkan]` at ERROR) are `TextureLoadFailure`, so they
  survive the engine-category chatter override, and carry the file as a
  `AttributionInput::Path` (read from the color-stripped head). The other PSD wording
  (`Unable to load PSD file <name> ...`, a bare file name) captures `file <name>`
  and resolves to no mod. Both wordings are confirmed by decompiling `Waylith.dll`
  (`GpuTextureService.DecodeTextureFile` / `TryLoadTextureWithPsdSharp`), whose
  log calls hold exactly these literal pieces around the formatted values.
  `FileGameLogReader` (implements `rim_session::ports::GameLogReader`)
  streams the file through the same single-pass core as `parse` (a
  `LogParser` state machine fed by `game_log/lines.rs`'s `BoundedLines`,
  one reused buffer, one line at a time) — **no file-size or line-count
  refusal, and no UTF-8 refusal**. Per-unit bounds truncate and are
  counted in `ParsedGameLog::read_stats` (`LogReadStats`): a line keeps
  its first 64 KiB (`MAX_LINE_BYTES`), a stack-trace block examines at
  most `MAX_BLOCK_LINES` lines, a multi-line join is abandoned past
  `MAX_LINE_BYTES` bytes or `MAX_JOIN_LINES` lines, and invalid UTF-8 is
  decoded lossily per line. Working memory does not grow with the file;
  only the per-occurrence result lists do. Without a forced kind it peeks the
  file, rewinds, then streams; **a forced kind never seeks** (`parse_forced`:
  the engine is learned from a bounded prefix read without seeking (whole
  `fill_buf` chunks until `PEEK_BYTES`, `PEEK_LINES` newlines or EOF: the very
  window detection reads, since a 1.7 console copy is recognised by a frame on
  line 2 or later), then
  parsed as `Cursor::new(prefix).chain(reader)`, so a pipe or device path
  that read fine on 1.6 with `--kind` still reads). It **no longer refuses** an
  in-game console snapshot: `read(path, formats, KindChoice)` decides the
  kind by content (`detect.rs`'s `detect_kind`; the full order across both
  engines is the numbered list in `detect.rs`'s module docs, and the 1.6
  rules within it are: within a peek window of 200
  lines and 1 MiB, any console-copy trace start, the exact line
  `UnityEngine.StackTraceUtility:ExtractStackTrace ()`, means `ConsoleSnapshot`
  wherever the banner is; a bare `No stack trace.` (both engines' copies hold
  it) means a Unity `ConsoleSnapshot` unless a four-space Waylith frame is in the
  window too;
  otherwise the exact `patterns::BANNER_PATTERN` banner, the same pattern
  the pass counter uses, means `PlayerLog`; neither means
  `ConsoleSnapshot`, the reading that only under-claims. The banner alone
  cannot decide, because the game logs it into the console queue: an
  uncleared console copy holds it near its top, and first of all after a
  launch without options. The peeked lines are then rewound, so a path that cannot
  rewind needs `--kind`), or takes the caller's
  `KindChoice::Force`; a file name is never consulted. The kind picks the `Framing` and the result's `coverage`
  (`coverage.rs`: `CoverageCollector` gathers the banner lines, the
  logging-gap stop/resume lines, the exit footer (Unity's memory footer or
  1.7's `Log closed.`) and the crash marker (Unity's crash handler or 1.7's
  `=== CRASH ===` entry)
  while entries open, then `finish` builds `LogCoverage` from those and the
  class tallies; a patch failure class is the patch-phase evidence, there
  is no config-check claim; a snapshot's `head_truncated` is observed from
  the file's first head, a trace start meaning the copy starts mid-entry;
  a crash-report location cut to its bound counts into
  `LogReadStats::crash_report_paths_truncated`). `parse_as(text, formats, Framing)` is the
  same with the framing given. **`ParsedGameLog` and its raw record types
  live in `rim_session::ports`** (defined in `rim-session`'s
  `ports/game_log.rs`), **not here**: this crate depends on
  `rim-session`, so a type the `GameLogReader` trait's signature names
  cannot live on this side without a circular dependency — `parse`
  returns the port's own type directly, with no field-by-field
  conversion anywhere. Stack-trace blocks pair with their terse
  `Patch operation ... failed` line **by key, within a startup pass**
  (`pairing.rs`), never by position: a patch-reporting mod prints its
  blocks *before* the engine's terse lines, and positional pairing shifts
  every later block onto the wrong failure when one block is missing. A
  failure first takes the earliest unclaimed block with the same pass, mod
  tag and file path (lowercased, `\` as `/`) — exact (pass, tag, path);
  what is left then falls back to the same pass and mod tag, and the tag
  alone decides only when a path is missing (on the failure's or the
  block's side): two present, different paths never pair. The mod tag is
  never relaxed. An unclaimed
  block is an `extra_stack_traces` entry, an unclaimed failure has no
  stack trace. A multi-line quoted xpath split across
  two physical lines is joined and whitespace-collapsed
  (`rim_resolve::domain::normalize_log_text`) before giving up on a
  parse.
  **Startup passes:** a real log has two `RimWorld <version>` banners (a
  pre-patching loader restarts the game in-process) and repeats its
  startup warnings in each. `dependency_warnings` keeps each (mod name,
  dependency id) from the last pass that logged it, in file order (so a
  last pass cut off partway keeps what only pass 1 logged, and identical
  passes give pass 2's list); `RawTimer` keeps every pass, tagged with
  its real, unfolded `pass` (no timer repeats across passes, and the
  first pass's own are real cost; a family's `count_by_pass` folds
  passes above 16 into 16, a timer's `pass` never does); every
  other typed list is unfiltered (no real log has a pass-1 occurrence),
  and each family keeps `count_by_pass`. Attribution (which active mod a raw path/tag belongs to) is
  `rim-session`'s `use_cases::ImportGameLog` job, not this crate's — this
  parser never sees a `Report`.
  - **The entry pipeline (`segment`/`entry`/`classify`/`aggregate`/
    `sentinels`/`pipeline`/`records`, fed by the same `LogParser` pass).**
    The typed lists come from it: `records.rs` turns the entries of the
    classes the port has a record for (patch failure with its `file:`
    line, stack-trace block with its body and trailer line,
    load event with its items, and the single-line cross-reference,
    texture-fallback and dependency-warning heads) into raw records.
    Only timers and def-cache lines stay a per-line tap in `LogParser`,
    because a timer can be an indented continuation of another message.
    `segment.rs` splits lines into entries (a head plus
    continuation lines) and blank separators, in either `Framing`
    (`PlayerLog`: continuation by line shape and per-head block contexts;
    `ConsoleCopy`: text, then trace, then a blank line, so a blank line
    inside a message text stays in the entry). `classify.rs` puts each
    entry in exactly one `EntryClass` from its head (one `RegexSet`, first
    match in `ARMS` order wins; `Unclassified` is a class, not a drop) and
    builds its family key; `aggregate.rs` groups entries into families
    under the bounds (5,000 families per class, 50,000 overall, 16 MiB of
    samples, 16 tracked passes; what does not fit is counted in the class
    overflow or a `ReadLoss` counter, never dropped); `sentinels.rs` flags
    a line that matches a known class's loose fragment yet sits in another
    class's entry. Two invariants are tested on every real log:
    `lines_read == entry lines + blank separators` (P1) and zero sentinel
    leaks (P2); a continuation line that is itself a head of its own
    entry's class (a second cross-reference indented under a first) is a
    leak too. **A stack-frame line is exempt from the sentinels** (a
    parameter named `loadID` is not a message); every other exception is a
    row of `sentinels.rs`'s `ALLOW_LIST`, scoped by sentinel, class, the
    line's segmentation rule and shape, with its reason in a comment.
  - **A family carries the evidence to attribute it** (`attribution.rs`,
    `Family::attribution_input: Option<AttributionInput>`), built once, when
    the family is first seen (the aggregator takes a closure, so a storm of
    one message pays it once). One arm per class: a patch failure or
    stack-trace block its tag and its `file:`/trailer path (from the open
    record in `records.rs`), a texture fallback the head template's `path`,
    a `[Tag]` line or timer the balanced `[Tag]`, a metadata warning the mod
    named by `MOD_METADATA_NAME_PATTERN`, an exception the one type its
    family key already names (the innermost non-engine frame, back-references
    resolved: `AttributionInput::TypeName`, so attribution is a function of
    the key), a type-load error the type (`Error in static constructor of
    <Type>`) or assembly (`Exception loading <x>.dll`, `... getting types in
    assembly <x>`) its head names, else none. **Frame types are outer types**
    (`entry.rs`): a Mono frame's generic argument lists are dropped
    (`BFS`1[T].Run` is `BFS`1`, the arity kept), `Type..ctor`/`..cctor` names
    `Type`, and a type is cut at its first `+` or `/` (`Outer+<>c` is
    `Outer`), for the family key and attribution alike, Unity frames
    included. The engine skip compares a type's first `.`-segment exactly
    against `patterns::ENGINE_NAMESPACE_ROOTS` (so `RimWorldColumns` is a
    mod's), a superset of the analyzer's ownership-excluded roots, pinned by a
    test. **Memory bound:** a family holds at most one input (a type or
    assembly is at most 128 bytes, a name 256, a path 1 KiB), so the 50,000
    family bound keeps evidence to the same order as the samples. The classes with no arm (cross-references, the engine's own
    classes) have `None`; the match over `EntryClass` is exhaustive. This
    crate never resolves the evidence to a mod: `rim-session` does.
  - **Every engine pattern string is a `const` in `patterns.rs`**, used by
    both the classifier's set and any individual capturing regex, so no
    pattern literal is written twice. Add a class by adding an `ARMS` entry
    in the position its first-match order needs, a `key_style` arm, and a
    sentinel.
  - **The three mod-produced formats are data, not code** (a patch-reporting
    mod's stack-trace block, a texture loader's fallback lines, a patching
    library's back-reference stubs): they are the `log_shapes` section of
    the rules data, arrive as `rim_session::ports::LogShapes` inside the
    `LogFormats` passed to every `read`/`parse` (a parameter, exactly like
    the def-cache carriers, never adapter state, since the data is
    per-project and the reader is built once), and `shapes.rs` compiles
    them once per parse into `CompiledShapes`. A `LogTemplate` is literal
    text plus typed placeholders (`{name:text|token|int|hex}`), never a
    regex: literals go through `regex::escape`, each placeholder maps to a
    fixed bounded sub-pattern, every template is compiled under a 1 MiB
    size limit and a nesting limit, and `regex` never backtracks, so a
    hostile fetched file cannot inject syntax or cause ReDoS. A template
    matches the whole trimmed line; where a role has several rows, any row
    matches. The stack-block start and texture-fallback head are special
    `ARMS` in `classify.rs` (checked at their position in the order, with
    the def-cache and timer arms); the shapes' loose sentinels are
    derived from the loaded head templates, so with no shapes there is no
    such sentinel. **An empty `LogShapes` must stay valid**: a log still
    conserves every line (P1) and reports no leak (P2); those formats'
    lines land in the generic classes and no typed record is built from
    them. `mod_knowledge.rs` checks every row at load (grammar and bounds via
    `rim_session::ports::LogTemplate::parse`, required captures, compile
    within the limits via `shapes::check_*`) and drops a bad row with one
    `UnknownModKnowledgeValue`. The generic op-line grammar
    (`STACK_OP_LINE_RE`) and the multi-line join stay in code; only the
    xpath prefix and the branch markers of a stack-block row are data.
  - **`parse_as(content, formats, Framing)`** reads either kind from text;
    the real-log directory tier goes through `FileGameLogReader` (detection
    by content, asserted against an oracle independent of the product's
    rule: a copy trace-start line means snapshot, a `Mono path[0] = ` first
    line means Player.log, a `=== <product> log ===` first line means a 1.7
    Player.log whose entry count must equal its header block plus its
    timestamped lines; the detected dialect equals an independent oracle's
    (`oracle_dialect`: a 1.7 header block or timestamped first line, or a 1.7
    console copy, a four-space `at ... [0x` frame (any method) with no timestamped line,
    header block or Unity trace start, and a first line that is not 1.6's
    `Mono path[0] = `; else Unity; a 1.7 console copy's entry
    count must equal its trace starts, counted independently: a four-space
    `at ... [0x` line whose previous line is not one, plus `No stack trace.`
    lines); and equal to
    `parse_as` under the detected framing) and
    uses `parse_as` for the foreign framing, with the formats of the
    embedded bundle (`vendored_knowledge()`), the way the product gets them
    without a fetched cache.
  - The pattern for an engine string must be the exact wording in the
    decompiled engine; a wording seen in a real log but not confirmed there
    stays `Unclassified`.
- **`databases.rs` + `databases/{cache,manifest}.rs`** (the transport seam
  itself lives in `net/`, shared with `release_feed.rs`):
  `GithubRuleDatabaseFetcher` implements
  `rim_session::ports::RuleDatabaseFetcher`, fetching each source's JSON
  from its hardcoded `raw.githubusercontent.com` URL into an app-global
  cache (`rim_io::databases_dir`, a sibling of `profiles/`, never inside
  one). `manifest.rs`'s `Manifest` is the versioned `manifest.json`
  envelope; every timestamp round-trips through a plain `String` (jiff
  has no `serde` feature enabled here). `RuleDatabaseFetcher::status`
  takes a caller-supplied `enabled: &BTreeMap<RuleDatabase, bool>` (a
  database missing from the map reads as disabled) — whether a source is
  fetched is a `rim-session` `NetworkPolicy` fact this adapter has no
  other way to see.
- **`def_source.rs` + `def_source/file_cache.rs`**: `FileDefSourceReader`
  parses each file once into an outline of its root element's element
  children (every locator `rim-session` reads names one of them: defs
  under `<Defs>`, and patch operations at their top-level `<Operation>`
  via `top_level_operations`; a deeper or empty path, which the analyzer
  does emit for nested operations, re-parses the text) and keeps the
  parses in a byte-bounded LRU (`CACHE_BUDGET_BYTES`, 16 MiB). Whether a
  cached parse is still good is the file's length and mtime (`stamp_of`,
  the one `fs::metadata` call); a change drops the entry. **When an edit on
  disk is noticed depends on how the reader is read.** A read on the
  reader itself checks the file every time. A read through a call view
  (`DefSourceReader::call_view`, `FileCallView`, which `rim-session` opens
  once per `VerifyOrder`, `RenderMergeMod` (one for its own planner, one on
  the session's reader for the clean-merge prefetch) and `InspectDef`
  call) checks each file the first time that view reads it and trusts that
  stamp until the view is dropped, so an edit made during a call is seen
  by the next call and an edit made between calls always is. A view's
  stamps are its own (never shared between views or readers); a file
  whose parse was evicted or replaced mid-call (a newer-stamp parse from another reader or view counts) is checked again, and a failed check is
  not remembered. The tests count checks through the `#[cfg(test)]`
  `stamp_checks` counter, never by timing. The cache lock is held for map
  operations only, never across a read or a parse: the replay pool reads
  through one reader (or one view) from four threads.
- **`ModsConfigFileStore::read` is bounded and typed.** It reads through
  `read_bounded_with_limit` at 4 MiB (`MAX_MODS_CONFIG_BYTES`; a real file is a few
  hundred kilobytes), because the desktop re-reads it every few seconds, and every
  failure is a `ConfigError` with a `ConfigErrorKind`: `Missing` (not found),
  `Unreadable` (any other I/O failure, including a failed write or backup),
  `Malformed` (not parseable as a `ModsConfigData` document, truncated or partly
  written files and an over-limit file included). Replacement is atomic only for
  Rimmerge's own writers (`write_atomically`), so a read that races one of those is
  one whole list or a failure, never a blend. Another program may write the file
  non-atomically: a read that lands mid-write sees a truncated document and reads as
  `Malformed`, which the desktop's baseline refresh absorbs as a stale status (the last
  good list stays the baseline) until the next good read.
- **`ModsConfigFileStore`** backs up first
  (`ModsConfig.xml.bak-<timestamp>`), refuses to proceed on any read
  error other than NotFound, and preserves `version`, `knownExpansions`,
  and the file's line endings. **The XML declaration is never
  hardcoded**: `declaration_of` reads the existing file's own first line
  verbatim (BOM-stripped, trailing whitespace trimmed) when it looks like
  a declaration, and falls back to `DEFAULT_DECLARATION` only when
  there's no existing file or no declaration to read — RimWorld/RimSort
  themselves write the bare `<?xml version="1.0" ?>` (no `encoding`), and
  a hardcoded full declaration would rewrite line 1 of every real
  `ModsConfig.xml` even when the active list didn't change.
- **The merge mod** is rendered into `<mods_dir>/.rimmerge_tmp_<pid>/`
  (any stale leftover from an earlier write that died before its own
  cleanup is cleared first, or `render_into`'s `create_dir_all` would
  happily write into it) with `About.xml` written last; the previous
  generation is copied to `<profile>/merge-mod.prev/`, then the folder is
  renamed into place. `MergeModFolderWriter::read_marker` returns
  `Ok(None)` for both a missing folder and a present-but-markerless one,
  never an error — `ExportPatch` pairs it with `exists` to tell the two
  apart.
- **`JsonPatchProjectStore`/`JsonAssignmentProjectStore`** (patches.rs /
  assignment_store.rs) mirror each other: one versioned envelope per
  project at `<profile>/{patches,assignments}/<id>.json`, `load_all`
  sorted by id for determinism, an absent directory is an empty list, a
  single unreadable/corrupt/unsupported-version file fails the **whole**
  call naming it (never silently skipped), and a file whose stem doesn't
  match the id its payload names is also rejected naming the file — "one
  file per id" is never trusted from the file name alone. `delete` also
  removes that id's own `<id>.prev/`/`<id>.installed.prev/` backup
  folders. Reconstituting a loaded patch project goes through
  `rim_resolve::domain::PatchProject::from_stored`, never a per-decision
  `decide` loop — a decision on file can legitimately name a key the
  project's *current* scope no longer admits (a scope shrink orphans it
  rather than deleting it), and `decide`'s scope check would reject the
  whole file over that one decision. `AssignmentSchema`/`AssignmentRow`/
  `TargetRef` derive `Serialize`/`Deserialize` directly; `rows` is a
  `Vec<{target, row}>`, never a JSON map, since `serde_json` only accepts
  a string as an object key and `TargetRef`/`RowKey` are structs.
  `AssignmentSchema`'s store `SCHEMA_VERSION` is **2**: a project is
  `sections: Vec<{defType, schema, rows: Vec<{key: RowKey, row}>}>`; a v1
  file (the single `schema`/`rows`/`standaloneRows` shape) still loads
  and migrates into one section keyed by the old schema's own def type,
  but `save` always writes v2 — a migrated v1 file is only rewritten on
  its project's next explicit save.
- **RimSort import**: `loadTop`/`loadBottom` without `value` mean false;
  `userRules.json`/`communityRules.json` match by `ModId::base()` alone;
  `steamDB.json` matches an entry to an active mod by workshop id first
  (via `Mod.workshop_id`) and falls back to `packageId` only for a mod
  with none — two uploads sharing one `packageId` are otherwise
  indistinguishable, and matching by `packageId` first would merge a
  second upload's dependencies onto the wrong installed copy.
- **`rules.json`'s `SCHEMA_VERSION` is 3** (`TagSignal`'s adjacently
  tagged serde representation); `MIN_SUPPORTED_VERSION` is 1.
  `JsonRuleStore::load` accepts every version in that range and never
  writes on its own — only `save` writes, and always the current
  version; a `"clusters"` array from a pre-migration file (the
  clustering feature no longer exists in the sorter) is read generically
  and surfaced as `RulesLoadWarning::DroppedClusterRules`, then dropped
  for good on the next save.
- **`app_config.rs` + `project_paths.rs`**: `AppConfig { game_dir,
  workshop_dir, mods_config }` at `<base>/config.json` is **not**
  `Settings` and cannot be — `Settings` lives in the profile's
  `rules.json`, and the profile directory is *derived from* the
  `ModsConfig.xml` path, so the setting that says where `ModsConfig.xml`
  is can't live in a file that answer locates. A missing, unreadable,
  unparseable, or unknown-`schema` config loads as `AppConfig::default()`,
  never an error — it's rung 3 of a resolution ladder and must degrade to
  "detect instead", not to a startup failure.
  `resolve_project_paths(PathOverrides) -> Result<ResolvedPaths, ...>` is
  the **one** implementation of that ladder: explicit flag >
  `RIMMERGE_GAME_DIR`/`_WORKSHOP_DIR`/`_MODS_CONFIG` > `config.json` >
  detection > a typed error listing every candidate — never a silent
  fallback path. A rung-1-through-3 answer that fails `is_game_dir` is
  disclosed as a non-fatal warning (`ResolvedPaths.warning`), never a
  refusal — an unmounted external drive should fail the scan about *that*
  directory, not before anything is attempted; rung 4 (detection) can
  never trigger it, since it only ever returns directories that already
  passed the predicate. The three `RIMMERGE_*` env-var name constants and
  `path_var` (reads `var_os`, never `var` — a path need not be UTF-8, and
  `var`'s `NotUnicode` would fall silently through to the next rung) live
  in `rim_analyzer::infra::paths` and are re-exported here.
  `not_an_install_warning` is the **one** shared sentence
  (`project_paths.rs`) that `config set`, the CLI's own resolution
  output, and the desktop Setup page all render for the same
  fails-`is_game_dir` case — never restate this text at a call site. A
  private `Environment` trait (`project_paths.rs`) is the seam behind
  `path_var`/`detect_game_dir`/`default_game_dir_candidates`, so the
  ladder's own tests run against a hand-written fake instead of the real
  process environment — two tests can't set the same env var
  concurrently without racing, and this seam is what keeps them from
  needing to; an env var set to the empty (or all-whitespace) string
  falls through to the next rung rather than resolving the install to
  the empty path. `AppConfigFile::schema` is `#[serde(default =
  "default_schema")]` — a hand-written `{"game_dir": "..."}` with no
  `schema` key must still load rather than silently coming back empty
  and sending the app off to detection instead; the file is always
  *written* with the current schema, so a hand-written file gains the
  key on its next save. `app_config::pinned_path` absolutizes (relative
  to absolute) and canonicalizes (resolving `.`/`..` and the
  `\\?\`-prefix) any of `game_dir`/`workshop_dir`/`mods_config`
  **before** it's written into `config.json` (both `apps/cli`'s
  `config set` and the desktop's own pin command go through it) — never
  store a path a later lexical comparison could be fooled by. `resolve_user_dir` strips the
  `\\?\` prefix (`\\?\UNC\server\share` becomes `\\server\share`, never the relative
  `UNC\server\share`) and keeps the caller's drive-letter spelling when a mapped drive
  canonicalizes to its share (`settled_dir`), so an export folder or a pinned path stays `Z:\...`.
  `PathOverrides::policy` (`PathPolicy`, default `TrustLocalUser`) is the one way a webview-driven
  caller restricts rungs 2 and 3: under `DriveLettersOnly` a game folder, workshop folder or
  `ModsConfig.xml` from an environment variable or `config.json` that `UserPath::parse` refuses is
  dropped **before** `is_game_dir` stats it or `profile_dir` canonicalizes it, and the first one
  dropped comes back in `PartialPaths::refused` so the caller can say so. The CLI keeps the default.
- **`mod_knowledge.rs` + `build.rs`**: `FsModKnowledgeStore`
  implements `rim_session::ports::ModKnowledgeStore` as "the fetched
  `<cache_dir>/rimmergeRules.json` if present and it parses, else the
  bundled snapshot compiled into the binary", with **per-section**
  fallback — a cache file supplying only `precedence` still keeps the
  bundled `patch_operations`/`def_cache_carriers`/`log_shapes`. `FsModKnowledgeStore::vendored()`
  skips the cache entirely, for a caller with no cache directory and for
  tests. The bundled snapshot is **one file, not one per section**: `build.rs`
  fails the build with a clear message
  (`rules/rimmerge-rules.json not found. Run: git submodule update
  --init`) when the `rimmerge-rules` git submodule at `rules/` (workspace
  root, sibling of `crates/`) hasn't been checked out, and
  `mod_knowledge.rs`'s own `EMBEDDED_RULES_BUNDLE` `include_str!`s
  `rules/rimmerge-rules.json` — the already-built, already-concatenated
  envelope the `rules` repo's own CI produces from its `data/*.json`
  — straight into the binary. There is no separate "ships empty" vs.
  "ships populated" split on this side any more (that split lives in the
  `rules` repo's own seed content instead). **The two layers fall back
  to different things, and that difference matters**: a section missing
  or malformed in a *fetched* file falls back to the *embedded
  snapshot's* own value for that section (per-section, independently —
  a cache file supplying only `precedence` still keeps the embedded
  `patch_operations`/`def_cache_carriers`/`log_shapes`); a section missing or
  malformed in the *embedded bundle itself* falls back to **empty**
  instead, since there's nothing further underneath it. Never conflate
  the two — the embedded bundle is the floor everything else falls back
  to, not itself falling back to anything but empty. There's never a
  state with zero knowledge merely because no cache file exists on this
  machine, only the embedded snapshot's own value standing in for a
  fresher fetched one that isn't there.
  - **Forward compatibility is a hard rule**: an unknown `behaviour`,
    match mode, rule discriminant, or conditional type is ignored with a
    `RulesLoadWarning::UnknownModKnowledgeValue` (for `log_shapes`, also an
    unknown role, an unknown placeholder type, an oversized template, a
    missing required capture or a row over the per-role limit, one warning
    each), and the rest of the
    file still loads — a newer data file must never break an older
    binary. The envelope deserializes **section by section**, so one
    malformed section falls back alone. `deny_unknown_fields` is
    deliberately never used, for the same reason; unknown top-level keys
    are captured via `#[serde(flatten)]` and reported the same way. The
    *embedded* bundle is held to a stricter bar: `mod_knowledge.rs`'s own
    `the_embedded_rules_bundle_parses_with_zero_warnings_and_every_section_present`
    contract test requires **zero** warnings and every section key
    present, since a warning there means this binary ships data it
    doesn't itself implement. That test — and `tests/real_game_log.rs`
    — are the only places in this crate allowed to read the real bundle;
    every other test (`mod_knowledge.rs`'s own behaviour tests,
    `def_cache.rs`, `game_log.rs`) uses synthetic in-test data instead,
    so this crate's own test suite never depends on what the `rules`
    repo happens to ship (see that repo's own `CONTRIBUTING.md` for the
    evidence bar a new row there needs).
  - `ModKnowledgeStore::load` takes `source_enabled: bool` — the toggle
    gates *consumption*, not merely the refresh, so turning it off with a
    cache file already on disk falls back to the bundled snapshot
    instead of silently keeping the last fetch forever.
  - `tag_rules` is deserialized at **load** time (both from the embedded
    bundle and from a fetched cache file — so a malformed section is
    reported in either case) and then **discarded**, never folded into
    the result — the sorter must never see it (see `rim-session`'s own
    entry for why). This is a load-time check only: `cache.rs`'s
    `parses()` commit gate (run once, before a fetched file ever reaches
    disk) validates the envelope shape alone, not each section's own
    contents — a section that fails to parse still commits, and degrades
    per-section at the next load instead (see this crate's own
    `mod_knowledge.rs::parses` doc comment).
- **`def_cache.rs`**: `FsDefCacheCarrierProbe` implements
  `rim_session::ports::DefCacheCarrierProbe`. The walk is code and is
  name-free (`<plugin_dir>/**/<file_prefix>*<file_extension>`,
  case-insensitive, `recursive` honoured); *which* folder/prefix/extension
  to look for is data, loaded the same way as `mod_knowledge.rs`'s other
  sections. `game_log.rs`'s `LogParser::is_def_cache_line` (over the carrier slice) is
  the matching split for the `Player.log` prefix — an empty carrier slice
  collects nothing, the honest answer when no carrier is known. Both
  adapters take the carrier list as a constructor **parameter**, not
  adapter state: they're built once at the composition root, before any
  project exists, while the carrier list is per-project data.
- Profile dir = `<base>/profiles/<12 hex of sha256(normalized ModsConfig
  path)>`; normalisation strips `\\?\`, lowercases, and uses forward
  slashes.
- **`asset_locator.rs`**: `FileAssetLocator::locate_texture` trusts
  `mod_loaded_folders` to already be in load-priority order (highest
  first — `Mod::loaded_folders`'s own contract) and never re-derives or
  re-sorts it. Two passes: every folder's own `Textures/` searched for a
  `.dds` match first (a `.dds` anywhere shadows a non-`.dds` file with
  the same key, whatever folder either sits in — ground-truthed against
  `ModContentLoader<T>.LoadAllForMod`'s own `ddsFiles` set, consulted
  before any other candidate for the whole mod, not per folder); only
  once that finds nothing does the second pass look for the first
  non-`.dds` match in priority order. Both passes walk each folder's own
  files via `rim_analyzer::infra::engine_enumeration_order` (the same
  breadth-first, NTFS-collation-ordered walk `rim-analyzer`'s own
  `Defs/`/`Patches/` scan uses), not a plain sorted walk — ties between
  two non-`.dds` extensions in one folder resolve the identical way the
  real engine's own `GetAllFilesForMod` would.
  `locate_non_dds_texture` is that second pass alone (a `.dds` does not
  shadow): how a caller finds the image copy beside a `.dds`. Same exact
  normalized-key comparison, so a key can never be joined into a path.
- **`game_launch.rs`**: `SystemGameLauncher` implements
  `rim_session::ports::GameLauncher`. `route` is `Steam` when
  `rim_analyzer::infra::paths::is_steam_managed_install` says so (the rule
  lives there, not here), else `Executable` when `is_game_dir` holds and
  the layout's executable (`rim_analyzer::infra::paths::detect_game_layout`: `RimWorldWin64.exe`, or
  `RimWorld.exe` on a Waylith install) has a `symlink_metadata` that is a regular file (a symlink or a
  folder of that name is refused), else `ExecutableMissing`. `launch(Steam)`
  hands the constant `steam://run/294100` to `open::that_detached`
  (`open` is a direct dependency, `shellexecute-on-windows`, already in the
  tree through `tauri-plugin-opener`); the URL is never built from input.
  `launch(Executable)` spawns the install's own exe through the private
  `executable_command`: no arguments, no shell, the install folder as the
  working directory, null stdio, `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP`
  on Windows (safe `CommandExt`), and the `Child` is dropped, never waited
  on. It never writes. The Steam check is a `fn(&Path) -> bool` field
  (`new()` uses the real one) so route tests never read this machine's
  Steam; **no test calls `launch`**: the `open::that_detached` and `.spawn()`
  lines are covered only by the manual check.
- **`process.rs`**: `SysinfoGameProcessProbe` is true when a process named
  `rimworldwin64.exe` (Unity build) or `rimworld.exe` (Waylith, 1.7) runs,
  case-insensitive, by name and not path, so it does not need to know which
  layout the configured install has and a copy started from elsewhere also
  counts (refusing is the safe side). `RIMWORLD_PROCESS_NAMES` is the one
  list; a test pins that every `GameLayout` executable name is in it. Tests
  never read the real process list beyond the does-not-panic smoke test.
- **`mod_list_file.rs` + `mod_list_file/{read,write,stream,sniff,save_header}.rs`**: `RmlFileStore`
  implements `rim_session::ports::ModListFileStore`. The input is
  untrusted and every bound is a `rim_session::mod_list::ModListLimits`
  constant, never a local number. **One bounded, incremental reader serves
  every source**: `stream::read_mod_list(impl Read)` (public, used by
  `RmlFileStore::read` and the CLI's stdin; the old `read_bounded` and
  `parse_mod_list_bytes` are gone) reads 64 KiB chunks into one buffer that
  never exceeds `MAX_INPUT_BYTES + 1`, and `sniff.rs` decides from the first
  bytes (BOM, declaration, comments, the root, the root's first child) what
  it is reading. Every read fills a whole 64 KiB chunk and the shape must be
  settled within the first chunk (past it an undecided input is a list, a
  save root with no first child is `SaveHeaderMissing`), so the re-sniff
  from the buffer start is at most two passes however small the source's
  reads are. The save root is `savegame` (ASCII case-insensitive, like the
  other roots) and its first child must be exactly `meta` (the game compares
  it exactly); the sniff returns the root's byte offset and
  `save_header.rs` starts from it, so a comment naming the root before it
  cannot confuse the parse. A header whose `</meta>` ends past
  `MAX_INPUT_BYTES` is `SaveHeaderTooLarge`; input ending after a bare
  save root is `SaveHeaderTruncated`. A list is read whole up to the budget
  and parsed as before (`Rejection::TooLarge` past it, so an oversized input
  is never held whole); a **RimWorld save** (`<savegame>`, root matched
  ASCII case-insensitively like the others) is read only up to its first
  `</meta>`, at most one chunk past it (a `</meta>` straddling two chunks is
  found), and `save_header.rs` closes that prefix, from the root on, with a
  synthetic `</savegame>` and runs the same `parse_document` safety
  sequence. A DOCTYPE
  before the root is `DtdNotAllowed` at the sniff, before any save logic; the
  first element after the root not being `meta` is `SaveHeaderMissing` (the
  game itself expects it first); input ending before `</meta>` is
  `SaveHeaderTruncated`; no `</meta>` within the budget is
  `SaveHeaderTooLarge`; a `</meta>` inside a comment makes the closed
  document malformed, so a crafted file is refused, never read as a shorter
  list. The save header's `modIds`/`modSteamIds`/`modNames` feed the shared
  `ListPieces` tail (id grammar, skipped entries, the official-content
  rule); `modSteamIds` is a mod's `About.xml` `steamAppId`, so most values
  are `0`, and ids can end in `_steam`. **A misaligned parallel list is never
  zipped short**: `ListPieces` reports `ListAlignment::{Absent, Aligned,
  Misaligned}` for the names and the Steam ids (for `.rml` and saves alike,
  where it used to drop them silently), drops a misaligned list whole and
  keeps every id; `ParsedModList` also carries the `ListFormat`. Decoding is
  lossy UTF-8 with the BOM stripped, and past the byte limit every U+FFFD
  becomes `?` so decoding can't grow the text. **The format is detected by
  content, never by extension**: after the BOM and leading whitespace, a
  start of `<?xml`, `<!`, `<savedModList`, `<ModsConfigData` or `<savegame` (ASCII
  case-insensitive) is XML, anything else (a hand-typed line starting with
  `<` included) goes to `rim_session::mod_list::parse_text`; an XML root
  other than `savedModList`, `ModsConfigData` and `savegame` is `UnrecognizedFormat`. XML safety: any
  `<!DOCTYPE` is `DtdNotAllowed` before parsing (roxmltree allows an empty
  DTD on its own), `raw_element_nesting_exceeds(MAX_RAW_ELEMENT_DEPTH)` is
  `TooDeep`, and roxmltree runs with `allow_dtd: false`, no entity
  resolver and `nodes_limit = MAX_XML_NODES` (16 nodes per entry, since
  roxmltree counts indentation text nodes; hitting it reads as
  `TooManyEntries`). A list's `<li>` count is checked before its texts are
  collected. A `.rml` reads `modList/ids` (else `MissingModList`) and
  `modList/names` only when it has one name per id; `meta/modIds` and
  `meta/modSteamIds` give Workshop ids only when their lengths agree, a
  `0` or non-numeric value gives none, and **official content (Core, any
  `ludeon.rimworld.*`) never gets a Workshop id**: the number a DLC row
  carries is its Steam app id. A `ModsConfigData` document reads
  `activeMods` and carries no names or links.
- **The `.rml` writer is game-exact** (`write.rs`, checked byte for byte by
  its golden test against the shape the game's own "Save list" writes):
  UTF-8 BOM, the `<?xml version="1.0" encoding="utf-8"?>` declaration,
  CRLF, tab indentation, `<meta>` (`gameVersion`, `modIds`,
  `modSteamIds`, `modNames`) then `<modList>` (`ids`, `names`) with the
  lists duplicated, and no line ending after `</savedModList>`. Text is
  escaped with `xml_text::xml_escape_text` (`&`, `<`, `>` only, as the
  game writes `'` and `"` raw in a text node), not the five-entity
  `xml_escape` `ModsConfigFileStore` uses. `modSteamIds` is the DLC's app
  id from the constant `DLC_APP_IDS` table (vanilla facts), else the
  entry's Workshop id, else `0`; a missing name is written as the id so
  the lists stay aligned. `gameVersion` is whatever the list carries,
  which an export fills with the scanned major.minor (`1.6`), not the raw
  `Version.txt` text, and is omitted when unknown. Writes go through
  `write_atomically`; which paths are allowed is the interface's call (the
  desktop accepts only a `.rml` that isn't `ModsConfig.xml`, the CLI
  refuses an existing file without `--overwrite`).
