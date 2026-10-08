<script setup lang="ts">
// What apply left behind: the force warning, the error, and the merge-mod summary.
import Message from "primevue/message";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useApplyDialogState } from "@/composables/useApplyDialog";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useMergeModQuery } from "@/queries/merge";
import { describeSkippedMergeReason } from "@/utils/format";

const { t } = useI18n();
const { awaitingForceConfirm, errorMessage, lastReport, mergeModRemoved } = useApplyDialogState();
const tm = useTranslateMessage();
// This component is mounted only while the dialog is open (see `ApplyMergeModSection`), which is
// what keeps the slow `get_merge_mod` render off every other page.
const { data: mergeMod, status: mergeModStatus } = useMergeModQuery();

/**
 * `lastReport.skippedMerges` paired with its own reason, read off `get_merge_mod`'s entries: the
 * same query, and the same session state, `apply`'s own render step just built the report from,
 * so no second command is needed to explain *why* a key was skipped. An entry that is not found
 * (the query not answered, or failed) falls back to the plain wording rather than guessing.
 * Entries are read only on `status === "success"`: after a failed refetch Colada keeps the
 * replaced session's `data`, whose entries must not explain this session's skips.
 */
const skippedMergeEntries = computed(() => {
  const entries = mergeModStatus.value === "success" ? (mergeMod.value?.entries ?? []) : [];
  return (lastReport.value?.skippedMerges ?? []).map((key) => {
    const entry = entries.find((candidate) => candidate.key === key);
    return { key, reason: describeSkippedMergeReason(entry) };
  });
});

/**
 * {@link describeSkippedMergeReason} returns a raw, untranslated string
 * for a `cannotMerge` entry's own diagnostic text (never translated —
 * see `apps/desktop/CLAUDE.md`'s i18n conventions) and a
 * `MessageDescriptor` for every other case; this is the `typeof` branch
 * its own doc comment calls for.
 */
function skippedReasonText(reason: (typeof skippedMergeEntries.value)[number]["reason"]): string {
  return typeof reason === "string" ? reason : tm(reason);
}
</script>

<template>
  <Message
    v-if="awaitingForceConfirm"
    severity="warn"
    data-testid="apply-dialog-force-warning"
  >
    {{ t("apply.result.forceWarning") }}
  </Message>

  <Message
    v-if="errorMessage"
    severity="error"
    data-testid="apply-dialog-error"
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

  <div
    v-if="lastReport"
    class="border-border-subtle flex flex-col gap-2 rounded border p-3 text-sm"
    data-testid="apply-dialog-merge-summary"
  >
    <p
      v-if="lastReport.mergeModPath"
      class="break-all"
      data-testid="apply-dialog-merge-mod-path"
    >
      {{ t("apply.result.mergeModPath", { path: lastReport.mergeModPath }) }}
    </p>
    <p
      v-else-if="mergeModRemoved"
      data-testid="apply-dialog-merge-mod-removed"
    >
      {{ t("apply.result.mergeModRemoved") }}
    </p>
    <p
      v-if="lastReport.mergeModBackupPath"
      class="break-all"
      data-testid="apply-dialog-merge-mod-backup-path"
    >
      {{ t("apply.result.mergeModBackupPath", { path: lastReport.mergeModBackupPath }) }}
    </p>
    <div
      v-if="skippedMergeEntries.length > 0"
      data-testid="apply-dialog-skipped-merges"
    >
      <p class="text-text-muted">
        {{ t("apply.result.skippedHeading") }}
      </p>
      <ul class="list-inside list-disc">
        <li
          v-for="entry in skippedMergeEntries"
          :key="entry.key"
          class="text-xs"
        >
          <span class="font-mono">{{ entry.key }}</span> — {{ skippedReasonText(entry.reason) }}
        </li>
      </ul>
    </div>
  </div>
</template>
