import { defineStore, storeToRefs } from "pinia";
import { ref } from "vue";

import { defineFindingFilterStore } from "@/stores/inbox";

/**
 * The compat patch's own scoped-inbox cursor/filter/expanded-note state —
 * the same shape `stores/inbox.ts`'s `useInboxStore` gets from the same
 * factory, kept as its own Pinia store id so a patch's cursor/filter never
 * bleeds into the profile inbox's.
 */
const usePatchFilterStore = defineFindingFilterStore("patchInbox");

/**
 * `usePatchFilterStore` plus `activate`: makes `patchId` the active patch,
 * resetting the cursor/filter/expanded note only when a *different* patch
 * becomes active. `PatchInbox.vue` calls `activate` on every `patchId`
 * change instead of unconditionally resetting — a plain
 * `watch(..., { immediate: true })` calling `reset()` fires on every
 * mount, including a remount of the *same* patch (e.g. returning from the
 * merge editor via `Esc`), which was wiping the user's cursor position on
 * every round trip.
 */
export const usePatchStore = defineStore("patchInboxActive", () => {
  const filterStore = usePatchFilterStore();
  const { cursor, filter, expandedKey, kindsOpen } = storeToRefs(filterStore);
  const activePatchId = ref<string | null>(null);

  /**
   * Makes `patchId` the active patch. A no-op beyond recording the id when
   * it's already active (so re-running this for the same patch — a
   * remount, not a navigation to a different one — never loses the user's
   * place); resets the cursor/filter/expanded note when it names a
   * *different* patch than before.
   */
  function activate(patchId: string): void {
    if (activePatchId.value === patchId) {
      return;
    }
    activePatchId.value = patchId;
    filterStore.reset();
  }

  return {
    cursor,
    filter,
    expandedKey,
    kindsOpen,
    activePatchId,
    setCursor: filterStore.setCursor,
    setSearch: filterStore.setSearch,
    cycleStatusFilter: filterStore.cycleStatusFilter,
    setStatus: filterStore.setStatus,
    setKinds: filterStore.setKinds,
    toggleKind: filterStore.toggleKind,
    setModId: filterStore.setModId,
    setExpandedKey: filterStore.setExpandedKey,
    setKindsOpen: filterStore.setKindsOpen,
    reset: filterStore.reset,
    activate,
  };
});
