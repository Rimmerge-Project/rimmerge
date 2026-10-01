<script setup lang="ts">
const { confidence } = defineProps<{ confidence: number }>();
</script>

<template>
  <span
    class="text-text-muted inline-flex items-center gap-1 text-xs"
    data-testid="confidence"
  >
    <span
      class="bg-surface-2 h-1.5 w-10 overflow-hidden rounded-full"
      role="progressbar"
      :aria-valuenow="confidence"
      aria-valuemin="0"
      aria-valuemax="100"
    >
      <span
        class="bg-accent block h-full"
        :style="{ width: `${confidence}%` }"
      />
    </span>
    {{ confidence }}%
  </span>
</template>
