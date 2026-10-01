import { PiniaColada, useQuery } from "@pinia/colada";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { describe, expect, it } from "vitest";
import { computed, defineComponent, ref } from "vue";

import { type PagedResult, usePagedRows } from "@/composables/usePagedRows";

function mountHarness() {
  const offset = ref(0);
  const data = ref<PagedResult<number> | null>(null);
  const resetKey = ref("a");
  let composable!: ReturnType<typeof usePagedRows<number>>;

  const Harness = defineComponent({
    setup() {
      composable = usePagedRows(offset, data, () => resetKey.value);
      return {};
    },
    template: "<div />",
  });

  const wrapper = mount(Harness);
  return {
    wrapper,
    offset,
    data,
    resetKey,
    get paged() {
      return composable;
    },
  };
}

describe("usePagedRows", () => {
  it("accumulates successive pages as offset grows", async () => {
    const { paged, data, wrapper } = mountHarness();

    data.value = { items: [1, 2], total: 4 };
    await flushPromises();
    expect(paged.rows.value).toEqual([1, 2]);
    expect(paged.hasMore.value).toBe(true);

    paged.loadMore();
    expect(paged.rows.value.length).toBe(2); // offset moved, no new page landed yet

    data.value = { items: [3, 4], total: 4 };
    await flushPromises();
    expect(paged.rows.value).toEqual([1, 2, 3, 4]);
    expect(paged.hasMore.value).toBe(false);

    wrapper.unmount();
  });

  it("replaces an already-seen offset's page instead of appending it again on an invalidation refetch", async () => {
    const { paged, data, offset, wrapper } = mountHarness();

    data.value = { items: [1, 2], total: 4 };
    await flushPromises();
    paged.loadMore();
    data.value = { items: [3, 4], total: 4 };
    await flushPromises();
    expect(paged.rows.value).toEqual([1, 2, 3, 4]);

    // A query invalidation refetches the *current* page (offset unchanged
    // at 2) and lands a brand-new array with the same content — a plain
    // "always append" strategy would double it up right here.
    expect(offset.value).toBe(2);
    data.value = { items: [3, 4], total: 4 };
    await flushPromises();

    expect(paged.rows.value).toEqual([1, 2, 3, 4]);

    wrapper.unmount();
  });

  it("resets to the first page when the reset source changes", async () => {
    const { paged, data, offset, resetKey, wrapper } = mountHarness();

    data.value = { items: [1, 2], total: 4 };
    await flushPromises();
    paged.loadMore();
    data.value = { items: [3, 4], total: 4 };
    await flushPromises();
    expect(paged.rows.value).toEqual([1, 2, 3, 4]);

    // A real caller's `data` always tracks the same key `resetSource`
    // reads, via a query library's own watcher on that key — a genuinely
    // new key with nothing cached for it clears `data` to `null`/
    // `undefined` here, same as it would in production, before this
    // composable's own reset watcher (below) runs.
    data.value = null;
    resetKey.value = "b";
    await flushPromises();

    expect(offset.value).toBe(0);
    expect(paged.rows.value).toEqual([]);
    expect(paged.total.value).toBe(0);

    data.value = { items: [10], total: 1 };
    await flushPromises();
    expect(paged.rows.value).toEqual([10]);

    wrapper.unmount();
  });

  it("recovers a real Pinia Colada query's fields on A → B → A within staleTime", async () => {
    // Reproduces the real hazard with the real library rather than a
    // hand-rolled stand-in: `useQuery`'s own internal watcher on `key` is
    // registered here *before* `usePagedRows` is called below, exactly
    // the ordering `DefConflictView.vue` has (its `useDefConflictViewQuery`
    // call precedes its `usePagedRows` call) that `usePagedRows`'s own doc
    // comment names as the root cause. Going back to `"a"` within the
    // query's default `staleTime` serves the cached entry synchronously,
    // in the same flush the reset watcher runs in.
    const key = ref<"a" | "b">("a");
    const offset = ref(0);
    let composable!: ReturnType<typeof usePagedRows<string>>;

    const Harness = defineComponent({
      setup() {
        const query = useQuery({
          key: () => ["finding6", key.value],
          query: () => Promise.resolve({ items: [`${key.value}1`], total: 1 }),
        });
        composable = usePagedRows(
          offset,
          computed(() => query.data.value ?? null),
          key,
        );
        return {};
      },
      template: "<div />",
    });

    const wrapper = mount(Harness, { global: { plugins: [createPinia(), PiniaColada] } });
    await flushPromises();
    expect(composable.rows.value).toEqual(["a1"]);

    key.value = "b";
    await flushPromises();
    expect(composable.rows.value).toEqual(["b1"]);

    // Back to "a" — the exact A → B → A hazard from the user's own report.
    key.value = "a";
    await flushPromises();
    expect(composable.rows.value).toEqual(["a1"]);

    wrapper.unmount();
  });

  it("loadMore is a no-op once every matching row is already loaded", async () => {
    const { paged, data, offset, wrapper } = mountHarness();

    data.value = { items: [1], total: 1 };
    await flushPromises();
    expect(paged.hasMore.value).toBe(false);

    paged.loadMore();
    expect(offset.value).toBe(0);

    wrapper.unmount();
  });
});
