import { describe, expect, it } from "vitest";

import type { HardProblemDto } from "@/types/generated/HardProblemDto";
import { describeHardProblem, type PreflightText } from "@/utils/preflight";

const text: PreflightText = {
  label: (id) => `<${id}>`,
  list: (items) => items.join(" + "),
};

describe("describeHardProblem", () => {
  it("names an installed-but-inactive dependency with its own sentence", () => {
    const problem: HardProblemDto = {
      kind: "missingDependency",
      modId: "app",
      dependency: "lib",
      displayName: null,
      availability: "installedInactive",
    };

    const described = describeHardProblem(problem, text);

    expect(described.key).toBe("apply.problem.missingDependencyInactive");
    expect(described.params).toEqual({ mod: "<app>", dependency: "<lib>" });
  });

  it("prefers the declared display name for a dependency that is not installed", () => {
    const problem: HardProblemDto = {
      kind: "missingDependency",
      modId: "app",
      dependency: "lib",
      displayName: "Library",
      availability: "notInstalled",
    };

    const described = describeHardProblem(problem, text);

    expect(described.key).toBe("apply.problem.missingDependencyNotInstalled");
    expect(described.params).toEqual({ mod: "<app>", dependency: "Library" });
  });

  it("distinguishes a removed missing mod from a kept one", () => {
    const removed = describeHardProblem(
      { kind: "missingMod", modId: "gone", outcome: "removedFromActiveList" },
      text,
    );
    const kept = describeHardProblem(
      { kind: "missingMod", modId: "gone", outcome: "keptInActiveList" },
      text,
    );

    expect(removed.key).toBe("apply.problem.missingModRemoved");
    expect(kept.key).toBe("apply.problem.missingModKept");
  });

  it("labels both sides of an incompatible pair and of a violated requirement", () => {
    const pair = describeHardProblem({ kind: "incompatiblePair", a: "x", b: "y" }, text);
    const requirement = describeHardProblem(
      { kind: "loadRequirementViolated", after: "x", before: "y", edgeKind: "assemblyRef" },
      text,
    );

    expect(pair.params).toEqual({ a: "<x>", b: "<y>" });
    expect(requirement.params).toEqual({ after: "<x>", before: "<y>" });
  });

  it("joins any-of candidates through the injected list formatter", () => {
    const described = describeHardProblem(
      { kind: "anyOfUnsatisfied", after: "x", candidates: ["c1", "c2"] },
      text,
    );

    expect(described.key).toBe("apply.problem.anyOfUnsatisfied");
    expect(described.params).toEqual({ mod: "<x>", candidates: "<c1> + <c2>" });
  });
});
