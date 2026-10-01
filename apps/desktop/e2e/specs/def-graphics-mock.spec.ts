import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import type { DefGraphicDto } from "../../src/types/generated/DefGraphicDto";
import type { DefTextureDto } from "../../src/types/generated/DefTextureDto";
import { loadScenario } from "./support";

/**
 * Mock-fidelity checks for `resolve_def_graphic` / `read_def_texture`:
 * they call the mock handlers directly (through Tauri's in-page IPC
 * bridge) and pin the semantics the real commands have, so a component
 * spec built on these mocks cannot pass against behavior the backend
 * does not have.
 */

type Outcome<T> = { ok: true; value: T } | { ok: false; code: string };

async function invoke<T>(page: Page, command: string, request: unknown): Promise<Outcome<T>> {
  return page.evaluate(
    async ([name, payload]) => {
      try {
        const value = await window.__TAURI_INTERNALS__.invoke(name as string, {
          request: payload,
        });
        return { ok: true as const, value: value as never };
      } catch (error) {
        return { ok: false as const, code: (error as { code: string }).code };
      }
    },
    [command, request] as const,
  );
}

function resolveGraphic(page: Page, defRef: string): Promise<Outcome<DefGraphicDto>> {
  return invoke<DefGraphicDto>(page, "resolve_def_graphic", { defRef });
}

function readTexture(
  page: Page,
  defRef: string,
  textureKey: string,
): Promise<Outcome<DefTextureDto>> {
  return invoke<DefTextureDto>(page, "read_def_texture", { defRef, textureKey });
}

function expectOk<T>(outcome: Outcome<T>): T {
  if (!outcome.ok) throw new Error(`expected success, got ${outcome.code}`);
  return outcome.value;
}

test.describe("def graphic mocks mirror the backend", () => {
  test.beforeEach(async ({ page }) => {
    await loadScenario(page);
  });

  test("an unindexed def is def_not_found and an unparseable ref is invalid_input", async ({
    page,
  }) => {
    expect(await resolveGraphic(page, "ThingDef/NoSuchDef")).toEqual({
      ok: false,
      code: "def_not_found",
    });
    expect(await resolveGraphic(page, "not a ref")).toEqual({ ok: false, code: "invalid_input" });
    expect(await readTexture(page, "ThingDef/NoSuchDef", "mock/statue")).toEqual({
      ok: false,
      code: "def_not_found",
    });
  });

  test("an indexed def with no graphic shows nothing, and a humanlike kind is composed at runtime", async ({
    page,
  }) => {
    expect(expectOk(await resolveGraphic(page, "ThingDef/CleanWall"))).toEqual({
      kind: "noGraphic",
    });
    expect(expectOk(await resolveGraphic(page, "PawnKindDef/MockColonist"))).toEqual({
      kind: "composedAtRuntime",
    });
  });

  test("a multi graphic lists four directions with the mirrored one flagged", async ({ page }) => {
    const graphic = expectOk(await resolveGraphic(page, "ThingDef/MockChair"));

    if (graphic.kind !== "resolved") throw new Error(`expected resolved, got ${graphic.kind}`);
    const faces = graphic.slots[0]?.variants[0]?.faces;
    if (faces?.kind !== "multi") throw new Error("expected a multi graphic");
    expect(faces.west.isMirrored).toBe(true);
    expect(faces.west.facing).toBe("west");
    expect(faces.north.isMirrored).toBe(false);
    expect(graphic.defaultView).toEqual({ slot: 0, variant: 0 });
  });

  test("a key the def's graphic did not produce is refused, even one another def shows", async ({
    page,
  }) => {
    // `mock/cloak_thin` is a real, readable texture of ThingDef/MockCloak.
    expect(expectOk(await readTexture(page, "ThingDef/MockCloak", "mock/cloak_thin")).kind).toBe(
      "image",
    );

    expect(await readTexture(page, "ThingDef/MockStatue", "mock/cloak_thin")).toEqual({
      ok: false,
      code: "invalid_input",
    });
    // A def with no graphic has no key to read.
    expect(await readTexture(page, "ThingDef/CleanWall", "mock/statue")).toEqual({
      ok: false,
      code: "invalid_input",
    });
  });

  test("a malformed key is invalid_input before the def is consulted", async ({ page }) => {
    for (const key of ["", "../secret", "/abs/path", "a//b", "a/{0}", "a/[x]"]) {
      expect(await readTexture(page, "ThingDef/NoSuchDef", key)).toEqual({
        ok: false,
        code: "invalid_input",
      });
    }
  });

  test("a key is normalized like the backend's: backslashes and case fold away", async ({
    page,
  }) => {
    const texture = expectOk(await readTexture(page, "ThingDef/MockStatue", "Mock\\Statue"));

    expect(texture.kind).toBe("image");
  });

  test("each availability reads back as the matching outcome", async ({ page }) => {
    const owner = "mod.004";
    expect(expectOk(await readTexture(page, "ThingDef/MockStatue", "mock/statue"))).toMatchObject({
      kind: "image",
      from: "direct",
      owner: "mod.002",
    });
    expect(
      expectOk(await readTexture(page, "ThingDef/MockDdsThing", "mock/sibling")),
    ).toMatchObject({
      kind: "image",
      from: "pngSibling",
      owner,
    });
    expect(expectOk(await readTexture(page, "ThingDef/MockDdsThing", "mock/undecodable"))).toEqual({
      kind: "undecodableInGame",
      owner,
    });
    expect(expectOk(await readTexture(page, "ThingDef/MockDdsThing", "mock/dds"))).toEqual({
      kind: "ddsNotPreviewable",
      owner,
    });
    expect(expectOk(await readTexture(page, "ThingDef/MockDdsThing", "mock/huge"))).toEqual({
      kind: "unreadable",
      reason: "tooLarge",
    });
    expect(expectOk(await readTexture(page, "ThingDef/MockCloak", "mock/cloak_icon"))).toEqual({
      kind: "notViewable",
      isUncertain: false,
    });
    expect(expectOk(await readTexture(page, "ThingDef/MockCloak", "mock/cloak_fat"))).toEqual({
      kind: "notFound",
    });
    expect(expectOk(await readTexture(page, "ThingDef/MockUnseen", "mock/maybe_builtin"))).toEqual({
      kind: "notViewable",
      isUncertain: true,
    });
  });

  test("a texture two mods ship is owned by the last-loaded one under the selected order", async ({
    page,
  }) => {
    const southOwner = async (): Promise<string | null> => {
      const graphic = expectOk(await resolveGraphic(page, "ThingDef/Race0"));
      if (graphic.kind !== "resolved") return null;
      const faces = graphic.slots[0]?.variants[0]?.faces;
      if (faces?.kind !== "multi" || faces.south.availability.kind !== "loose") return null;
      return faces.south.availability.owner;
    };
    const select = (source: "current" | "suggested") =>
      page.evaluate(
        (chosen) => window.__TAURI_INTERNALS__.invoke("select_order", { source: chosen }),
        source,
      );

    // `suggestedOrder` swaps mod.010 and mod.015, so each wins under one order.
    await select("current");
    expect(await southOwner()).toBe("mod.015");
    expect(expectOk(await readTexture(page, "ThingDef/Race0", "mock/race0_south"))).toMatchObject({
      kind: "image",
      owner: "mod.015",
    });

    await select("suggested");
    expect(await southOwner()).toBe("mod.010");
    expect(expectOk(await readTexture(page, "ThingDef/Race0", "mock/race0_south"))).toMatchObject({
      kind: "image",
      owner: "mod.010",
    });
  });

  test("the texture-override read refuses a DDS with its own error code", async ({ page }) => {
    const code = await page.evaluate(async () => {
      try {
        await window.__TAURI_INTERNALS__.invoke("read_texture", {
          modId: "mod.002",
          texturePath: "mock/dds_thing",
        });
        return null;
      } catch (error) {
        return (error as { code: string }).code;
      }
    });

    expect(code).toBe("texture_unsupported_format");
  });
});
