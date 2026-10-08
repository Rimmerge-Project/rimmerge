<script setup lang="ts">
// One collapsible section of the import preview: a heading with its count, then the rows.
// A native <details> keeps it keyboard-operable and announced as a disclosure.
const {
  heading,
  testId,
  isOpen = false,
} = defineProps<{
  heading: string;
  testId: string;
  isOpen?: boolean;
}>();
</script>

<template>
  <details
    :open="isOpen"
    class="border-border-subtle rounded border"
    :data-testid="testId"
  >
    <summary
      class="text-text cursor-pointer px-3 py-2 text-sm font-medium"
      :data-testid="`${testId}-heading`"
    >
      {{ heading }}
    </summary>
    <div class="border-border-subtle border-t px-3 py-2">
      <slot />
    </div>
  </details>
</template>
