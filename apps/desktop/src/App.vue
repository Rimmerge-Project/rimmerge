<script setup lang="ts">
import Toast from "primevue/toast";

import { useSessionEvents } from "@/composables/useSessionEvents";

useSessionEvents();
</script>

<template>
  <RouterView />
  <Toast />
</template>
