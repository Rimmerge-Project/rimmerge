<script setup lang="ts">
import Button from "primevue/button";
import Message from "primevue/message";
import { useToast } from "primevue/usetoast";
import { ref } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useImportGameLogMutation } from "@/queries/gameLog";
import { pickFile } from "@/services/dialogs";
import { useSessionStore } from "@/stores/session";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/**
 * The "Import game log" button — shared by the `/startup` page and
 * the dashboard so both trigger the identical flow: pick a game log (a
 * `Player.log` or a console snapshot; the kind is detected from content),
 * import it, and store the result in {@link useSessionStore} for every
 * page that reads `session.gameLogSummary` to use.
 */

const { t } = useI18n();
const tm = useTranslateMessage();
const session = useSessionStore();
const { mutateAsync: runImport, isLoading } = useImportGameLogMutation();
const errorMessage = ref<CommandErrorDescriptor | null>(null);
const toast = useToast();

async function importPlayerLog(): Promise<void> {
  errorMessage.value = null;
  const path = await pickFile({
    logFiles: t("dashboard.importLog.logFilter"),
    allFiles: t("dashboard.importLog.allFilesFilter"),
  });
  if (path === null) {
    return;
  }
  try {
    const summary = await runImport(path);
    session.setGameLogSummary(summary);
    // A successful import confirms itself with a toast — `useToast` is this
    // app's own house pattern for exactly this (see `ApplyDialog.vue`).
    toast.add({ severity: "success", summary: t("dashboard.importLog.importedToast"), life: 4000 });
  } catch (error: unknown) {
    errorMessage.value = describeCommandError(error);
  }
}
</script>

<template>
  <div class="flex flex-col items-end gap-1">
    <Button
      :label="t('dashboard.importLog.button')"
      severity="secondary"
      :loading="isLoading"
      data-testid="import-game-log-button"
      @click="importPlayerLog"
    />
    <Message
      v-if="errorMessage"
      severity="error"
      data-testid="import-game-log-error"
    >
      <div class="font-medium">
        {{ tm(errorMessage.title) }}
      </div>
      <div>{{ tm(errorMessage.detail) }}</div>
      <div
        v-if="errorMessage.technicalDetail"
        class="text-xs opacity-75"
      >
        {{ errorMessage.technicalDetail }}
      </div>
    </Message>
  </div>
</template>
