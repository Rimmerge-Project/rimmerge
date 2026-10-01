<script setup lang="ts">
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import InputText from "primevue/inputtext";
import Message from "primevue/message";
import { computed, toRef } from "vue";
import { useI18n } from "vue-i18n";

import { useExportFlow } from "@/composables/useExportFlow";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { usePendingActiveChangesQuery } from "@/queries/activeSet";
import { useExportAssignmentMutation } from "@/queries/assignments";
import { pickFolder } from "@/services/dialogs";
import type { AssignmentSkipDto } from "@/types/generated/AssignmentSkipDto";
import type { ExportAssignmentReportDto } from "@/types/generated/ExportAssignmentReportDto";
import type { ExportAssignmentRequestDto } from "@/types/generated/ExportAssignmentRequestDto";

/**
 * A skip's own row address: a target-keyed skip's target def, a
 * free-standing skip's own bare `defName` — exactly one of
 * {@link AssignmentSkipDto.target}/`.ownDefName` is ever set.
 */
function formatSkipRow(skip: AssignmentSkipDto): string {
  return skip.target
    ? `${skip.target.def.defType}/${skip.target.def.defName}`
    : (skip.ownDefName ?? "");
}

/**
 * Renders, writes, and (optionally) installs an assignment project —
 * the export dialog, "parameterised"
 * from `PatchExportPanel`'s own out-dir/install/force-confirm flow via
 * {@link useExportFlow} rather than a second hand-rolled copy of it (see
 * that composable's own doc comment for why `PatchExportPanel` itself
 * isn't migrated onto it). No render-preview pane — an
 * assignment project has no `Merge`/`ShipAsset` decisions to browse, only
 * `skipped` rows, shown after a real export.
 */
const { assignmentId, exportDir } = defineProps<{
  assignmentId: string;
  exportDir: string | null;
}>();

const { t } = useI18n();
const tm = useTranslateMessage();
const { mutateAsync: runExport } = useExportAssignmentMutation();

const {
  outDir,
  install,
  awaitingForceConfirm,
  errorMessage,
  lastOutcome,
  isExporting,
  doExport,
  exportAnyway,
  handleOutDirEnter,
} = useExportFlow<ExportAssignmentRequestDto, ExportAssignmentReportDto>(
  (outDirValue, overrides) => ({ assignmentId, outDir: outDirValue, ...overrides }),
  (request) => runExport(request),
  toRef(() => exportDir),
);

/**
 * See `PatchExportPanel.vue`'s identical note on install writes ignoring
 * pending activation changes while the working set is stale.
 */
const { data: pendingActiveChanges } = usePendingActiveChangesQuery();
const hasUnscannedActiveChanges = computed(() => {
  const diff = pendingActiveChanges.value?.unscanned;
  return diff !== undefined && (diff.added.length > 0 || diff.removed.length > 0);
});

async function choose(): Promise<void> {
  const picked = await pickFolder(outDir.value.length > 0 ? outDir.value : undefined);
  if (picked !== null) {
    outDir.value = picked;
  }
}

const canExport = computed(() => outDir.value.trim().length > 0);
</script>

<template>
  <div
    class="flex flex-col gap-4"
    data-testid="assignment-export-panel"
  >
    <div class="flex flex-col gap-2 sm:flex-row sm:items-end">
      <label class="flex flex-1 flex-col gap-1 text-sm">
        <span class="text-text-muted">{{ t("assignments.exportPanel.exportDirLabel") }}</span>
        <InputText
          v-model="outDir"
          :placeholder="t('assignments.exportPanel.exportDirPlaceholder')"
          data-testid="assignment-export-dir-input"
          @keydown.enter.prevent="handleOutDirEnter"
        />
      </label>
      <Button
        :label="t('assignments.exportPanel.chooseButton')"
        size="small"
        severity="secondary"
        data-testid="assignment-export-choose-button"
        @click="choose"
      />
    </div>

    <label class="flex items-center gap-2 text-sm">
      <Checkbox
        v-model="install"
        class="shrink-0"
        binary
        data-testid="assignment-export-install-checkbox"
      />
      {{ t("assignments.exportPanel.installCheckboxLabel") }}
    </label>

    <Message
      v-if="install && hasUnscannedActiveChanges"
      severity="warn"
      data-testid="assignment-export-stale-note"
    >
      {{ t("assignments.exportPanel.staleActiveSetNote") }}
    </Message>

    <Message
      v-if="awaitingForceConfirm"
      severity="warn"
      data-testid="assignment-export-force-warning"
    >
      {{ t("assignments.exportPanel.forceWarning") }}
    </Message>

    <Message
      v-if="errorMessage"
      severity="error"
      data-testid="assignment-export-error"
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

    <div class="flex gap-2">
      <Button
        v-if="awaitingForceConfirm"
        :label="t('assignments.exportPanel.exportAnyway')"
        severity="danger"
        size="small"
        :loading="isExporting"
        data-testid="assignment-export-force-button"
        @click="exportAnyway"
      />
      <Button
        v-else
        :label="t('assignments.exportPanel.export')"
        size="small"
        :disabled="!canExport"
        :loading="isExporting"
        data-testid="assignment-export-button"
        @click="doExport"
      />
    </div>

    <div
      v-if="lastOutcome"
      class="surface-card flex flex-col gap-1 p-3 text-sm"
      data-testid="assignment-export-last"
    >
      <p
        class="break-all"
        data-testid="assignment-export-last-path"
      >
        {{ t("assignments.exportPanel.exportedTo", { path: lastOutcome.exportPath }) }}
      </p>
      <p
        v-if="lastOutcome.installedPath"
        class="break-all"
        data-testid="assignment-export-last-installed-path"
      >
        {{ t("assignments.exportPanel.installedAt", { path: lastOutcome.installedPath }) }}
      </p>
      <div
        v-if="lastOutcome.skipped.length > 0"
        data-testid="assignment-export-skipped"
      >
        <h3 class="text-status-input mb-1 text-xs font-semibold tracking-wide uppercase">
          {{ t("assignments.exportPanel.skippedHeading") }}
        </h3>
        <ul class="text-text-muted list-inside list-disc text-xs">
          <li
            v-for="(skip, index) in lastOutcome.skipped"
            :key="index"
          >
            {{ skip.defType }} {{ formatSkipRow(skip) }}{{
              skip.path ? t("assignments.exportPanel.skippedPath", { path: skip.path }) : ""
            }}
            — {{ skip.reason }}
          </li>
        </ul>
      </div>
    </div>
  </div>
</template>
