<script setup lang="ts">
const { label } = defineProps<{ label: string }>();
</script>

<template>
  <kbd
    class="border-border-subtle bg-surface-2 text-text-muted rounded border px-1.5 py-0.5 font-mono text-xs"
  >{{ label }}</kbd>
</template>
