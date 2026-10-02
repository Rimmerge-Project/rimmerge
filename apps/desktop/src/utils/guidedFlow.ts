import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";
import type { RecommendedRulesStepDto } from "@/types/generated/RecommendedRulesStepDto";
import { assertNever } from "@/utils/assertNever";

/** The facts the Dashboard strip is derived from. Nothing here is stored. */
export interface GuidedFacts {
  /**
   * The "Get the recommended rules" step's own state, derived in Rust
   * (`get_recommended_rules_step`). `null` until that query has answered
   * (or if it failed): the strip then ignores step 1 and runs on the
   * other facts, so the Dashboard never waits on it.
   */
  rules: RecommendedRulesStepDto | null;
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
  | { kind: "getRules" }
  | { kind: "chooseSuggested" }
  | { kind: "apply"; blockedByStaleActiveSet: boolean }
  | { kind: "confirm" }
  | { kind: "done" };

/** One step's state, shown as text as well as an icon. */
export type StepStatus = "done" | "current" | "upcoming" | "skipped" | "unavailable";

/** Whether step 1 is still being offered (a click would act, or is acting). */
function isRulesOffered(rules: RecommendedRulesStepDto | null): boolean {
  if (rules === null) {
    return false;
  }
  switch (rules.kind) {
    case "needsAction":
    case "inProgress":
      return true;
    case "done":
    case "skipped":
    case "unavailable":
      return false;
    default:
      return assertNever(rules);
  }
}

/** Maps facts to the current step. */
export function guidedStep(facts: GuidedFacts): GuidedStep {
  // Step 1 outranks every other step, "done" included: importing changes the suggested order,
  // so a file that matched the old one says nothing about the one the user will review next.
  // `skipped` and `unavailable` never hold the flow. An open Apply dialog still wins over it:
  // the user opened Apply, so the strip respects that.
  if (isRulesOffered(facts.rules)) {
    return facts.dialogOpen ? { kind: "confirm" } : { kind: "getRules" };
  }
  // A stale working set outranks the file match: the match is "as of the last scan", and
  // activations made since then have not been scanned, so the apply step must offer the Rescan.
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

/**
 * Step 1's own status. While it is the current step it is `current`; when the user has
 * moved on to the Apply dialog with the step still offered, it is `upcoming` (never a
 * second `current`).
 */
function rulesStatus(step: GuidedStep, rules: RecommendedRulesStepDto | null): StepStatus {
  if (step.kind === "getRules") {
    return "current";
  }
  if (rules === null) {
    return "upcoming";
  }
  switch (rules.kind) {
    case "done":
      return "done";
    case "skipped":
      return "skipped";
    case "unavailable":
      return "unavailable";
    case "needsAction":
    case "inProgress":
      return "upcoming";
    default:
      return assertNever(rules);
  }
}

/** Statuses of the four steps (get rules, choose, apply, confirm) for `step`. */
export function stepStatuses(
  step: GuidedStep,
  rules: RecommendedRulesStepDto | null,
): readonly [StepStatus, StepStatus, StepStatus, StepStatus] {
  const first = rulesStatus(step, rules);
  switch (step.kind) {
    case "getRules":
      return [first, "upcoming", "upcoming", "upcoming"];
    case "chooseSuggested":
      return [first, "current", "upcoming", "upcoming"];
    case "apply":
      return [first, "done", "current", "upcoming"];
    case "confirm":
      return [first, "done", "done", "current"];
    case "done":
      return [first, "done", "done", "done"];
    default:
      return assertNever(step);
  }
}
