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
    localStorage.clear();
    setActivePinia(createPinia());
  });

  it("starts at load order for Active and name A to Z for Inactive", () => {
    const { activeSort, inactiveSort } = useModListSort();

    expect(activeSort.value).toEqual(ACTIVE_DEFAULT_SORT);
    expect(inactiveSort.value).toEqual(INACTIVE_DEFAULT_SORT);
  });

  it("keeps both sorts across a rescan and a project switch", async () => {
    const session = useSessionStore();
    session.setLoaded(pathsFor("profile-a"), "current");
    const { activeSort, inactiveSort } = useModListSort();
    activeSort.value = { key: "lastUpdated", direction: "descending" };
    inactiveSort.value = { key: "packageId", direction: "descending" };

    session.setLoaded(pathsFor("profile-a"), "current");
    session.setLoaded(pathsFor("profile-b"), "current");
    await nextTick();

    expect(activeSort.value).toEqual({ key: "lastUpdated", direction: "descending" });
    expect(inactiveSort.value).toEqual({ key: "packageId", direction: "descending" });
  });

  it("restores each pane's own sort after a restart", () => {
    const first = useModListSort();
    first.activeSort.value = { key: "name", direction: "descending" };
    first.inactiveSort.value = { key: "lastUpdated", direction: "ascending" };

    setActivePinia(createPinia());
    const restarted = useModListSort();

    expect(restarted.activeSort.value).toEqual({ key: "name", direction: "descending" });
    expect(restarted.inactiveSort.value).toEqual({ key: "lastUpdated", direction: "ascending" });
  });
});
