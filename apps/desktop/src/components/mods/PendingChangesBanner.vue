<script setup lang="ts">
import Button from "primevue/button";
import Message from "primevue/message";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { usePendingActiveChangesQuery, useRescanMutation } from "@/queries/activeSet";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

const { t } = useI18n();
const tm = useTranslateMessage();
const { data: pending } = usePendingActiveChangesQuery();
const { mutateAsync: rescan, isLoading: rescanning } = useRescanMutation();

const unscannedCount = computed(() => {
  const diff = pending.value?.unscanned;
  return diff ? diff.added.length + diff.removed.length : 0;
});
const unappliedCount = computed(() => {
  const diff = pending.value?.unapplied;
  return diff ? diff.added.length + diff.removed.length : 0;
});

const errorMessage = ref<CommandErrorDescriptor | null>(null);

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
        <Button
          :label="t('mods.pendingChanges.rescanButton')"
          size="small"
          :loading="rescanning"
          data-testid="pending-changes-rescan-button"
          @click="rescanNow"
        />
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
