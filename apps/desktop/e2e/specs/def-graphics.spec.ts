import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * Opens a fresh patch-maker project so its coverage queue lists the
 * scenario's five target defs (`ThingDef/Race0` .. `Race4`), each of which
 * the mock answers with one of the outcomes the texture preview shows.
 */
async function openCoverageQueue(page: Page): Promise<void> {
  await loadScenario(page);
  await page.getByTestId("nav-assignments").click();
  await page.getByTestId("new-assignment-button").click();
  await page.getByTestId("assignment-name-input").fill("Texture preview");
  await page.getByTestId("assignment-package-id-input").fill("mypatch.textures");
  await page.getByTestId("assignment-display-name-input").fill("Texture Preview");
  await page.getByTestId("refs-search").fill("fixture.framework");
  await page.getByTestId("refs-add-fixture.framework").click();
  await page.getByTestId("targets-search").fill("fixture.target");
  await page.getByTestId("targets-add-fixture.target").click();
  await page.getByTestId("propose-button").click();
  await page.getByTestId("candidate-radio-example.PartAssignmentDef").click();
  await page.getByTestId("assignment-create-button").click();
  await expect(page.getByTestId("coverage-row-Race0")).toBeVisible();
}

const resolveCalls = (page: Page) =>
  page.evaluate(() => window.__RESOLVE_DEF_GRAPHIC_CALLS__ ?? []);
const readCalls = (page: Page) => page.evaluate(() => window.__READ_DEF_TEXTURE_CALLS__ ?? []);

test.describe("def graphics in the patch maker", () => {
  test("the queue asks for each visible row's default view, and nothing else", async ({ page }) => {
    await openCoverageQueue(page);

    await expect(page.getByTestId("coverage-list").getByTestId("def-thumbnail")).toHaveCount(5);
    const isRace = (value: string) => value.startsWith("ThingDef/");
    await expect
      .poll(async () =>
        (await resolveCalls(page))
          .map((call) => call.defRef)
          .filter(isRace)
          .sort(),
      )
      .toEqual([
        "ThingDef/Race0",
        "ThingDef/Race1",
        "ThingDef/Race2",
        "ThingDef/Race3",
        "ThingDef/Race4",
      ]);
    await expect
      .poll(async () =>
        (await readCalls(page))
          .map((call) => call.textureKey)
          .filter((key) => key.startsWith("mock/race"))
          .sort(),
      )
      .toEqual(["mock/race0_south", "mock/race1", "mock/race2_bundle"]);
    for (const thumbnail of await page
      .getByTestId("coverage-list")
      .getByTestId("def-thumbnail")
      .all()) {
      await expect(thumbnail).toHaveAttribute("aria-hidden", "true");
    }
  });

  test("selecting a row shows the viewer; East requests the east texture, West is mirrored", async ({
    page,
  }) => {
    await openCoverageQueue(page);
    await page.getByTestId("coverage-row-Race0").click();

    const viewer = page.getByTestId("def-graphic-viewer");
    await expect(viewer).toBeVisible();
    await expect(page.getByTestId("def-graphic-facing-south")).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await expect(viewer.getByTestId("def-texture-image")).toHaveAttribute(
      "alt",
      "ThingDef/Race0, South view",
    );

    await page.getByTestId("def-graphic-facing-east").click();
    await expect
      .poll(async () => (await readCalls(page)).map((call) => call.textureKey))
      .toContain("mock/race0_east");
    expect(await readCalls(page)).toContainEqual({
      defRef: "ThingDef/Race0",
      textureKey: "mock/race0_east",
    });

    await page.getByTestId("def-graphic-facing-west").click();
    await expect(page.getByTestId("def-graphic-mirrored")).toHaveText("Drawn mirrored");
    await expect(viewer.getByTestId("def-texture-image")).toHaveClass(/-scale-x-100/);
  });

  test("the facing group is keyboard-operable and does not move the queue's selection", async ({
    page,
  }) => {
    await openCoverageQueue(page);
    await page.getByTestId("coverage-row-Race0").click();
    await page.getByTestId("def-graphic-facing-south").focus();

    await page.keyboard.press("ArrowRight");
    await expect(page.getByTestId("def-graphic-facing-west")).toHaveAttribute(
      "aria-checked",
      "true",
    );
    await expect(page.getByTestId("def-graphic-facing-west")).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(page.getByTestId("def-graphic-facing-north")).toBeFocused();

    // The queue's own ArrowDown/`j` cursor stayed put.
    await expect(page.getByTestId("coverage-row-Race0")).toHaveAttribute("aria-current", "true");
    await expect(page.getByTestId("row-editor")).toBeVisible();
  });

  test("a retexture from another mod is named and flagged", async ({ page }) => {
    await openCoverageQueue(page);
    await page.getByTestId("coverage-row-Race1").click();

    await expect(page.getByTestId("def-graphic-provider")).toContainText("From Mod 2");
    await expect(page.getByTestId("def-graphic-override")).toContainText("Fixture Target");
    await expect(page.getByTestId("def-graphic-facing")).toHaveCount(0);
  });

  test("the provider follows the selected order when two mods ship the same texture", async ({
    page,
  }) => {
    await openCoverageQueue(page);
    await page.getByTestId("coverage-row-Race0").click();
    const provider = page.getByTestId("def-graphic-provider");

    // A freshly loaded session opens on Suggested, which loads mod.010 after mod.015.
    await expect(provider).toContainText("From Mod 10");

    await page.getByTestId("order-source-current").click();
    await expect(provider).toContainText("From Mod 15");

    await page.getByTestId("order-source-suggested").click();
    await expect(provider).toContainText("From Mod 10");
  });

  test("a texture that cannot be shown says why, in a sentence", async ({ page }) => {
    await openCoverageQueue(page);
    await page.getByTestId("coverage-row-Race2").click();

    await expect(page.getByTestId("def-texture-message")).toContainText(
      "Built into the game or an asset bundle",
    );
    await expect(
      page.getByTestId("def-graphic-viewer").getByTestId("def-texture-image"),
    ).toHaveCount(0);
  });

  test("a def with no graphic, and a race assembled in game, are each worded", async ({ page }) => {
    await openCoverageQueue(page);

    await page.getByTestId("coverage-row-Race3").click();
    await expect(page.getByTestId("def-graphic-empty")).toHaveText(
      "This def shows no texture of its own.",
    );

    await page.getByTestId("coverage-row-Race4").click();
    await expect(page.getByTestId("def-graphic-empty")).toContainText("assembles this pawn");
  });

  test("the item picker shows a thumbnail per listed def, asking for each visible one", async ({
    page,
  }) => {
    // Tall enough that every row of the picker is on screen; a row below the
    // fold is not asked for until it scrolls into view.
    await page.setViewportSize({ width: 1280, height: 1000 });
    await openCoverageQueue(page);
    await page.getByTestId("coverage-row-Race0").click();

    const list = page.getByTestId("item-picker-list-example.PartDef");
    await expect(list.getByTestId("def-thumbnail")).toHaveCount(5);
    await expect
      .poll(async () =>
        (await resolveCalls(page))
          .map((call) => call.defRef)
          .filter((defRef) => defRef.startsWith("example.PartDef/"))
          .sort(),
      )
      .toEqual([
        "example.PartDef/Part0",
        "example.PartDef/Part1",
        "example.PartDef/Part2",
        "example.PartDef/Part3",
        "example.PartDef/Part4",
      ]);
    await expect
      .poll(async () => (await readCalls(page)).map((call) => call.textureKey))
      .toContain("mock/part0");
  });
});
