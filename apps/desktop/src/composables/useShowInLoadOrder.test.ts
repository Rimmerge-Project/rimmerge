import { describe, expect, it, vi } from "vitest";
import { nextTick, ref } from "vue";

import { useShowInLoadOrder } from "@/composables/useShowInLoadOrder";
import type { ListSortDto } from "@/types/generated/ListSortDto";
import type { ModListDto } from "@/types/generated/ModListDto";
import { ACTIVE_DEFAULT_SORT } from "@/utils/modListSort";
import type { PaneView } from "@/utils/panePrefs";

function listOf(...ids: string[]): ModListDto {
  return {
    total: ids.length,
    reorderable: false,
    items: ids.map((modId) => ({ modId })),
  } as unknown as ModListDto;
}

interface PendingRefresh {
  resolve: (rows: ModListDto | null) => void;
}

/**
 * A fake Active pane that models Colada's behaviour: switching to a key that was used before shows
 * that key's cached rows at once (stale, `isPending` false) while the fetch runs, and `refresh`
 * answers only when the test says so.
 */
function setup(
  start: {
    view?: PaneView;
    sort?: ListSortDto;
    list?: ModListDto;
    isFiltered?: boolean;
    cachedLoadOrder?: ModListDto;
  } = {},
) {
  const view = ref<PaneView>(start.view ?? "groups");
  const sort = ref<ListSortDto>(start.sort ?? { key: "name", direction: "ascending" });
  const list = ref<ModListDto | undefined>(start.list ?? listOf("a", "b", "c"));
  const isFiltered = ref(start.isFiltered ?? false);
  const filterSignature = ref("typed-nothing");
  const cache = new Map<string, ModListDto>();
  if (start.cachedLoadOrder) {
    cache.set("list:loadOrder:ascending", start.cachedLoadOrder);
  }
  const keyOf = (): string => `${view.value}:${sort.value.key}:${sort.value.direction}`;
  const showKey = (): void => {
    list.value = cache.get(keyOf());
  };

  const pending: PendingRefresh[] = [];
  const refresh = vi.fn(
    () =>
      new Promise<ModListDto | null>((resolve) => {
        pending.push({ resolve });
      }),
  );
  const focusRow = vi.fn(async (_modId: string) => undefined);
  const announce = vi.fn(async (_text: string) => undefined);
  const clearFilters = vi.fn(() => {
    isFiltered.value = false;
    filterSignature.value = "cleared";
  });
  const shown = useShowInLoadOrder({
    list,
    refresh,
    view,
    sort,
    isFiltered,
    filterSignature,
    setView: (next) => {
      view.value = next;
      showKey();
    },
    setSort: (next) => {
      sort.value = next;
      showKey();
    },
    clearFilters,
    clearedNote: (modId) => `cleared for ${modId}`,
    announcement: (modId) => `showing ${modId}`,
    focusRow,
    announce,
  });

  /** The oldest unanswered refresh gets `rows` (`null` is a failed query). */
  function answer(rows: ModListDto | null): void {
    const next = pending.shift();
    if (next === undefined) {
      throw new Error("no refresh is waiting");
    }
    if (rows !== null) {
      cache.set(keyOf(), rows);
      list.value = rows;
    }
    next.resolve(rows);
  }

  return {
    view,
    sort,
    list,
    isFiltered,
    filterSignature,
    refresh,
    focusRow,
    announce,
    clearFilters,
    shown,
    answer,
    pendingCount: () => pending.length,
  };
}

async function flush(): Promise<void> {
  for (let tick = 0; tick < 6; tick++) {
    await nextTick();
  }
}

describe("useShowInLoadOrder", () => {
  it("switches to List in load order and focuses the mod once the fresh answer arrives", async () => {
    const pane = setup();

    void pane.shown.show("b");
    expect(pane.view.value).toBe("list");
    expect(pane.sort.value).toEqual(ACTIVE_DEFAULT_SORT);
    await flush();
    expect(pane.focusRow).not.toHaveBeenCalled();

    pane.answer(listOf("c", "b", "a"));
    await flush();

    expect(pane.focusRow).toHaveBeenCalledExactlyOnceWith("b");
    expect(pane.announce).toHaveBeenCalledWith("showing b");
  });

  it("does not act on the cached rows of a key used before, which lack a mod activated since", async () => {
    const pane = setup({ cachedLoadOrder: listOf("a", "b"), isFiltered: false });

    void pane.shown.show("x");
    await flush();
    // The cached (stale) rows are on screen already; they lack x, and nothing may end on them.
    expect(pane.list.value).toEqual(listOf("a", "b"));
    expect(pane.focusRow).not.toHaveBeenCalled();
    expect(pane.shown.note.value).toBeNull();

    pane.answer(listOf("a", "b", "x"));
    await flush();

    expect(pane.focusRow).toHaveBeenCalledExactlyOnceWith("x");
    expect(pane.clearFilters).not.toHaveBeenCalled();
  });

  it("does not clear a filter, or say so, because stale cached rows lack the mod", async () => {
    const pane = setup({ cachedLoadOrder: listOf("a"), isFiltered: true });

    void pane.shown.show("x");
    await flush();
    pane.answer(listOf("a", "x"));
    await flush();

    expect(pane.clearFilters).not.toHaveBeenCalled();
    expect(pane.shown.note.value).toBeNull();
    expect(pane.focusRow).toHaveBeenCalledWith("x");
  });

  it("keeps the filter and leaves no note when the mod is still shown under it", async () => {
    const pane = setup({ isFiltered: true });
    void pane.shown.show("b");
    await flush();

    pane.answer(listOf("b"));
    await flush();

    expect(pane.clearFilters).not.toHaveBeenCalled();
    expect(pane.shown.note.value).toBeNull();
    expect(pane.focusRow).toHaveBeenCalledWith("b");
  });

  it("clears the filter with a note when the filter hides the mod, then focuses it", async () => {
    const pane = setup({ isFiltered: true });
    void pane.shown.show("z");
    await flush();

    pane.answer(listOf("a"));
    await flush();
    expect(pane.clearFilters).toHaveBeenCalledOnce();
    expect(pane.shown.note.value).toBe("cleared for z");
    expect(pane.focusRow).not.toHaveBeenCalled();

    pane.answer(listOf("a", "z"));
    await flush();

    expect(pane.focusRow).toHaveBeenCalledExactlyOnceWith("z");
    expect(pane.announce).toHaveBeenCalledWith("cleared for z");
  });

  it("handles a hidden mod when the pane already shows load order", async () => {
    const pane = setup({
      view: "list",
      sort: ACTIVE_DEFAULT_SORT,
      isFiltered: true,
      list: listOf("a"),
    });

    void pane.shown.show("z");
    await flush();
    pane.answer(listOf("a"));
    await flush();
    expect(pane.clearFilters).toHaveBeenCalledOnce();

    pane.answer(listOf("a", "z"));
    await flush();
    expect(pane.focusRow).toHaveBeenCalledWith("z");
  });

  it("gives up without clearing anything when an unfiltered list lacks the mod", async () => {
    const pane = setup({ isFiltered: false });
    void pane.shown.show("gone");
    await flush();

    pane.answer(listOf("a", "b"));
    await flush();

    expect(pane.clearFilters).not.toHaveBeenCalled();
    expect(pane.focusRow).not.toHaveBeenCalled();
    expect(pane.shown.note.value).toBeNull();
  });

  it("does not clear twice when the mod is still missing after the filter was cleared", async () => {
    const pane = setup({ isFiltered: true });
    void pane.shown.show("gone");
    await flush();
    pane.answer(listOf("a"));
    await flush();

    pane.answer(listOf("a", "b"));
    await flush();

    expect(pane.clearFilters).toHaveBeenCalledOnce();
    expect(pane.focusRow).not.toHaveBeenCalled();
  });

  it("drops the cleared-filter note when the mod is still missing after the filter was cleared", async () => {
    const pane = setup({ isFiltered: true });
    void pane.shown.show("gone");
    await flush();
    pane.answer(listOf("a"));
    await flush();
    expect(pane.shown.note.value).toBe("cleared for gone");

    pane.answer(listOf("a", "b"));
    await flush();

    expect(pane.shown.note.value).toBeNull();
  });

  it("ends on a failed query, and a later answer does nothing", async () => {
    const pane = setup({ isFiltered: true });
    void pane.shown.show("z");
    await flush();

    pane.answer(null);
    await flush();
    pane.list.value = listOf("z");
    await flush();

    expect(pane.focusRow).not.toHaveBeenCalled();
    expect(pane.clearFilters).not.toHaveBeenCalled();
    expect(pane.pendingCount()).toBe(0);
  });

  it("ends when the user picks another sort while the answer is pending", async () => {
    const pane = setup({ isFiltered: true });
    void pane.shown.show("z");
    await flush();

    pane.sort.value = { key: "name", direction: "descending" };
    await flush();
    pane.answer(listOf("a"));
    await flush();

    expect(pane.clearFilters).not.toHaveBeenCalled();
    expect(pane.focusRow).not.toHaveBeenCalled();
  });

  it("ends when the user leaves List view while the answer is pending", async () => {
    const pane = setup();
    void pane.shown.show("b");
    await flush();

    pane.view.value = "groups";
    await flush();
    pane.answer(listOf("a", "b"));
    await flush();

    expect(pane.focusRow).not.toHaveBeenCalled();
  });

  it("never wipes a search typed while the answer is pending", async () => {
    const pane = setup({ isFiltered: true });
    void pane.shown.show("z");
    await flush();

    pane.filterSignature.value = "user typed something";
    pane.answer(listOf("a"));
    await flush();

    expect(pane.clearFilters).not.toHaveBeenCalled();
    expect(pane.filterSignature.value).toBe("user typed something");
    expect(pane.focusRow).not.toHaveBeenCalled();
    expect(pane.shown.note.value).toBeNull();
  });

  it("ends when the user changes a filter after it cleared the old one", async () => {
    const pane = setup({ isFiltered: true });
    void pane.shown.show("z");
    await flush();
    pane.answer(listOf("a"));
    await flush();

    pane.filterSignature.value = "user typed something";
    pane.answer(listOf("a", "z"));
    await flush();

    expect(pane.focusRow).not.toHaveBeenCalled();
  });

  it("a second request supersedes the first: only the second mod is focused", async () => {
    const pane = setup();
    void pane.shown.show("a");
    await flush();
    void pane.shown.show("b");
    await flush();
    expect(pane.pendingCount()).toBe(2);

    pane.answer(listOf("a", "b"));
    pane.answer(listOf("a", "b"));
    await flush();

    expect(pane.focusRow).toHaveBeenCalledExactlyOnceWith("b");
    expect(pane.announce).toHaveBeenCalledExactlyOnceWith("showing b");
  });

  it("a double click before the first answer was even asked for asks once", async () => {
    const pane = setup();
    void pane.shown.show("a");
    void pane.shown.show("b");
    await flush();

    expect(pane.refresh).toHaveBeenCalledOnce();
    pane.answer(listOf("a", "b"));
    await flush();
    expect(pane.focusRow).toHaveBeenCalledExactlyOnceWith("b");
  });

  it("drops the note when the user leaves load order or filters again", async () => {
    const pane = setup({ isFiltered: true });
    void pane.shown.show("z");
    await flush();
    pane.answer(listOf("a"));
    await flush();
    expect(pane.shown.note.value).not.toBeNull();

    pane.sort.value = { key: "name", direction: "ascending" };
    await flush();
    expect(pane.shown.note.value).toBeNull();

    const other = setup({ isFiltered: true });
    void other.shown.show("z");
    await flush();
    other.answer(listOf("a"));
    await flush();
    other.isFiltered.value = true;
    await flush();
    expect(other.shown.note.value).toBeNull();
  });

  it("says the pane shows load order only in List view with the default sort", () => {
    const listInLoadOrder = setup({ view: "list", sort: ACTIVE_DEFAULT_SORT });
    const groupsInLoadOrder = setup({ view: "groups", sort: ACTIVE_DEFAULT_SORT });
    const listByName = setup({ view: "list" });
    const listDescending = setup({
      view: "list",
      sort: { key: "loadOrder", direction: "descending" },
    });

    expect(listInLoadOrder.shown.isLoadOrderShown.value).toBe(true);
    expect(groupsInLoadOrder.shown.isLoadOrderShown.value).toBe(false);
    expect(listByName.shown.isLoadOrderShown.value).toBe(false);
    expect(listDescending.shown.isLoadOrderShown.value).toBe(false);
  });
});
