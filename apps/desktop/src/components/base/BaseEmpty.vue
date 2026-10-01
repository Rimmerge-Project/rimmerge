<script setup lang="ts">
// `message` is required, not defaulted: every call site already passes
// its own translated text (there is no locale-neutral "empty" wording
// to fall back to), and a silent English default here would be exactly
// the kind of hardcoded string this app's own localization convention
// forbids.
defineProps<{ message: string }>();
</script>

<template>
  <p
    class="text-text-faint p-4 text-sm"
    data-testid="empty-state"
  >
    {{ message }}
  </p>
</template>
