<script setup lang="ts">
// "This isn't your active order yet" with an Apply button: shown on the Load order page and the
// Mods page while the Current order is selected and Rimmerge has not written it to the file yet
// (after an import, after activation changes were rescanned, or after a move on the Mods page,
// which settles into Current without writing the file). Apply is the normal dialog; nothing is
// written here.
import Button from "primevue/button";
import Message from "primevue/message";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import ApplyDialog from "@/components/apply/ApplyDialog.vue";
import { useOrderSource } from "@/composables/useOrderSource";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { usePendingActiveChangesQuery, useRevertOrderToFileMutation } from "@/queries/activeSet";
import { useFileBaselineQuery } from "@/queries/fileBaseline";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/** The Mods page offers going back to the file's own order; the Load order page does not. */
const { allowRevert = false } = defineProps<{ allowRevert?: boolean }>();

const { t } = useI18n();
const tm = useTranslateMessage();
const { selected } = useOrderSource();
const { data: pending } = usePendingActiveChangesQuery();
const { data: baseline } = useFileBaselineQuery();
const { mutateAsync: revert, isLoading: isReverting } = useRevertOrderToFileMutation();
const revertError = ref<CommandErrorDescriptor | null>(null);

const isApplyOpen = ref(false);
// Going back to the file's order applies only to a pure reorder: the same mods in another
// sequence, with nothing the last scan has not seen. Anything else would be refused. The backend
// compares with the live file, so an outside edit hides the button too (the outside-change note
// offers its own action): a membership change would be refused, a reorder silently adopted.
// Unknown (not loaded yet) shows nothing rather than a claim.
const canRevert = computed(() => {
  const changes = pending.value;
  if (!allowRevert || changes === undefined || baseline.value?.externalChange.kind !== "none") {
    return false;
  }
  const isMembershipSame =
    changes.unscanned.added.length === 0 &&
    changes.unscanned.removed.length === 0 &&
    changes.unapplied.added.length === 0 &&
    changes.unapplied.removed.length === 0;
  return changes.orderUnapplied && isMembershipSame;
});

async function revertNow(): Promise<void> {
  revertError.value = null;
  try {
    await revert();
  } catch (caught: unknown) {
    revertError.value = describeCommandError(caught);
  }
}
// Your own unapplied order only: an outside edit of the file is not your work, and the
// outside-change note explains it. Unknown (not loaded yet) shows nothing rather than a claim.
const isShown = computed(
  () => selected.value === "current" && pending.value?.orderUnapplied === true,
);
</script>

<template>
  <Message
    v-if="isShown"
    severity="info"
    class="shrink-0"
    data-testid="order-not-in-file"
  >
    <div class="flex flex-wrap items-center justify-between gap-3">
      <span>{{ t("orderShare.notInFile") }}</span>
      <div class="flex flex-wrap items-center gap-2">
        <Button
          v-if="canRevert"
          :label="t('orderShare.revertOrder')"
          :title="t('orderShare.revertOrderTitle')"
          size="small"
          severity="secondary"
          :loading="isReverting"
          data-testid="order-not-in-file-revert"
          @click="revertNow"
        />
        <Button
          :label="t('orderShare.notInFileApply')"
          size="small"
          data-testid="order-not-in-file-apply"
          @click="isApplyOpen = true"
        />
      </div>
    </div>
    <div
      v-if="revertError"
      class="mt-1 text-xs"
      role="alert"
      data-testid="order-not-in-file-revert-error"
    >
      <div class="text-status-danger font-medium">
        {{ tm(revertError.title) }}
      </div>
      <div class="text-status-danger">
        {{ tm(revertError.detail) }}
      </div>
      <div
        v-if="revertError.technicalDetail"
        class="text-status-danger opacity-75"
      >
        {{ revertError.technicalDetail }}
      </div>
    </div>
  </Message>
  <ApplyDialog v-model:visible="isApplyOpen" />
</template>
