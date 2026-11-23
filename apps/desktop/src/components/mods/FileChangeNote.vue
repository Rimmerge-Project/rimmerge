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
import { useBannerAnnouncement } from "@/composables/useBannerAnnouncement";
import { useOrderShare } from "@/composables/useOrderShare";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { usePendingActiveChangesQuery } from "@/queries/activeSet";
import { useFileBaselineQuery } from "@/queries/fileBaseline";
import { assertNever } from "@/utils/assertNever";
import { staleBaselineMessage } from "@/utils/fileBaseline";
import { hasOwnPendingChanges } from "@/utils/pendingActiveChanges";
import { QUIET_MESSAGE_PT } from "@/utils/quietMessage";

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
// Whole sentences per branch: "your pending changes are kept" is only said when there are some,
// and "applying would overwrite" only while the file does not already hold the selected order
// (the backend's `fileMatchesSelected`: then an Apply overwrites nothing and asks nothing).
const outsideText = computed<string | null>(() => {
  const current = baseline.value;
  if (current === undefined) {
    return null;
  }
  const change = current.externalChange;
  const hasPending = pending.value !== undefined && hasOwnPendingChanges(pending.value);
  switch (change.kind) {
    case "none":
      return null;
    case "membership": {
      const counts = { added: change.added, removed: change.removed };
      if (current.fileMatchesSelected) {
        return hasPending
          ? t("mods.fileChange.outsideMatches", counts)
          : t("mods.fileChange.outsideMatchesNoPending", counts);
      }
      return hasPending
        ? t("mods.fileChange.outside", counts)
        : t("mods.fileChange.outsideNoPending", counts);
    }
    case "reordered":
      if (current.fileMatchesSelected) {
        return hasPending
          ? t("mods.fileChange.reorderedMatches")
          : t("mods.fileChange.reorderedMatchesNoPending");
      }
      return hasPending ? t("mods.fileChange.reordered") : t("mods.fileChange.reorderedNoPending");
    default:
      return assertNever(change);
  }
});
const staleText = computed<string | null>(() =>
  staleReason.value === null ? null : tm(staleBaselineMessage(staleReason.value)),
);
useBannerAnnouncement("staleBaseline", staleText);
useBannerAnnouncement("outsideNote", outsideText);
// Unknown until the pending changes answer: no claim is made about an order we cannot see.
const replacesUnwritten = computed(() => pending.value?.orderUnapplied === true);
</script>

<template>
  <Message
    v-if="staleReason !== null"
    severity="warn"
    class="shrink-0"
    :pt="QUIET_MESSAGE_PT"
    data-testid="file-baseline-stale"
  >
    {{ staleText }}
  </Message>
  <Message
    v-if="outsideText !== null"
    severity="info"
    class="shrink-0"
    :pt="QUIET_MESSAGE_PT"
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
