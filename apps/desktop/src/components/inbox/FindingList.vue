<script setup lang="ts">
import { useVirtualizer, type VirtualItem } from "@tanstack/vue-virtual";
import { refDebounced } from "@vueuse/core";
import InputText from "primevue/inputtext";
import { type ComponentPublicInstance, computed, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import FindingCard from "@/components/inbox/FindingCard.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { FindingFilterStore } from "@/stores/inbox";
import type { ResolutionStatusDto } from "@/types/generated/ResolutionStatusDto";
import type { ResolutionSummaryDto } from "@/types/generated/ResolutionSummaryDto";
import { ALL_FINDING_KINDS, findingKindLabel } from "@/utils/finding";

const { t } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();

const { items, total, activeKey, store } = defineProps<{
  items: ResolutionSummaryDto[];
  total: number;
  activeKey: string | null;
  /**
   * Backs the search box and the status/kind chips — `InboxPage` passes
   * `useInboxStore()`, `PatchInbox` passes `usePatchStore()`. Injected
   * rather than looked up internally so this component works for either
   * without copying it.
   */
  store: FindingFilterStore;
}>();
const emit = defineEmits<{ select: [key: string]; loadMore: [] }>();

// The input stays responsive on every keystroke; only the value actually
// committed to the store (and so to `useFindingsQuery`'s filter) is
// debounced, so typing a search term doesn't fire a `list_findings`
// query per character. Synced both ways: the store can also change
// `filter.search` on its own (e.g. `InboxPage` seeding it from a
// `?search=` query param on arrival), which must show up in the draft
// immediately rather than being overwritten by a stale debounce.
const searchDraft = ref(store.filter.search);
const debouncedSearch = refDebounced(searchDraft, 300);
watch(debouncedSearch, (value) => {
  if (value !== store.filter.search) {
    store.setSearch(value);
  }
});
watch(
  () => store.filter.search,
  (value) => {
    if (value !== searchDraft.value) {
      searchDraft.value = value;
    }
  },
);

// `VerifyOrder`'s own `PatchWillFail` finding is deliberately excluded
// from this chip list: it's a synthetic, order-dependent prediction with
// its own dedicated `verify` surface (`ApplyDialog.vue`), not a stored,
// resolvable finding, so it can never match anything `list_findings`/the
// normal inbox surfaces — offering it here would be a dead option that
// always returns zero results.
//
// `ContributesNothing` shares the identical shape:
// `rim_session::use_cases::ContributesNothing` is a standalone use case,
// never wired into `Session::ensure_ledger`/`list_findings`, so this
// chip would always return zero rows too, exactly the trap this file's
// own comment already names for `patchWillFail` — excluded here for the
// same reason, not merely by analogy.
const FILTERABLE_KINDS = ALL_FINDING_KINDS.filter(
  (kind) => kind !== "patchWillFail" && kind !== "contributesNothing",
);

// A `labelKey` per chip, never a translated `label` — see
// `TheShell.vue`'s `NAV_ITEMS` for why a module-level constant can't
// hold translated text directly.
const STATUS_CHIPS: { status: ResolutionStatusDto | null; labelKey: string }[] = [
  { status: null, labelKey: "inbox.findingList.statusAll" },
  { status: "needsInput", labelKey: "inbox.findingList.statusNeedsInput" },
  { status: "auto", labelKey: "inbox.findingList.statusAuto" },
  { status: "userOverridden", labelKey: "inbox.findingList.statusOverridden" },
];

// Typed as the generic `ComponentPublicInstance` (not `InstanceType<typeof
// InputText>`): referencing the component only by type here, with its
// only value-usage down in the template, would read as "type-only" to a
// script-level linter and risk the import being rewritten to `import
// type` — which `<script setup>` can't render as a component.
const searchInputRef = useTemplateRef<ComponentPublicInstance>("searchInput");

/** Focuses the search box — the `/` shortcut's entry point. */
function focusSearch(): void {
  const element = searchInputRef.value?.$el as HTMLElement | undefined;
  element?.focus();
}
defineExpose({ focusSearch });

const scrollElementRef = useTemplateRef<HTMLDivElement>("scrollElement");
const ROW_HEIGHT_PX = 76;

// The whole options object is one `computed` — see `OrderTable.vue`'s
// identical comment for why.
const virtualizer = useVirtualizer(
  computed(() => ({
    count: items.length,
    getScrollElement: () => scrollElementRef.value,
    estimateSize: () => ROW_HEIGHT_PX,
    overscan: 8,
  })),
);

/**
 * Registers a row with the virtualizer's dynamic sizing (needs the matching
 * `data-index` in the template): a card's height depends on the locale (a
 * title wraps to two lines) and on its chips, so `ROW_HEIGHT_PX` is only the
 * estimate until the real height is measured.
 */
function measureRow(el: Element | ComponentPublicInstance | null): void {
  if (el instanceof Element) {
    virtualizer.value.measureElement(el);
  }
}

interface VisibleRow {
  virtualRow: VirtualItem;
  resolution: ResolutionSummaryDto;
}

const visibleRows = computed<VisibleRow[]>(() =>
  virtualizer.value
    .getVirtualItems()
    .map((virtualRow) => ({ virtualRow, resolution: items[virtualRow.index] }))
    .filter((entry): entry is VisibleRow => entry.resolution !== undefined),
);
const totalSize = computed(() => virtualizer.value.getTotalSize());

function onScroll(): void {
  const element = scrollElementRef.value;
  if (!element) {
    return;
  }
  const nearBottom = element.scrollTop + element.clientHeight >= element.scrollHeight - 200;
  if (nearBottom && items.length < total) {
    emit("loadMore");
  }
}

/**
 * Persists the "Filter by kind" disclosure's open state to the store
 * (see `store.kindsOpen`'s own doc comment) instead of leaving it as
 * uncontrolled `<details>` state, which a brief unmount — this list's
 * parent swaps to a loading placeholder while a newly-picked kind's
 * `list_findings` query has no cached page yet — would otherwise reset.
 */
function onKindsToggle(event: Event): void {
  const details = event.target as HTMLDetailsElement;
  store.setKindsOpen(details.open);
}
</script>

<template>
  <div
    class="flex h-full flex-col"
    data-testid="finding-list"
  >
    <div class="border-border-subtle flex flex-col gap-2 border-b p-2">
      <div
        v-if="store.filter.modId !== null"
        class="flex items-center gap-1"
      >
        <span
          class="bg-accent-soft text-accent flex items-center gap-1 rounded-full px-2 py-0.5 text-xs"
          data-testid="mod-filter-chip"
        >
          {{ t("inbox.findingList.filteredByMod", { mod: modLabel.label(store.filter.modId) }) }}
          <button
            type="button"
            class="cursor-pointer"
            :aria-label="t('inbox.findingList.clearModFilter')"
            data-testid="mod-filter-chip-clear"
            @click="store.setModId(null)"
          >
            <i
              class="pi pi-times text-[0.6rem]"
              aria-hidden="true"
            />
          </button>
        </span>
      </div>
      <InputText
        ref="searchInput"
        v-model="searchDraft"
        :placeholder="t('inbox.findingList.searchPlaceholder')"
        data-testid="finding-search"
        :aria-label="t('inbox.findingList.searchPlaceholder')"
      />
      <div
        class="flex flex-wrap gap-1"
        role="group"
        :aria-label="t('inbox.findingList.filterByStatus')"
      >
        <button
          v-for="chip in STATUS_CHIPS"
          :key="String(chip.status)"
          type="button"
          class="cursor-pointer rounded-full border px-2 py-0.5 text-xs"
          :class="
            store.filter.status === chip.status
              ? 'bg-accent border-accent text-on-accent'
              : 'border-border-subtle bg-surface-1 text-text-muted hover:bg-surface-2 hover:text-text'
          "
          :aria-pressed="store.filter.status === chip.status"
          :data-testid="`status-chip-${chip.status ?? 'all'}`"
          @click="store.setStatus(chip.status)"
        >
          {{ t(chip.labelKey) }}
        </button>
      </div>
      <details
        class="text-xs"
        data-testid="kind-filter-disclosure"
        :open="store.kindsOpen"
        @toggle="onKindsToggle"
      >
        <summary class="text-text-muted cursor-pointer">
          {{ t("inbox.findingList.filterByKind") }}
        </summary>
        <div
          class="mt-1 flex flex-wrap gap-1"
          role="group"
          :aria-label="t('inbox.findingList.filterByKind')"
        >
          <button
            v-for="kind in FILTERABLE_KINDS"
            :key="kind"
            type="button"
            class="cursor-pointer rounded-full border px-2 py-0.5"
            :class="
              (store.filter.kinds ?? []).includes(kind)
                ? 'bg-accent border-accent text-on-accent'
                : 'border-border-subtle bg-surface-1 text-text-muted hover:bg-surface-2 hover:text-text'
            "
            :aria-pressed="(store.filter.kinds ?? []).includes(kind)"
            :data-testid="`kind-chip-${kind}`"
            @click="store.toggleKind(kind)"
          >
            {{ tm(findingKindLabel(kind)) }}
          </button>
        </div>
      </details>
    </div>

    <p
      v-if="items.length === 0"
      class="text-text-muted p-4 text-sm"
      data-testid="finding-list-empty"
    >
      {{ t("inbox.findingList.empty") }}
    </p>
    <div
      v-else
      ref="scrollElement"
      class="flex-1 overflow-y-auto"
      role="list"
      :aria-label="t('inbox.findingList.findingsAriaLabel')"
      data-testid="finding-list-scroll"
      @scroll="onScroll"
    >
      <div :style="{ height: `${totalSize}px`, position: 'relative', width: '100%' }">
        <div
          v-for="{ virtualRow, resolution } in visibleRows"
          :key="resolution.key"
          :ref="measureRow"
          :data-index="virtualRow.index"
          role="listitem"
          class="absolute top-0 left-0 w-full"
          :style="{ transform: `translateY(${virtualRow.start}px)` }"
        >
          <FindingCard
            :resolution="resolution"
            :active="resolution.key === activeKey"
            @select="emit('select', resolution.key)"
          />
        </div>
      </div>
    </div>
  </div>
</template>
