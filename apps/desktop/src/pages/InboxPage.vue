<script setup lang="ts">
import Dialog from "primevue/dialog";
import { computed, onMounted, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import BaseKbd from "@/components/base/BaseKbd.vue";
import FindingList from "@/components/inbox/FindingList.vue";
import SuggestionPanel from "@/components/inbox/SuggestionPanel.vue";
import { useInboxKeys } from "@/composables/useInboxKeys";
import { useShortcutHelp } from "@/composables/useShortcutHelp";
import { useDecideMutation, useFindingQuery, useFindingsQuery } from "@/queries/findings";
import { useInboxStore } from "@/stores/inbox";
import { asFindingKey } from "@/types/brands";

const { t } = useI18n();
const route = useRoute();
const router = useRouter();
const inbox = useInboxStore();
const help = useShortcutHelp();

// A `?search=` query param (e.g. from an order-page `EdgeChip`'s
// "finding" link) seeds the inbox's own search filter once, on arrival.
// `?mod=` (the mod info panel's own "Findings" link) seeds the mod
// filter the same way.
onMounted(() => {
  const search = route.query["search"];
  if (typeof search === "string" && search.length > 0) {
    inbox.setSearch(search);
  }
  const mod = route.query["mod"];
  if (typeof mod === "string" && mod.length > 0) {
    inbox.setModId(mod);
  }
});

const pageSize = ref(50);
// The store replaces `filter` wholesale on every change (immutable
// update, never mutated in place), so a shallow watch on the getter
// already sees each change — no `deep: true` needed.
watch(
  () => inbox.filter,
  () => {
    pageSize.value = 50;
  },
);

const {
  data: page,
  isPending,
  error,
} = useFindingsQuery(() => ({
  status: inbox.filter.status,
  kinds: inbox.filter.kinds,
  modId: inbox.filter.modId,
  search: inbox.filter.search.length > 0 ? inbox.filter.search : null,
  offset: 0,
  limit: pageSize.value,
}));

const items = computed(() => page.value?.items ?? []);
const total = computed(() => page.value?.total ?? 0);
// Clamped, not just read, so a fake/backend that removes the decided key
// from `items` (shrinking the list) can never leave the cursor pointing
// past the end — `watch` rather than a `computed` because `inbox.cursor`
// itself needs correcting in the store, not just what's displayed.
watch(items, (newItems) => {
  const maxIndex = Math.max(newItems.length - 1, 0);
  if (inbox.cursor > maxIndex) {
    inbox.setCursor(maxIndex);
  }
});
const currentKey = computed(() => items.value[inbox.cursor]?.key ?? null);

const { data: currentDetail } = useFindingQuery(() =>
  currentKey.value ? asFindingKey(currentKey.value) : null,
);

const { mutateAsync: decide } = useDecideMutation();

// A local shape, not `InstanceType<typeof FindingList>`: referencing the
// component only by type here (its only value-usage is in the template)
// would read as "type-only" to a script-level linter and risk the
// import being rewritten to `import type`, which breaks `<template>`'s
// resolution of `<FindingList>` as a component.
interface FindingListInstance {
  focusSearch: () => void;
}
const findingListRef = useTemplateRef<FindingListInstance>("findingList");
const findingListContainerRef = useTemplateRef<HTMLDivElement>("findingListContainer");

function selectRow(key: string): void {
  const index = items.value.findIndex((item) => item.key === key);
  if (index >= 0) {
    inbox.setCursor(index);
  }
}

function loadMore(): void {
  pageSize.value = Math.min(pageSize.value + 50, 200);
}

// The mouse-driven buttons in `SuggestionPanel` below reuse the exact
// same deciding functions the keyboard shortcuts do — one
// mutation/invalidation path, not two copies of it.
const { acceptSuggestion, pickAlternative, ignoreCurrent, revertCurrent } = useInboxKeys({
  items,
  currentDetail: () => currentDetail.value ?? undefined,
  containerRef: findingListContainerRef,
  onOpenOrder: (modId) => void router.push({ name: "order-mod", params: { modId } }),
  onOpenModDetail: (modId) => void router.push({ name: "mod-detail", params: { modId } }),
  onFocusSearch: () => findingListRef.value?.focusSearch(),
  onToggleHelp: () => help.toggle(),
  // `router.push`'s object form percent-encodes params itself (a finding
  // key's canonical text can contain `:`/`[`/`]`/`,`/`/`), and the
  // matching route reads back the decoded value — encoding it here too
  // would double-encode.
  onOpenMerge: (key) => void router.push({ name: "merge-editor", params: { key } }),
});

async function saveNote(note: string): Promise<void> {
  const detail = currentDetail.value;
  if (!detail) {
    return;
  }
  await decide({
    key: detail.key,
    action: detail.suggestion.action,
    note: note.length > 0 ? note : null,
  });
  inbox.setExpandedKey(null);
}
</script>

<template>
  <div class="grid h-full grid-cols-[24rem_1fr] gap-4 p-4">
    <div
      ref="findingListContainer"
      class="surface-card overflow-hidden"
    >
      <p
        v-if="isPending"
        class="text-text-muted p-4 text-sm"
        data-testid="inbox-loading"
      >
        {{ t("common.loading") }}
      </p>
      <p
        v-else-if="error"
        class="text-status-danger p-4 text-sm"
        data-testid="inbox-error"
      >
        {{ error instanceof Error ? error.message : t("inbox.page.loadFailed") }}
      </p>
      <FindingList
        v-else
        ref="findingList"
        :items="items"
        :total="total"
        :active-key="currentKey"
        :store="inbox"
        @select="selectRow"
        @load-more="loadMore"
      />
    </div>

    <div class="surface-card overflow-y-auto">
      <SuggestionPanel
        v-if="currentDetail"
        :detail="currentDetail"
        :note-open="inbox.expandedKey === currentDetail.key"
        @accept="acceptSuggestion"
        @pick-alternative="pickAlternative"
        @ignore="ignoreCurrent"
        @revert="revertCurrent"
        @open-note="inbox.setExpandedKey(currentDetail.key)"
        @save-note="saveNote"
        @close-note="inbox.setExpandedKey(null)"
      />
      <p
        v-else
        class="text-text-muted p-4 text-sm"
        data-testid="inbox-no-selection"
      >
        {{ t("inbox.page.noSelection") }}
      </p>
    </div>

    <Dialog
      v-model:visible="help.visible.value"
      modal
      :header="t('inbox.page.shortcutsHeader')"
      data-testid="shortcut-help"
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
        <li><BaseKbd label="t" /> {{ t("inbox.page.shortcutToggleModLabel") }}</li>
        <li><BaseKbd label="?" /> {{ t("inbox.page.shortcutToggleHelp") }}</li>
      </ul>
      <!-- eslint-enable vue/no-bare-strings-in-template -->
    </Dialog>
  </div>
</template>
