import { defineConfig } from "@playwright/test";

import {
  buildScratchAssignGame,
  buildScratchAssignGraphicsGame,
  buildScratchGame,
  buildScratchMergeGame,
  buildScratchProfileOnly,
  writeScratchAppSettingsNetworkOff,
} from "./scratch-game";

// The CDP smoke tier: runs the real `bun run tauri dev` build (a real
// Rust backend, a real WebView2 window) and attaches Playwright to it
// over the Chrome DevTools Protocol — `chromium.connectOverCDP`, never
// `chromium.launch()` (there is no separate browser to launch; WebView2
// *is* the browser here). `RIMMERGE_PROFILE_DIR` points the app's
// `get_default_paths` hash base at a scratch directory so a bug here
// can never touch the real profile store, and the scratch game trees
// (built fresh per run, see `scratch-game.ts`) mean this never touches a
// real RimWorld install either. Both scratch trees are removed after the
// run — see `global-teardown.ts`.
//
// All three scratch trees are built once, up front, and every spec
// shares the one `webServer` process this config starts (`workers: 1`,
// `fullyParallel: false` — see below): each spec does a full page
// navigation to `/setup` rather than assuming a fresh app, since an
// earlier spec may already have loaded its own project into the same
// running window by the time it runs. Each spec explicitly fills
// `profile-dir-input` with its own scratch `profileDir` rather than
// leaving the setup page's prefilled default in place: that default
// comes from `get_default_paths`, which hashes the *real* default
// `ModsConfig.xml` path (unrelated to any scratch tree), so every spec's
// default resolves to the identical directory — leaving it unfilled
// would have every spec reading and writing
// `decisions.json`/`rules.json` (and, for `patch.spec.ts`, its own
// `patches/` directory) in the same place despite loading unrelated
// projects.
const scratch = buildScratchGame();
const mergeScratch = buildScratchMergeGame();
// `patch.spec.ts` gets its own `merge_game` copy (a second,
// independently-`randomUUID()`-named tree from the same fixture,
// `buildScratchMergeGame` is a factory, not a singleton) — sharing
// `mergeScratch`'s profile dir with `merge.spec.ts` would have both
// specs' decisions/patches land in one `patches/` directory despite
// being unrelated runs.
const patchScratch = buildScratchMergeGame();
// `defs.spec.ts` gets its own `merge_game` copy too, for the same reason
// `patchScratch` does: `merge.spec.ts`'s own test decides and applies a
// merge against `mergeScratch`, appending a merge-mod folder to its
// `Mods/` and a new id to its `ModsConfig.xml` — sharing that tree with
// a read-only def-inspector spec would make its result depend on
// whichever spec happened to run first.
const defsScratch = buildScratchMergeGame();
// The real-install spec (`def-conflict-real.spec.ts`) is the one
// exception to "every scratch tree is a fabricated game" above: it reads
// the machine's real, read-only RimWorld install (`game-dir-input`/
// `workshop-dir-input`/`mods-config-input` left at the setup page's own
// `get_default_paths` prefill) so it can select the real, named
// contested defs (`BiomeDef/AridShrubland`, `HediffDef/BionicHeart`) —
// but still needs its own scratch *profile* directory, same as every
// other spec here, so it never reads or writes the real profile store.
const realProfileScratch = buildScratchProfileOnly();
// Belt and braces (`docs/testing.md`): `RIMMERGE_PROFILE_DIR` below is
// the one `<base>` this whole shared `webServer` process resolves
// `app-settings.json`/`notifications.json` under — never the real
// `%LOCALAPPDATA%\rimmerge`, and network features default to *on* when
// no file exists at all, so this keeps the tier hermetic even if some
// future spec ever answered the Welcome notice.
writeScratchAppSettingsNetworkOff(scratch.profileDir);
// `assignments.spec.ts` gets its own
// `assign_game` copy — a two-mod-reference/one-target-mod fixture
// framework, never shared with any of the other scratch trees above.
const assignScratch = buildScratchAssignGame();
// `def-graphics.spec.ts` gets its own `assign_game` copy with a texture set
// for one target def, so no other spec sees the extra files.
const graphicsScratch = buildScratchAssignGraphicsGame();

// Threaded to the spec through the environment rather than a fixture
// file: Playwright's worker processes inherit `process.env` from the
// config process that set it.
process.env.RIMMERGE_SMOKE_GAME_DIR = scratch.gameDir;
process.env.RIMMERGE_SMOKE_WORKSHOP_DIR = scratch.workshopDir;
process.env.RIMMERGE_SMOKE_MODS_CONFIG = scratch.modsConfig;
process.env.RIMMERGE_SMOKE_PROFILE_DIR = scratch.profileDir;
process.env.RIMMERGE_SMOKE_MERGE_GAME_DIR = mergeScratch.gameDir;
process.env.RIMMERGE_SMOKE_MERGE_WORKSHOP_DIR = mergeScratch.workshopDir;
process.env.RIMMERGE_SMOKE_MERGE_MODS_CONFIG = mergeScratch.modsConfig;
process.env.RIMMERGE_SMOKE_MERGE_PROFILE_DIR = mergeScratch.profileDir;
process.env.RIMMERGE_SMOKE_PATCH_GAME_DIR = patchScratch.gameDir;
process.env.RIMMERGE_SMOKE_PATCH_WORKSHOP_DIR = patchScratch.workshopDir;
process.env.RIMMERGE_SMOKE_PATCH_MODS_CONFIG = patchScratch.modsConfig;
process.env.RIMMERGE_SMOKE_PATCH_PROFILE_DIR = patchScratch.profileDir;
process.env.RIMMERGE_SMOKE_DEFS_GAME_DIR = defsScratch.gameDir;
process.env.RIMMERGE_SMOKE_DEFS_WORKSHOP_DIR = defsScratch.workshopDir;
process.env.RIMMERGE_SMOKE_DEFS_MODS_CONFIG = defsScratch.modsConfig;
process.env.RIMMERGE_SMOKE_DEFS_PROFILE_DIR = defsScratch.profileDir;
process.env.RIMMERGE_SMOKE_REAL_PROFILE_DIR = realProfileScratch.profileDir;
process.env.RIMMERGE_SMOKE_ASSIGN_GAME_DIR = assignScratch.gameDir;
process.env.RIMMERGE_SMOKE_ASSIGN_WORKSHOP_DIR = assignScratch.workshopDir;
process.env.RIMMERGE_SMOKE_ASSIGN_MODS_CONFIG = assignScratch.modsConfig;
process.env.RIMMERGE_SMOKE_ASSIGN_PROFILE_DIR = assignScratch.profileDir;
process.env.RIMMERGE_SMOKE_GRAPHICS_GAME_DIR = graphicsScratch.gameDir;
process.env.RIMMERGE_SMOKE_GRAPHICS_WORKSHOP_DIR = graphicsScratch.workshopDir;
process.env.RIMMERGE_SMOKE_GRAPHICS_MODS_CONFIG = graphicsScratch.modsConfig;
process.env.RIMMERGE_SMOKE_GRAPHICS_PROFILE_DIR = graphicsScratch.profileDir;
// `get_default_paths` hashes the *real* default `ModsConfig.xml` path,
// not whichever scratch one a spec fills in — so every spec's setup page
// prefills the same profile directory unless it explicitly fills
// `profile-dir-input` with its own scratch `profileDir` (see the specs).

const CDP_PORT = 9222;

export default defineConfig({
  testDir: ".",
  testMatch: /.*\.spec\.ts/,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: "list",
  timeout: 90_000,
  globalTeardown: "./global-teardown.ts",
  webServer: {
    command: "bun run tauri dev",
    // WebView2 exposes the same DevTools Protocol `/json/version`
    // endpoint a headless-Chrome would once it's actually up, so this
    // doubles as "the window is ready" for Playwright's own wait.
    url: `http://localhost:${CDP_PORT}/json/version`,
    reuseExistingServer: false,
    cwd: "../..",
    timeout: 180_000,
    env: {
      RIMMERGE_PROFILE_DIR: scratch.profileDir,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}`,
    },
  },
  projects: [{ name: "smoke" }],
});
