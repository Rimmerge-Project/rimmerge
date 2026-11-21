import { describe, expect, it, vi } from "vitest";

import { handleSessionChanged, ORGANIZATION_QUERY_KEYS } from "@/composables/useSessionEvents";
import { queryKeys } from "@/queries/keys";
import type { GroupedListQueryDto } from "@/types/generated/GroupedListQueryDto";
import type { ModListQueryDto } from "@/types/generated/ModListQueryDto";
import type { SessionChangeReasonDto } from "@/types/generated/SessionChangeReasonDto";

/**
 * `mergeChanged`/
 * `patchDecided` are already handled completely by
 * `useSetMergeChoicesMutation`'s own targeted `onSuccess` (the only
 * command that ever produces either reason) — invalidating everything
 * again here would refetch the same two expensive preview queries a
 * second time for no gain. Every other reason still invalidates
 * everything.
 */
describe("handleSessionChanged", () => {
  it.each<SessionChangeReasonDto>(["mergeChanged", "patchDecided"])(
    "skips invalidation for %s — already handled by the merge mutation itself",
    (reason) => {
      const invalidateAll = vi.fn();

      handleSessionChanged({ reason }, invalidateAll);

      expect(invalidateAll).not.toHaveBeenCalled();
    },
  );

  it.each<SessionChangeReasonDto>([
    "decided",
    "reverted",
    "ruleUpserted",
    "ruleDeleted",
    "tagSet",
    "imported",
    "settingsChanged",
    "applied",
    "rescanned",
    "activeSetChanged",
    "fileBaselineChanged",
    "patchCreated",
    "patchChanged",
    "patchDeleted",
    "patchReverted",
    "patchExported",
    "assignmentCreated",
    "assignmentChanged",
    "assignmentDeleted",
    "assignmentRowChanged",
    "assignmentExported",
  ])("invalidates everything for %s", (reason) => {
    const invalidateAll = vi.fn();

    handleSessionChanged({ reason }, invalidateAll);

    expect(invalidateAll).toHaveBeenCalledOnce();
  });
});

describe("handleSessionChanged for organization", () => {
  it("refetches only the organization targets, never everything", () => {
    const invalidateAll = vi.fn();
    const invalidateOrganization = vi.fn();

    handleSessionChanged({ reason: "organizationChanged" }, invalidateAll, invalidateOrganization);

    expect(invalidateOrganization).toHaveBeenCalledOnce();
    expect(invalidateAll).not.toHaveBeenCalled();
  });

  it("covers both Groups-view list queries through the Mods list prefixes", () => {
    const grouped: GroupedListQueryDto = {
      list: { search: null, tag: null, source: null, sort: null },
      collapsed: [{ kind: "ungrouped" }],
    };
    const concreteKeys = [
      queryKeys.workingModsGrouped(grouped),
      queryKeys.inactiveModsGrouped(grouped),
    ];

    for (const concrete of concreteKeys) {
      const covered = ORGANIZATION_QUERY_KEYS.some((prefix) =>
        prefix.every((part, index) => concrete[index] === part),
      );
      expect(covered, JSON.stringify(concrete)).toBe(true);
    }
  });

  it("targets the organization query and both Mods lists, as queryKeys builds them", () => {
    const query: ModListQueryDto = { search: null, tag: null, source: null, sort: null };
    const concreteKeys = [
      queryKeys.organization(),
      queryKeys.workingMods(query),
      queryKeys.inactiveMods(query),
    ];

    for (const concrete of concreteKeys) {
      const covered = ORGANIZATION_QUERY_KEYS.some((prefix) =>
        prefix.every((part, index) => concrete[index] === part),
      );
      expect(covered, JSON.stringify(concrete)).toBe(true);
    }
    expect(ORGANIZATION_QUERY_KEYS).toHaveLength(3);
  });
});
