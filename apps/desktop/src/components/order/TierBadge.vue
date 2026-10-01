<script setup lang="ts">
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { TierDto } from "@/types/generated/TierDto";
import { tierLabel } from "@/utils/tier";

const { tier } = defineProps<{ tier: TierDto }>();
const tm = useTranslateMessage();
</script>

<template>
  <span
    class="rounded-full px-2 py-0.5 text-xs font-medium"
    :class="{
      'bg-surface-2 text-text': tier === 'core' || tier === 'dlc',
      'bg-accent-soft text-accent': tier === 'top',
      'border-border-subtle bg-surface-1 text-text-muted border': tier === 'body',
      'border-border-subtle bg-surface-1 text-text-faint border': tier === 'bottom',
    }"
    :data-testid="`tier-badge-${tier}`"
  >
    {{ tm(tierLabel(tier)) }}
  </span>
</template>
