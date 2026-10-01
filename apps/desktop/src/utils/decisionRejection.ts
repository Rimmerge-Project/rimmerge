import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { DecisionRejectionDto } from "@/types/generated/DecisionRejectionDto";
import type { UnpatchableActionDto } from "@/types/generated/UnpatchableActionDto";
import { assertNever } from "@/utils/assertNever";

/**
 * The name of each action kind a compat patch cannot carry. Keyed by
 * `satisfies Record`, so a new kind is a compile error here.
 */
const UNPATCHABLE_ACTION_NAME_KEYS = {
  accept: "patches.detail.rejection.actionName.accept",
  reorder: "patches.detail.rejection.actionName.reorder",
  preferWinner: "patches.detail.rejection.actionName.preferWinner",
  chooseCandidate: "patches.detail.rejection.actionName.chooseCandidate",
  dropEdge: "patches.detail.rejection.actionName.dropEdge",
  keepEdge: "patches.detail.rejection.actionName.keepEdge",
  addTag: "patches.detail.rejection.actionName.addTag",
  removeTag: "patches.detail.rejection.actionName.removeTag",
  excludeFromCluster: "patches.detail.rejection.actionName.excludeFromCluster",
  removeMod: "patches.detail.rejection.actionName.removeMod",
  promoteRule: "patches.detail.rejection.actionName.promoteRule",
  dropRule: "patches.detail.rejection.actionName.dropRule",
} as const satisfies Record<UnpatchableActionDto, string>;

/**
 * Why a compat patch's scope rejected a profile decision it was asked to
 * import. Exhaustive via {@link assertNever}; `label` is the caller's
 * `useModLabel().label`, since a rejection names mods.
 */
export function describeDecisionRejection(
  rejection: DecisionRejectionDto,
  label: (id: string) => string,
): MessageDescriptor {
  switch (rejection.kind) {
    case "outOfScope":
      return descriptor("patches.detail.rejection.outOfScope");
    case "notPatchable":
      return descriptor("patches.detail.rejection.notPatchable", {
        action: descriptor(UNPATCHABLE_ACTION_NAME_KEYS[rejection.action]),
      });
    case "choiceOutsideScope":
      return descriptor("patches.detail.rejection.choiceOutsideScope", {
        path: rejection.path,
        mod: label(rejection.modId),
      });
    case "assetOutsideScope":
      return descriptor("patches.detail.rejection.assetOutsideScope", {
        mod: label(rejection.modId),
      });
    default:
      return assertNever(rejection);
  }
}
