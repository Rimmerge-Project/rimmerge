<script setup lang="ts">
import Message from "primevue/message";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { CommandErrorDescriptor } from "@/utils/errors";

/** A failed organization edit, worded by its code, shown inside the dialog that asked for it. */
const { error } = defineProps<{ error: CommandErrorDescriptor }>();

const tm = useTranslateMessage();
</script>

<template>
  <Message
    severity="error"
    data-testid="organization-error"
  >
    <div class="font-medium">
      {{ tm(error.title) }}
    </div>
    <div>{{ tm(error.detail) }}</div>
    <div
      v-if="error.technicalDetail"
      class="text-xs opacity-75"
    >
      {{ error.technicalDetail }}
    </div>
  </Message>
</template>
