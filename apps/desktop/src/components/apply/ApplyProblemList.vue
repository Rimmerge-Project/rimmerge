<script setup lang="ts">
// One hard problem per row, as a real list: an icon plus the sentence, never colour alone.
import { useI18n } from "vue-i18n";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import type { PreflightItemDto } from "@/types/generated/PreflightItemDto";
import { describeHardProblem, hardProblemKey } from "@/utils/preflight";

defineProps<{ items: readonly PreflightItemDto[] }>();

const { locale } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();

function describe(item: PreflightItemDto) {
  return describeHardProblem(item.problem, {
    label: modLabel.label,
    list: (labels) => formatList(locale.value, labels),
  });
}
</script>

<template>
  <ul class="flex flex-col gap-1 text-sm">
    <li
      v-for="item in items"
      :key="hardProblemKey(item.problem)"
      class="flex items-start gap-2"
      data-testid="apply-problem-item"
    >
      <i
        :class="[
          'pi mt-0.5 shrink-0 text-xs',
          item.acknowledged ? 'pi-check text-status-auto' : 'pi-exclamation-triangle text-status-input',
        ]"
        aria-hidden="true"
      />
      <span class="text-text">{{ tm(describe(item)) }}</span>
    </li>
  </ul>
</template>
