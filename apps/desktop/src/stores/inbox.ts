import { defineStore } from "pinia";
import { ref } from "vue";

import type { FindingKindDto } from "@/types/generated/FindingKindDto";
import type { ResolutionStatusDto } from "@/types/generated/ResolutionStatusDto";

/** The inbox's active filter — mirrors the fields of `FindingFilterDto` this UI exposes. */
export interface InboxFilter {
  status: ResolutionStatusDto | null;
  kinds: FindingKindDto[] | null;
  /**
   * Only findings naming this mod — seeded from a `?mod=` query param
   * (the mod info panel's own "Findings" link), never set from the
   * search box. `null` means unfiltered.
   */
  modId: string | null;
  search: string;
}

/**
 * The subset of a findings-filter store `FindingList.vue` needs to render
 * its own search box and status/kind chips. `useInboxStore` satisfies
 * this structurally; so does `usePatchStore` (`stores/patch.ts`) — the
 * shared shape is what lets `FindingList` back both the profile inbox and
 * a compat patch's own scoped inbox without copying it.
 */
export interface FindingFilterStore {
  filter: InboxFilter;
  kindsOpen: boolean;
  setSearch(search: string): void;
  setStatus(status: ResolutionStatusDto | null): void;
  toggleKind(kind: FindingKindDto): void;
  setKindsOpen(open: boolean): void;
  setModId(modId: string | null): void;
}

const DEFAULT_FILTER: InboxFilter = {
  status: "needsInput",
  kinds: null,
  modId: null,
  search: "",
};

/** `f` cycles through these, in this order, wrapping back to the start. */
const STATUS_CYCLE: readonly (ResolutionStatusDto | null)[] = [
  "needsInput",
  "auto",
  "userOverridden",
  null,
];

/**
 * Builds a Pinia setup store for one findings list's own cursor/filter/
 * expanded-note state — the shape `useInboxStore` (the profile inbox) and
 * `usePatchStore` (`stores/patch.ts`, a compat patch's own scoped inbox)
 * both need, kept in one place so the two never drift. `id` is the Pinia store id, so each
 * caller still gets its own independent, separately-persisted store
 * instance — `usePatchStore` layers `activate` on top of the store this
 * returns rather than sharing state with the profile inbox's.
 */
export function defineFindingFilterStore(id: string) {
  return defineStore(id, () => {
    const cursor = ref(0);
    const filter = ref<InboxFilter>({ ...DEFAULT_FILTER });
    const expandedKey = ref<string | null>(null);
    // Lives here, not as local state in `FindingList`, so a kind-chip click
    // (which changes `filter.kinds` and so `useFindingsQuery`'s key) can
    // never lose it even if the list re-renders or is briefly unmounted —
    // see `FindingList.vue`'s `<details>` binding.
    const kindsOpen = ref(false);

    /** Moves the cursor to `index`, clamped to a non-negative row. */
    function setCursor(index: number): void {
      cursor.value = Math.max(0, index);
    }

    /** Updates the search text, resetting the cursor to the top of the new results. */
    function setSearch(search: string): void {
      filter.value = { ...filter.value, search };
      cursor.value = 0;
    }

    /** Advances the status filter to the next value in {@link STATUS_CYCLE} (the `f` shortcut). */
    function cycleStatusFilter(): void {
      const currentIndex = STATUS_CYCLE.indexOf(filter.value.status);
      const nextIndex = (currentIndex + 1) % STATUS_CYCLE.length;
      filter.value = { ...filter.value, status: STATUS_CYCLE[nextIndex] ?? null };
      cursor.value = 0;
    }

    /** Sets the status filter directly (a status chip click). */
    function setStatus(status: ResolutionStatusDto | null): void {
      filter.value = { ...filter.value, status };
      cursor.value = 0;
    }

    /** Replaces the kind filter (`null` clears it), resetting the cursor. */
    function setKinds(kinds: FindingKindDto[] | null): void {
      filter.value = { ...filter.value, kinds };
      cursor.value = 0;
    }

    /** Toggles one kind in the kind filter on or off. */
    function toggleKind(kind: FindingKindDto): void {
      const current = filter.value.kinds ?? [];
      const next = current.includes(kind) ? current.filter((k) => k !== kind) : [...current, kind];
      setKinds(next.length > 0 ? next : null);
    }

    /** Replaces the mod filter (`null` clears it), resetting the cursor. */
    function setModId(modId: string | null): void {
      filter.value = { ...filter.value, modId };
      cursor.value = 0;
    }

    /** Expands (or, given `null`, collapses) a finding's note editor. */
    function setExpandedKey(key: string | null): void {
      expandedKey.value = key;
    }

    /** Opens or closes the "Filter by kind" disclosure. */
    function setKindsOpen(open: boolean): void {
      kindsOpen.value = open;
    }

    /** Resets every field back to its default. */
    function reset(): void {
      cursor.value = 0;
      filter.value = { ...DEFAULT_FILTER };
      expandedKey.value = null;
      kindsOpen.value = false;
    }

    return {
      cursor,
      filter,
      expandedKey,
      kindsOpen,
      setCursor,
      setSearch,
      cycleStatusFilter,
      setStatus,
      setKinds,
      toggleKind,
      setModId,
      setExpandedKey,
      setKindsOpen,
      reset,
    };
  });
}

/**
 * UI-only inbox state: the keyboard cursor's row, the active filter, and
 * which finding's note editor is expanded. Server data (the finding list
 * and each finding's detail) is Pinia Colada's job — see
 * `queries/findings.ts`; this store never holds a DTO.
 */
export const useInboxStore = defineFindingFilterStore("inbox");
