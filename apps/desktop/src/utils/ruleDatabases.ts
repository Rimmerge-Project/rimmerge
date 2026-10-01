import { renderMessage } from "@/i18n/messageDescriptor";
import type { RuleDatabaseDto } from "@/types/generated/RuleDatabaseDto";
import type { RuleDatabaseViewDto } from "@/types/generated/RuleDatabaseViewDto";
import { describeFetchFailure } from "@/utils/fetchFailure";

/** The `t()` shape {@link formatDatabaseStatusLine} takes — a caller's own `useI18n().t`. */
type Translate = (key: string, params?: Record<string, unknown>, count?: number) => string;

/**
 * A literal `en.json` key per source, resolved by `t()` at render
 * time — shared by `RuleDatabasesCard.vue` and the notifications
 * components, so the two never carry two hand-copied label tables that
 * could drift apart.
 */
export const DATABASE_LABEL_KEYS: Record<RuleDatabaseDto, string> = {
  community: "rules.databases.communityLabel",
  steam: "rules.databases.steamLabel",
  rimmerge: "rules.databases.rimmergeLabel",
};

/**
 * Whether the once-a-day automatic refresh may fetch this source at
 * all — mirrors `rim_session::ports::RuleDatabase::is_auto_refresh_eligible`
 * exactly (Steam Workshop is manual-only regardless of its own fetch
 * toggle, since a download that size must never happen without the
 * user asking). Duplicated here, not sent by the backend, because it's
 * a display-only label on the Databases card, not something any command
 * response currently carries per row.
 */
export function isAutoRefreshEligible(database: RuleDatabaseDto): boolean {
  return database !== "steam";
}

/** The first 12 hex characters of a sha256 digest — this workspace's own display convention. */
export function sha12(sha256: string): string {
  return sha256.slice(0, 12);
}

/** `394 KB`/`49.0 MB`/`512 B` — human units for a cached database's byte count. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Whole days elapsed between `iso` (RFC 3339) and `now`, floored, never negative. */
function daysSince(iso: string, now: Date): number {
  const elapsedMs = now.getTime() - new Date(iso).getTime();
  return Math.max(0, Math.floor(elapsedMs / (24 * 60 * 60 * 1000)));
}

/** `today`/`1 day ago`/`N days ago`. */
function relativeDays(iso: string, now: Date, t: Translate): string {
  const days = daysSince(iso, now);
  if (days === 0) return t("rules.databases.today");
  return t("rules.databases.daysAgo", { count: days }, days);
}

/** `Stale (47 days)`. */
function staleLabel(iso: string, now: Date, t: Translate): string {
  const days = daysSince(iso, now);
  return t("rules.databases.stale", { count: days }, days);
}

/**
 * One Databases-card status line, covering all four states: `Updated 2 days ago · 394 KB
 * · b00fa1529656` / `Never fetched` / `Stale (47 days) · ... ` / `...
 * Last refresh failed: connection timed out`. A failure is rendered as a
 * **suffix** on whatever the cached facts already say (never fetched, or
 * the last successful fetch's own facts) rather than replacing them —
 * mirroring `apps/cli`'s own `db status` line (`format_status_line`) so
 * the two interfaces agree: a failed refresh never hides what's still
 * cached and in use.
 *
 * `now` is an **injected** `Date`, never read from the system clock
 * inside this function, per the root `CLAUDE.md` determinism rule — the
 * component calls this with `new Date()` once per render tick, so the
 * clock read happens only at that one composition-root edge.
 *
 * `view.bundledSha256` (set only for the `rimmerge` source) is rendered
 * as a trailing `· Bundled b00fa1529656` — the embedded snapshot's own
 * identity, always present for that source even when nothing has ever
 * been fetched, mirroring `apps/cli`'s own `[bundled: ...]` marker.
 */
export function formatDatabaseStatusLine(
  view: RuleDatabaseViewDto,
  now: Date,
  t: Translate,
  locale: string,
): string {
  let line: string;
  if (!view.cached) {
    line = t("rules.databases.neverFetched");
  } else {
    const recency = view.isStale
      ? staleLabel(view.cached.fetchedAt, now, t)
      : t("rules.databases.updated", { relative: relativeDays(view.cached.fetchedAt, now, t) });
    line = t("rules.databases.recencyLine", {
      recency,
      bytes: formatBytes(view.cached.bytes),
      sha: sha12(view.cached.sha256),
    });
  }
  if (view.lastFailure) {
    line += t("rules.databases.lastFailureSuffix", {
      reason: renderMessage(t, describeFetchFailure(view.lastFailure, locale)),
    });
  }
  if (view.bundledSha256) {
    line += t("rules.databases.bundledSuffix", { sha: sha12(view.bundledSha256) });
  }
  return line;
}
