<script setup lang="ts">
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import Dialog from "primevue/dialog";
import Message from "primevue/message";
import { computed, nextTick, provide, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";
import ApplyConfirm from "@/components/apply/ApplyConfirm.vue";
import ApplyPreflight from "@/components/apply/ApplyPreflight.vue";
import ApplyProgress from "@/components/apply/ApplyProgress.vue";
import ApplyResult from "@/components/apply/ApplyResult.vue";
import { APPLY_DIALOG_KEY, type ApplyOutcome, useApplyDialog } from "@/composables/useApplyDialog";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { applyButtonLabel } from "@/utils/applyButtonLabel";

const { visible } = defineProps<{ visible: boolean }>();
// `applied` fires once per successful apply (even when the dialog stays open for the merge-mod
// summary); the Dashboard and sidebar hosts ignore it, the Launch RimWorld button's own
// dialog launches on it.
const emit = defineEmits<{
  "update:visible": [value: boolean];
  applied: [outcome: ApplyOutcome];
}>();

const { t } = useI18n();
const tm = useTranslateMessage();
const state = useApplyDialog(() => visible, emit);
provide(APPLY_DIALOG_KEY, state);
const {
  isLoading,
  defCacheNote,
  writeModsConfig,
  awaitingProblemConfirm,
  isStaleActiveSet,
  rescanning,
  rescanError,
  rescanFromApplyDialog,
  mergeModGroupsSummary,
  writeMergeMod,
  awaitingForceConfirm,
  lastReport,
  close,
  submit,
  writeAnyway,
} = state;

const submitLabel = computed(() =>
  tm(
    applyButtonLabel({
      writeModsConfig: writeModsConfig.value,
      writeMergeMod: writeMergeMod.value,
    }),
  ),
);

const submitButton = useTemplateRef<{ $el: HTMLElement }>("submitButton");

// The confirmation panel (and its focused "Go back") is gone after going back, so focus
// would fall to <body>; hand it to the button the user just came from.
async function focusSubmitAfterGoBack(): Promise<void> {
  await nextTick();
  submitButton.value?.$el.focus();
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :header="t('apply.dialog.header')"
    data-testid="apply-dialog"
    @update:visible="(value: boolean) => emit('update:visible', value)"
  >
    <div class="flex w-96 flex-col gap-4">
      <ApplyPreflight />

      <Message
        v-if="defCacheNote"
        severity="info"
        data-testid="apply-dialog-def-cache-note"
      >
        {{ defCacheNote }}
      </Message>

      <!-- Disabled once a force-confirm is pending: `writeAnyway` always
           sends `writeModsConfig: true` regardless of this checkbox, so
           letting the user toggle it here would suggest a choice that no
           longer does anything. Also disabled (and forced off) while the
           working active-mod set is stale — the backend refuses the
           write anyway; see `apply-dialog-stale-active-set` below. -->
      <label class="flex items-center gap-2 text-sm">
        <Checkbox
          v-model="writeModsConfig"
          class="shrink-0"
          binary
          :disabled="awaitingForceConfirm || awaitingProblemConfirm || isStaleActiveSet"
          data-testid="apply-dialog-write-modsconfig-checkbox"
        />
        {{ t("apply.dialog.writeModsConfigLabel") }}
      </label>

      <Message
        v-if="isStaleActiveSet"
        severity="warn"
        data-testid="apply-dialog-stale-active-set"
      >
        <div class="flex flex-wrap items-center justify-between gap-2">
          <span>{{ t("apply.dialog.staleActiveSetMessage") }}</span>
          <Button
            :label="t('apply.dialog.rescanButton')"
            size="small"
            :loading="rescanning"
            data-testid="apply-dialog-rescan-button"
            @click="rescanFromApplyDialog"
          />
        </div>
        <div
          v-if="rescanError"
          class="mt-1"
          data-testid="apply-dialog-rescan-error"
        >
          <div class="text-status-danger font-medium">
            {{ tm(rescanError.title) }}
          </div>
          <div class="text-status-danger">
            {{ tm(rescanError.detail) }}
          </div>
          <div
            v-if="rescanError.technicalDetail"
            class="text-status-danger text-xs opacity-75"
          >
            {{ rescanError.technicalDetail }}
          </div>
        </div>
      </Message>

      <label class="flex items-center gap-2 text-sm">
        <Checkbox
          v-model="writeMergeMod"
          class="shrink-0"
          binary
          data-testid="apply-dialog-write-merge-mod-checkbox"
        />
        {{ t("apply.dialog.writeMergeModLabel") }}
      </label>
      <p
        v-if="mergeModGroupsSummary"
        class="text-text-faint pl-6 text-xs"
        data-testid="apply-dialog-merge-mod-groups"
      >
        {{ mergeModGroupsSummary }}
      </p>

      <ApplyProgress />

      <ApplyResult />

      <!-- `flex-wrap` + `whitespace-nowrap` buttons: a long translated primary label
           (e.g. German "save decisions and rules") moves Cancel to its own line
           instead of wrapping the label into a two-line button. -->
      <div class="flex flex-wrap justify-end gap-2">
        <Button
          v-if="lastReport"
          :label="t('apply.dialog.closeButton')"
          data-testid="apply-dialog-close-summary"
          @click="close"
        />
        <ApplyConfirm
          v-else-if="awaitingProblemConfirm"
          class="w-full"
          @go-back="focusSubmitAfterGoBack"
        />
        <template v-else>
          <Button
            :label="t('apply.dialog.cancelButton')"
            severity="secondary"
            class="whitespace-nowrap"
            data-testid="apply-dialog-cancel"
            @click="close"
          />
          <Button
            v-if="awaitingForceConfirm && !isStaleActiveSet"
            :label="t('apply.dialog.writeAnywayButton')"
            severity="danger"
            class="whitespace-nowrap"
            :loading="isLoading"
            data-testid="apply-dialog-write-anyway"
            @click="writeAnyway"
          />
          <Button
            v-else
            ref="submitButton"
            :label="submitLabel"
            class="whitespace-nowrap"
            :loading="isLoading"
            data-testid="apply-dialog-submit"
            @click="submit"
          />
        </template>
      </div>
    </div>
  </Dialog>
</template>
