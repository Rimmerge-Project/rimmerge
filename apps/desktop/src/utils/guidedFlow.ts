import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import { assertNever } from "@/utils/assertNever";

/** The facts the Dashboard strip is derived from. Nothing here is stored. */
export interface GuidedFacts {
  /** The order the session has selected (backend-led). */
  selected: OrderSourceDto;
  /** `DashboardDto.fileMatchesSuggested`: as of the last scan or apply. */
  fileMatchesSuggested: boolean;
  /** The working active-mod set has changes no rescan picked up yet. */
  isStaleActiveSet: boolean;
  /** The strip's own Apply dialog is open (UI state, never persisted). */
  dialogOpen: boolean;
}

/** The step the strip highlights as current. */
export type GuidedStep =
  | { kind: "chooseSuggested" }
  | { kind: "apply"; blockedByStaleActiveSet: boolean }
  | { kind: "confirm" }
  | { kind: "done" };

/** One step's state, shown as text as well as an icon. */
export type StepStatus = "done" | "current" | "upcoming";

/** Maps facts to the current step. */
export function guidedStep(facts: GuidedFacts): GuidedStep {
  // A stale working set outranks the file match: the match is "as of the last scan", and
  // activations made since then have not been scanned, so step 2 must offer the Rescan.
  if (facts.fileMatchesSuggested && !facts.isStaleActiveSet) {
    return { kind: "done" };
  }
  if (facts.dialogOpen) {
    return { kind: "confirm" };
  }
  if (facts.selected !== "suggested") {
    return { kind: "chooseSuggested" };
  }
  return { kind: "apply", blockedByStaleActiveSet: facts.isStaleActiveSet };
}

/** Statuses of steps 1 to 3 (choose, apply, confirm) for `step`. */
export function stepStatuses(step: GuidedStep): readonly [StepStatus, StepStatus, StepStatus] {
  switch (step.kind) {
    case "chooseSuggested":
      return ["current", "upcoming", "upcoming"];
    case "apply":
      return ["done", "current", "upcoming"];
    case "confirm":
      return ["done", "done", "current"];
    case "done":
      return ["done", "done", "done"];
    default:
      return assertNever(step);
  }
}
