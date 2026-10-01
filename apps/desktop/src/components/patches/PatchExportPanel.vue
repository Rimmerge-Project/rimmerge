<script setup lang="ts">
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import InputText from "primevue/inputtext";
import Message from "primevue/message";
import { type ComponentPublicInstance, computed, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import type { RouteLocationRaw } from "vue-router";

import BaseEmpty from "@/components/base/BaseEmpty.vue";
import MergeModEntryList from "@/components/merge/MergeModEntryList.vue";
import XmlPreview from "@/components/merge/XmlPreview.vue";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { usePendingActiveChangesQuery } from "@/queries/activeSet";
import { useExportPatchMutation, usePatchFileQuery, usePatchRenderQuery } from "@/queries/patches";
import { pickFolder } from "@/services/dialogs";
import { RimmergeError } from "@/services/ipc";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";

/**
 * Renders, writes, and (optionally) installs a compat patch — the
 * `PatchDetailPage`'s export section. Mirrors `MergeModPage`'s own
 * dependency/entry/file layout (`get_patch_render`/`preview_patch_file`
 * are its patch-scoped equivalents of `get_merge_mod`/
 * `preview_merge_mod_file`) plus `ApplyDialog`'s running-game
 * refusal/force-confirm shape for `install`.
 */
const { patchId, exportDir } = defineProps<{
  patchId: string;
  /** The patch's last export directory, if any — seeds the directory field. */
  exportDir: string | null;
}>();
const { t } = useI18n();
const tm = useTranslateMessage();

const {
  data: render,
  isPending: isRenderPending,
  error: renderError,
} = usePatchRenderQuery(() => patchId);

const selectedFile = ref<string | null>(null);
// `About/About.xml` is selected by default whenever it renders; otherwise
// the first file — same rule as `MergeModPage`. Re-picks only when the
// current selection is no longer among the rendered files, never
// overriding a selection the user already made among files that still
// exist.
watch(
  () => render.value?.files,
  (files) => {
    if (!files) {
      return;
    }
    if (selectedFile.value !== null && files.includes(selectedFile.value)) {
      return;
    }
    selectedFile.value = files.includes("About/About.xml") ? "About/About.xml" : (files[0] ?? null);
  },
  { immediate: true },
);

const {
  data: file,
  isPending: isFilePending,
  error: fileError,
} = usePatchFileQuery(() => patchId, selectedFile);

function editorRouteFor(key: string): RouteLocationRaw {
  return { name: "patch-merge-editor", params: { patchId, key } };
}

const outDir = ref(exportDir ?? "");
// Reseeded whenever a *different* patch's own panel is shown — never
// mid-edit for the same patch (matches `PatchIdentityForm`'s own
// `initial`-reset rule, just keyed on `patchId` instead of the whole
// identity object since `exportDir` alone changing — e.g. this very
// panel's own successful export — must not clobber what's already typed
// in the field, which already equals it).
watch(
  () => patchId,
  () => {
    outDir.value = exportDir ?? "";
  },
);

const install = ref(false);
const awaitingForceConfirm = ref(false);
const errorMessage = ref<CommandErrorDescriptor | null>(null);

/**
 * An install still writes `ModsConfig.xml`'s own `current` order plus
 * this patch's package id even while the working active-mod set is
 * stale — pending activation changes are simply omitted, not refused.
 * This note says so before the write happens.
 */
const { data: pendingActiveChanges } = usePendingActiveChangesQuery();
const hasUnscannedActiveChanges = computed(() => {
  const diff = pendingActiveChanges.value?.unscanned;
  return diff !== undefined && (diff.added.length > 0 || diff.removed.length > 0);
});

interface LastExport {
  path: string;
  installedPath: string | null;
  /**
   * `ExportPatchReportDto.decisionsSha256` — the backend's own hash of
   * the decisions this export was rendered from, compared against the
   * currently rendered hash below to show "unchanged since last export".
   * Read from the report itself, never from the sibling
   * `usePatchRenderQuery`'s cached value: that query can still be
   * holding a pre-invalidation snapshot the instant this mutation
   * resolves.
   */
  hash: string;
}
const lastExport = ref<LastExport | null>(null);

watch(install, (value) => {
  if (!value) {
    awaitingForceConfirm.value = false;
  }
});

const unchangedSinceLastExport = computed(
  () =>
    lastExport.value !== null &&
    render.value != null &&
    lastExport.value.hash === render.value.decisionsSha256,
);

const outDirInputRef = useTemplateRef<ComponentPublicInstance>("outDirInput");

/** Focuses the export directory field — the patch detail page's `x` shortcut. */
function focusOutDir(): void {
  const element = outDirInputRef.value?.$el as HTMLElement | undefined;
  element?.focus();
}
defineExpose({ focusOutDir });

async function choose(): Promise<void> {
  const picked = await pickFolder(outDir.value.length > 0 ? outDir.value : undefined);
  if (picked !== null) {
    outDir.value = picked;
  }
}

const { mutateAsync: runExport, isLoading: isExporting } = useExportPatchMutation();

async function submitExport(force: boolean): Promise<void> {
  errorMessage.value = null;
  try {
    const report = await runExport({
      patchId,
      outDir: outDir.value,
      install: install.value,
      force,
    });
    awaitingForceConfirm.value = false;
    lastExport.value = {
      path: report.exportPath,
      installedPath: report.installedPath,
      hash: report.decisionsSha256,
    };
  } catch (error: unknown) {
    if (!force && error instanceof RimmergeError && error.code === "rimworld_running") {
      awaitingForceConfirm.value = true;
      return;
    }
    errorMessage.value = describeCommandError(error);
  }
}

function doExport(): void {
  void submitExport(false);
}

function exportAnyway(): void {
  void submitExport(true);
}

/**
 * Enter in the export-directory field runs the export under the exact
 * same guard as whichever button is currently showing — a no-op while
 * the directory is empty (the `Export` button's own `:disabled`
 * condition), and "Export anyway" once a running-game refusal is already
 * pending, never a second plain attempt on top of it.
 */
function handleOutDirEnter(): void {
  if (outDir.value.trim().length === 0) {
    return;
  }
  if (awaitingForceConfirm.value) {
    exportAnyway();
  } else {
    doExport();
  }
}
</script>

<template>
  <div
    class="flex flex-col gap-4"
    data-testid="patch-export-panel"
  >
    <div class="flex flex-col gap-2 sm:flex-row sm:items-end">
      <label class="flex flex-1 flex-col gap-1 text-sm">
        <span class="text-text-muted">{{ t("patches.exportPanel.exportDirLabel") }}</span>
        <InputText
          ref="outDirInput"
          v-model="outDir"
          :placeholder="t('patches.exportPanel.exportDirPlaceholder')"
          data-testid="patch-export-dir-input"
          @keydown.enter.prevent="handleOutDirEnter"
        />
      </label>
      <Button
        :label="t('patches.exportPanel.chooseButton')"
        size="small"
        severity="secondary"
        data-testid="patch-export-choose-button"
        @click="choose"
      />
    </div>

    <label class="flex items-center gap-2 text-sm">
      <Checkbox
        v-model="install"
        class="shrink-0"
        binary
        data-testid="patch-export-install-checkbox"
      />
      {{ t("patches.exportPanel.installCheckboxLabel") }}
    </label>

    <Message
      v-if="install && hasUnscannedActiveChanges"
      severity="warn"
      data-testid="patch-export-stale-note"
    >
      {{ t("patches.exportPanel.staleActiveSetNote") }}
    </Message>

    <Message
      v-if="awaitingForceConfirm"
      severity="warn"
      data-testid="patch-export-force-warning"
    >
      {{ t("patches.exportPanel.forceWarning") }}
    </Message>

    <Message
      v-if="errorMessage"
      severity="error"
      data-testid="patch-export-error"
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
        :label="t('patches.exportPanel.exportAnyway')"
        severity="danger"
        size="small"
        :loading="isExporting"
        data-testid="patch-export-force-button"
        @click="exportAnyway"
      />
      <Button
        v-else
        :label="t('patches.exportPanel.export')"
        size="small"
        :disabled="outDir.trim().length === 0"
        :loading="isExporting"
        data-testid="patch-export-button"
        @click="doExport"
      />
    </div>

    <div
      v-if="lastExport"
      class="surface-card flex flex-col gap-1 p-3 text-sm"
      data-testid="patch-export-last"
    >
      <p
        class="break-all"
        data-testid="patch-export-last-path"
      >
        {{ t("patches.exportPanel.exportedTo", { path: lastExport.path }) }}
      </p>
      <p
        v-if="lastExport.installedPath"
        class="break-all"
        data-testid="patch-export-last-installed-path"
      >
        {{ t("patches.exportPanel.installedAt", { path: lastExport.installedPath }) }}
      </p>
      <p
        v-if="lastExport.hash"
        class="text-text-muted font-mono text-xs"
        data-testid="patch-export-last-hash"
      >
        {{ t("patches.exportPanel.hash", { hash: lastExport.hash }) }}
      </p>
      <p
        v-if="unchangedSinceLastExport"
        class="text-text-muted text-xs"
        data-testid="patch-export-unchanged"
      >
        {{ t("patches.exportPanel.unchangedSinceLastExport") }}
      </p>
    </div>

    <p
      v-if="isRenderPending"
      class="text-text-muted text-sm"
      data-testid="patch-export-render-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="renderError"
      class="text-status-danger text-sm"
      data-testid="patch-export-render-error"
    >
      {{ renderError instanceof Error ? renderError.message : t("patches.exportPanel.renderLoadFailed") }}
    </p>

    <div
      v-else-if="render"
      class="grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-6"
    >
      <div class="flex flex-col gap-4">
        <div>
          <h3 class="text-text-muted mb-1 text-xs font-semibold tracking-wide uppercase">
            {{ t("patches.exportPanel.dependenciesHeading") }}
          </h3>
          <ul
            class="flex flex-wrap gap-1"
            data-testid="patch-export-dependencies"
          >
            <li
              v-for="dep in render.dependencies"
              :key="dep.modId"
              class="bg-surface-2 text-text-muted rounded-full px-2 py-0.5 text-xs"
            >
              {{ dep.name }}
            </li>
          </ul>
        </div>

        <div>
          <h3 class="text-text-muted mb-1 text-xs font-semibold tracking-wide uppercase">
            {{ t("patches.exportPanel.entriesHeading") }}
          </h3>
          <BaseEmpty
            v-if="render.entries.length === 0"
            :message="t('patches.exportPanel.noEntriesYet')"
          />
          <MergeModEntryList
            v-else
            :entries="render.entries"
            :editor-route="editorRouteFor"
          />
        </div>

        <div
          v-if="render.skipped.length > 0"
          data-testid="patch-export-skipped"
        >
          <h3 class="text-status-input mb-1 text-xs font-semibold tracking-wide uppercase">
            {{ t("patches.exportPanel.skippedHeading") }}
          </h3>
          <ul class="text-text-muted list-inside list-disc text-xs">
            <li
              v-for="key in render.skipped"
              :key="key"
            >
              {{ key }}
            </li>
          </ul>
        </div>
      </div>

      <div class="flex min-w-0 flex-col gap-2">
        <h3 class="text-text-muted text-xs font-semibold tracking-wide uppercase">
          {{ t("patches.exportPanel.filesHeading") }}
        </h3>
        <BaseEmpty
          v-if="render.files.length === 0"
          :message="t('patches.exportPanel.nothingRendersYet')"
        />
        <template v-else>
          <div
            class="flex flex-wrap gap-1"
            data-testid="patch-export-file-list"
          >
            <button
              v-for="path in render.files"
              :key="path"
              type="button"
              class="cursor-pointer rounded px-2 py-1 text-xs font-mono focus-visible:outline"
              :class="
                path === selectedFile
                  ? 'bg-accent-soft text-accent font-medium'
                  : 'bg-surface-2 text-text-muted hover:text-text'
              "
              :data-testid="`patch-export-file-${path}`"
              @click="selectedFile = path"
            >
              {{ path }}
            </button>
          </div>

          <p
            v-if="isFilePending"
            class="text-text-muted text-sm"
            data-testid="patch-export-file-loading"
          >
            {{ t("common.loading") }}
          </p>
          <p
            v-else-if="fileError"
            class="text-status-danger text-sm"
            data-testid="patch-export-file-error"
          >
            {{ fileError instanceof Error ? fileError.message : t("patches.exportPanel.fileLoadFailed") }}
          </p>
          <XmlPreview
            v-else-if="file"
            :content="file.content"
          />
        </template>
      </div>
    </div>
  </div>
</template>
