import { descriptor, type MessageDescriptor } from "@/i18n/messageDescriptor";
import type { HardProblemDto } from "@/types/generated/HardProblemDto";
import { assertNever } from "@/utils/assertNever";

/** How a problem's mod ids and candidate lists become text, injected so this stays pure. */
export interface PreflightText {
  /** A mod id as the user currently reads it (`useModLabel().label`). */
  label: (modId: string) => string;
  /** Items joined through the locale's list conjunction (`formatList`). */
  list: (items: string[]) => string;
}

/**
 * The one sentence that describes a hard problem, as a
 * {@link MessageDescriptor}. Exhaustive over {@link HardProblemDto}: a new
 * problem kind is a compile error here, not a silent blank row.
 */
export function describeHardProblem(
  problem: HardProblemDto,
  text: PreflightText,
): MessageDescriptor {
  switch (problem.kind) {
    case "missingDependency": {
      // The declared `displayName` wins over `useModLabel`: a not-installed dependency has
      // no installed mod to look up, so the label would fall back to the bare package id.
      const params = {
        mod: text.label(problem.modId),
        dependency: problem.displayName ?? text.label(problem.dependency),
      };
      switch (problem.availability) {
        case "installedInactive":
          return descriptor("apply.problem.missingDependencyInactive", params);
        case "notInstalled":
          return descriptor("apply.problem.missingDependencyNotInstalled", params);
        default:
          return assertNever(problem.availability);
      }
    }
    case "incompatiblePair":
      return descriptor("apply.problem.incompatiblePair", {
        a: text.label(problem.a),
        b: text.label(problem.b),
      });
    case "missingMod": {
      const params = { mod: text.label(problem.modId) };
      switch (problem.outcome) {
        case "removedFromActiveList":
          return descriptor("apply.problem.missingModRemoved", params);
        case "keptInActiveList":
          return descriptor("apply.problem.missingModKept", params);
        default:
          return assertNever(problem.outcome);
      }
    }
    case "loadRequirementViolated":
      return descriptor("apply.problem.loadRequirementViolated", {
        after: text.label(problem.after),
        before: text.label(problem.before),
      });
    case "anyOfUnsatisfied":
      return descriptor("apply.problem.anyOfUnsatisfied", {
        mod: text.label(problem.after),
        candidates: text.list(problem.candidates.map(text.label)),
      });
    default:
      return assertNever(problem);
  }
}

/** A stable list key for a problem: its own kind and ids, never its position. */
export function hardProblemKey(problem: HardProblemDto): string {
  return JSON.stringify(problem);
}
