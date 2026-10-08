import { useMutation, useQuery, useQueryCache } from "@pinia/colada";

import { startInvalidation } from "@/queries/invalidation";
import { queryKeys } from "@/queries/keys";
import {
  enableRecommendedRuleDatabases,
  getAppSettings,
  getDefaultSettings,
  getSettings,
  resetNetworkPolicy,
  setSettings,
  updateAppSettings,
} from "@/services/ipc";
import type { AppSettingsDto } from "@/types/generated/AppSettingsDto";
import type { SettingsDto } from "@/types/generated/SettingsDto";

/** The current sorter/ledger settings. */
export function useSettingsQuery() {
  return useQuery({
    key: queryKeys.settings,
    query: getSettings,
  });
}

/** Replaces the sorter/ledger settings, persisting the change. */
export function useSetSettingsMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (settings: SettingsDto) => setSettings(settings),
    onSuccess: () => {
      // Re-computes every status derivation and every advisory-edge
      // satisfaction check downstream of the threshold/enforcement
      // toggles — cheapest correct answer is to refetch everything.
      startInvalidation(queryCache);
    },
  });
}

/**
 * The sorter/ledger settings' own hard-coded defaults — a mutation, not
 * a cached query, since it's explicitly triggered ("Reset to defaults")
 * rather than loaded automatically, and its result is never persisted
 * on its own: the Settings page only refills its form from it, leaving
 * Save as the user's own separate step.
 */
export function useDefaultSettingsMutation() {
  return useMutation({
    mutation: () => getDefaultSettings(),
  });
}

/**
 * The app-global network/reminder preferences, plus how
 * `app-settings.json` loaded (`loadStatus`).
 */
export function useAppSettingsQuery() {
  return useQuery({
    key: queryKeys.appSettings,
    query: getAppSettings,
  });
}

/**
 * Replaces the app-global preferences wholesale, persisting the change.
 * Also invalidates the notice list: `Welcome`'s own `network` field is a
 * snapshot taken at evaluation time, so a card/page reading the new
 * value through that notice (rather than through this same query) would
 * otherwise keep showing whatever was true before this save. The Dashboard
 * step reads `allow_network` too, so its query is invalidated as well.
 */
export function useUpdateAppSettingsMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: (settings: AppSettingsDto) => updateAppSettings(settings),
    onSuccess: () => {
      startInvalidation(queryCache, { key: queryKeys.appSettings() });
      startInvalidation(queryCache, { key: queryKeys.notifications() });
      startInvalidation(queryCache, { key: queryKeys.recommendedRulesStep() });
    },
  });
}

/**
 * Restores just the network half of the app settings to its defaults —
 * every switch back on — leaving the reminder threshold untouched.
 * Also invalidates the notice list, whose `Welcome` snapshot of the
 * network policy is stale after this reset, and the Dashboard step, whose
 * `unavailable` state follows `allow_network`.
 */
export function useResetNetworkPolicyMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: () => resetNetworkPolicy(),
    onSuccess: () => {
      startInvalidation(queryCache, { key: queryKeys.appSettings() });
      startInvalidation(queryCache, { key: queryKeys.notifications() });
      startInvalidation(queryCache, { key: queryKeys.recommendedRulesStep() });
    },
  });
}

/**
 * Turns on every recommended rule-database source (Rust decides which).
 * Invalidates the settings, the Databases card and the notice list: the
 * recommended-sources notice derives from all three.
 */
export function useEnableRecommendedSourcesMutation() {
  const queryCache = useQueryCache();
  return useMutation({
    mutation: () => enableRecommendedRuleDatabases(),
    onSuccess: () => {
      startInvalidation(queryCache, { key: queryKeys.appSettings() });
      startInvalidation(queryCache, { key: queryKeys.ruleDatabases() });
      startInvalidation(queryCache, { key: queryKeys.notifications() });
    },
  });
}
