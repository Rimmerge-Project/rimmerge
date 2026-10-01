import { PiniaColada, useQueryCache } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent } from "vue";

import { useSetMergeChoicesMutation } from "@/queries/merge";
import { installMockIpc } from "@/services/ipc.mock";
import { asFindingKey } from "@/types/brands";
import type { MergeStateDto } from "@/types/generated/MergeStateDto";

const KEY = asFindingKey("def_override:ThingDef/Wall:[a.mod,b.mod]");

function mountHarness(setMergeChoices: () => MergeStateDto) {
  installMockIpc({ set_merge_choices: setMergeChoices });

  let mutation!: ReturnType<typeof useSetMergeChoicesMutation>;
  let queryCache!: ReturnType<typeof useQueryCache>;
  const Harness = defineComponent({
    setup() {
      mutation = useSetMergeChoicesMutation();
      queryCache = useQueryCache();
      return {};
    },
    template: "<div />",
  });

  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(Harness, { global: { plugins: [pinia, PiniaColada] } });

  return {
    wrapper,
    get mutation() {
      return mutation;
    },
    get queryCache() {
      return queryCache;
    },
  };
}

/**
 * A merge decision must not invalidate every query on success — the same
 * blast radius as `useDecideMutation`'s own, but on a page whose two
 * preview queries are each genuinely expensive to rebuild. It must
 * invalidate only the `merge`/`findings`/`defs`+`"conflictView"` key
 * prefixes (see `useSetMergeChoicesMutation`'s own doc comment).
 */
describe("useSetMergeChoicesMutation", () => {
  afterEach(() => {
    clearMocks();
  });

  it("invalidates only the merge, findings, and defs/conflictView query key prefixes, never a bare (invalidate-everything) call", async () => {
    const { mutation, queryCache, wrapper } = mountHarness(() => ({
      kind: "complete",
      opCount: 1,
    }));
    const invalidateSpy = vi.spyOn(queryCache, "invalidateQueries");

    await mutation.mutateAsync({ key: KEY, choices: {} });

    const calledKeys = invalidateSpy.mock.calls.map((call) => call[0]?.key);
    expect(calledKeys).toContainEqual(["merge"]);
    expect(calledKeys).toContainEqual(["findings"]);
    expect(calledKeys).toContainEqual(["defs", "conflictView"]);
    expect(invalidateSpy.mock.calls.every((call) => call[0]?.key !== undefined)).toBe(true);
    wrapper.unmount();
  });

  it("invalidates each targeted prefix exactly once per decision — no double refetch of the same query", async () => {
    const { mutation, queryCache, wrapper } = mountHarness(() => ({
      kind: "complete",
      opCount: 1,
    }));
    const invalidateSpy = vi.spyOn(queryCache, "invalidateQueries");

    await mutation.mutateAsync({ key: KEY, choices: {} });

    const mergeCalls = invalidateSpy.mock.calls.filter(
      (call) => JSON.stringify(call[0]?.key) === JSON.stringify(["merge"]),
    );
    expect(mergeCalls).toHaveLength(1);
    wrapper.unmount();
  });
});
