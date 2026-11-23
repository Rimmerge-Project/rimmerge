import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { nextTick, ref } from "vue";

import { useModListFilters } from "@/composables/useModListFilters";
import type { ListSortDto } from "@/types/generated/ListSortDto";
import { ALL_MODS } from "@/utils/organization";

const SORT: ListSortDto = { key: "loadOrder", direction: "ascending" };

describe("useModListFilters.clear", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("empties the search, tag and source in the query at once, without the search debounce", async () => {
    const filters = useModListFilters({ sort: ref(SORT), group: ALL_MODS });
    filters.search.value = "first";
    filters.tag.value = "combat";
    filters.source.value = "local";
    await vi.advanceTimersByTimeAsync(400);
    expect(filters.query.value.search).toBe("first");

    // Typed again, and not yet debounced into the query when the filter is cleared.
    filters.search.value = "second";
    filters.clear();

    expect(filters.query.value.search).toBeNull();
    expect(filters.query.value.tag).toBeNull();
    expect(filters.query.value.source).toBeNull();
    expect(filters.isFiltered.value).toBe(false);
    expect(filters.search.value).toBe("");
  });

  it("does not let a debounce already waiting bring the old search back", async () => {
    const filters = useModListFilters({ sort: ref(SORT), group: ALL_MODS });
    filters.search.value = "pending";
    await nextTick();

    filters.clear();
    await vi.advanceTimersByTimeAsync(500);

    expect(filters.query.value.search).toBeNull();
  });
});
