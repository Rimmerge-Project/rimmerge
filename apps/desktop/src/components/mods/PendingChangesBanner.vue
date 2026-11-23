<script setup lang="ts">
import Button from "primevue/button";
import Message from "primevue/message";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useBannerAnnouncement } from "@/composables/useBannerAnnouncement";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import {
  useDiscardPendingChangesMutation,
  usePendingActiveChangesQuery,
  useRescanMutation,
} from "@/queries/activeSet";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";
import { QUIET_MESSAGE_PT } from "@/utils/quietMessage";

const { t } = useI18n();
const tm = useTranslateMessage();
const { data: pending } = usePendingActiveChangesQuery();
const { mutateAsync: rescan, isLoading: rescanning } = useRescanMutation();
const { mutateAsync: discard, isLoading: discarding } = useDiscardPendingChangesMutation();

const unscannedCount = computed(() => {
  const diff = pending.value?.unscanned;
  return diff ? diff.added.length + diff.removed.length : 0;
});
const unappliedCount = computed(() => {
  const diff = pending.value?.unapplied;
  return diff ? diff.added.length + diff.removed.length : 0;
});

useBannerAnnouncement("pendingChanges", () =>
  unscannedCount.value > 0
    ? t("mods.pendingChanges.unscanned", { count: unscannedCount.value }, unscannedCount.value)
    : null,
);

const errorMessage = ref<CommandErrorDescriptor | null>(null);

async function discardNow(): Promise<void> {
  errorMessage.value = null;
  try {
    await discard();
  } catch (error: unknown) {
    errorMessage.value = describeCommandError(error);
  }
}

async function rescanNow(): Promise<void> {
  errorMessage.value = null;
  try {
    await rescan();
  } catch (error: unknown) {
    errorMessage.value = describeCommandError(error);
  }
}
</script>

<template>
  <div
    v-if="unscannedCount > 0"
    class="shrink-0"
    data-testid="pending-changes-banner"
  >
    <Message
      severity="warn"
      :closable="false"
      :pt="QUIET_MESSAGE_PT"
    >
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div class="flex flex-col">
          <span data-testid="pending-changes-unscanned-text">
            {{ t("mods.pendingChanges.unscanned", { count: unscannedCount }, unscannedCount) }}
          </span>
          <span
            v-if="unappliedCount > 0"
            class="text-text-muted text-xs"
            data-testid="pending-changes-unapplied-text"
          >
            {{ t("mods.pendingChanges.unapplied", { count: unappliedCount }, unappliedCount) }}
          </span>
        </div>
        <div class="flex items-center gap-2">
          <Button
            :label="t('mods.pendingChanges.discardButton')"
            size="small"
            severity="secondary"
            :loading="discarding"
            :disabled="rescanning"
            data-testid="pending-changes-discard-button"
            @click="discardNow"
          />
          <Button
            :label="t('mods.pendingChanges.rescanButton')"
            size="small"
            :loading="rescanning"
            :disabled="discarding"
            data-testid="pending-changes-rescan-button"
            @click="rescanNow"
          />
        </div>
      </div>
      <div
        v-if="errorMessage"
        class="mt-1 text-xs"
        data-testid="pending-changes-rescan-error"
      >
        <div class="text-status-danger font-medium">
          {{ tm(errorMessage.title) }}
        </div>
        <div class="text-status-danger">
          {{ tm(errorMessage.detail) }}
        </div>
        <div
          v-if="errorMessage.technicalDetail"
          class="text-status-danger opacity-75"
        >
          {{ errorMessage.technicalDetail }}
        </div>
      </div>
    </Message>
  </div>
</template>
