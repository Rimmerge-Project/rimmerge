import { describe, expect, it } from "vitest";

import {
  checkedLocations,
  describeDefaultPathsError,
  describeDefaultPathsWarning,
} from "@/utils/defaultPaths";

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
    expect(describeDefaultPathsError({ kind: "modsConfigNotFound" })).toEqual({
      key: "defaultPathsError.modsConfigNotFound",
    });
  });
});

describe("checkedLocations", () => {
  it("lists the candidates of gameDirNotFound in order", () => {
    expect(checkedLocations({ kind: "gameDirNotFound", candidates: ["C:/a", "C:/b"] })).toEqual([
      "C:/a",
      "C:/b",
    ]);
  });

  it("is empty for every other error", () => {
    expect(checkedLocations({ kind: "modsConfigNotFound" })).toEqual([]);
    expect(checkedLocations({ kind: "noProfileBase" })).toEqual([]);
  });
});
