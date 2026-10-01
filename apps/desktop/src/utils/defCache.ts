/**
 * Pure helpers for the apply dialog's def-cache note. Deliberately not a
 * `rim-session` field — that crate's own `GameLogSummary.def_cache_lines`
 * only surfaces the raw lines for this note to read; extracting
 * presentation logic with no other consumer, so it stays here rather
 * than growing the DTO.
 */

const COLOR_TAG_RE = /<\/?color(?:=[^>]*)?>/g;
const CACHE_BUILD_SECONDS_RE = /took\s+(\d+(?:\.\d+)?)\s+seconds/i;

/**
 * Extracts the cache-build duration (in seconds) from a `DEFCACHE: ...`
 * line shaped like `DEFCACHE: <color=white>Cache created!</color>
 * creating cache took <color=green>8 seconds</color>` — the one
 * `defCacheLines` rendering that carries a duration at all (the others,
 * "Cache disabled from"/"Mod list changed! Deleting cache"/"Cache not
 * found or got purged!", don't). A real run can rebuild the cache more
 * than once (mod list changed mid-session, or a second purge-and-rebuild
 * later in the same log), so this returns the **last** match — the copy
 * that quotes it says "on your last log", meaning the most recent build,
 * not the first one that happened to appear. `null` when none of
 * `defCacheLines` carries one (no cache was (re)built on the imported
 * run, or no log has been imported).
 */
export function defCacheBuildSeconds(defCacheLines: readonly string[]): number | null {
  let last: number | null = null;
  for (const line of defCacheLines) {
    const stripped = line.replace(COLOR_TAG_RE, "");
    const match = CACHE_BUILD_SECONDS_RE.exec(stripped);
    const seconds = match?.[1];
    if (seconds !== undefined) {
      last = Number(seconds);
    }
  }
  return last;
}
