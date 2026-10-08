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
  only the per-occurrence result lists do. It **no longer refuses** an
  in-game console snapshot: `read(path, formats, KindChoice)` decides the
  kind by content (`detect.rs`'s `detect_kind`: within a peek window of 200
  lines and 1 MiB, any console-copy trace start, the exact line
  `UnityEngine.StackTraceUtility:ExtractStackTrace ()` or
  `No stack trace.`, means `ConsoleSnapshot` wherever the banner is;
  otherwise the exact `patterns::BANNER_PATTERN` banner, the same pattern
  the pass counter uses, means `PlayerLog`; neither means
  `ConsoleSnapshot`, the reading that only under-claims. The banner alone
  cannot decide, because the game logs it into the console queue: an
  uncleared console copy holds it near its top, and first of all after a
  launch without options. The peeked lines are then rewound, and a path that cannot
  rewind errors saying `--kind` skips detection), or takes the caller's
  `KindChoice::Force`; a file name is never consulted. The kind picks the `Framing` and the result's `coverage`
  (`coverage.rs`: `CoverageCollector` gathers the banner lines, the
  logging-gap stop/resume lines, Unity's memory footer and crash handler
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
    line means Player.log; and equal to `parse_as` under the detected
    framing) and
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
  store a path a later lexical comparison could be fooled by.
- **`mod_knowledge.rs` + `build.rs`**: `FsModKnowledgeStore`
  implements `rim_session::ports::ModKnowledgeStore` as "the fetched
  `<cache_dir>/rimmergeRules.json` if present and it parses, else the
  bundled snapshot compiled into the binary", with **per-section**
  fallback — a cache file supplying only `precedence` still keeps the
  bundled `patch_operations`/`def_cache_carriers`/`log_shapes`. `FsModKnowledgeStore::vendored()`
  skips the cache entirely, for a caller with no cache directory and for
  tests. The bundled snapshot is **one file, not one per section**: `build.rs`
  fails the build with a clear message
  (`rules/rimmerge-rules.json not found — run: git submodule update
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
  `RimWorldWin64.exe`'s `symlink_metadata` is a regular file (a symlink or a
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
- **`mod_list_file.rs` + `mod_list_file/{read,write}.rs`**: `RmlFileStore`
  implements `rim_session::ports::ModListFileStore`. The input is
  untrusted and every bound is a `rim_session::mod_list::ModListLimits`
  constant, never a local number: `read_bounded` (re-exported as
  `read_mod_list_bounded`, which the CLI's stdin uses) takes at most
  `MAX_INPUT_BYTES + 1` bytes and returns `None` past the limit, so an
  oversized input is never held whole (`Rejection::TooLarge`); decoding is
  lossy UTF-8 with the BOM stripped, and past the byte limit every U+FFFD
  becomes `?` so decoding can't grow the text. **The format is detected by
  content, never by extension**: after the BOM and leading whitespace, a
  start of `<?xml`, `<!`, `<savedModList` or `<ModsConfigData` (ASCII
  case-insensitive) is XML, anything else (a hand-typed line starting with
  `<` included) goes to `rim_session::mod_list::parse_text`; an XML root
  other than those two is `UnrecognizedFormat`. XML safety: any
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
