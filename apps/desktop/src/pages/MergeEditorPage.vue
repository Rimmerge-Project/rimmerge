<script setup lang="ts">
import Dialog from "primevue/dialog";
import { computed, onUnmounted, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import BaseKbd from "@/components/base/BaseKbd.vue";
import MergeFieldTable from "@/components/merge/MergeFieldTable.vue";
import MergeHeader from "@/components/merge/MergeHeader.vue";
import { useMergeChoices } from "@/composables/useMergeChoices";
import { useMergeFieldPage } from "@/composables/useMergeFieldPage";
import { useMergeKeys } from "@/composables/useMergeKeys";
import { useShortcutHelp } from "@/composables/useShortcutHelp";
import { RimmergeError } from "@/services/ipc";
import { useMergeStore } from "@/stores/merge";
import { asFindingKey, type FieldPath } from "@/types/brands";
import type { MergeChoiceDto } from "@/types/generated/MergeChoiceDto";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import { containerStats, effectiveContainerCollapsed } from "@/utils/mergeFieldContainers";

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const store = useMergeStore();
const help = useShortcutHelp();

// Vue Router already decodes params — a finding key's canonical text can
// contain `:`/`[`/`]`/`,`, but the router handles that itself; encoding
// it again here (or at the `router.push` call site) would double-encode.
const findingKey = computed(() => asFindingKey(String(route.params["key"] ?? "")));

/**
 * Reached via `/patches/:patchId/merge/:key` (route name
 * `patch-merge-editor`) when set, `/merge/:key` (`merge-editor`)
 * otherwise — the same page either way, threaded into every merge
 * command below so a patch's own scoped decisions are read and written
 * instead of the profile's.
 */
const patchId = computed(() => {
  const raw = route.params["patchId"];
  return typeof raw === "string" && raw.length > 0 ? raw : null;
});

// Conflicts are always fetched and shown, open. The full page (every
// class, including auto and unchanged rows) is fetched too — simpler
// than gating it on section-expansion. Each page tops out at 200 rows
// (`rim_session::MAX_PAGE_SIZE`); `useMergeFieldPage` accumulates further
// pages via `loadMore` so a def with more matching rows is still fully
// reachable, not silently truncated.
const conflictsPage = useMergeFieldPage(findingKey, true, patchId);
const allPage = useMergeFieldPage(findingKey, false, patchId);

const preview = computed(() => conflictsPage.preview.value ?? allPage.preview.value ?? null);
const isPatchCollision = computed(() => preview.value?.kind === "patchCollision");
const conflictFields = computed(() => conflictsPage.fields.value);
const autoFields = computed(() =>
  allPage.fields.value.filter((field) => field.class === "oneSided" || field.class === "agreeing"),
);
const unchangedFields = computed(() =>
  allPage.fields.value.filter((field) => field.class === "unchanged"),
);
const hasMoreFields = computed(() => conflictsPage.hasMore.value || allPage.hasMore.value);
function loadMoreFields(): void {
  conflictsPage.loadMore();
  allPage.loadMore();
}

/**
 * Every currently-loaded field, across every section — the input
 * `containerStats` aggregates over, so a container's `total`/`conflicts`
 * (and thus its own default collapse) never depends on which section
 * happens to render it.
 */
const allKnownFields = computed(() => [
  ...conflictFields.value,
  ...autoFields.value,
  ...unchangedFields.value,
]);
const stats = computed(() => containerStats(allKnownFields.value));

/** Hidden behind a collapsed container header — mirrors `MergeFieldTable`'s own `pushSectionRows`, so `visibleFields` below always agrees with what's actually on screen. The conflicts section never hides a field this way (`MergeFieldTable` never makes its own container header collapsible there either). */
function hiddenByCollapsedContainer(field: MergeFieldDto): boolean {
  if (field.container === null) {
    return false;
  }
  const containerStat = stats.value.get(field.container);
  if (!containerStat) {
    return false;
  }
  return effectiveContainerCollapsed(field.container, containerStat, store.containerOverrides);
}

/** Mirrors `MergeFieldTable`'s own flattening — the array `useMergeKeys`'s row cursor indexes into. */
const visibleFields = computed(() => [
  ...conflictFields.value,
  ...(store.collapsedAuto
    ? []
    : autoFields.value.filter((field) => !hiddenByCollapsedContainer(field))),
  ...(store.showUnchanged
    ? unchangedFields.value.filter((field) => !hiddenByCollapsedContainer(field))
    : []),
]);

const choices = useMergeChoices(findingKey, patchId);
watch(
  [conflictsPage.fields, allPage.fields],
  ([conflicts, all]) => {
    choices.seed(conflicts);
    choices.seed(all);
  },
  { immediate: true },
);

/** `Esc`/`q`: back to the patch's own detail page when reached in patch context, the profile inbox otherwise. */
function exitToInbox(): void {
  if (patchId.value) {
    void router.push({ name: "patch-detail", params: { patchId: patchId.value } });
  } else {
    void router.push({ name: "inbox" });
  }
}

const mergeKeys = useMergeKeys({
  rows: visibleFields,
  owners: () => preview.value?.owners ?? [],
  winner: () => preview.value?.winner ?? "",
  choices,
  isPatchCollision,
  onExit: exitToInbox,
  onToggleHelp: () => help.toggle(),
});

onUnmounted(() => store.reset());

function handleChoose(path: FieldPath, choice: MergeChoiceDto): void {
  void choices.setChoice(path, choice);
}

function handleRevert(path: FieldPath): void {
  void choices.clearChoice(path);
}

function handleToggleContainer(container: string, collapsed: boolean): void {
  store.setContainerCollapsed(container, collapsed);
}

function handleEditCommit(text: string): void {
  void mergeKeys.commitEdit(text);
}

const sourceFailed = computed(() => {
  const error = conflictsPage.error.value ?? allPage.error.value;
  return error instanceof RimmergeError && error.code === "merge_source_failed" ? error : null;
});
const otherError = computed(() => {
  const error = conflictsPage.error.value ?? allPage.error.value;
  return sourceFailed.value === null ? error : null;
});
</script>

<template>
  <div class="grid h-full grid-rows-[auto_1fr] gap-2 p-4">
    <p
      v-if="sourceFailed"
      class="text-status-danger text-sm"
      data-testid="merge-source-failed"
    >
      {{ t("merge.editor.sourceFailedSuffix", { message: sourceFailed.message }) }}
      <button
        type="button"
        class="bg-accent text-on-accent ml-2 cursor-pointer rounded px-2 py-1 text-xs"
        data-testid="merge-reload-project"
        @click="router.push({ name: 'setup' })"
      >
        {{ t("merge.editor.reloadProject") }}
      </button>
    </p>
    <p
      v-else-if="otherError"
      class="text-status-danger p-4 text-sm"
      data-testid="merge-error"
    >
      {{ otherError instanceof Error ? otherError.message : t("merge.editor.loadFailed") }}
    </p>
    <p
      v-else-if="conflictsPage.isPending.value"
      class="text-text-muted p-4 text-sm"
      data-testid="merge-loading"
    >
      {{ t("common.loading") }}
    </p>
    <MergeHeader
      v-else-if="preview"
      :preview="preview"
    />

    <div
      v-if="preview"
      class="grid min-h-0 grid-rows-[1fr_auto] gap-2"
    >
      <MergeFieldTable
        :conflict-fields="conflictFields"
        :auto-fields="autoFields"
        :unchanged-fields="unchangedFields"
        :conflict-total="preview.totals.conflicts"
        :auto-total="preview.totals.auto"
        :unchanged-total="preview.totals.unchanged"
        :owners="preview.owners"
        :collapsed-auto="store.collapsedAuto"
        :show-unchanged="store.showUnchanged"
        :row-cursor="store.rowCursor"
        :column-cursor="store.columnCursor"
        :editing-path="mergeKeys.editingPath.value"
        :editing-draft="mergeKeys.editingDraft.value"
        :is-patch-collision="isPatchCollision"
        :pending-choices="choices.choices.value"
        :container-overrides="store.containerOverrides"
        @toggle-auto="store.toggleAuto"
        @toggle-unchanged="store.toggleUnchanged"
        @toggle-container="handleToggleContainer"
        @select-row="store.setRowCursor"
        @choose="handleChoose"
        @drop="mergeKeys.toggleDrop"
        @revert="handleRevert"
        @edit-commit="handleEditCommit"
        @edit-cancel="mergeKeys.cancelEdit"
      />
      <button
        v-if="hasMoreFields"
        type="button"
        class="border-border-subtle text-text-muted hover:bg-surface-2 hover:text-text shrink-0 cursor-pointer self-start rounded border px-2 py-1 text-xs focus-visible:outline"
        data-testid="merge-show-more"
        @click="loadMoreFields"
      >
        {{
          t("merge.editor.showMore", { loaded: allPage.fields.value.length, total: allPage.total.value })
        }}
      </button>
    </div>

    <Dialog
      v-model:visible="help.visible.value"
      modal
      :header="t('merge.editor.shortcutsHeader')"
      data-testid="merge-shortcut-help"
    >
      <!-- eslint-disable vue/no-bare-strings-in-template -- `BaseKbd`'s
           `label` names a literal keyboard key/shortcut ("j/k", "1-9",
           "?", …), which this app's i18n conventions keep in Latin
           script in every locale (`apps/desktop/CLAUDE.md`'s "keyboard-
           shortcut letters" rule) — every other string in this block
           already goes through `t()`. -->
      <ul class="flex flex-col gap-2 text-sm">
        <li><BaseKbd label="j/k, ↓/↑" /> {{ t("merge.editor.shortcutMoveRowCursor") }}</li>
        <li><BaseKbd label="c/C" /> {{ t("merge.editor.shortcutNextPrevConflict") }}</li>
        <li><BaseKbd label="h/l, ←/→" /> {{ t("merge.editor.shortcutMoveColumnCursor") }}</li>
        <li><BaseKbd label="Enter/Space" /> {{ t("merge.editor.shortcutChooseColumn") }}</li>
        <li><BaseKbd label="1-9" /> {{ t("merge.editor.shortcutChooseOwnerColumn") }}</li>
        <li><BaseKbd label="w" /> {{ t("merge.editor.shortcutChooseWinner") }}</li>
        <li><BaseKbd label="e" /> {{ t("merge.editor.shortcutEditValue") }}</li>
        <li><BaseKbd label="d" /> {{ t("merge.editor.shortcutToggleDrop") }}</li>
        <li><BaseKbd label="a" /> {{ t("merge.editor.shortcutRevertToAuto") }}</li>
        <li><BaseKbd label="x" /> {{ t("merge.editor.shortcutToggleAutoSection") }}</li>
        <li><BaseKbd label="X" /> {{ t("merge.editor.shortcutToggleUnchanged") }}</li>
        <li><BaseKbd label="Esc/q" /> {{ t("merge.editor.shortcutBackToFindings") }}</li>
        <li><BaseKbd label="t" /> {{ t("inbox.page.shortcutToggleModLabel") }}</li>
        <li><BaseKbd label="?" /> {{ t("inbox.page.shortcutToggleHelp") }}</li>
      </ul>
      <!-- eslint-enable vue/no-bare-strings-in-template -->
    </Dialog>
  </div>
</template>
