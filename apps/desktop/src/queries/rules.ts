import { useMutation, useQuery, useQueryCache } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { awaitInvalidation, startInvalidation } from "@/queries/invalidation";
import { queryKeys } from "@/queries/keys";
import {
  deleteRule,
  getDefaultRimSortPaths,
  getFinding,
  getRecommendedRules,
  getRecommendedRulesStep,
  getRuleDatabases,
  importRimSort,
  listFindings,
  listOrphanedDecisions,
  listRules,
  promoteImportedRule,
  refreshRuleDatabases,
  skipRecommendedRulesStep,
  upsertRule,
} from "@/services/ipc";
import { asFindingKey } from "@/types/brands";
import type { DeleteRuleRequestDto } from "@/types/generated/DeleteRuleRequestDto";
import type { FindingFilterDto } from "@/types/generated/FindingFilterDto";
import type { RefreshRuleDatabasesRequestDto } from "@/types/generated/RefreshRuleDatabasesRequestDto";
import type { RimSortPathsDto } from "@/types/generated/RimSortPathsDto";
import type { RuleDto } from "@/types/generated/RuleDto";
import type { RuleKeyDto } from "@/types/generated/RuleKeyDto";
import type { RuleOriginDto } from "@/types/generated/RuleOriginDto";

/** Lists every rule in effect, optionally filtered to one origin. */
export function useRulesQuery(origin: MaybeRefOrGetter<RuleOriginDto | null>) {
  return useQuery({
    key: () => queryKeys.rules(toValue(origin)),
    query: () => listRules({ origin: toValue(origin) }),
  });
}

/** RimSort's own default database directory, for prefilling the import dialog. */
export function useDefaultRimSortPathsQuery() {
  return useQuery({
    key: queryKeys.defaultRimSortPaths,
    query: getDefaultRimSortPaths,
  });
}

/** Decisions whose key no longer matches a live finding — the orphan list. */
export function useOrphanedDecisionsQuery() {
  return useQuery({
    key: queryKeys.orphanedDecisions,
    query: listOrphanedDecisions,
  });
}

/**
 * Each rule database's cache status plus this profile's own re-import
 * signal — the Databases
 * card's own data, also read directly by `RulesPage.vue` to render the
 * inline re-import hint next to the import button (Pinia Colada dedupes
 * the two callers against the same cache entry, so this is one fetch, not
 * two).
 */
export function useRuleDatabasesQuery() {
  return useQuery({
    key: queryKeys.ruleDatabases,
    query: getRuleDatabases,
  });
}

/**
 * The Dashboard's "Get the recommended rules" step, derived in Rust.
 * Shares the Databases card's key prefix, so a refresh or an enable
 * invalidates it with no extra call.
 */
export function useRecommendedRulesStepQuery() {
  return useQuery({
    key: queryKeys.recommendedRulesStep,
    query: getRecommendedRulesStep,
  });
}

/**
 * Turns on, downloads and imports the recommended rule databases (one
 * click). Invalidates in `onSettled`, not only `onSuccess`, and waits for
 * the refetch: a `NotNeeded`/`Failed`/`ProfileChanged` report, or a refused
 * run, emits no `session://changed`, yet phase 1 may already have written
 * the app settings, the cache or a last-failure record, and a step query
 * that read `inProgress` mid-run would otherwise stay stale. Awaiting the
 * refetch keeps the mutation "loading" until the step shows its real state.
 * The step's key sits under `ruleDatabases()`, so that invalidation refetches it too
 * (listing it again would cost a second IPC call).
 */
export function useGetRecommendedRulesMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: () => getRecommendedRules(),
    onSettled: async () => {
      await awaitInvalidation(
        queryCache,
        { key: queryKeys.appSettings() },
        { key: queryKeys.ruleDatabases() },
        { key: queryKeys.notifications() },
      );
    },
  });
}

/**
 * Remembers that this profile skipped the step. Invalidates the step and
 * the notice list (the two recommended-rules notices step aside only while
 * the step is offered), and waits for the refetch so the caller can move
 * focus onto the settled state.
 */
export function useSkipRecommendedRulesStepMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: () => skipRecommendedRulesStep(),
    onSuccess: async () => {
      await awaitInvalidation(
        queryCache,
        { key: queryKeys.recommendedRulesStep() },
        { key: queryKeys.notifications() },
      );
    },
  });
}

/**
 * Refreshes rule databases into the global cache: exactly the sources the
 * variable's request names, or every enabled source for `null` (see
 * `refreshRuleDatabases`). Never
 * touches the session/rules/sort, so its own success never triggers the
 * blanket `invalidateRules()` every other rules-page mutation uses — it
 * invalidates only this card's own query, plus `notifications`: a
 * refresh can change what `ImportedRulesOutdated`/`RuleDatabasesStale`
 * derive from the same cache state the Databases card just read fresh,
 * and (like this mutation itself) `refresh_rule_databases` never emits
 * `session://changed` either, so nothing else would pick the change up.
 */
export function useRefreshRuleDatabasesMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: RefreshRuleDatabasesRequestDto | null) =>
      refreshRuleDatabases(request ?? undefined),
    onSuccess: () => {
      startInvalidation(queryCache, { key: queryKeys.ruleDatabases() });
      startInvalidation(queryCache, { key: queryKeys.notifications() });
    },
  });
}

/**
 * Every query, invalidated after a rules-page mutation. In production the
 * backend's own `session://changed` event (see `useSessionEvents`) would
 * refetch everything anyway, making this a duplicate refetch there — kept
 * unfiltered regardless so the Vitest/Playwright mock tier, which never
 * emits that event, ends up with the same fully-fresh cache a real run
 * would (a rule change can move mod tags/status too, not just the order
 * and dashboard).
 */
function invalidateRules(queryCache: ReturnType<typeof useQueryCache>): void {
  startInvalidation(queryCache);
}

/** Adds or replaces a rule, persisting the change. */
export function useUpsertRuleMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (rule: RuleDto) => upsertRule(rule),
    onSuccess: () => invalidateRules(queryCache),
  });
}

/** Removes a rule, persisting the change. */
export function useDeleteRuleMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (request: DeleteRuleRequestDto) => deleteRule(request),
    onSuccess: () => invalidateRules(queryCache),
  });
}

/** Promotes an imported pair or placement rule to a `UserDecision`-owned copy. */
export function usePromoteImportedRuleMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (key: RuleKeyDto) => promoteImportedRule(key),
    onSuccess: () => invalidateRules(queryCache),
  });
}

/** Imports RimSort's three database files, replacing any previously imported rules. */
export function useImportRimSortMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (paths: RimSortPathsDto) => importRimSort(paths),
    onSuccess: () => invalidateRules(queryCache),
  });
}

/** Filters `list_findings` down to every current `placementPromotesDependents` finding. */
const PLACEMENT_PROMOTION_FILTER: FindingFilterDto = {
  status: null,
  kinds: ["placementPromotesDependents"],
  modId: null,
  search: null,
  offset: 0,
  // `rim_session::MAX_PAGE_SIZE` — one row per pin that promotes at
  // least one dependent, bounded by how many placement rules are in
  // effect, so this is never actually paged in practice.
  limit: 200,
};

/**
 * Every currently-pinned mod's own `PlacementPromotesDependents` count
 * (the rule-level promotes-dependents companion) — "how many other mods
 * does this pin
 * promote past its own tier boundary", the blast-radius signal the rules
 * page had no way to show. The finding only exists once a mod's own
 * placement rule is already in effect (the ledger walks the current sort
 * outcome to compute it), so there is no way to preview it *before*
 * saving a new pin without new backend work. It's reachable with zero new
 * command, though: `list_findings`'s own summary carries no `modId`
 * field for any finding kind (only a canonical `key` and a human
 * `title`), so this composes one `get_finding` call per match (both
 * already-existing commands) to read the real `promoted` count off each
 * one's full detail — the match count is always small (bounded by the
 * number of placement rules in effect), so the extra round trips are not
 * a real cost. Keyed by mod id so `RuleTable.vue` can render it on every
 * placement row, new or pre-existing alike.
 */
export function usePlacementDependentCountsQuery() {
  return useQuery({
    key: queryKeys.findings(PLACEMENT_PROMOTION_FILTER),
    query: async () => {
      const page = await listFindings(PLACEMENT_PROMOTION_FILTER);
      const details = await Promise.all(
        page.items.map((item) => getFinding(asFindingKey(item.key))),
      );
      const counts: Record<string, number> = {};
      for (const detail of details) {
        if (detail.finding.kind !== "placementPromotesDependents") continue;
        counts[detail.finding.modId] = detail.finding.promoted.length;
      }
      return counts;
    },
  });
}
