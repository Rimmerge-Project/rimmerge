import { randomUUID } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const smokeDir = path.dirname(fileURLToPath(import.meta.url));

/** `crates/rim-analyzer/tests/fixtures/sample_game`, the checked-in fixture mod tree. */
const SAMPLE_GAME_SOURCE = path.resolve(
  smokeDir,
  "../../../../crates/rim-analyzer/tests/fixtures/sample_game",
);

/**
 * `crates/rim-io/tests/fixtures/merge_game` — `ModA`/`ModB` both defining
 * `ThingDef/Fixture_Wall` through a two-level `ParentName` chain with
 * differing `statBases/MaxHitPoints`, the same fixture
 * `crates/rim-io/tests/merge_end_to_end.rs` uses for the equivalent
 * backend-only test.
 */
const MERGE_GAME_SOURCE = path.resolve(
  smokeDir,
  "../../../../crates/rim-io/tests/fixtures/merge_game",
);

/**
 * `crates/rim-io/tests/fixtures/assign_game` — `fixture.framework` (five
 * `example.PartAssignmentDef` instances: a `speciesNames` target key, a `parts`
 * item slot, an `enabled` scalar), `fixture.parts` (the five
 * `example.PartDef` items they reference), and `fixture.target` (five
 * `ThingDef`s with a `<race>` marker) — the same fixture
 * `apps/cli/tests/assign_cli.rs` uses for the equivalent backend-only
 * end-to-end test.
 */
const ASSIGN_GAME_SOURCE = path.resolve(
  smokeDir,
  "../../../../crates/rim-io/tests/fixtures/assign_game",
);

export interface ScratchPaths {
  /** The OS-temp root this tree was built under — what `globalTeardown` removes. */
  root: string;
  gameDir: string;
  workshopDir: string;
  modsConfig: string;
  profileDir: string;
}

/**
 * Writes `<baseDir>/app-settings.json` with `allowNetwork: false` before
 * the smoke tier's own `bun run tauri dev` process ever starts — belt
 * and braces alongside the first-run gate (which already keeps every
 * automatic network feature off until the Welcome notice is answered,
 * and no smoke spec ever answers it): `docs/testing.md`'s own
 * hermeticity rule for this tier. `baseDir` is whatever this run passes
 * as `RIMMERGE_PROFILE_DIR` (the webServer's own env, set once for the
 * whole shared process) — the app-global base `app-settings.json` lives
 * under, distinct from any one spec's own `profile-dir-input` value.
 * Mirrors `rim_io::app_settings`'s on-disk shape (`AppSettingsFile`)
 * exactly; every other field matches `AppSettings::default()`, since
 * only `allowNetwork` needs to differ from it here.
 */
export function writeScratchAppSettingsNetworkOff(baseDir: string): void {
  mkdirSync(baseDir, { recursive: true });
  const file = {
    schema: 1,
    network: {
      allow_network: false,
      check_for_updates: true,
      auto_refresh_rule_databases: true,
      fetch_community_rules: true,
      fetch_steam_workshop: true,
      fetch_rimmerge_rules: true,
    },
    reminders: {
      rule_databases_stale_after_days: 30,
    },
  };
  writeFileSync(path.join(baseDir, "app-settings.json"), JSON.stringify(file, null, 2));
}

/** A fresh, empty scratch directory for `profile-dir-input` alone (no game tree). */
export interface ScratchProfileOnly {
  /** The OS-temp root this directory was built under — what `globalTeardown` removes. */
  root: string;
  profileDir: string;
}

/**
 * The real-install desktop spec (`def-conflict-real.spec.ts`)
 * (unlike every other smoke spec) points `game-dir-input`/
 * `workshop-dir-input`/`mods-config-input` at the machine's real,
 * read-only RimWorld install — left at the setup page's own
 * `get_default_paths` prefill, never filled by hand — so it can select
 * the real, named contested defs, but must still never touch the real
 * profile store. This builds just the one directory that spec needs to
 * fill `profile-dir-input` with: `rim-io`'s stores default an empty
 * directory to "no rules, no decisions yet" (the same assumption
 * `RIMMERGE_PERF_PROFILE_DIR` relies on for the Rust-tier real-install
 * tests in `apps/cli/tests/real_install_defs.rs`), so no seed files are
 * needed.
 */
export function buildScratchProfileOnly(): ScratchProfileOnly {
  const root = path.join(os.tmpdir(), `rimmerge-smoke-realprofile-${randomUUID()}`);
  const profileDir = path.join(root, "profile");
  mkdirSync(profileDir, { recursive: true });

  return { root, profileDir };
}

/**
 * Copies the analyzer's checked-in `sample_game` fixture into a fresh
 * scratch directory under the OS temp dir, adds the `Version.txt` the
 * fixture doesn't ship (its own tests build a `ScanConfig` with the game
 * version given directly, bypassing the file read the real
 * `AnalyzerScanner` does), and creates empty workshop/profile
 * directories alongside it — never the real game install or the real
 * RimSort/profile directories.
 */
export function buildScratchGame(): ScratchPaths {
  const root = path.join(os.tmpdir(), `rimmerge-smoke-${randomUUID()}`);
  const gameDir = path.join(root, "game");
  const workshopDir = path.join(root, "workshop");
  const profileDir = path.join(root, "profile");

  cpSync(SAMPLE_GAME_SOURCE, gameDir, { recursive: true });
  writeFileSync(path.join(gameDir, "Version.txt"), "1.6.4871 rev590\n");
  mkdirSync(workshopDir, { recursive: true });
  mkdirSync(profileDir, { recursive: true });

  return {
    root,
    gameDir,
    workshopDir,
    modsConfig: path.join(gameDir, "ModsConfig.xml"),
    profileDir,
  };
}

/**
 * Copies the `assign_game` fixture (`fixture.framework`/`fixture.parts`/
 * `fixture.target`, already carrying their own `ModsConfig.xml` and
 * `Version.txt`) into a fresh scratch directory under the OS temp dir —
 * never the real game install or the real RimSort/profile directories.
 */
export function buildScratchAssignGame(): ScratchPaths {
  const root = path.join(os.tmpdir(), `rimmerge-smoke-assign-${randomUUID()}`);
  const gameDir = path.join(root, "game");
  const workshopDir = path.join(root, "workshop");
  const profileDir = path.join(root, "profile");

  cpSync(ASSIGN_GAME_SOURCE, gameDir, { recursive: true });
  const versionFile = path.join(gameDir, "Version.txt");
  if (!existsSync(versionFile)) {
    writeFileSync(versionFile, "1.6.4871 rev590\n");
  }
  mkdirSync(workshopDir, { recursive: true });
  mkdirSync(profileDir, { recursive: true });

  return {
    root,
    gameDir,
    workshopDir,
    modsConfig: path.join(gameDir, "ModsConfig.xml"),
    profileDir,
  };
}

/**
 * Copies the `merge_game` fixture (`ModA`/`ModB`, both already carrying
 * their own `ModsConfig.xml` and `Version.txt`) into a fresh scratch
 * directory under the OS temp dir, adding `Version.txt` only if a future
 * change to the fixture drops it — never the real game install or the
 * real RimSort/profile directories.
 */
export function buildScratchMergeGame(): ScratchPaths {
  const root = path.join(os.tmpdir(), `rimmerge-smoke-merge-${randomUUID()}`);
  const gameDir = path.join(root, "game");
  const workshopDir = path.join(root, "workshop");
  const profileDir = path.join(root, "profile");

  cpSync(MERGE_GAME_SOURCE, gameDir, { recursive: true });
  const versionFile = path.join(gameDir, "Version.txt");
  if (!existsSync(versionFile)) {
    writeFileSync(versionFile, "1.6.4871 rev590\n");
  }
  mkdirSync(workshopDir, { recursive: true });
  mkdirSync(profileDir, { recursive: true });

  return {
    root,
    gameDir,
    workshopDir,
    modsConfig: path.join(gameDir, "ModsConfig.xml"),
    profileDir,
  };
}

/** A valid 1x1 PNG, so the backend sniffs a real image format. */
const ONE_PIXEL_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==",
  "base64",
);

/** The directions a `Graphic_Multi` texture set ships. */
const MULTI_FACINGS = ["north", "east", "south", "west"] as const;

/**
 * The `assign_game` fixture plus a texture set for `Race0`: the target mod
 * gets `Textures/Things/Race0_{north,east,south,west}.png` and `Race0`
 * gains a `Graphic_Multi` `graphicData` pointing at them, so the row
 * editor has something real to show. Everything is written into the
 * scratch copy; the checked-in fixture is never touched.
 */
export function buildScratchAssignGraphicsGame(): ScratchPaths {
  const root = path.join(os.tmpdir(), `rimmerge-smoke-graphics-${randomUUID()}`);
  const gameDir = path.join(root, "game");
  const workshopDir = path.join(root, "workshop");
  const profileDir = path.join(root, "profile");

  cpSync(ASSIGN_GAME_SOURCE, gameDir, { recursive: true });
  const versionFile = path.join(gameDir, "Version.txt");
  if (!existsSync(versionFile)) {
    writeFileSync(versionFile, "1.6.4871 rev590\n");
  }
  mkdirSync(workshopDir, { recursive: true });
  mkdirSync(profileDir, { recursive: true });

  const targetMod = path.join(gameDir, "Mods", "target");
  const texturesDir = path.join(targetMod, "Textures", "Things");
  mkdirSync(texturesDir, { recursive: true });
  for (const facing of MULTI_FACINGS) {
    writeFileSync(path.join(texturesDir, `Race0_${facing}.png`), ONE_PIXEL_PNG);
  }
  const defsFile = path.join(targetMod, "1.6", "Defs", "ThingDefs_Races.xml");
  const defs = readFileSync(defsFile, "utf-8");
  const graphicData =
    "<graphicData><texPath>Things/Race0</texPath>" +
    "<graphicClass>Graphic_Multi</graphicClass></graphicData>";
  const withGraphic = defs.replace(
    "<label>race zero</label>",
    `<label>race zero</label>\n    ${graphicData}`,
  );
  if (withGraphic === defs) {
    throw new Error("assign_game's Race0 no longer has the label this builder anchors on");
  }
  writeFileSync(defsFile, withGraphic);

  return {
    root,
    gameDir,
    workshopDir,
    modsConfig: path.join(gameDir, "ModsConfig.xml"),
    profileDir,
  };
}
