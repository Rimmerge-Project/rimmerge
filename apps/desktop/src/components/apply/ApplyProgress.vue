<script setup lang="ts">
// The Verify order panel: the button, the progress bar, and the report.

import Button from "primevue/button";
import Message from "primevue/message";
import { useI18n } from "vue-i18n";
import ApplyOrderDiff from "@/components/apply/ApplyOrderDiff.vue";
import BaseProgressBar from "@/components/base/BaseProgressBar.vue";
import { useApplyDialogState } from "@/composables/useApplyDialog";
import { useTranslateMessage } from "@/composables/useTranslateMessage";

const { t } = useI18n();
const tm = useTranslateMessage();
const {
  verifying,
  lastReport,
  verifyProgress,
  verifyErrorMessage,
  verifyProgressPercent,
  verifyOrderNow,
} = useApplyDialogState();
</script>

<template>
  <div
    v-if="!lastReport"
    class="border-border-subtle flex flex-col gap-2 rounded border p-3 text-sm"
    data-testid="apply-dialog-verify"
  >
    <div class="flex items-center justify-between gap-2">
      <div>
        <span class="text-text font-medium">{{ t("apply.progress.heading") }}</span>
        <p class="text-text-muted text-xs">
          {{ t("apply.progress.description") }}
        </p>
      </div>
      <Button
        :label="t('apply.progress.heading')"
        severity="secondary"
        size="small"
        class="shrink-0 whitespace-nowrap"
        :loading="verifying"
        data-testid="apply-dialog-verify-button"
        @click="verifyOrderNow"
      />
    </div>

    <BaseProgressBar
      v-if="verifying"
      :progress="{ percent: verifyProgressPercent }"
      :label="t('apply.progress.verifyingLabel')"
      :caption="
        verifyProgress
          ? t('apply.progress.checksCaption', {
            checked: verifyProgress.checked,
            total: verifyProgress.total,
          })
          : t('apply.progress.startingCaption')
      "
      data-testid="apply-dialog-verify-progress"
    />

    <Message
      v-if="verifyErrorMessage"
      severity="error"
      data-testid="apply-dialog-verify-error"
    >
      <div class="font-medium">
        {{ tm(verifyErrorMessage.title) }}
      </div>
      <div>{{ tm(verifyErrorMessage.detail) }}</div>
      <div
        v-if="verifyErrorMessage.technicalDetail"
        class="text-xs opacity-75"
      >
        {{ verifyErrorMessage.technicalDetail }}
      </div>
    </Message>

    <ApplyOrderDiff />
  </div>
</template>
