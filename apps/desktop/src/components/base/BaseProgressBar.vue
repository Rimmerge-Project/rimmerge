<script lang="ts">
/**
 * How far along a job is. The two arms are exclusive by construction, so
 * a caller can't hand over a half-filled `done`-without-`total` pair, or
 * a `percent` that a `done`/`total` pair would silently override.
 */
export type Progress = { done: number; total: number } | { percent: number };
</script>

<script setup lang="ts">
import ProgressBar from "primevue/progressbar";
import { computed } from "vue";

import { formatPercentLabel } from "@/utils/format";

const { progress, label, caption = null } = defineProps<{
  progress: Progress;
  /** Accessible name for the bar, e.g. `"Scanning mods"` — `role="progressbar"` has none otherwise. */
  label: string;
  /** Stage text rendered under the bar. */
  caption?: string | null;
}>();

/**
 * `data-testid` and friends belong on the element that actually carries
 * `role="progressbar"`, not on this wrapper — a test or a screen reader
 * looking up `setup-progress` should land on the bar itself.
 */
defineOptions({ inheritAttrs: false });

/**
 * PrimeVue's own stylesheet sets `transition: width 1s ease-in-out` on
 * the determinate fill (`@primeuix/styles/progressbar`). A real scan
 * emits ~1000 per-mod ticks in a couple of seconds, so the fill spends
 * the whole run a second behind the label rendered inside it — the bar
 * reads "99%" while sitting a third of the way across. An inline style
 * through `pt` beats the stylesheet rule without an `!important`.
 */
const PASS_THROUGH = { value: { style: { transition: "width 120ms linear" } } } as const;

/**
 * Always a real number in 0..=100: a job with nothing to do yet
 * (`total: 0`), and any non-finite arithmetic behind it, read as 0
 * rather than reaching `aria-valuenow` and the fill width as `NaN`.
 */
const percent = computed(() => {
  const raw = "percent" in progress ? progress.percent : ratioPercent(progress);
  if (!Number.isFinite(raw)) {
    return 0;
  }
  return Math.min(100, Math.max(0, raw));
});

const percentLabel = computed(() => formatPercentLabel(percent.value));

function ratioPercent({ done, total }: { done: number; total: number }): number {
  return total > 0 ? (done / total) * 100 : 0;
}
</script>

<template>
  <div class="flex shrink-0 flex-col gap-1">
    <ProgressBar
      v-bind="$attrs"
      :value="percent"
      :pt="PASS_THROUGH"
      :aria-label="label"
    >
      {{ percentLabel }}
    </ProgressBar>
    <span
      v-if="caption"
      class="text-text-muted text-xs"
      data-testid="base-progress-caption"
    >
      {{ caption }}
    </span>
  </div>
</template>
