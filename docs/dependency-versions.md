# Dependency versions

The versions every Rust crate and the desktop app's JS packages resolve
to. The authoritative source is always `Cargo.lock` and
`apps/desktop/package.json`/`bun.lock`; this page is a human-readable
snapshot plus the reasoning behind every deliberate pin.

## Toolchain

`rust-toolchain.toml` pins `channel = "stable"` (resolves to whatever
stable is installed on the host — every contributor and CI runner is a
fixed Windows/MSVC host, so a triple-suffixed channel buys nothing
here). The reference toolchain is `rustc 1.98.1`; `rust-version` in
`Cargo.toml` floors it at `1.98`.

## Why Windows/MSVC only

Several dependencies here are Windows-specific, not merely
Windows-first: `sysinfo` is pulled in with its Windows process APIs
(`rim-io` reads the running-game-process check through it), and the
desktop app links WebView2 through Tauri's `wry`/`tauri-runtime-wry`.
`deny.toml`'s `[graph].targets = ["x86_64-pc-windows-msvc"]` reflects
this: it prunes the dependency graph to the one target this app
actually ships for, which also keeps `cargo deny check` from flagging
advisories/licenses in non-Windows branches of Tauri's dependency tree
(`wry`'s Linux webview backend) that this build never compiles.

## Rust — workspace dependencies (`[workspace.dependencies]`)

| Crate | Version | Notes |
| --- | --- | --- |
| anyhow | 1.0.104 | |
| assert_cmd | 2.2.2 | manifest range `2.0` |
| clap (derive) | 4.6.7 | manifest range `4.6.6` |
| insta | 1.48.0 | manifest range `1.41` |
| jiff | 0.2.37 | manifest range `0.2.35` |
| petgraph | 0.8.3 | manifest range `0.8` |
| proptest | 1.11.0 | |
| pretty_assertions | 1.4.1 | |
| regex | 1.13.1 | |
| roxmltree | 0.21.1 | |
| serde (derive) | 1.0.229 | |
| serde_json | 1.0.151 | |
| sha2 | 0.11.0 | a separate transitive `sha2 0.10.9` also resolves via another dependency's own requirement — unrelated to this crate's pin, `bans.multiple-versions` is `warn` not `deny` in `deny.toml` |
| sysinfo | 0.39.6 | `default-features = false`, `features = ["system"]` |
| tauri | 2.11.6 | manifest range `2.11`; stays on the 2.x line — see "Tauri stays on v2" below |
| tauri-build | 2.6.3 | manifest range `2.6` |
| tauri-plugin-dialog | 2.7.3 | manifest range `2` |
| tempfile | 3.27.0 | manifest range `3.14` |
| thiserror | 2.0.20 | a separate transitive `thiserror 1.0.69` also resolves via another dependency |
| tokio | 1.53.1 | manifest range `1` |
| tracing | 0.1.44 | manifest range `0.1` |
| tracing-subscriber | 0.3.23 | manifest range `0.3` |
| ts-rs | 12.0.1 | on the 12.x line |
| ureq | 3.4.2 | `default-features = false`, `rustls-no-provider` + `_ring` + `platform-verifier` — see the pin's own comment in `Cargo.toml` for why |

## Rust — per-crate dependencies (not in `[workspace.dependencies]`)

| Crate | Version | Used by | Notes |
| --- | --- | --- | --- |
| rayon | 1.12.0 | rim-analyzer | |
| walkdir | 2.5.0 | rim-analyzer, rim-io | |
| predicates | 3.1.4 | apps/cli (dev) | |
| base64 | 0.23.1 | apps/desktop/src-tauri | encodes `read_texture`'s response as a `data:` URL; the `Engine`/`general_purpose::STANDARD` API this crate uses is stable across the 0.22–0.23 line |

Several crates duplicate a workspace-level pin string directly in
`crates/rim-analyzer/Cargo.toml` (`anyhow`, `clap`, `jiff`, `roxmltree`,
`serde`, `serde_json`, `thiserror`) rather than using
`{ workspace = true }` — a deliberate choice documented in that file's
own closing comment. Their resolved versions match the workspace table
above.

## Rust — left un-bumped

- **Tauri stays on v2** (`tauri`/`tauri-build`/`tauri-plugin-dialog`,
  and the JS `@tauri-apps/*` packages below): 3.x is alpha-only on both
  the Rust and JS sides (`3.0.0-alpha.*`), so v2 is the only
  production-ready line. Each 2.x crate here is pinned to its newest
  version (`2.11.6`, `2.6.3`, `2.7.3`); there is nothing older to bump
  within v2.

## JS — `apps/desktop/package.json`

| Package | Version | Notes |
| --- | --- | --- |
| @pinia/colada | ^1.4.5 | |
| @primevue/themes | 4.5.4 | pinned — see "PrimeVue stays on 4.x" below |
| @tanstack/vue-virtual | ^3.13.39 | |
| @tauri-apps/api | ^2.11.1 | stays on 2.x, see Tauri note above |
| @tauri-apps/plugin-dialog | ^2.7.3 | |
| @vueuse/core | ^14.4.0 | 15.0.0 available (major); not a named pinned family, stays on its caret range |
| pinia | ^4.0.3 | |
| primeicons | ^8.0.1 | |
| primevue | 4.5.5 | pinned — see "PrimeVue stays on 4.x" below |
| vue | 3.5.43 | pinned to the 3.5.x line |
| vue-i18n | ^11.4.12 | Composition API (`legacy: false`); pulls in `@intlify/core-base`/`@intlify/shared`/`@intlify/devtools-types` and `@vue/devtools-api`, none tracked separately here |
| vue-router | ^5.3.1 | |
| @biomejs/biome (dev) | ^2.5.14 | |
| @pinia/testing (dev) | ^2.0.1 | |
| @playwright/test (dev) | ^1.63.0 | already at the newest version satisfying its range |
| @tailwindcss/vite (dev) | ^4.3.3 | |
| @tauri-apps/cli (dev) | ^2.11.5 | |
| @types/node (dev) | ^24.13.6 | 26.6.2 available (major); left un-bumped, see below |
| @typescript-eslint/parser (dev) | ^8.70.1 | |
| @vitejs/plugin-vue (dev) | ^6.0.9 | |
| @vue/test-utils (dev) | ^2.5.1 | |
| @vue/tsconfig (dev) | ^0.9.1 | |
| eslint (dev) | ^10.11.0 | |
| eslint-plugin-vue (dev) | ^10.11.0 | |
| happy-dom (dev) | ^20.14.5 | |
| tailwindcss (dev) | ^4.3.3 | |
| typescript (dev) | ~6.0.3 | 7.0.2 available (major); left un-bumped, see below |
| vite (dev) | ^8.3.0 | |
| vitest (dev) | ^5.0.1 | |
| vue-tsc (dev) | ^3.3.11 | already at the newest published version |

## JS — left un-bumped

- **PrimeVue stays on 4.x** (`primevue` 4.5.5, `@primevue/themes`
  4.5.4): documented in `apps/desktop/CLAUDE.md` — 4.5.5 is the last
  MIT-licensed release; 5.x became commercially licensed. Never bump
  past 4.x without checking the license first.
- **TypeScript stays on the 6.x line** (`typescript` `~6.0.3`, 7.0.2
  available): TypeScript 7 breaks `vue-tsc@3.3.11` (the newest
  published `vue-tsc`), which resolves TypeScript's `./lib/tsc` entry
  point directly via `require.resolve` — TypeScript 7's restructured
  package `exports` no longer publishes that subpath, so
  `vue-tsc -b --noEmit` fails immediately with
  `ERR_PACKAGE_PATH_NOT_EXPORTED`. No `vue-tsc` release fixes this yet;
  revisit once one does.
- **`@types/node` stays on the 24.x line** (26.6.2 available): a Node
  types major is not a zero-risk bump given the toolchain doesn't
  otherwise pin a Node version, so it's left on its caret range rather
  than force-bumped.
- **`@vueuse/core` stays on the 14.x line** (15.0.0 available): same
  reasoning as `@types/node` — its caret range already keeps it current
  within 14.x, so a major bump isn't forced.

## `rules/` (the `rimmerge-rules` submodule)

Not tracked here — a separate repository with zero dependencies of its
own (`bun scripts/build.mjs --check` has no lockfile to update).
