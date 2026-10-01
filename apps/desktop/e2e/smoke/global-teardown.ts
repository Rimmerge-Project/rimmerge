import { readdirSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";

/** Both `buildScratchGame` and `buildScratchMergeGame` name their root with this prefix. */
const SCRATCH_ROOT_PREFIX = "rimmerge-smoke-";

/**
 * Removes every scratch game tree under the OS temp dir after the smoke
 * run finishes, by name prefix rather than by threading the two exact
 * roots through the environment: Playwright's config loader
 * re-evaluates `playwright.config.ts` more than once per `playwright
 * test` invocation, and each evaluation's `buildScratchGame`/
 * `buildScratchMergeGame` call builds a fresh tree (a new
 * `randomUUID()`-named directory) as a side effect — an env var can only
 * ever hold the last evaluation's pair, leaking every earlier one's tree.
 * A prefix sweep is immune to that, and also catches a root a previous
 * run's teardown never got to run for (a crash, a `Ctrl+C`).
 */
export default function globalTeardown(): void {
  const tmpDir = os.tmpdir();
  for (const entry of readdirSync(tmpDir, { withFileTypes: true })) {
    if (entry.isDirectory() && entry.name.startsWith(SCRATCH_ROOT_PREFIX)) {
      rmSync(path.join(tmpDir, entry.name), { recursive: true, force: true });
    }
  }
}
