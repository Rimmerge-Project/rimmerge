import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it } from "vitest";
import { defineComponent, nextTick } from "vue";

import { useGroupScope } from "@/composables/useGroupScope";
import { RimmergeError } from "@/services/ipc";
import { installMockIpc } from "@/services/ipc.mock";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { OrganizationDto } from "@/types/generated/OrganizationDto";

const WEAPONS: GroupNodeDto = {
  id: 1,
  name: "Weapons",
  color: null,
  parent: null,
  depth: 1,
  childCount: 0,
  directCount: 0,
  scopeCount: 0,
};

let wrappers: ReturnType<typeof mount>[] = [];
let organization: OrganizationDto = { kind: "loaded", groups: [WEAPONS], coloredModCount: 0 };
let organizationReads = 0;

async function mountScope() {
  organizationReads = 0;
  installMockIpc({
    get_organization: () => {
      organizationReads += 1;
      return organization;
    },
  });
  let scope: ReturnType<typeof useGroupScope> | null = null;
  const wrapper = mount(
    defineComponent({
      setup() {
        scope = useGroupScope();
        return () => null;
      },
    }),
    { global: { plugins: [createPinia(), PiniaColada] } },
  );
  wrappers.push(wrapper);
  await flushPromises();
  if (scope === null) {
    throw new Error("the composable did not run");
  }
  return scope as ReturnType<typeof useGroupScope>;
}

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
  organization = { kind: "loaded", groups: [WEAPONS], coloredModCount: 0 };
});

describe("useGroupScope", () => {
  it("starts on every mod and exposes the tree once loaded", async () => {
    const groups = await mountScope();

    expect(groups.scope.value).toEqual({ kind: "all" });
    expect(groups.isUsable.value).toBe(true);
    expect(groups.nodes.value.map((node) => node.name)).toEqual(["Weapons"]);
  });

  it("keeps a scope on a group that exists", async () => {
    const groups = await mountScope();

    groups.scope.value = { kind: "group", id: 1 };
    await nextTick();

    expect(groups.scope.value).toEqual({ kind: "group", id: 1 });
  });

  it("drops a scope on a group that is not in the tree, quietly", async () => {
    const groups = await mountScope();

    groups.scope.value = { kind: "group", id: 99 };
    await nextTick();

    expect(groups.scope.value).toEqual({ kind: "all" });
  });

  it("drops any scope when organization is not usable, so a Recovered rescan shows no error pane", async () => {
    organization = { kind: "recovered", issue: { kind: "malformed" } };
    const groups = await mountScope();

    groups.scope.value = { kind: "ungrouped" };
    await nextTick();

    expect(groups.isUsable.value).toBe(false);
    expect(groups.nodes.value).toEqual([]);
    expect(groups.scope.value).toEqual({ kind: "all" });
  });

  it("drops the scope and refetches the tree when a list failed because the group is gone", async () => {
    const groups = await mountScope();
    groups.scope.value = { kind: "group", id: 1 };
    await nextTick();
    const readsBefore = organizationReads;

    groups.dropScopeIfStale(
      new RimmergeError({ code: "group_not_found", message: "no group 1", detail: null }),
    );
    await flushPromises();

    expect(groups.scope.value).toEqual({ kind: "all" });
    expect(organizationReads).toBeGreaterThan(readsBefore);
  });

  it("keeps the scope for any other list failure", async () => {
    const groups = await mountScope();
    groups.scope.value = { kind: "group", id: 1 };
    await nextTick();

    groups.dropScopeIfStale(new RimmergeError({ code: "internal", message: "boom", detail: null }));
    groups.dropScopeIfStale(new Error("not from the backend"));

    expect(groups.scope.value).toEqual({ kind: "group", id: 1 });
  });
});
