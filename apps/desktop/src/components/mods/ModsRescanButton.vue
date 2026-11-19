<script setup lang="ts">
import Button from "primevue/button";
import { ref } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useRescanMutation } from "@/queries/activeSet";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

const { t } = useI18n();
const tm = useTranslateMessage();
const { mutateAsync: rescan, isLoading: isRescanning } = useRescanMutation();
const error = ref<CommandErrorDescriptor | null>(null);

async function rescanNow(): Promise<void> {
  error.value = null;
  try {
    await rescan();
  } catch (caught: unknown) {
    error.value = describeCommandError(caught);
  }
}
</script>

<template>
  <div class="flex flex-col items-end gap-1">
    <Button
      :label="t('mods.page.rescanButton')"
      icon="pi pi-refresh"
      severity="secondary"
      outlined
      size="small"
      :loading="isRescanning"
      :disabled="isRescanning"
      data-testid="mods-rescan-button"
      @click="rescanNow"
    />
    <div
      v-if="error"
      class="text-status-danger max-w-sm text-right text-xs"
      role="alert"
      data-testid="mods-rescan-error"
    >
      <div class="font-medium">
        {{ tm(error.title) }}
      </div>
      <div>{{ tm(error.detail) }}</div>
      <div
        v-if="error.technicalDetail"
        class="opacity-75"
      >
        {{ error.technicalDetail }}
      </div>
    </div>
  </div>
</template>
