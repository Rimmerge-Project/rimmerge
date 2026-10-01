<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import GuidedFlowStrip from "@/components/dashboard/GuidedFlowStrip.vue";
import ScanNoteList from "@/components/mods/ScanNoteList.vue";
import WelcomeCard from "@/components/notifications/WelcomeCard.vue";
import ImportedLogSummary from "@/components/startup/ImportedLogSummary.vue";
import ImportGameLogButton from "@/components/startup/ImportGameLogButton.vue";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useDashboardQuery } from "@/queries/dashboard";
import { useSessionStore } from "@/stores/session";
import type { FindingKindDto } from "@/types/generated/FindingKindDto";
import { describeCommandError } from "@/utils/errors";
import { findingKindLabel } from "@/utils/finding";
import { orderSourceLabel } from "@/utils/orderSource";
import { describeSortProvenance } from "@/utils/sortProvenance";

const { t } = useI18n();
const tm = useTranslateMessage();
const { data, isPending, error } = useDashboardQuery();
const session = useSessionStore();

/** The finding kinds with the most `needsInput` entries, most first, at most 5. */
const topNeedsInputKinds = computed(() => {
  if (!data.value) {
    return [];
  }
  const counts = data.value.needsInputByKind;
  return (Object.keys(counts) as FindingKindDto[])
    .map((kind) => ({ kind, count: counts[kind] }))
    .filter((entry) => entry.count > 0)
    .toSorted((a, b) => b.count - a.count)
    .slice(0, 5);
});

/** The dashboard query's own failure, localized by `CommandErrorCode` — see `error.code.*`/`error.generic.*`. `null` while the query is pending or has no error. */
const loadError = computed(() => (error.value ? describeCommandError(error.value) : null));
</script>

<template>
  <div class="mx-auto flex max-w-6xl flex-col gap-6 p-6">
    <div class="flex items-center justify-between gap-2">
      <h1 class="text-text text-xl font-semibold">
        {{ t("dashboard.title") }}
      </h1>
      <ImportGameLogButton />
    </div>

    <ImportedLogSummary />

    <WelcomeCard />

    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="dashboard-loading"
    >
      {{ t("common.loading") }}
    </p>
    <div
      v-else-if="loadError"
      class="text-status-danger text-sm"
      data-testid="dashboard-error"
    >
      <div class="font-medium">
        {{ tm(loadError.title) }}
      </div>
      <div>{{ tm(loadError.detail) }}</div>
      <div
        v-if="loadError.technicalDetail"
        class="text-xs opacity-75"
      >
        {{ loadError.technicalDetail }}
      </div>
    </div>

    <template v-else-if="data">
      <GuidedFlowStrip :data="data" />

      <div
        class="grid grid-cols-2 gap-4 sm:grid-cols-3"
        data-testid="dashboard-counts"
      >
        <div class="surface-card p-4">
          <div class="text-text-muted text-sm">
            {{ t("dashboard.counts.mods") }}
          </div>
          <div
            class="text-text text-[28px] leading-none font-semibold"
            data-testid="mod-count"
          >
            {{ data.modCount }}
          </div>
        </div>
        <div class="surface-card p-4">
          <div class="text-text-muted text-sm">
            {{ t("dashboard.counts.needsInputCurrent") }}
          </div>
          <div
            class="text-text text-[28px] leading-none font-semibold"
            data-testid="needs-input-current"
          >
            {{ data.ledgerStats.current.needsInput }}
          </div>
        </div>
        <div class="surface-card p-4">
          <div class="text-text-muted text-sm">
            {{ t("dashboard.counts.needsInputSuggested") }}
          </div>
          <div
            class="text-text text-[28px] leading-none font-semibold"
            data-testid="needs-input-suggested"
          >
            {{ data.ledgerStats.suggested.needsInput }}
          </div>
        </div>
        <div class="surface-card p-4">
          <div class="text-text-muted text-sm">
            {{ t("dashboard.counts.movedMods") }}
          </div>
          <div
            class="text-text text-[28px] leading-none font-semibold"
            data-testid="moved-mods"
          >
            {{ data.movedMods }}
          </div>
          <div
            class="text-text-faint text-xs"
            data-testid="moved-mods-provenance"
          >
            {{ describeSortProvenance(data.sortProvenance, t) }}
          </div>
        </div>
        <div class="surface-card p-4">
          <div class="text-text-muted text-sm">
            {{ t("dashboard.counts.selectedOrder") }}
          </div>
          <div
            class="text-text text-[28px] leading-none font-semibold"
            data-testid="selected-order"
          >
            {{ tm(orderSourceLabel(data.selected)) }}
          </div>
        </div>
      </div>

      <section aria-labelledby="ledger-stats-heading">
        <h2
          id="ledger-stats-heading"
          class="text-text mb-2 text-base font-semibold"
        >
          {{ t("dashboard.ledgerStatsHeading") }}
        </h2>
        <div class="surface-card overflow-hidden">
          <table
            class="w-full border-collapse px-3 text-sm"
            data-testid="ledger-stats-table"
          >
            <thead>
              <tr class="table-head text-left">
                <th
                  scope="col"
                  class="py-2 pl-3 font-medium"
                />
                <th
                  scope="col"
                  class="py-2 font-medium"
                >
                  {{ t("dashboard.table.auto") }}
                </th>
                <th
                  scope="col"
                  class="py-2 font-medium"
                >
                  {{ t("dashboard.table.needsInput") }}
                </th>
                <th
                  scope="col"
                  class="py-2 font-medium"
                >
                  {{ t("dashboard.table.overridden") }}
                </th>
                <th
                  scope="col"
                  class="py-2 pr-3 font-medium"
                >
                  {{ t("dashboard.table.resolvedBySuggested") }}
                </th>
              </tr>
            </thead>
            <tbody>
              <tr class="border-border-subtle border-b">
                <th
                  scope="row"
                  class="py-2 pl-3 text-left font-medium"
                >
                  {{ t("dashboard.table.current") }}
                </th>
                <td>{{ data.ledgerStats.current.auto }}</td>
                <td>{{ data.ledgerStats.current.needsInput }}</td>
                <td>{{ data.ledgerStats.current.overridden }}</td>
                <td class="pr-3">
                  {{ data.ledgerStats.current.resolvedBySuggested }}
                </td>
              </tr>
              <tr>
                <th
                  scope="row"
                  class="py-2 pl-3 text-left font-medium"
                >
                  {{ t("dashboard.table.suggested") }}
                </th>
                <td>{{ data.ledgerStats.suggested.auto }}</td>
                <td>{{ data.ledgerStats.suggested.needsInput }}</td>
                <td>{{ data.ledgerStats.suggested.overridden }}</td>
                <td class="pr-3">
                  {{ data.ledgerStats.suggested.resolvedBySuggested }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </section>

      <section aria-labelledby="top-kinds-heading">
        <h2
          id="top-kinds-heading"
          class="text-text mb-2 text-base font-semibold"
        >
          {{ t("dashboard.topKindsHeading") }}
        </h2>
        <ul
          v-if="topNeedsInputKinds.length > 0"
          class="flex flex-col gap-1"
          data-testid="top-needs-input-kinds"
        >
          <li
            v-for="entry in topNeedsInputKinds"
            :key="entry.kind"
            class="surface-card flex items-center justify-between px-3 py-1.5 text-sm"
            :data-testid="`needs-input-kind-${entry.kind}`"
          >
            <span class="text-text">{{ tm(findingKindLabel(entry.kind)) }}</span>
            <span class="text-status-input font-semibold">{{ entry.count }}</span>
          </li>
        </ul>
        <p
          v-else
          class="text-text-muted text-sm"
          data-testid="top-needs-input-kinds-empty"
        >
          {{ t("dashboard.topKindsEmpty") }}
        </p>
      </section>

      <section
        v-if="session.scanNotes.length > 0"
        class="surface-card p-4"
      >
        <details data-testid="scan-notes-disclosure">
          <summary class="text-text cursor-pointer text-base font-semibold">
            {{ t("dashboard.scanNotesDisclosure", { count: session.scanNotes.length }) }}
          </summary>
          <div class="mt-2">
            <ScanNoteList :notes="session.scanNotes" />
          </div>
        </details>
      </section>
    </template>
  </div>
</template>
