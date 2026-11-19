<script setup lang="ts">
// Two notes about the active mod list on disk, above the Mods lists: it was changed outside
// Rimmerge (the one place an outside edit is explained: the close guard, the pending banner and
// the "not applied yet" notes only ever count your own work; the action opens the existing
// import preview with the file's list), and it cannot be read right now (the Added/Removed marks keep the last
// list that worked). Both are information: nothing here writes anything.
import Button from "primevue/button";
import Message from "primevue/message";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import ImportPreviewDialog from "@/components/order/ImportPreviewDialog.vue";
import { useOrderShare } from "@/composables/useOrderShare";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { usePendingActiveChangesQuery } from "@/queries/activeSet";
import { useFileBaselineQuery } from "@/queries/fileBaseline";
import { assertNever } from "@/utils/assertNever";
import { staleBaselineMessage } from "@/utils/fileBaseline";
import { hasOwnPendingChanges } from "@/utils/pendingActiveChanges";

const { t } = useI18n();
const tm = useTranslateMessage();
const { data: baseline } = useFileBaselineQuery();
const { data: pending } = usePendingActiveChangesQuery();
const {
  isBusy,
  isImporting,
  isPreviewOpen,
  outcome,
  previewModsConfig,
  confirmImport,
  openWorkshop,
  copyMissing,
} = useOrderShare();

const staleReason = computed(() => {
  const state = baseline.value?.state;
  return state?.kind === "stale" ? state.reason : null;
});
// Whole sentences per branch: "your pending changes are kept" is only said when there are some.
const outsideText = computed<string | null>(() => {
  const change = baseline.value?.externalChange;
  if (change === undefined) {
    return null;
  }
  const hasPending = pending.value !== undefined && hasOwnPendingChanges(pending.value);
  switch (change.kind) {
    case "none":
      return null;
    case "membership":
      return hasPending
        ? t("mods.fileChange.outside", { added: change.added, removed: change.removed })
        : t("mods.fileChange.outsideNoPending", { added: change.added, removed: change.removed });
    case "reordered":
      return hasPending ? t("mods.fileChange.reordered") : t("mods.fileChange.reorderedNoPending");
    default:
      return assertNever(change);
  }
});
// Unknown until the pending changes answer: no claim is made about an order we cannot see.
const replacesUnwritten = computed(() => pending.value?.orderUnapplied === true);
</script>

<template>
  <Message
    v-if="staleReason !== null"
    severity="warn"
    class="shrink-0"
    data-testid="file-baseline-stale"
  >
    {{ tm(staleBaselineMessage(staleReason)) }}
  </Message>
  <Message
    v-if="outsideText !== null"
    severity="info"
    class="shrink-0"
    data-testid="file-changed-outside"
  >
    <div class="flex flex-wrap items-center justify-between gap-3">
      <span data-testid="file-changed-outside-text">{{ outsideText }}</span>
      <Button
        :label="t('mods.fileChange.useFile')"
        :title="t('mods.fileChange.useFileTitle')"
        size="small"
        severity="secondary"
        :disabled="isBusy"
        data-testid="file-changed-outside-use"
        @click="previewModsConfig"
      />
    </div>
  </Message>
  <ImportPreviewDialog
    v-model:visible="isPreviewOpen"
    :outcome="outcome"
    :is-importing="isImporting"
    :replaces-unwritten="replacesUnwritten"
    @confirm="confirmImport"
    @open-workshop="openWorkshop"
    @copy-missing="copyMissing"
  />
</template>
