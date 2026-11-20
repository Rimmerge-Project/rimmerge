<script setup lang="ts">
import Toast from "primevue/toast";

import { useSessionEvents } from "@/composables/useSessionEvents";
import { ORDER_SHARE_TOAST_GROUP } from "@/utils/orderShare";

useSessionEvents();
</script>

<template>
  <RouterView />
  <!-- A toast's detail may hold several lines (an action's extra counts); keep the line breaks. -->
  <Toast :pt="{ detail: { class: 'whitespace-pre-line' } }" />
  <!-- The Load order page header's Export and Import menus sit top-right, where the default
       toasts land; sharing a load order reports in its own group at the bottom instead. -->
  <Toast
    :group="ORDER_SHARE_TOAST_GROUP"
    position="bottom-right"
  />
</template>
