<script setup lang="ts">
// The import preview: what a shared list would do on this install, then "Use this order".
// A document that cannot be read shows its reason and only a Close button. "Use this order"
// is enabled exactly when the backend sent no `importBlocked` reason; the dialog never re-derives it.
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import Message from "primevue/message";
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import BaseProgressBar from "@/components/base/BaseProgressBar.vue";
import ImportPreviewBody from "@/components/order/ImportPreviewBody.vue";
import { useTauriEvent } from "@/composables/useTauriEvent";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { OrderImportOutcomeDto } from "@/types/generated/OrderImportOutcomeDto";
import type { ProgressEventDto } from "@/types/generated/ProgressEventDto";
import { groupImportEntries, missingListText, rejectionMessage } from "@/utils/orderShare";

const { outcome, isImporting, replacesUnwritten } = defineProps<{
  outcome: OrderImportOutcomeDto | null;
  isImporting: boolean;
  /** This import replaces the Current order, which is not in ModsConfig.xml yet. */
  replacesUnwritten: boolean;
}>();
const visible = defineModel<boolean>("visible", { required: true });
const emit = defineEmits<{
  confirm: [order: readonly string[]];
  openWorkshop: [workshopId: number];
  copyMissing: [text: string];
}>();

const { t } = useI18n();
const tm = useTranslateMessage();

const progress = ref<ProgressEventDto | null>(null);
useTauriEvent<ProgressEventDto>("project://progress", (payload) => {
  progress.value = payload;
});
// A previous import's last tick must not flash at the start of the next one, so it is cleared
// when an import ends. Clearing at the start would race the first tick: the prop reaches this
// component only after the parent re-renders, by which time the import's first tick (it is
// called straight away) has often already arrived.
watch(
  () => isImporting,
  (isRunning) => {
    if (!isRunning) {
      progress.value = null;
    }
  },
);
const showsProgress = computed(
  () =>
    isImporting &&
    progress.value !== null &&
    progress.value.total > 0 &&
    progress.value.done < progress.value.total,
);

const missingText = computed(() =>
  outcome?.kind === "ready"
    ? missingListText(groupImportEntries(outcome.preview.entries).notInstalled)
    : "",
);

function close(): void {
  visible.value = false;
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="t('orderShare.preview.title')"
    :closable="!isImporting"
    :close-on-escape="!isImporting"
    class="w-[40rem] max-w-[95vw]"
    data-testid="import-preview-dialog"
  >
    <template v-if="outcome !== null">
      <div
        v-if="outcome.kind === 'rejected'"
        class="flex flex-col gap-4"
        data-testid="import-preview-rejected"
      >
        <Message severity="error">
          <div class="font-medium">
            {{ t("orderShare.rejected.title") }}
          </div>
          <div data-testid="import-preview-rejected-reason">
            {{ tm(rejectionMessage(outcome.reason)) }}
          </div>
        </Message>
        <div class="flex justify-end">
          <Button
            :label="t('orderShare.preview.close')"
            data-testid="import-preview-close"
            autofocus
            @click="close"
          />
        </div>
      </div>

      <div
        v-else
        class="flex flex-col gap-4"
      >
        <ImportPreviewBody
          :preview="outcome.preview"
          :replaces-unwritten="replacesUnwritten"
          @open-workshop="emit('openWorkshop', $event)"
        />
        <BaseProgressBar
          v-if="showsProgress && progress"
          :progress="{ done: progress.done, total: progress.total }"
          :label="t('orderShare.preview.importing')"
          data-testid="import-preview-progress"
        />
        <div class="flex flex-wrap justify-end gap-2">
          <Button
            v-if="missingText !== ''"
            :label="t('orderShare.preview.copyMissing')"
            severity="secondary"
            class="mr-auto whitespace-nowrap"
            data-testid="import-preview-copy-missing"
            @click="emit('copyMissing', missingText)"
          />
          <Button
            :label="t('common.cancel')"
            severity="secondary"
            :disabled="isImporting"
            data-testid="import-preview-cancel"
            @click="close"
          />
          <Button
            :label="t('orderShare.preview.confirm')"
            :disabled="outcome.preview.importBlocked !== null || isImporting"
            :loading="isImporting"
            data-testid="import-preview-confirm"
            @click="emit('confirm', outcome.preview.order)"
          />
        </div>
      </div>
    </template>
  </Dialog>
</template>
