<script setup lang="ts">
import Button from "primevue/button";
import Message from "primevue/message";
import ToggleSwitch from "primevue/toggleswitch";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useRefreshRuleDatabasesMutation, useRuleDatabasesQuery } from "@/queries/rules";
import { useAppSettingsQuery, useUpdateAppSettingsMutation } from "@/queries/settings";
import type { RuleDatabaseDto } from "@/types/generated/RuleDatabaseDto";
import type { RuleDatabaseRefreshResultDto } from "@/types/generated/RuleDatabaseRefreshResultDto";
import { assertNever } from "@/utils/assertNever";
import { describeFetchFailure, fetchFailureTechnicalDetail } from "@/utils/fetchFailure";
import {
  DATABASE_LABEL_KEYS,
  formatBytes,
  formatDatabaseStatusLine,
  isAutoRefreshEligible,
  sha12,
} from "@/utils/ruleDatabases";

const { t, locale } = useI18n();
const tm = useTranslateMessage();
const { data: views, isPending, error } = useRuleDatabasesQuery();
const { data: appSettings } = useAppSettingsQuery();
const { mutate: saveAppSettings } = useUpdateAppSettingsMutation();
const { mutateAsync: runRefresh, isLoading: isRefreshing } = useRefreshRuleDatabasesMutation();

/**
 * "Automatic"/"manual only" label per source, for the row's own status
 * line — a source refreshes automatically only when the app-global
 * toggle is on *and* the source itself is eligible (never Steam
 * Workshop, per its own size).
 */
function refreshModeLabel(database: RuleDatabaseDto): string {
  const automatic =
    (appSettings.value?.settings.network.autoRefreshRuleDatabases ?? true) &&
    isAutoRefreshEligible(database);
  return automatic
    ? t("rules.databases.refreshModeAutomatic")
    : t("rules.databases.refreshModeManual");
}

// Defaults `true` for the brief window before the app-settings query
// resolves, matching `RulesPage.vue`'s own `useImportedPairs`/
// `useImportedPlacements` convention.
const allowNetworkRefresh = computed(
  () => appSettings.value?.settings.network.allowNetwork ?? true,
);

function toggleFetch(database: RuleDatabaseDto, enabled: boolean): void {
  if (!appSettings.value) return;
  const network = appSettings.value.settings.network;
  saveAppSettings({
    ...appSettings.value.settings,
    network: {
      ...network,
      fetchCommunityRules: database === "community" ? enabled : network.fetchCommunityRules,
      fetchSteamWorkshop: database === "steam" ? enabled : network.fetchSteamWorkshop,
      fetchRimmergeRules: database === "rimmerge" ? enabled : network.fetchRimmergeRules,
    },
  });
}

const anyLastFailure = computed(() => views.value?.some((view) => view.lastFailure) ?? false);

// The per-source result summary shown once a refresh finishes — cleared
// the moment a new refresh starts, so a stale summary from a previous
// click never lingers next to a fresh one still in flight.
const lastRefreshResults = ref<RuleDatabaseRefreshResultDto[] | null>(null);

function describeOutcome(result: RuleDatabaseRefreshResultDto): string {
  const outcome = result.outcome;
  switch (outcome.kind) {
    case "updated":
      return t("rules.databases.outcomeUpdated", {
        bytes: formatBytes(outcome.bytes),
        sha: sha12(outcome.sha256),
      });
    case "unchanged":
      return t("rules.databases.outcomeUnchanged", { sha: sha12(outcome.sha256) });
    case "failed":
      return t("rules.databases.outcomeFailed", {
        reason: tm(describeFetchFailure(outcome.failure, locale.value)),
      });
    case "skipped": {
      const reason = outcome.reason;
      switch (reason) {
        case "sourceDisabled":
          return t("rules.databases.outcomeSkippedSourceDisabled");
        case "networkDisabled":
          return t("rules.databases.outcomeSkippedNetworkDisabled");
        case "autoRefreshDisabled":
          return t("rules.databases.outcomeSkippedAutoRefreshDisabled");
        case "notDue":
          return t("rules.databases.outcomeSkippedNotDue");
        default:
          return assertNever(reason);
      }
    }
    default:
      return assertNever(outcome);
  }
}

/** The technical-details line of a failed refresh, `null` for any other outcome. */
function outcomeTechnicalDetail(result: RuleDatabaseRefreshResultDto): string | null {
  return result.outcome.kind === "failed"
    ? fetchFailureTechnicalDetail(result.outcome.failure)
    : null;
}

async function refresh(): Promise<void> {
  lastRefreshResults.value = null;
  lastRefreshResults.value = await runRefresh(null);
}

// Read once here rather than inside the template
// (`formatDatabaseStatusLine(view, new Date())` would read the clock on
// every render instead of once at a composition-root edge); the status
// line only ever needs "now" to render a relative "N days ago", not to
// re-render live as time passes, so no ticking interval is warranted.
const now = new Date();
</script>

<template>
  <section
    aria-labelledby="rule-databases-heading"
    class="border-border-subtle flex flex-col gap-3 rounded border p-4"
    data-testid="rule-databases-card"
  >
    <div class="flex items-center justify-between">
      <h2
        id="rule-databases-heading"
        class="text-text text-base font-semibold"
      >
        {{ t("rules.databases.heading") }}
      </h2>
      <!-- A native `title` on the `Button` itself was unreliable (a
           `disabled` control can suppress hover/pointer events entirely
           in some browsers) and keyboard-unreachable (a `disabled`
           element drops out of the tab order, so a keyboard user could
           never discover the tooltip at all). The tooltip now lives on
           this wrapper instead — never disabled, so
           it always receives hover, and only gains `tabindex="0"`
           precisely when the button itself is inert and there is
           something worth announcing. -->
      <span
        :tabindex="allowNetworkRefresh ? undefined : 0"
        :title="allowNetworkRefresh ? undefined : t('rules.databases.refreshDisabledTitle')"
        data-testid="rule-databases-refresh-tooltip"
      >
        <Button
          :label="t('rules.databases.refreshButton')"
          size="small"
          :loading="isRefreshing"
          :disabled="!allowNetworkRefresh"
          data-testid="rule-databases-refresh"
          @click="refresh"
        />
      </span>
    </div>

    <p
      v-if="!allowNetworkRefresh"
      class="text-text-muted text-xs"
      data-testid="rule-databases-network-disabled-notice"
    >
      {{ t("rules.databases.networkDisabledNotice") }}
    </p>

    <Message
      v-if="anyLastFailure"
      severity="warn"
      :closable="false"
      data-testid="rule-databases-failure-banner"
    >
      {{ t("rules.databases.failureBanner") }}
    </Message>

    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="rule-databases-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="rule-databases-error"
    >
      {{ error instanceof Error ? error.message : t("rules.databases.loadFailed") }}
    </p>
    <ul
      v-else-if="views"
      class="flex flex-col gap-2"
    >
      <li
        v-for="view in views"
        :key="view.database"
        class="flex items-center justify-between gap-4 text-sm"
        :data-testid="`rule-database-row-${view.database}`"
      >
        <span class="flex flex-col">
          <span class="text-text font-medium">
            {{ t(DATABASE_LABEL_KEYS[view.database]) }}
            <span
              v-if="view.needsReimport"
              class="text-status-warn ml-1 text-xs font-normal"
              :data-testid="`rule-database-needs-reimport-${view.database}`"
            >
              {{ t("rules.databases.needsReimport") }}
            </span>
          </span>
          <span
            class="text-text-muted text-xs"
            :data-testid="`rule-database-status-${view.database}`"
          >
            {{ formatDatabaseStatusLine(view, now, t, locale) }}
          </span>
          <span
            v-if="view.lastFailure && fetchFailureTechnicalDetail(view.lastFailure)"
            class="text-text-faint font-mono text-xs break-all select-text"
            :data-testid="`rule-database-failure-detail-${view.database}`"
          >
            {{ t("common.technicalDetail", { detail: fetchFailureTechnicalDetail(view.lastFailure) }) }}
          </span>
          <span
            class="text-text-faint text-xs"
            :data-testid="`rule-database-refresh-mode-${view.database}`"
          >
            {{ refreshModeLabel(view.database) }}
          </span>
        </span>
        <ToggleSwitch
          class="shrink-0"
          :model-value="view.enabled"
          :data-testid="`rule-database-toggle-${view.database}`"
          @update:model-value="(enabled: boolean) => toggleFetch(view.database, enabled)"
        />
      </li>
    </ul>

    <ul
      v-if="lastRefreshResults"
      class="text-text-muted flex flex-col gap-1 text-xs"
      data-testid="rule-databases-refresh-results"
    >
      <li
        v-for="result in lastRefreshResults"
        :key="result.database"
        :data-testid="`rule-database-refresh-result-${result.database}`"
      >
        {{ t(DATABASE_LABEL_KEYS[result.database]) }}: {{ describeOutcome(result) }}
        <span
          v-if="outcomeTechnicalDetail(result)"
          class="text-text-faint block font-mono break-all select-text"
          :data-testid="`rule-database-refresh-result-detail-${result.database}`"
        >
          {{ t("common.technicalDetail", { detail: outcomeTechnicalDetail(result) }) }}
        </span>
      </li>
    </ul>
  </section>
</template>
