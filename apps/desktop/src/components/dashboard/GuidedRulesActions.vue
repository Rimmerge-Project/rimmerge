<script setup lang="ts">
// Step 1's controls: the primary click and a real Skip button while it is offered, "Get them
// now" once skipped. `unavailable` offers no click (Skip stays); `done` offers nothing.
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { RecommendedRulesStepDto } from "@/types/generated/RecommendedRulesStepDto";
import { rulesClickLabel } from "@/utils/recommendedRules";

const { rules, isSkipping, runLabel } = defineProps<{
  /** `null` until the step query answers. */
  rules: RecommendedRulesStepDto | null;
  isSkipping: boolean;
  /** The label of the click this window is running, kept so the button does not change mid-run. */
  runLabel: MessageDescriptor | null;
}>();

const emit = defineEmits<{ get: []; skip: [] }>();

const { t } = useI18n();
const tm = useTranslateMessage();

/** The offered click's label from the step's sources (what the click would do). */
const offeredLabel = computed(() => {
  const label = rules === null ? null : rulesClickLabel(rules);
  return label === null ? "" : tm(label);
});

/** The running button keeps the wording it started with; a run from another window has none. */
const runningLabel = computed(() =>
  tm(runLabel ?? descriptor("dashboard.guide.getRules.downloadButton")),
);
</script>

<template>
  <div
    v-if="rules !== null"
    class="flex flex-wrap items-center justify-end gap-2"
  >
    <template v-if="rules.kind === 'needsAction'">
      <Button
        :label="offeredLabel"
        data-testid="guide-rules-button"
        @click="emit('get')"
      />
      <button
        type="button"
        class="text-text-muted hover:text-text cursor-pointer text-sm underline"
        :disabled="isSkipping"
        data-testid="guide-rules-skip"
        @click="emit('skip')"
      >
        {{ t("dashboard.guide.getRules.skip") }}
      </button>
    </template>
    <Button
      v-else-if="rules.kind === 'inProgress'"
      :label="runningLabel"
      loading
      disabled
      data-testid="guide-rules-button"
    />
    <button
      v-else-if="rules.kind === 'unavailable'"
      type="button"
      class="text-text-muted hover:text-text cursor-pointer text-sm underline"
      :disabled="isSkipping"
      data-testid="guide-rules-skip"
      @click="emit('skip')"
    >
      {{ t("dashboard.guide.getRules.skip") }}
    </button>
    <Button
      v-else-if="rules.kind === 'skipped'"
      :label="offeredLabel"
      severity="secondary"
      data-testid="guide-rules-get-now"
      @click="emit('get')"
    />
  </div>
</template>
