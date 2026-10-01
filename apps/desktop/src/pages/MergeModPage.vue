<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import BaseEmpty from "@/components/base/BaseEmpty.vue";
import MergeModEntryList from "@/components/merge/MergeModEntryList.vue";
import XmlPreview from "@/components/merge/XmlPreview.vue";
import { useModLabel } from "@/composables/useModLabel";
import { formatList } from "@/i18n/format";
import { useMergeModFileQuery, useMergeModQuery } from "@/queries/merge";

const { t, locale } = useI18n();
const { data, isPending, error } = useMergeModQuery();
const modLabel = useModLabel();

const selectedFile = ref<string | null>(null);

// `About/About.xml` is selected by default whenever it renders; otherwise
// the first file. Re-picks only when the current selection is no longer
// among the rendered files (a fresh decision changed what renders) —
// never overrides a selection the user already made among files that
// still exist.
watch(
  () => data.value?.files,
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
} = useMergeModFileQuery(selectedFile);

const hasEntries = computed(() => (data.value?.entries.length ?? 0) > 0);

/**
 * A plain-language statement of what this generated mod actually holds,
 * derived from the same `entries`/`sourceMods` the sections below render
 * in detail — answers "what does the merge output contain?" up front
 * instead of leaving the reader to infer it from a package id and a raw
 * entry table.
 */
const summary = computed(() => {
  if (!data.value) {
    return null;
  }
  const defCount = data.value.entries.filter((entry) => entry.kind !== "asset").length;
  const assetCount = data.value.entries.length - defCount;
  const parts: string[] = [];
  if (defCount > 0) {
    parts.push(t("merge.mod.mergedDefs", { count: defCount }, defCount));
  }
  if (assetCount > 0) {
    parts.push(t("merge.summary.asset", { count: assetCount }, assetCount));
  }
  const contents = parts.length > 0 ? formatList(locale.value, parts) : t("merge.mod.nothingYet");
  const modCount = data.value.sourceMods.length;
  return modCount > 0
    ? t("merge.mod.summaryWithSources", { contents, count: modCount }, modCount)
    : t("merge.mod.summary", { contents });
});
</script>

<template>
  <div class="mx-auto flex max-w-6xl flex-col gap-6 p-6">
    <h1 class="text-text text-xl font-semibold">
      {{ t("merge.mod.heading") }}
    </h1>
    <p
      v-if="summary"
      class="text-text-muted text-sm"
      data-testid="merge-mod-summary"
    >
      {{ summary }}
    </p>

    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="merge-mod-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="merge-mod-error"
    >
      {{ error instanceof Error ? error.message : t("merge.mod.loadFailed") }}
    </p>

    <template v-else-if="data">
      <dl class="surface-card grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 p-4 text-sm">
        <dt class="text-text-muted">
          {{ t("merge.mod.packageIdLabel") }}
        </dt>
        <dd data-testid="merge-mod-package-id">
          {{ data.packageId }}
        </dd>
        <dt class="text-text-muted">
          {{ t("merge.mod.folderLabel") }}
        </dt>
        <dd data-testid="merge-mod-folder-name">
          {{ data.folderName }}
        </dd>
        <dt class="text-text-muted">
          {{ t("merge.mod.pathLabel") }}
        </dt>
        <dd
          class="break-all"
          data-testid="merge-mod-path"
        >
          {{ data.modsPath }}
        </dd>
        <dt class="text-text-muted">
          {{ t("merge.mod.onDiskLabel") }}
        </dt>
        <dd data-testid="merge-mod-exists">
          {{ data.exists ? t("merge.mod.existsYes") : t("merge.mod.existsNo") }}
        </dd>
        <dt class="text-text-muted">
          {{ t("merge.mod.placementLabel") }}
        </dt>
        <dd data-testid="merge-mod-placement">
          {{ t("merge.mod.placementValue") }}
        </dd>
        <template v-if="data.sourceMods.length > 0">
          <dt class="text-text-muted">
            {{ t("merge.mod.sourceModsLabel") }}
          </dt>
          <dd data-testid="merge-mod-source-mods">
            {{ formatList(locale, data.sourceMods.map((mod) => modLabel.label(mod.modId))) }}
          </dd>
        </template>
      </dl>

      <div class="grid grid-cols-[minmax(0,1fr)_minmax(0,2fr)] gap-6">
        <div class="flex flex-col gap-2">
          <h2 class="text-text text-base font-semibold">
            {{ t("merge.mod.entriesHeading") }}
          </h2>
          <BaseEmpty
            v-if="!hasEntries"
            :message="t('merge.mod.noEntriesYet')"
          />
          <MergeModEntryList
            v-else
            :entries="data.entries"
          />
        </div>

        <div class="flex min-w-0 flex-col gap-2">
          <h2 class="text-text text-base font-semibold">
            {{ t("merge.mod.filesHeading") }}
          </h2>
          <BaseEmpty
            v-if="data.files.length === 0"
            :message="t('merge.mod.nothingRendersYet')"
          />
          <template v-else>
            <div
              class="flex flex-wrap gap-1"
              data-testid="merge-mod-file-list"
            >
              <button
                v-for="path in data.files"
                :key="path"
                type="button"
                class="cursor-pointer rounded px-2 py-1 text-xs font-mono focus-visible:outline"
                :class="
                  path === selectedFile
                    ? 'bg-accent-soft text-accent font-medium'
                    : 'bg-surface-2 text-text-muted hover:text-text'
                "
                :data-testid="`merge-mod-file-${path}`"
                @click="selectedFile = path"
              >
                {{ path }}
              </button>
            </div>

            <p
              v-if="isFilePending"
              class="text-text-muted text-sm"
              data-testid="merge-mod-file-loading"
            >
              {{ t("common.loading") }}
            </p>
            <p
              v-else-if="fileError"
              class="text-status-danger text-sm"
              data-testid="merge-mod-file-error"
            >
              {{ fileError instanceof Error ? fileError.message : t("merge.mod.fileLoadFailed") }}
            </p>
            <XmlPreview
              v-else-if="file"
              :content="file.content"
            />
          </template>
        </div>
      </div>
    </template>
  </div>
</template>
