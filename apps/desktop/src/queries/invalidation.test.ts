import { PiniaColada, useQuery, useQueryCache } from "@pinia/colada";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { defineComponent } from "vue";

import { awaitInvalidation, startInvalidation } from "@/queries/invalidation";

const KEY = ["invalidation-test"];

const unhandled: unknown[] = [];
const recordUnhandled = (reason: unknown): void => {
  unhandled.push(reason);
};

beforeEach(() => {
  unhandled.length = 0;
  process.on("unhandledRejection", recordUnhandled);
});

afterEach(() => {
  process.off("unhandledRejection", recordUnhandled);
});

/**
 * Mounts one active query whose first fetch succeeds and whose later ones fail when asked. A
 * fetch takes a few microtasks to answer (`v1`, `v2`, ...), so a helper that stops waiting for
 * the refetch is observable as stale data.
 */
function mountQuery(refetchFails: boolean) {
  let fetchCount = 0;
  let queryCache!: ReturnType<typeof useQueryCache>;
  let status!: () => string;
  let data!: () => string | undefined;
  const Harness = defineComponent({
    setup() {
      queryCache = useQueryCache();
      const query = useQuery({
        key: KEY,
        query: async () => {
          fetchCount += 1;
          await Promise.resolve();
          await Promise.resolve();
          if (refetchFails && fetchCount > 1) {
            throw new Error("refetch failed");
          }
          return `v${fetchCount}`;
        },
      });
      status = () => query.status.value;
      data = () => query.data.value;
      return {};
    },
    template: "<div />",
  });
  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(Harness, { global: { plugins: [pinia, PiniaColada] } });
  return { wrapper, queryCache, status: () => status(), data: () => data() };
}

/** Lets Node deliver any `unhandledRejection` for promises that rejected so far. */
async function settleRejections(): Promise<void> {
  await flushPromises();
  await flushPromises();
}

describe("startInvalidation", () => {
  it("refetches the active query on the happy path", async () => {
    const { wrapper, queryCache, data } = mountQuery(false);
    await flushPromises();
    expect(data()).toBe("v1");

    startInvalidation(queryCache, { key: KEY });
    await settleRejections();

    expect(data()).toBe("v2");
    expect(unhandled).toEqual([]);
    wrapper.unmount();
  });

  it("leaves no unhandled rejection when the refetch fails, and the query keeps the error", async () => {
    const { wrapper, queryCache, status } = mountQuery(true);
    await flushPromises();

    startInvalidation(queryCache);
    await settleRejections();

    expect(status()).toBe("error");
    expect(unhandled).toEqual([]);
    wrapper.unmount();
  });
});

describe("awaitInvalidation", () => {
  it("resolves only after the refetch has landed", async () => {
    const { wrapper, queryCache, data } = mountQuery(false);
    await flushPromises();
    expect(data()).toBe("v1");

    await awaitInvalidation(queryCache, { key: KEY });

    expect(data()).toBe("v2");
    wrapper.unmount();
  });

  it("does not reject when a refetch fails, and the query keeps the error", async () => {
    const { wrapper, queryCache, status } = mountQuery(true);
    await flushPromises();

    await expect(awaitInvalidation(queryCache)).resolves.toBeUndefined();

    expect(status()).toBe("error");
    expect(unhandled).toEqual([]);
    wrapper.unmount();
  });
});
