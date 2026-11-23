import { PiniaColada, useQuery } from "@pinia/colada";
import { invoke } from "@tauri-apps/api/core";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";

import { EMPTY_PENDING_ACTIVE_CHANGES } from "@/components/apply/ApplyDialog.test-support";
import {
  useActivateModsMutation,
  useInactiveModsGroupedQuery,
  useWorkingModsGroupedQuery,
} from "@/queries/activeSet";
import { installMockIpc } from "@/services/ipc.mock";
import type { ActivateModsResultDto } from "@/types/generated/ActivateModsResultDto";
import type { ActivateRequestDto } from "@/types/generated/ActivateRequestDto";
import type { GroupedListQueryDto } from "@/types/generated/GroupedListQueryDto";
import type { GroupedModListDto } from "@/types/generated/GroupedModListDto";

const RESULT: ActivateModsResultDto = {
  pending: EMPTY_PENDING_ACTIVE_CHANGES,
  activated: [],
  alreadyActive: [],
  selected: "current",
  didSwitchToCurrent: false,
};

/**
 * A committed activation must stay a success when an unrelated query's refetch fails: Colada
 * awaits `onSuccess` inside `mutate`, so a raw `invalidateQueries` there would flip the mutation
 * to `error`, fire the app-wide `mutationOptions.onError` toast for the wrong operation and
 * reject `mutateAsync`.
 */
describe("useActivateModsMutation", () => {
  afterEach(() => {
    clearMocks();
  });

  it("stays successful, with no error toast, when an unrelated refetch fails", async () => {
    let reads = 0;
    installMockIpc({
      activate_mods: () => RESULT,
      unrelated_read: () => {
        reads += 1;
        if (reads > 1) {
          throw new Error("refetch failed");
        }
        return "first";
      },
    });
    const onError = vi.fn();
    let mutation!: ReturnType<typeof useActivateModsMutation>;
    const Harness = defineComponent({
      setup() {
        mutation = useActivateModsMutation();
        useQuery({ key: ["unrelated"], query: () => invoke<string>("unrelated_read") });
        return {};
      },
      template: "<div />",
    });
    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mount(Harness, {
      global: { plugins: [pinia, [PiniaColada, { mutationOptions: { onError } }]] },
    });
    await flushPromises();

    const request: ActivateRequestDto = {
      ids: ["author.mod"],
      withDependencies: false,
      anchor: null,
    };
    await expect(mutation.mutateAsync(request)).resolves.toEqual(RESULT);

    expect(reads).toBe(2);
    expect(mutation.status.value).toBe("success");
    expect(onError).not.toHaveBeenCalled();
    wrapper.unmount();
  });
});

function groupedAnswer(total: number): GroupedModListDto {
  return { total, items: [], reorderable: false, entries: [], totalEntries: 0 };
}

function groupedQuery(search: string | null): GroupedListQueryDto {
  return { list: { search, tag: null, source: null, sort: null }, collapsed: [] };
}

/**
 * A fold or a keystroke changes the query key. The Groups list keeps the previous answer on
 * screen while the new one loads (`placeholderData`: Colada reads it as `success` with
 * `isPlaceholderData`), so the pane neither blanks nor loses its scroll position.
 */
describe.each([
  ["Active", useWorkingModsGroupedQuery, "list_working_mods_grouped"],
  ["Inactive", useInactiveModsGroupedQuery, "list_inactive_mods_grouped"],
] as const)("the %s Groups query", (_pane, useGrouped, command) => {
  afterEach(() => {
    clearMocks();
  });

  it("keeps the previous answer visible while a changed key loads", async () => {
    let finishSecond!: (answer: GroupedModListDto) => void;
    installMockIpc({
      [command]: (payload: unknown) => {
        const { query } = payload as { query: GroupedListQueryDto };
        if (query.list.search === null) {
          return groupedAnswer(1);
        }
        return new Promise<GroupedModListDto>((resolve) => {
          finishSecond = resolve;
        });
      },
    });
    const query = ref(groupedQuery(null));
    let result!: ReturnType<typeof useGrouped>;
    const Harness = defineComponent({
      setup() {
        result = useGrouped(query);
        return {};
      },
      template: "<div />",
    });
    const pinia = createPinia();
    setActivePinia(pinia);
    const wrapper = mount(Harness, { global: { plugins: [pinia, PiniaColada] } });
    await flushPromises();
    expect(result.data.value?.total).toBe(1);

    query.value = groupedQuery("alpha");
    await flushPromises();

    expect(result.isLoading.value).toBe(true);
    expect(result.data.value?.total).toBe(1);
    finishSecond(groupedAnswer(2));
    await flushPromises();
    expect(result.data.value?.total).toBe(2);
    wrapper.unmount();
  });
});
