<script setup lang="ts">
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { ResolutionStatusDto } from "@/types/generated/ResolutionStatusDto";
import { resolutionStatusLabel } from "@/utils/resolutionStatus";

const { status } = defineProps<{ status: ResolutionStatusDto }>();

const tm = useTranslateMessage();
</script>

<template>
  <span
    class="shrink-0 rounded-full px-2 py-0.5 text-xs font-medium whitespace-nowrap"
    :class="{
      'bg-status-auto-soft text-status-auto': status === 'auto',
      'bg-status-input-soft text-status-input': status === 'needsInput',
      'bg-status-override-soft text-status-override': status === 'userOverridden',
    }"
    :data-testid="`status-pill-${status}`"
  >
    {{ tm(resolutionStatusLabel(status)) }}
  </span>
</template>
