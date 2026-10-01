<script setup lang="ts">
import Dialog from "primevue/dialog";
import { computed, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import BaseKbd from "@/components/base/BaseKbd.vue";
import FindingList from "@/components/inbox/FindingList.vue";
import SuggestionPanel from "@/components/inbox/SuggestionPanel.vue";
import { type InboxKeysBackend, useInboxKeys } from "@/composables/useInboxKeys";
import { useShortcutHelp } from "@/composables/useShortcutHelp";
import {
  useDecidePatchMutation,
  usePatchFindingQuery,
  usePatchFindingsQuery,
  useRevertPatchDecisionMutation,
} from "@/queries/patches";
import { usePatchStore } from "@/stores/patch";
import { asFindingKey } from "@/types/brands";

const { patchId } = defineProps<{ patchId: string }>();
/**
 * `Escape`: `useInboxKeys` calls this instead of navigating anywhere
 * itself — the patch detail page owns what "back" means here (its own
 * header, never the profile findings; see `useInboxKeys`'s `onEscape`
 * doc comment).
 */
const emit = defineEmits<{ escape: [] }>();

const { t } = useI18n();
const router = useRouter();
const patchStore = usePatchStore();
const help = useShortcutHelp();

// A single shared store (not one per patch id, see `stores/patch.ts`) —
// `activate` resets the cursor/filter only when a *different* patch
// becomes active, so a remount of this same patch (e.g. `Esc`/`q`
// returning from the merge editor, which unmounts and remounts this
// component) never loses the user's place the way an unconditional
// `reset()` on every mount would.
watch(
  () => patchId,
  (id) => patchStore.activate(id),
  { immediate: true },
);

const pageSize = ref(50);
watch(
  () => patchStore.filter,
  () => {
    pageSize.value = 50;
  },
);

const {
  data: page,
  isPending,
  error,
} = usePatchFindingsQuery(
  () => patchId,
  () => ({
    status: patchStore.filter.status,
    kinds: patchStore.filter.kinds,
    modId: null,
    search: patchStore.filter.search.length > 0 ? patchStore.filter.search : null,
    offset: 0,
    limit: pageSize.value,
  }),
);

const items = computed(() => page.value?.items ?? []);
const total = computed(() => page.value?.total ?? 0);
watch(items, (newItems) => {
  const maxIndex = Math.max(newItems.length - 1, 0);
  if (patchStore.cursor > maxIndex) {
    patchStore.setCursor(maxIndex);
  }
});
const currentKey = computed(() => items.value[patchStore.cursor]?.key ?? null);

const { data: currentDetail } = usePatchFindingQuery(
  () => patchId,
  () => (currentKey.value ? asFindingKey(currentKey.value) : null),
);

const { mutateAsync: decidePatch } = useDecidePatchMutation();
const { mutateAsync: revertPatchDecision } = useRevertPatchDecisionMutation();
const backend: InboxKeysBackend = {
  decide: (request) => decidePatch({ patchId, ...request }),
  revert: (key) => revertPatchDecision({ patchId, key }),
};

interface FindingListInstance {
  focusSearch: () => void;
}
const findingListRef = useTemplateRef<FindingListInstance>("findingList");
const findingListContainerRef = useTemplateRef<HTMLDivElement>("findingListContainer");

function selectRow(key: string): void {
  const index = items.value.findIndex((item) => item.key === key);
  if (index >= 0) {
    patchStore.setCursor(index);
  }
}

function loadMore(): void {
  pageSize.value = Math.min(pageSize.value + 50, 200);
}

const { acceptSuggestion, pickAlternative, ignoreCurrent, revertCurrent } = useInboxKeys({
  items,
  currentDetail: () => currentDetail.value ?? undefined,
  containerRef: findingListContainerRef,
  store: patchStore,
  backend,
  onOpenOrder: (modId) => void router.push({ name: "order-mod", params: { modId } }),
  onOpenModDetail: (modId) => void router.push({ name: "mod-detail", params: { modId } }),
  onFocusSearch: () => findingListRef.value?.focusSearch(),
  onToggleHelp: () => help.toggle(),
  onOpenMerge: (key) => void router.push({ name: "patch-merge-editor", params: { patchId, key } }),
  onEscape: () => emit("escape"),
});

async function saveNote(note: string): Promise<void> {
  const detail = currentDetail.value;
  if (!detail) {
    return;
  }
  await decidePatch({
    patchId,
    key: detail.key,
    action: detail.suggestion.action,
    note: note.length > 0 ? note : null,
  });
  patchStore.setExpandedKey(null);
}
</script>

<template>
  <div
    class="grid h-[36rem] grid-cols-[24rem_1fr] gap-4"
    data-testid="patch-inbox"
  >
    <div
      ref="findingListContainer"
      class="surface-card overflow-hidden"
    >
      <p
        v-if="isPending"
        class="text-text-muted p-4 text-sm"
        data-testid="patch-inbox-loading"
      >
        {{ t("common.loading") }}
      </p>
      <p
        v-else-if="error"
        class="text-status-danger p-4 text-sm"
        data-testid="patch-inbox-error"
      >
        {{ error instanceof Error ? error.message : t("patches.inbox.loadFailed") }}
      </p>
      <FindingList
        v-else
        ref="findingList"
        :items="items"
        :total="total"
        :active-key="currentKey"
        :store="patchStore"
        @select="selectRow"
        @load-more="loadMore"
      />
    </div>

    <div class="surface-card overflow-y-auto">
      <SuggestionPanel
        v-if="currentDetail"
        :detail="currentDetail"
        :note-open="patchStore.expandedKey === currentDetail.key"
        :patch-id="patchId"
        @accept="acceptSuggestion"
        @pick-alternative="pickAlternative"
        @ignore="ignoreCurrent"
        @revert="revertCurrent"
        @open-note="patchStore.setExpandedKey(currentDetail.key)"
        @save-note="saveNote"
        @close-note="patchStore.setExpandedKey(null)"
      />
      <p
        v-else
        class="text-text-muted p-4 text-sm"
        data-testid="patch-inbox-no-selection"
      >
        {{ t("inbox.page.noSelection") }}
      </p>
    </div>

    <Dialog
      v-model:visible="help.visible.value"
      modal
      :header="t('inbox.page.shortcutsHeader')"
      data-testid="patch-shortcut-help"
    >
      <!-- eslint-disable vue/no-bare-strings-in-template -- `BaseKbd`'s
           `label` names a literal keyboard key/shortcut ("j/k", "1-9",
           "?", …), which this app's i18n conventions keep in Latin
           script in every locale (`apps/desktop/CLAUDE.md`'s "keyboard-
           shortcut letters" rule) — every other string in this block
           already goes through `t()`. -->
      <ul class="flex flex-col gap-2 text-sm">
        <li><BaseKbd label="j/k, ↓/↑" /> {{ t("inbox.page.shortcutMoveCursor") }}</li>
        <li><BaseKbd label="Enter" /> {{ t("inbox.page.shortcutAccept") }}</li>
        <li><BaseKbd label="1-9" /> {{ t("inbox.page.shortcutPickAlternative") }}</li>
        <li><BaseKbd label="i" /> {{ t("inbox.page.shortcutIgnore") }}</li>
        <li><BaseKbd label="u" /> {{ t("inbox.page.shortcutRevert") }}</li>
        <li><BaseKbd label="m" /> {{ t("inbox.page.shortcutMerge") }}</li>
        <li><BaseKbd label="n" /> {{ t("inbox.page.shortcutAddNote") }}</li>
        <li><BaseKbd label="o" /> {{ t("inbox.page.shortcutOpenOrder") }}</li>
        <li><BaseKbd label="g" /> {{ t("inbox.page.shortcutOpenModDetail") }}</li>
        <li><BaseKbd label="/" /> {{ t("inbox.page.shortcutFocusSearch") }}</li>
        <li><BaseKbd label="f" /> {{ t("inbox.page.shortcutCycleStatus") }}</li>
        <li><BaseKbd label="s" /> {{ t("patches.inbox.shortcutFocusScopeSearch") }}</li>
        <li><BaseKbd label="x" /> {{ t("patches.inbox.shortcutFocusExportDir") }}</li>
        <li><BaseKbd label="Esc" /> {{ t("patches.inbox.shortcutBackToPatch") }}</li>
        <li><BaseKbd label="?" /> {{ t("inbox.page.shortcutToggleHelp") }}</li>
      </ul>
      <!-- eslint-enable vue/no-bare-strings-in-template -->
    </Dialog>
  </div>
</template>
