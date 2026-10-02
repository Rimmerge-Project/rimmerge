import { describe, expect, it } from "vitest";

import type { RecommendedRulesStepDto } from "@/types/generated/RecommendedRulesStepDto";
import { type GuidedFacts, guidedStep, type StepStatus, stepStatuses } from "@/utils/guidedFlow";

const DONE: RecommendedRulesStepDto = { kind: "done", importedRulesInUse: true };
const NEEDS_ACTION: RecommendedRulesStepDto = {
  kind: "needsAction",
  sources: [{ database: "community", need: { kind: "import" } }],
};
const IN_PROGRESS: RecommendedRulesStepDto = { kind: "inProgress" };
const SKIPPED: RecommendedRulesStepDto = {
  kind: "skipped",
  sources: [{ database: "community", need: { kind: "import" } }],
};
const UNAVAILABLE: RecommendedRulesStepDto = { kind: "unavailable", reason: "networkOff" };

const base: GuidedFacts = {
  rules: DONE,
  selected: "suggested",
  fileMatchesSuggested: false,
  isStaleActiveSet: false,
  dialogOpen: false,
};

describe("guidedStep", () => {
  it("asks to choose the suggested order while Current is selected", () => {
    expect(guidedStep({ ...base, selected: "current" })).toEqual({ kind: "chooseSuggested" });
  });

  it("moves to Apply once Suggested is selected and the file differs", () => {
    expect(guidedStep(base)).toEqual({ kind: "apply", blockedByStaleActiveSet: false });
  });

  it("blocks Apply on a stale active set", () => {
    expect(guidedStep({ ...base, isStaleActiveSet: true })).toEqual({
      kind: "apply",
      blockedByStaleActiveSet: true,
    });
  });

  it("is on Confirm while the strip dialog is open", () => {
    expect(guidedStep({ ...base, dialogOpen: true })).toEqual({ kind: "confirm" });
  });

  it("is done as soon as the file matches, even while Current is selected", () => {
    expect(guidedStep({ ...base, selected: "current", fileMatchesSuggested: true })).toEqual({
      kind: "done",
    });
  });

  it("is not done while the working set is stale, even if the file matches", () => {
    expect(guidedStep({ ...base, fileMatchesSuggested: true, isStaleActiveSet: true })).toEqual({
      kind: "apply",
      blockedByStaleActiveSet: true,
    });
  });

  it("stays done when the dialog is still open on its result summary", () => {
    expect(guidedStep({ ...base, fileMatchesSuggested: true, dialogOpen: true })).toEqual({
      kind: "done",
    });
  });
});

describe("guidedStep with the recommended-rules step", () => {
  it.each([
    ["needsAction", NEEDS_ACTION],
    ["inProgress", IN_PROGRESS],
  ] as const)("holds the flow on step 1 while rules are %s", (_name, rules) => {
    expect(guidedStep({ ...base, rules })).toEqual({ kind: "getRules" });
  });

  it.each([
    ["Current selected", { selected: "current" }],
    ["Suggested selected", {}],
    ["a stale active set", { isStaleActiveSet: true }],
  ] as const)("outranks the later steps with %s", (_name, facts) => {
    expect(guidedStep({ ...base, ...facts, rules: NEEDS_ACTION })).toEqual({ kind: "getRules" });
  });

  it("outranks done: importing changes the order a matching file was compared with", () => {
    expect(guidedStep({ ...base, rules: NEEDS_ACTION, fileMatchesSuggested: true })).toEqual({
      kind: "getRules",
    });
  });

  it("lets an open Apply dialog outrank step 1", () => {
    expect(guidedStep({ ...base, rules: NEEDS_ACTION, dialogOpen: true })).toEqual({
      kind: "confirm",
    });
  });

  it.each([
    ["skipped", SKIPPED],
    ["unavailable", UNAVAILABLE],
    ["done", DONE],
  ] as const)("lets the flow reach step 2 while rules are %s", (_name, rules) => {
    expect(guidedStep({ ...base, rules, selected: "current" })).toEqual({
      kind: "chooseSuggested",
    });
    expect(guidedStep({ ...base, rules })).toEqual({
      kind: "apply",
      blockedByStaleActiveSet: false,
    });
    expect(guidedStep({ ...base, rules, fileMatchesSuggested: true })).toEqual({ kind: "done" });
  });

  it("falls back to the three-step logic until the rules step has answered", () => {
    expect(guidedStep({ ...base, rules: null })).toEqual({
      kind: "apply",
      blockedByStaleActiveSet: false,
    });
    expect(guidedStep({ ...base, rules: null, selected: "current" })).toEqual({
      kind: "chooseSuggested",
    });
  });
});

describe("stepStatuses", () => {
  it.each([
    [{ kind: "getRules" }, ["current", "upcoming", "upcoming", "upcoming"]],
    [{ kind: "chooseSuggested" }, ["done", "current", "upcoming", "upcoming"]],
    [{ kind: "apply", blockedByStaleActiveSet: false }, ["done", "done", "current", "upcoming"]],
    [{ kind: "confirm" }, ["done", "done", "done", "current"]],
    [{ kind: "done" }, ["done", "done", "done", "done"]],
  ] as const)("maps %j to the four statuses with step 1 done", (step, expected) => {
    expect(stepStatuses(step, DONE)).toEqual(expected);
  });

  it.each([
    ["done", DONE, "done"],
    ["skipped", SKIPPED, "skipped"],
    ["unavailable", UNAVAILABLE, "unavailable"],
    ["still offered under an open dialog", NEEDS_ACTION, "upcoming"],
    ["not yet loaded", null, "upcoming"],
  ] as const)(
    "shows step 1 as its own state when the flow moved on: %s",
    (_name, rules, status) => {
      const [first] = stepStatuses({ kind: "apply", blockedByStaleActiveSet: false }, rules);
      expect(first satisfies StepStatus).toBe(status);
    },
  );

  it.each([
    ["needsAction", NEEDS_ACTION],
    ["inProgress", IN_PROGRESS],
  ] as const)("marks step 1 current, and only it, while rules are %s", (_name, rules) => {
    const statuses = stepStatuses(guidedStep({ ...base, rules }), rules);
    expect(statuses.filter((status) => status === "current")).toHaveLength(1);
    expect(statuses[0]).toBe("current");
  });

  it("never marks two steps current when the Apply dialog opens over an offered step 1", () => {
    const step = guidedStep({ ...base, rules: NEEDS_ACTION, dialogOpen: true });
    const statuses = stepStatuses(step, NEEDS_ACTION);

    expect(statuses).toEqual(["upcoming", "done", "done", "current"]);
  });
});
