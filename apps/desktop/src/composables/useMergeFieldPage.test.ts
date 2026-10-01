import { PiniaColada, useQueryCache } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { defineComponent } from "vue";

import { useMergeFieldPage } from "@/composables/useMergeFieldPage";
import { installMockIpc } from "@/services/ipc.mock";
import { asFindingKey } from "@/types/brands";
import type { MergeFieldDto } from "@/types/generated/MergeFieldDto";
import type { MergeFieldFilterDto } from "@/types/generated/MergeFieldFilterDto";
import type { MergePreviewDto } from "@/types/generated/MergePreviewDto";
import type { MergePreviewRequestDto } from "@/types/generated/MergePreviewRequestDto";

const KEY = asFindingKey("def_override:ThingDef/Wall:[a,b]");
const TOTAL_ROWS = 250;

function field(index: number): MergeFieldDto {
  return {
    path: `field-${index}`,
    depth: 0,
    isListItem: false,
    entry: "leaf",
    container: null,
    class: "oneSided",
    changedBy: [],
    confidence: 0,
    base: "base",
    candidates: {},
    result: null,
    choice: null,
    preselected: null,
  };
}

const ALL_ROWS = Array.from({ length: TOTAL_ROWS }, (_, index) => field(index));

function previewPage(filter: MergeFieldFilterDto): MergePreviewDto {
  return {
    key: KEY,
    defKey: { defType: "ThingDef", defName: "Wall" },
    kind: "defOverride",
    owners: [{ modId: "a.mod", name: "A", position: 0 }],
    base: "a.mod",
    winner: "a.mod",
    totals: { fields: TOTAL_ROWS, unresolved: 0, conflicts: 0, auto: TOTAL_ROWS, unchanged: 0 },
    state: { kind: "complete", opCount: 0 },
    structuralGuard: null,
    caveats: [],
    fields: ALL_ROWS.slice(filter.offset, filter.offset + filter.limit),
    total: TOTAL_ROWS,
    resolvedXml: null,
    outOfScopeOwners: [],
    defRef: "ThingDef/Wall",
  };
}

function mountHarness(onlyConflicts = false, patchId: string | null = null) {
  const requests: MergePreviewRequestDto[] = [];
  installMockIpc({
    get_merge_preview: (payload: unknown) => {
      const request = (payload as { request: MergePreviewRequestDto }).request;
      requests.push(request);
      return previewPage(request.filter);
    },
  });

  let composable!: ReturnType<typeof useMergeFieldPage>;
  let invalidate!: () => Promise<unknown>;
  const Harness = defineComponent({
    setup() {
      composable = useMergeFieldPage(
        () => KEY,
        onlyConflicts,
        () => patchId,
      );
      const queryCache = useQueryCache();
      invalidate = () => queryCache.invalidateQueries();
      return {};
    },
    template: "<div />",
  });

  const pinia = createPinia();
  setActivePinia(pinia);
  const wrapper = mount(Harness, { global: { plugins: [pinia, PiniaColada] } });

  return {
    wrapper,
    requests,
    get page() {
      return composable;
    },
    invalidate: () => invalidate(),
  };
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("useMergeFieldPage", () => {
  afterEach(() => {
    clearMocks();
  });

  it("fetches the first page and reports more rows remain when total exceeds it", async () => {
    const { page, wrapper } = mountHarness();
    await flush();

    expect(page.fields.value).toHaveLength(200);
    expect(page.total.value).toBe(TOTAL_ROWS);
    expect(page.hasMore.value).toBe(true);
    wrapper.unmount();
  });

  it("loadMore appends the next page instead of replacing what's already loaded", async () => {
    const { page, requests, wrapper } = mountHarness();
    await flush();

    page.loadMore();
    await flush();

    expect(requests.at(-1)?.filter).toMatchObject({ offset: 200, limit: 200 });
    expect(page.fields.value).toHaveLength(TOTAL_ROWS);
    expect(page.fields.value.map((f) => f.path)).toEqual(ALL_ROWS.map((f) => f.path));
    expect(page.hasMore.value).toBe(false);
    wrapper.unmount();
  });

  it("loadMore is a no-op once every matching row is already loaded", async () => {
    const { page, requests, wrapper } = mountHarness();
    await flush();
    page.loadMore();
    await flush();
    const requestCountAfterFull = requests.length;

    page.loadMore();
    await flush();

    expect(requests).toHaveLength(requestCountAfterFull);
    wrapper.unmount();
  });

  it("an invalidation refetch of the loaded-past-page-one list never duplicates its rows", async () => {
    const { page, invalidate, wrapper } = mountHarness();
    await flush();
    page.loadMore();
    await flush();
    expect(page.fields.value).toHaveLength(TOTAL_ROWS);

    // `session://changed` (or any mutation) invalidates every query,
    // including this one's *current* page (offset 200) — refetching it
    // must replace that page's own slot, never append a second copy.
    await invalidate();
    await flush();

    expect(page.fields.value).toHaveLength(TOTAL_ROWS);
    expect(page.fields.value.map((f) => f.path)).toEqual(ALL_ROWS.map((f) => f.path));
    wrapper.unmount();
  });

  it("folds patchId into every get_merge_preview request when given, and null otherwise", async () => {
    const { requests: withPatch, wrapper: wrapperWithPatch } = mountHarness(false, "patch-1");
    await flush();
    expect(withPatch.every((request) => request.patchId === "patch-1")).toBe(true);
    wrapperWithPatch.unmount();

    const { requests: withoutPatch, wrapper: wrapperWithoutPatch } = mountHarness(false, null);
    await flush();
    expect(withoutPatch.every((request) => request.patchId === null)).toBe(true);
    wrapperWithoutPatch.unmount();
  });
});
