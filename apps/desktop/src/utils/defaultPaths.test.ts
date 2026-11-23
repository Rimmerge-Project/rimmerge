import { describe, expect, it } from "vitest";

import {
  checkedLocations,
  describeDefaultPathsError,
  describeDefaultPathsWarning,
  warningConcernsTheField,
} from "@/utils/defaultPaths";

describe("describeDefaultPathsWarning", () => {
  it("carries the game dir into notAnInstall's params", () => {
    expect(describeDefaultPathsWarning({ kind: "notAnInstall", gameDir: "C:/Games/Foo" })).toEqual({
      key: "defaultPathsWarning.notAnInstall",
      params: { gameDir: "C:/Games/Foo" },
    });
  });

  it("words a refused typed text by its reason, with no field or path parameter", () => {
    expect(
      describeDefaultPathsWarning({
        kind: "pathRefused",
        field: "gameDir",
        reason: "networkShare",
        source: "typed",
      }),
    ).toEqual({ key: "defaultPathsWarning.typedNetworkShare" });
    expect(
      describeDefaultPathsWarning({
        kind: "pathRefused",
        field: "gameDir",
        reason: "invalid",
        source: "typed",
      }),
    ).toEqual({ key: "defaultPathsWarning.typedInvalid" });
  });

  it("words a refused stored value by its field", () => {
    const keys = (["gameDir", "workshopDir", "modsConfig"] as const).map(
      (field) =>
        describeDefaultPathsWarning({
          kind: "pathRefused",
          field,
          reason: "networkShare",
          source: "configured",
        }).key,
    );

    expect(keys).toEqual([
      "defaultPathsWarning.configuredGameDir",
      "defaultPathsWarning.configuredWorkshopDir",
      "defaultPathsWarning.configuredModsConfig",
    ]);
  });
});

describe("warningConcernsTheField", () => {
  it("is true for the install note and a refused typed text, false for a stored value", () => {
    const refused = (source: "typed" | "configured") =>
      ({ kind: "pathRefused", field: "gameDir", reason: "invalid", source }) as const;

    expect(warningConcernsTheField({ kind: "notAnInstall", gameDir: "C:/x" })).toBe(true);
    expect(warningConcernsTheField(refused("typed"))).toBe(true);
    expect(warningConcernsTheField(refused("configured"))).toBe(false);
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
