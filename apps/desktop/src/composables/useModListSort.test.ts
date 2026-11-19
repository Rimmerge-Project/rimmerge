import { createPinia, setActivePinia } from "pinia";
import { beforeEach, describe, expect, it } from "vitest";
import { nextTick } from "vue";

import { useModListSort } from "@/composables/useModListSort";
import { useSessionStore } from "@/stores/session";
import type { ProjectPathsDto } from "@/types/generated/ProjectPathsDto";
import { ACTIVE_DEFAULT_SORT, INACTIVE_DEFAULT_SORT } from "@/utils/modListSort";

function pathsFor(profileDir: string): ProjectPathsDto {
  return { gameDir: "game", workshopDir: "workshop", modsConfig: "ModsConfig.xml", profileDir };
}

describe("useModListSort", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("starts at load order for Active and name A to Z for Inactive", () => {
    const { activeSort, inactiveSort, isActiveInLoadOrder } = useModListSort();

    expect(activeSort.value).toEqual(ACTIVE_DEFAULT_SORT);
    expect(inactiveSort.value).toEqual(INACTIVE_DEFAULT_SORT);
    expect(isActiveInLoadOrder.value).toBe(true);
  });

  it("is not in load order for any other criterion, nor for reversed load order", () => {
    const { activeSort, isActiveInLoadOrder } = useModListSort();

    activeSort.value = { key: "name", direction: "ascending" };
    expect(isActiveInLoadOrder.value).toBe(false);

    activeSort.value = { key: "loadOrder", direction: "descending" };
    expect(isActiveInLoadOrder.value).toBe(false);
  });

  it("resets both sorts when another project is loaded, but keeps them across a rescan", async () => {
    const session = useSessionStore();
    session.setLoaded(pathsFor("profile-a"), "current");
    const { activeSort, inactiveSort } = useModListSort();
    activeSort.value = { key: "lastUpdated", direction: "descending" };
    inactiveSort.value = { key: "packageId", direction: "descending" };

    session.setLoaded(pathsFor("profile-a"), "current");
    await nextTick();
    expect(activeSort.value.key).toBe("lastUpdated");

    session.setLoaded(pathsFor("profile-b"), "current");
    await nextTick();
    expect(activeSort.value).toEqual(ACTIVE_DEFAULT_SORT);
    expect(inactiveSort.value).toEqual(INACTIVE_DEFAULT_SORT);
  });
});
