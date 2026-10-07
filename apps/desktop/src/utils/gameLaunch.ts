import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { GameLaunchRouteDto } from "@/types/generated/GameLaunchRouteDto";
import type { GameLaunchStatusDto } from "@/types/generated/GameLaunchStatusDto";
import type { LaunchUnavailableDto } from "@/types/generated/LaunchUnavailableDto";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import type { UnappliedReasonDto } from "@/types/generated/UnappliedReasonDto";
import { assertNever } from "@/utils/assertNever";
import { orderSourceSentenceLabel } from "@/utils/orderSource";

/**
 * What the Launch RimWorld button knows about the game, with the two "not an answer yet"
 * cases kept out of the backend's closed {@link GameLaunchStatusDto}: the query has not
 * resolved, or it failed. Both fail closed (the button is disabled).
 */
export type GameLaunchView =
  | { readonly kind: "unknown"; readonly hasFailed: boolean }
  | { readonly kind: "known"; readonly status: GameLaunchStatusDto };

/** The button's face while no launch is in progress; `starting` is UI state, not a status. */
export function gameLaunchLabel(view: GameLaunchView): MessageDescriptor {
  if (view.kind === "unknown") {
    return descriptor("gameLaunch.button");
  }
  switch (view.status.kind) {
    case "ready":
    case "needsApply":
    case "unavailable":
      return descriptor("gameLaunch.button");
    case "gameRunning":
      return descriptor("gameLaunch.running");
    default:
      return assertNever(view.status);
  }
}

/** The "Starting RimWorld…" face, from the click's success until the game shows up. */
export function gameLaunchStartingLabel(): MessageDescriptor {
  return descriptor("gameLaunch.starting");
}

/** Whether a click on the button is offered at all (it is disabled for every other view). */
export function isGameLaunchClickable(view: GameLaunchView): boolean {
  if (view.kind === "unknown") {
    return false;
  }
  switch (view.status.kind) {
    case "ready":
    case "needsApply":
      return true;
    case "gameRunning":
    case "unavailable":
      return false;
    default:
      return assertNever(view.status);
  }
}

function unavailableHint(reason: LaunchUnavailableDto): MessageDescriptor {
  switch (reason) {
    case "executableMissing":
      return descriptor("gameLaunch.unavailable.executableMissing");
    default:
      return assertNever(reason);
  }
}

/** The short line under the button, or `null` when there is nothing to say. */
export function gameLaunchHint(view: GameLaunchView): MessageDescriptor | null {
  if (view.kind === "unknown") {
    return view.hasFailed ? descriptor("gameLaunch.statusFailed") : null;
  }
  switch (view.status.kind) {
    case "ready":
    case "needsApply":
    case "gameRunning":
      return null;
    case "unavailable":
      return unavailableHint(view.status.reason);
    default:
      return assertNever(view.status);
  }
}

/** The Apply-first prompt's sentence: why `ModsConfig.xml` is behind what the user sees. */
export function applyFirstSentence(
  reason: UnappliedReasonDto,
  selected: OrderSourceDto,
): MessageDescriptor {
  switch (reason) {
    case "orderDiffers":
      return descriptor("gameLaunch.applyFirst.orderDiffers", {
        order: orderSourceSentenceLabel(selected),
      });
    case "activationChangesNotScanned":
      return descriptor("gameLaunch.applyFirst.activationChanges");
    default:
      return assertNever(reason);
  }
}

/** The success toast, by how the game was started. */
export function gameLaunchedLabel(route: GameLaunchRouteDto): MessageDescriptor {
  switch (route) {
    case "steam":
      return descriptor("gameLaunch.launchedSteam");
    case "executable":
      return descriptor("gameLaunch.launchedExecutable");
    default:
      return assertNever(route);
  }
}
