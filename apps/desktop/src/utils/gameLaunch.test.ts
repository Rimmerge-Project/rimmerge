import { describe, expect, it } from "vitest";
import type { GameLaunchStatusDto } from "@/types/generated/GameLaunchStatusDto";
import {
  applyFirstSentence,
  type GameLaunchView,
  gameLaunchedLabel,
  gameLaunchHint,
  gameLaunchLabel,
  isGameLaunchClickable,
} from "@/utils/gameLaunch";

const known = (status: GameLaunchStatusDto): GameLaunchView => ({ kind: "known", status });

describe("gameLaunch descriptors", () => {
  it.each<[string, GameLaunchView, string, boolean]>([
    ["ready", known({ kind: "ready", route: "steam" }), "gameLaunch.button", true],
    [
      "needsApply",
      known({ kind: "needsApply", route: "executable", reason: "orderDiffers" }),
      "gameLaunch.button",
      true,
    ],
    ["gameRunning", known({ kind: "gameRunning" }), "gameLaunch.running", false],
    [
      "unavailable",
      known({ kind: "unavailable", reason: "executableMissing" }),
      "gameLaunch.button",
      false,
    ],
    ["unknown while pending", { kind: "unknown", hasFailed: false }, "gameLaunch.button", false],
    ["unknown after a failure", { kind: "unknown", hasFailed: true }, "gameLaunch.button", false],
  ])("%s: label and clickability", (_name, view, labelKey, clickable) => {
    expect(gameLaunchLabel(view).key).toBe(labelKey);
    expect(isGameLaunchClickable(view)).toBe(clickable);
  });

  it("hints only for an unavailable install and for a failed status", () => {
    expect(gameLaunchHint(known({ kind: "ready", route: "steam" }))).toBeNull();
    expect(
      gameLaunchHint(known({ kind: "needsApply", route: "steam", reason: "orderDiffers" })),
    ).toBeNull();
    expect(gameLaunchHint(known({ kind: "gameRunning" }))).toBeNull();
    expect(gameLaunchHint({ kind: "unknown", hasFailed: false })).toBeNull();
    expect(gameLaunchHint(known({ kind: "unavailable", reason: "executableMissing" }))?.key).toBe(
      "gameLaunch.unavailable.executableMissing",
    );
    expect(gameLaunchHint({ kind: "unknown", hasFailed: true })?.key).toBe(
      "gameLaunch.statusFailed",
    );
  });

  it("words the Apply-first sentence per reason, naming the selected order", () => {
    const differs = applyFirstSentence("orderDiffers", "suggested");
    expect(differs.key).toBe("gameLaunch.applyFirst.orderDiffers");
    expect(differs.params?.["order"]).toMatchObject({
      key: "shell.orderSource.inSentenceSuggested",
    });
    expect(applyFirstSentence("activationChangesNotScanned", "current").key).toBe(
      "gameLaunch.applyFirst.activationChanges",
    );
  });

  it("words the success toast per route", () => {
    expect(gameLaunchedLabel("steam").key).toBe("gameLaunch.launchedSteam");
    expect(gameLaunchedLabel("executable").key).toBe("gameLaunch.launchedExecutable");
  });
});
