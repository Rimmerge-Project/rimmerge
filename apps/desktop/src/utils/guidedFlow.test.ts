import { describe, expect, it } from "vitest";

import { type GuidedFacts, guidedStep, stepStatuses } from "@/utils/guidedFlow";

const base: GuidedFacts = {
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

describe("stepStatuses", () => {
  it.each([
    [{ kind: "chooseSuggested" }, ["current", "upcoming", "upcoming"]],
    [{ kind: "apply", blockedByStaleActiveSet: false }, ["done", "current", "upcoming"]],
    [{ kind: "confirm" }, ["done", "done", "current"]],
    [{ kind: "done" }, ["done", "done", "done"]],
  ] as const)("maps %j to the three statuses", (step, expected) => {
    expect(stepStatuses(step)).toEqual(expected);
  });
});
