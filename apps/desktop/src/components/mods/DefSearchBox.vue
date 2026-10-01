<script setup lang="ts">
import { refDebounced } from "@vueuse/core";
import { computed, ref, useId, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import { useDefSearchQuery } from "@/queries/defs";
import { inspectRoute } from "@/utils/format";

/**
 * Jump-to-a-def search box:
 * debounced, up to 10 hits, name matches before type-only matches (the
 * backend's own ranking — see `Session::search_defs`). Reused on the mods
 * page and the def page's own header, so both reach the inspector without
 * going through a mod's changes list first.
 *
 * A WAI-ARIA combobox: `role="combobox"` on the input with
 * `aria-expanded`/`aria-controls`/`aria-activedescendant`, `role="listbox"`/
 * `"option"` on the results. Arrow keys move a keyboard-active index
 * (wrapping), Enter navigates to it, Escape closes the list without
 * navigating — focus never actually leaves the input. Losing focus
 * entirely (clicking away, tabbing off) also closes it, but that's not
 * the *only* thing a selection depends on: each result's own
 * `@mousedown.prevent` keeps the input focused through a mouse click so
 * blur never races the click that actually navigates — the real bug this
 * once had: closing the list
 * eagerly from a bare `@mousedown` handler removed the link from the DOM
 * before its own `click` ever fired, so every mouse selection silently
 * did nothing.
 */
const HIT_LIMIT = 10;

const router = useRouter();
const { t } = useI18n();
const listboxId = useId();

const search = ref("");
const debouncedSearch = refDebounced(search, 200);
const open = ref(false);
const activeIndex = ref(-1);

const { data } = useDefSearchQuery(debouncedSearch, HIT_LIMIT);
const hits = computed(() => data.value ?? []);

// Typing always reopens the list (a fresh debounce may replace an empty
// result set with hits, or vice versa); only Escape or leaving the field
// closes it again.
watch(search, () => {
  open.value = true;
});
watch(hits, (nextHits) => {
  activeIndex.value = nextHits.length > 0 ? 0 : -1;
});

function optionId(index: number): string {
  return `${listboxId}-option-${index}`;
}

function close(): void {
  open.value = false;
  activeIndex.value = -1;
}

function moveActive(delta: number): void {
  if (hits.value.length === 0) {
    return;
  }
  open.value = true;
  const count = hits.value.length;
  activeIndex.value = (activeIndex.value + delta + count) % count;
}

function selectActive(): void {
  const hit = hits.value[activeIndex.value];
  if (!hit) {
    return;
  }
  close();
  void router.push(inspectRoute(hit.defRef));
}

function handleKeydown(event: KeyboardEvent): void {
  switch (event.key) {
    case "ArrowDown":
      event.preventDefault();
      moveActive(1);
      break;
    case "ArrowUp":
      event.preventDefault();
      moveActive(-1);
      break;
    case "Enter":
      if (open.value && activeIndex.value >= 0) {
        event.preventDefault();
        selectActive();
      }
      break;
    case "Escape":
      if (open.value) {
        event.preventDefault();
        close();
      }
      break;
    default:
      break;
  }
}
</script>

<template>
  <div
    class="relative"
    data-testid="def-search-box"
  >
    <input
      v-model="search"
      type="search"
      role="combobox"
      :aria-label="t('mods.defSearch.ariaLabel')"
      aria-autocomplete="list"
      :aria-expanded="open"
      :aria-controls="listboxId"
      :aria-activedescendant="open && activeIndex >= 0 ? optionId(activeIndex) : undefined"
      :placeholder="t('mods.defSearch.placeholder')"
      class="border-border-subtle bg-surface-1 rounded border px-2 py-1 text-xs"
      data-testid="def-search-input"
      @focus="open = true"
      @blur="close"
      @keydown="handleKeydown"
    >
    <ul
      v-if="open && hits.length > 0"
      :id="listboxId"
      role="listbox"
      class="border-border-subtle bg-surface-1 absolute z-10 mt-1 flex w-64 flex-col rounded border shadow"
      data-testid="def-search-results"
    >
      <li
        v-for="(hit, index) in hits"
        :id="optionId(index)"
        :key="hit.defRef"
        role="option"
        :aria-selected="index === activeIndex"
      >
        <RouterLink
          :to="inspectRoute(hit.defRef)"
          class="text-text hover:bg-surface-2 block px-2 py-1 font-mono text-xs"
          :class="index === activeIndex ? 'bg-surface-2' : ''"
          :data-testid="`def-search-result-${hit.defRef}`"
          @mousedown.prevent
          @mouseenter="activeIndex = index"
          @click="close"
        >
          {{ hit.defRef }}
          <span class="text-text-faint">({{ hit.owners }})</span>
        </RouterLink>
      </li>
    </ul>
  </div>
</template>
