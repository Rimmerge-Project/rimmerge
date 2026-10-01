import { describe, expect, it, vi } from "vitest";

import { handleSessionChanged } from "@/composables/useSessionEvents";
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
