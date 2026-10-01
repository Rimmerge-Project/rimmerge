import { describe, expect, it } from "vitest";

import { describeDefaultPathsError, describeDefaultPathsWarning } from "@/utils/defaultPaths";

describe("describeDefaultPathsWarning", () => {
  it("carries the game dir into notAnInstall's params", () => {
    expect(describeDefaultPathsWarning({ kind: "notAnInstall", gameDir: "C:/Games/Foo" })).toEqual({
      key: "defaultPathsWarning.notAnInstall",
      params: { gameDir: "C:/Games/Foo" },
    });
  });
});

describe("describeDefaultPathsError", () => {
  it("maps noProfileBase to its own key with no params", () => {
    expect(describeDefaultPathsError({ kind: "noProfileBase" })).toEqual({
      key: "defaultPathsError.noProfileBase",
    });
  });

  it("maps gameDirNotFound to its own key", () => {
    expect(
      describeDefaultPathsError({ kind: "gameDirNotFound", candidates: ["C:/a", "C:/b"] }),
    ).toEqual({ key: "defaultPathsError.gameDirNotFound" });
  });

  it("maps modsConfigNotFound to its own key", () => {
    expect(
      describeDefaultPathsError({ kind: "modsConfigNotFound", reason: "no %USERPROFILE%" }),
    ).toEqual({ key: "defaultPathsError.modsConfigNotFound" });
  });
});
