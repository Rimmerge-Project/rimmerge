<script setup lang="ts">
import Message from "primevue/message";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useSessionStore } from "@/stores/session";
import {
  coverageLines,
  describeGap,
  loggingGapCounts,
  logKindLabel,
  MAX_GAPS_LISTED,
  readLossLines,
} from "@/utils/gameLogCoverage";

/**
 * What the last imported game log is and what it covers: its kind
 * (`Player.log` or console snapshot), a one-line coverage statement, the
 * places where the game itself stopped logging, and anything the reader had
 * to bound. A log's counts are only as good as its coverage, so this is
 * shown wherever an import can be triggered — a snapshot must never read as
 * a whole session. Renders nothing until a log is imported.
 */

const { t } = useI18n();
const tm = useTranslateMessage();
const session = useSessionStore();

const summary = computed(() => session.gameLogSummary);
const coverage = computed(() => (summary.value ? coverageLines(summary.value.coverage) : []));
// The reader bounds the list it keeps; the gaps it dropped still happened, so
// the total counts them and the "and N more" line covers them too.
const droppedGapCount = computed(() => summary.value?.readStats.loggingGapsDropped ?? 0);
const gapCounts = computed(() => {
  const counts = loggingGapCounts(summary.value?.loggingGaps ?? []);
  return { ...counts, total: counts.total + droppedGapCount.value };
});
const listedGaps = computed(() => (summary.value?.loggingGaps ?? []).slice(0, MAX_GAPS_LISTED));
const unlistedGapCount = computed(() => gapCounts.value.total - listedGaps.value.length);
const losses = computed(() => (summary.value ? readLossLines(summary.value.readStats) : []));
</script>

<template>
  <section
    v-if="summary"
    class="surface-card flex shrink-0 flex-col gap-2 p-3 text-sm"
    data-testid="imported-log-summary"
  >
    <div class="flex flex-wrap items-baseline gap-x-2">
      <span class="text-text-muted">{{ t("gameLog.heading") }}</span>
      <span
        class="text-text font-medium"
        data-testid="imported-log-kind"
      >{{ tm(logKindLabel(summary.coverage)) }}</span>
    </div>

    <p
      v-for="(line, index) in coverage"
      :key="line.key"
      class="text-text-muted"
      :data-testid="index === 0 ? 'imported-log-coverage' : 'imported-log-coverage-note'"
    >
      {{ tm(line) }}
    </p>

    <Message
      v-if="gapCounts.total > 0"
      severity="warn"
      data-testid="imported-log-gaps-warning"
    >
      <p>{{ t("gameLog.gaps.warning", { count: gapCounts.total }, gapCounts.total) }}</p>
      <p v-if="gapCounts.neverResumed > 0">
        {{
          t(
            "gameLog.gaps.neverResumed",
            { count: gapCounts.neverResumed },
            gapCounts.neverResumed,
          )
        }}
      </p>
      <p v-if="summary.lowerBound">
        {{
          t(
            "gameLog.gaps.lowerBound",
            { count: summary.lowerBound.families, percent: summary.lowerBound.entriesPercent },
            summary.lowerBound.families,
          )
        }}
      </p>
      <ul
        class="mt-1 list-disc pl-5 text-xs"
        data-testid="imported-log-gaps-list"
      >
        <li
          v-for="gap in listedGaps"
          :key="gap.stopLine"
        >
          {{ tm(describeGap(gap)) }}
        </li>
        <li v-if="unlistedGapCount > 0">
          {{ t("gameLog.gaps.more", { count: unlistedGapCount }) }}
        </li>
      </ul>
    </Message>

    <Message
      v-if="losses.length > 0"
      severity="warn"
      data-testid="imported-log-losses"
    >
      <p class="font-medium">
        {{ t("gameLog.losses.heading") }}
      </p>
      <ul class="list-disc pl-5 text-xs">
        <li
          v-for="loss in losses"
          :key="loss.key"
        >
          {{ tm(loss) }}
        </li>
      </ul>
    </Message>
  </section>
</template>
