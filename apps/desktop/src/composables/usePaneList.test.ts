import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { defineComponent, nextTick, ref } from "vue";

import { type PaneView, usePaneList, usePaneViews } from "@/composables/usePaneList";
import { installMockIpc } from "@/services/ipc.mock";
import { useSessionStore } from "@/stores/session";
import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import type { GroupedListQueryDto } from "@/types/generated/GroupedListQueryDto";
import type { GroupedModListDto } from "@/types/generated/GroupedModListDto";
import type { ModListDto } from "@/types/generated/ModListDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";

const LIST_QUERY: ModListQueryDto = { search: null, tag: null, source: null, sort: null };

const LIST: ModListDto = { total: 0, items: [], reorderable: true };
const GROUPED: GroupedModListDto = {
  total: 0,
  items: [],
  reorderable: false,
  entries: [],
  totalEntries: 0,
};

let wrappers: ReturnType<typeof mount>[] = [];

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
});

function mountPane(kind: "active" | "inactive", initialView: PaneView) {
  const calls: { command: string; payload: unknown }[] = [];
  const record = (command: string, answer: unknown) => (payload: unknown) => {
    calls.push({ command, payload });
    return answer;
  };
  installMockIpc({
    list_working_mods: record("list_working_mods", LIST),
    list_inactive_mods: record("list_inactive_mods", LIST),
    list_working_mods_grouped: record("list_working_mods_grouped", GROUPED),
    list_inactive_mods_grouped: record("list_inactive_mods_grouped", GROUPED),
  });
  const view = ref<PaneView>(initialView);
  const collapsed = ref<FolderKeyDto[]>([]);
  let pane: ReturnType<typeof usePaneList> | null = null;
  const wrapper = mount(
    defineComponent({
      setup() {
        pane = usePaneList({ kind, view, query: LIST_QUERY, collapsed });
        return () => null;
      },
    }),
    { global: { plugins: [createPinia(), PiniaColada] } },
  );
  wrappers.push(wrapper);
  if (pane === null) {
    throw new Error("the composable did not run");
  }
  return { pane: pane as ReturnType<typeof usePaneList>, view, collapsed, calls };
}

describe("usePaneList", () => {
  it.each([
    ["active", "list_working_mods"],
    ["inactive", "list_inactive_mods"],
  ] as const)("the %s pane in List view runs the list query only", async (kind, command) => {
    const { calls, pane } = mountPane(kind, "list");
    await flushPromises();

    expect(calls.map((call) => call.command)).toEqual([command]);
    expect(pane.data.value).toEqual(LIST);
    expect(pane.grouped.value).toBeUndefined();
  });

  it.each([
    ["active", "list_working_mods_grouped"],
    ["inactive", "list_inactive_mods_grouped"],
  ] as const)("the %s pane in Groups view runs the grouped query only", async (kind, command) => {
    const { calls, pane } = mountPane(kind, "groups");
    await flushPromises();

    expect(calls.map((call) => call.command)).toEqual([command]);
    expect(pane.grouped.value).toEqual(GROUPED);
    // The rows are the same shape in both views, so readers of `data` need no second code path.
    expect(pane.data.value).toEqual(GROUPED);
  });

  it("sends the pane's own query with the folded folders", async () => {
    const { calls, collapsed } = mountPane("active", "groups");
    await flushPromises();
    collapsed.value = [{ kind: "group", groupId: 2 }, { kind: "ungrouped" }];
    await flushPromises();

    const expected: GroupedListQueryDto = {
      list: LIST_QUERY,
      collapsed: [{ kind: "group", groupId: 2 }, { kind: "ungrouped" }],
    };
    expect(calls.at(-1)).toEqual({
      command: "list_working_mods_grouped",
      payload: { query: expected },
    });
  });

  it("flags the previous key's rows as a placeholder until the new key's answer lands", async () => {
    const calls: unknown[] = [];
    let release: (answer: GroupedModListDto) => void = () => undefined;
    installMockIpc({
      list_working_mods_grouped: (payload: unknown) => {
        calls.push(payload);
        if (calls.length === 1) {
          return GROUPED;
        }
        return new Promise<GroupedModListDto>((resolve) => {
          release = resolve;
        });
      },
    });
    const collapsed = ref<FolderKeyDto[]>([]);
    let pane: ReturnType<typeof usePaneList> | null = null;
    wrappers.push(
      mount(
        defineComponent({
          setup() {
            pane = usePaneList({ kind: "active", view: "groups", query: LIST_QUERY, collapsed });
            return () => null;
          },
        }),
        { global: { plugins: [createPinia(), PiniaColada] } },
      ),
    );
    const current = () => pane as unknown as ReturnType<typeof usePaneList>;
    await flushPromises();
    expect(current().isPlaceholder.value).toBe(false);

    collapsed.value = [{ kind: "ungrouped" }];
    await flushPromises();
    expect(current().isPlaceholder.value).toBe(true);
    expect(current().data.value).toEqual(GROUPED);

    const answer = { ...GROUPED, total: 5 };
    release(answer);
    await flushPromises();
    expect(current().isPlaceholder.value).toBe(false);
    expect(current().data.value).toEqual(answer);
  });

  it("is never a placeholder in List view", async () => {
    const { pane } = mountPane("active", "list");
    await flushPromises();

    expect(pane.isPlaceholder.value).toBe(false);
  });

  it("switching the view starts the other query and stops reading the first", async () => {
    const { calls, pane, view } = mountPane("active", "list");
    await flushPromises();

    view.value = "groups";
    await flushPromises();

    expect(calls.map((call) => call.command)).toEqual([
      "list_working_mods",
      "list_working_mods_grouped",
    ]);
    expect(pane.grouped.value).toEqual(GROUPED);
  });
});

describe("usePaneViews", () => {
  it("starts both panes in List view and goes back to it when another project loads", async () => {
    installMockIpc({});
    let views: ReturnType<typeof usePaneViews> | null = null;
    let session: ReturnType<typeof useSessionStore> | null = null;
    const wrapper = mount(
      defineComponent({
        setup() {
          session = useSessionStore();
          views = usePaneViews();
          return () => null;
        },
      }),
      { global: { plugins: [createPinia(), PiniaColada] } },
    );
    wrappers.push(wrapper);
    if (views === null || session === null) {
      throw new Error("the composables did not run");
    }
    const { activeView, inactiveView } = views as ReturnType<typeof usePaneViews>;
    expect(activeView.value).toBe("list");
    expect(inactiveView.value).toBe("list");

    activeView.value = "groups";
    inactiveView.value = "groups";
    (session as ReturnType<typeof useSessionStore>).setLoaded(
      {
        gameDir: "g",
        modsConfig: "m",
        workshopDir: null,
        profileDir: "p-two",
      } as never,
      "current",
    );
    await nextTick();

    expect(activeView.value).toBe("list");
    expect(inactiveView.value).toBe("list");
  });
});
