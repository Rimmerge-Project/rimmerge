# rimmerge desktop

Tauri v2 + Vue 3 desktop app for rimmerge. `src-tauri/` (crate
`rimmerge-desktop`) is a thin composition root: Tauri commands wire
`rim-io` adapters into `rim-session` use cases and map the result to a
serializable DTO — no business logic lives there, and none lives in
`src/` either. See the workspace root [README.md](../../README.md) for
the crate graph this app sits on top of, and
[docs/desktop.md](../../docs/desktop.md) for a page-by-page walkthrough
with screenshots.

## Dev loop

```sh
bun install
bun run tauri dev
```

First run is slow (several minutes — a from-scratch `cargo build` of the
full Tauri/WebView2 dependency tree); after that, `bun run tauri dev` is
fast and hot-reloads the Vue frontend on save, rebuilding and restarting
the Rust backend automatically when `src-tauri/` or a crate it depends on
changes.

Other useful commands:

```sh
bun run dev                      # Vite dev server only, no Tauri window (mocked-IPC UI work)
bun run build                    # vue-tsc -b && vite build
bun run tauri build --debug      # a real debug binary, no dev server (bundling is off: no installer)
bun run tauri build              # a real release binary: target/release/rimmerge-desktop.exe at the workspace root
```

## Pages

- **Setup** — point the app at your RimWorld install, workshop folder,
  and `ModsConfig.xml`, or confirm what auto-detection found.
- **Dashboard** — a summary of the current report and quick links into
  the pages below.
- **Inbox** — the ledger, one row per finding, filterable and
  searchable.
- **Order** — the suggested load order, with a why-panel explaining any
  mod's position.
- **Mods** — every discovered mod, active or inactive, with activate/
  deactivate actions on `ModsConfig.xml`'s own active-mods list.
- **Rules** — every rule feeding the sorter, your own and (if imported)
  RimSort's, with promote/delete actions.
- **Def inspector** — everything about one def or template: owners,
  patchers, template chain, and the effective merged def.
- **Merge editor** — one finding's field-level diff and merge plan.
- **Merge output** — the generated merge mod, its coverage, and an
  export action.
- **Compatibility patches** — a user-chosen scope of two or more mods
  with its own decisions, exported as a standalone, publishable mod.
- **Patch maker (assignments)** — infers a reference-mod-to-target-mod
  field schema and gives you a row editor to fill it in, exported the
  same way a compatibility patch is.
- **Startup cost** — the per-mod startup-cost table, plus real per-mod
  timing once you've imported a `Player.log`.
- **Settings** — every setting, editable, with a note on whether it can
  change what the sorter produces.

Every page above is read-only except the Mods page's activate/deactivate
actions, the Apply dialog, and the Compatibility patches/Patch maker
export panels' "install" option — every one of them confirms before
writing. The Mods page and Apply dialog write `ModsConfig.xml`; the
Apply dialog (with "Write merge mod" checked) and the two export
panels' install option additionally write a generated mod folder under
your RimWorld install's `Mods/` folder and add its package id to
`ModsConfig.xml`. Every `ModsConfig.xml` write takes a backup first, and
nothing writes while RimWorld looks like it's still running unless you
confirm past that warning. Nothing else in the app writes anything.

## ts-rs bindings

Every DTO in `src-tauri/src/dto/*.rs` derives `ts_rs::TS` and is
exported to `src/types/generated/*.ts` by `cargo test -p
rimmerge-desktop` (committed — never hand-edit a generated file). After
changing a DTO's shape, re-run that test and commit the regenerated
bindings; `bun run types:check` does the same thing and fails if the
checked-in files don't match, which is what CI enforces.

## Testing — three tiers

| Tier | Command | What it covers | Runs in CI |
|---|---|---|---|
| Component/composable (Vitest) | `bun run test` | Individual `.vue` components and composables, mounted with Vue Test Utils; IPC calls go through a mocked backend (`src/services/ipc.mock.ts`), never a real one. | Yes |
| Mock-IPC E2E (Playwright) | `bun run e2e` | Full user flows against a Vite dev server on a dedicated port (`5183`, always started by this config with `--strictPort`, never reused), with `@tauri-apps/api/mocks`' `mockIPC` replacing every Tauri command. No Rust backend runs at all — fast, and what CI runs on every push/PR. Must pass twice in a row with zero local retries. | Yes |
| CDP smoke (Playwright) | `bun run e2e:smoke` | A handful of specs, all against a **real** `bun run tauri dev` build (a real Rust backend, a real WebView2 window), attached to over the Chrome DevTools Protocol. Proves the real IPC round-trips this app depends on actually work end to end — a real scan, a real merge/export, a real def-inspector query — against scratch copies of committed test fixtures. One spec reads this machine's own real, read-only RimWorld/Workshop/`ModsConfig.xml` to exercise real-world conflict shapes no synthetic fixture reproduces, but never writes to them. | **No — local/pre-release only** |

Run the CDP smoke tier locally before a release, or after touching
`apply`, project loading, or anything else it exercises:

```sh
bunx playwright install chromium   # once
bun run e2e:smoke
```

It takes a few minutes (a full `cargo build` of `rimmerge-desktop` the
first time) and needs a real Windows desktop session with WebView2
installed — that's why it isn't part of CI, which only runs the
mock-IPC Playwright tier plus the Vitest suite.

## License

Licensed under either of Apache-2.0 or MIT at your option. Unless you
explicitly state otherwise, any contribution intentionally submitted for
inclusion shall be dual licensed as above, without additional terms or
conditions.
