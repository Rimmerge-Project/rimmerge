import { expect, type Locator, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import type { SupportedLocale } from "../../src/i18n/locales";
import { chooseLocale, chooseSort, loadScenario, modsPane } from "./support";

/**
 * Every Mods page control stays readable at the narrowest width where the two lists sit side by
 * side (1,024 px), in the locales whose words run longest (#50). German, Russian, Polish and
 * French are preview locales: their catalogues do not yet carry every key this milestone added
 * (those fall back to English until the translation round), so the checks measure what is
 * rendered rather than particular strings, and hold for any longer wording.
 *
 * Definitions, so "readable" is not a feeling:
 *
 * - A placeholder is *clipped* when its text, measured with the input's own font on a canvas
 *   (a placeholder is not an element, so it has no box to ask), is wider than the input's
 *   content box. Longer text is shortened with a tooltip, or the input gets a line of its own.
 * - A control *overflows* when its bounding box leaves its container's, or overlaps a sibling.
 * - A text cell is *clipped* when its `scrollWidth` exceeds its `clientWidth` (the cells use
 *   `truncate`, so an ellipsis is exactly that case).
 * - A bar *wraps cleanly* when every control lies inside it; the sort bar must not wrap at all
 *   (it collapses to a select instead), and the toolbar's controls never overlap.
 */

const LOCALES: readonly SupportedLocale[] = ["en", "de", "ru", "pl", "fr"];
const NARROW = { width: 1024, height: 800 } as const;
/** Sub-pixel layout rounds; a whole pixel of slack never hides a real overflow. */
const SLACK = 1;

type Pane = "active" | "inactive";
const PANES: readonly Pane[] = ["active", "inactive"];

type Box = { x: number; y: number; width: number; height: number };

async function boxOf(locator: Locator): Promise<Box> {
  await expect(locator, `${String(locator)} is in the page`).toBeAttached();
  const box = await locator.boundingBox();
  if (box === null) {
    throw new Error(`no box for ${String(locator)}`);
  }
  return box;
}

/** What is wrong with `inner` not lying within `outer` (to a pixel), or `null` when it does. */
async function insideViolation(
  inner: Locator,
  outer: Locator,
  what: string,
): Promise<string | null> {
  let innerBox: Box;
  let outerBox: Box;
  try {
    [innerBox, outerBox] = [await boxOf(inner), await boxOf(outer)];
  } catch (error) {
    // A row the virtualizer is re-placing has no box for a moment: report it so the poll retries.
    return `${what}: ${error instanceof Error ? error.message : String(error)}`;
  }
  const message = `${what} (inner ${JSON.stringify(innerBox)}, outer ${JSON.stringify(outerBox)})`;
  const isInside =
    innerBox.x >= outerBox.x - SLACK &&
    innerBox.y >= outerBox.y - SLACK &&
    innerBox.x + innerBox.width <= outerBox.x + outerBox.width + SLACK &&
    innerBox.y + innerBox.height <= outerBox.y + outerBox.height + SLACK;
  return isInside ? null : message;
}

/**
 * `inner` lies within `outer`, to a pixel. Two boxes read one after the other can straddle a
 * re-layout (a virtualized list placing its rows), so the check retries until the page is still
 * and reports the last mismatch.
 */
async function expectInside(inner: Locator, outer: Locator, what: string): Promise<void> {
  await expect
    .poll(() => insideViolation(inner, outer, what), { message: `${what} is not inside` })
    .toBeNull();
}

/** No two of the controls share any area. */
async function expectNoOverlap(controls: readonly Locator[], what: string): Promise<void> {
  const boxes = await Promise.all(controls.map(boxOf));
  for (const [index, first] of boxes.entries()) {
    for (const second of boxes.slice(index + 1)) {
      const isOverlapping =
        first.x + SLACK < second.x + second.width &&
        second.x + SLACK < first.x + first.width &&
        first.y + SLACK < second.y + second.height &&
        second.y + SLACK < first.y + first.height;
      expect(isOverlapping, `${what} overlap: ${JSON.stringify([first, second])}`).toBe(false);
    }
  }
}

/** How many pixels the input's placeholder is wider than its content box (<= 0 means it fits). */
function placeholderOverflow(input: Locator): Promise<number> {
  return input.evaluate((element) => {
    const field = element as HTMLInputElement;
    const style = getComputedStyle(field);
    const context = document.createElement("canvas").getContext("2d");
    if (context === null) {
      throw new Error("no 2d canvas to measure the placeholder with");
    }
    context.font = style.font;
    const content =
      field.clientWidth -
      Number.parseFloat(style.paddingLeft) -
      Number.parseFloat(style.paddingRight);
    return context.measureText(field.placeholder).width - content;
  });
}

async function expectPlaceholderFits(input: Locator, what: string): Promise<void> {
  await expect(input).toHaveValue("");
  const overflow = await placeholderOverflow(input);
  expect(
    overflow,
    `${what} placeholder is ${overflow.toFixed(1)}px wider than its box`,
  ).toBeLessThanOrEqual(SLACK);
}

/** The element's text is not cut off (no ellipsis, no hidden overflow). */
async function expectNotClipped(cell: Locator, what: string): Promise<void> {
  await expect(cell, `${what} is in the page`).toBeAttached();
  const { scrollWidth, clientWidth } = await cell.evaluate((element) => ({
    scrollWidth: element.scrollWidth,
    clientWidth: element.clientWidth,
  }));
  expect(
    scrollWidth,
    `${what} is clipped (scroll ${scrollWidth}, client ${clientWidth})`,
  ).toBeLessThanOrEqual(clientWidth + SLACK);
}

/** Every child control of a bar lies inside it, none overlaps another, and it does not scroll sideways. */
async function expectBarContains(bar: Locator, what: string): Promise<void> {
  await expect(bar).toBeVisible();
  const children = await bar.locator("> *").all();
  expect(children.length, `${what} has controls`).toBeGreaterThan(0);
  for (const child of children) {
    await expectInside(child, bar, `${what} control`);
  }
  await expectNoOverlap(children, what);
  await expectNotClipped(bar, what);
}

/**
 * The bar's visible controls (its buttons, or its select and direction button) sit on one line
 * and end inside it. Hidden ones (the measured-but-collapsed button group) are not counted.
 */
async function expectOneLine(bar: Locator, what: string): Promise<void> {
  const { tops, overshoot, wrapped } = await bar.evaluate((element) => {
    const controls = [
      ...element.querySelectorAll('button, [data-testid="mods-sort-select"]'),
    ].filter((control) => getComputedStyle(control).visibility !== "hidden");
    const visible = controls.map((control) => control.getBoundingClientRect());
    const wrapped = controls
      .filter((control) => control.scrollHeight > control.clientHeight + 1)
      .map((control) => control.getAttribute("data-testid"));
    const barRight = element.getBoundingClientRect().right;
    return {
      wrapped,
      tops: visible.map((box) => Math.round(box.top)),
      overshoot: Math.max(...visible.map((box) => box.right - barRight)),
    };
  });
  // A label that wraps inside its fixed-height button grows past it: scrollHeight exceeds clientHeight.
  expect(wrapped, `${what} has controls whose label wraps`).toEqual([]);
  expect(tops.length, `${what} has controls`).toBeGreaterThan(0);
  const spread = Math.max(...tops) - Math.min(...tops);
  expect(spread, `${what} wraps: control tops ${tops.join(",")}`).toBeLessThanOrEqual(4);
  expect(overshoot, `${what} runs ${overshoot}px past its pane`).toBeLessThanOrEqual(SLACK);
}

/** Activates a mod without a rescan, so its row is Added and "not scanned yet". */
async function activateWithoutRescan(page: Page, modId: string): Promise<void> {
  const inactive = modsPane(page, "inactive");
  await inactive.getByTestId("mods-search").fill(modId);
  await inactive.getByTestId(`mod-row-select-${modId}`).locator("input").check();
  await page.getByTestId("activate-selected-button").click();
  await page.getByTestId("activate-confirm-submit").click();
  await expect(page.getByTestId("activate-confirm-dialog")).toBeHidden();
  await inactive.getByTestId("mods-search").fill("");
}

/** One coloured group with two active mods in it, as a setup step. */
async function seedGroup(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await window.__TAURI_INTERNALS__.invoke("create_group", {
      request: { name: "Weapons", parent: null, modIds: ["mod.012", "mod.013"] },
    });
    await window.__TAURI_INTERNALS__.invoke("set_group_color", {
      request: { id: 1, color: "red" },
    });
  });
}

async function expectToolbarFits(page: Page, pane: Pane): Promise<void> {
  const root = modsPane(page, pane);
  const search = root.getByTestId("mods-search");
  const tag = root.getByTestId("mods-tag-filter");
  const source = root.getByTestId("mods-source-filter");
  const sortBar = root.getByTestId("mods-sort-bar");
  const viewSwitch = root.getByTestId("mods-view-switch");

  await expectPlaceholderFits(search, `${pane} search`);
  await expectPlaceholderFits(tag, `${pane} tag filter`);
  for (const [control, what] of [
    [search, "search"],
    [tag, "tag filter"],
    [source, "source filter"],
    [sortBar, "sort bar"],
    [viewSwitch, "view switch"],
  ] as const) {
    await expectInside(control, root, `${pane} ${what}`);
  }
  await expectNoOverlap([search, tag, source], `${pane} filters`);
  await expectOneLine(sortBar, `${pane} sort bar`);
}

async function expectUpdatedCellsFit(page: Page, pane: Pane): Promise<void> {
  const cells = await modsPane(page, pane).locator('[data-testid^="mod-row-updated-"]').all();
  expect(cells.length, `${pane} shows dates`).toBeGreaterThan(0);
  for (const cell of cells) {
    await expectNotClipped(cell, `${pane} date cell ${await cell.innerText()}`);
    await expect(cell).toHaveText(/\d/);
  }
}

async function expectBulkBarFits(page: Page, pane: Pane, ids: readonly string[]): Promise<void> {
  const root = modsPane(page, pane);
  for (const id of ids) {
    await root.getByTestId(`mod-row-select-${id}`).locator("input").check();
  }
  const bar = root.getByTestId(`mods-bulk-bar-${pane}`);
  await expectBarContains(bar, `${pane} bulk bar`);
  await expectInside(bar, root, `${pane} bulk bar`);
  // A label that wraps inside its button makes a tall, ragged button; one line, truncated, is the rule.
  for (const button of await bar.getByRole("button").all()) {
    expect((await boxOf(button)).height, `${pane} bulk button wraps its label`).toBeLessThanOrEqual(
      40,
    );
  }
  await root.getByTestId("mods-clear-selection").click();
  await expect(bar).toBeHidden();
}

/** A row keeps its name readable, and the group chips, dots and colour mark stay inside the row. */
async function expectRowMarksFit(row: Locator, what: string): Promise<void> {
  await expect(row).toBeVisible();
  // The colour dot and the group chip (or, in a narrow row, its dots) must be there to be measured.
  const color = row.getByTestId("mod-row-color").first();
  const groups = row.locator('[data-testid="mod-row-groups"], [data-testid="mod-row-dots"]');
  await expect(color, `${what} shows its colour dot`).toBeVisible();
  await expect(
    groups.locator("visible=true").first(),
    `${what} shows its group mark`,
  ).toBeVisible();
  for (const mark of [color, groups.locator("visible=true").first()]) {
    await expectInside(mark, row, `${what} mark`);
  }
  await expectNotClipped(row.locator('[data-testid^="mod-row-name-"]'), `${what} name`);
}

test.describe("Mods page layout at 1,024 px", () => {
  for (const locale of LOCALES) {
    test(`controls stay readable (${locale})`, async ({ page }) => {
      await page.setViewportSize(NARROW);
      await loadScenario(page);
      await page.getByTestId("welcome-card-keep").click();
      await chooseLocale(page, locale);
      await page.getByTestId("nav-mods").click();
      await expect(modsPane(page, "active").getByTestId("mod-table")).toBeVisible();
      await activateWithoutRescan(page, "mod.905");
      await seedGroup(page);

      await test.step("toolbars: placeholders, filters, sort bar, view switch", async () => {
        for (const pane of PANES) {
          await expectToolbarFits(page, pane);
        }
      });

      await test.step("date cells, including a freshly added row", async () => {
        const active = modsPane(page, "active");
        await chooseSort(page, "active", "lastUpdated");
        await expect(active.locator('[data-testid^="mod-row-updated-"]').first()).toBeVisible();
        await expectUpdatedCellsFit(page, "active");
        await active.getByTestId("mods-search").fill("mod.905");
        await expect(active.getByTestId("mod-row-updated-mod.905")).toBeVisible();
        // The filtered answer replaces the rows; measure once only the match is left.
        await expect(active.locator('[data-testid^="mod-row-updated-"]')).toHaveCount(1);
        await expectUpdatedCellsFit(page, "active");
        await active.getByTestId("mods-search").fill("");
        await chooseSort(page, "active", "loadOrder");
      });

      await test.step("bulk bars", async () => {
        await expectBulkBarFits(page, "active", ["mod.004", "mod.005"]);
        await expectBulkBarFits(page, "inactive", ["mod.901", "mod.902"]);
      });

      await test.step("group marks and the search highlight on rows", async () => {
        const active = modsPane(page, "active");
        await expectRowMarksFit(active.getByTestId("mod-row-mod.012"), "grouped row");
        await active.getByTestId("mods-search").fill("d 12");
        const label = active.getByTestId("mod-row-name-mod.012");
        const marks = label.locator('[data-testid="search-match"]');
        await expect(marks.first()).toBeVisible();
        for (const mark of await marks.all()) {
          await expectInside(mark, active.getByTestId("mod-row-mod.012"), "search match");
        }
        await active.getByTestId("mods-search").fill("");
      });

      await test.step("Groups view folder headers", async () => {
        const active = modsPane(page, "active");
        await active.getByTestId("mods-view-groups").click();
        await expect(active.getByTestId("mod-groups-table")).toBeVisible();
        await expectToolbarFits(page, "active");
        // By test id, not a snapshot of elements: the virtualized table may re-render a header.
        const headerIds = await active
          .locator('[data-testid^="mods-folder-"]')
          .evaluateAll((elements) =>
            elements.map((element) => element.getAttribute("data-testid")),
          );
        expect(headerIds.length).toBeGreaterThan(0);
        for (const headerId of headerIds) {
          const header = active.getByTestId(headerId ?? "");
          await expect(header).toBeVisible();
          for (const part of [
            "folder-chevron",
            "folder-name",
            "folder-count",
            "folder-menu-button",
          ]) {
            await expectInside(header.getByTestId(part), header, `folder header ${part}`);
          }
          await expectNotClipped(header.getByTestId("folder-count"), "folder count");
          await expectNotClipped(header.getByTestId("folder-name"), "folder name");
        }
        await expectRowMarksFit(
          active.getByTestId("mod-entry-1-mod.012"),
          "grouped row in a folder",
        );
        await active.getByTestId("mods-view-list").click();
      });

      await test.step("the Show in load order note", async () => {
        const active = modsPane(page, "active");
        await active.getByTestId("mods-search").fill("mod.05");
        await active.getByTestId("mod-row-mod.050").click();
        await expect(page.getByTestId("mod-info-panel")).toBeVisible();
        // `fill` does not need the drawer-covered box to be hittable.
        await active.getByTestId("mods-search").fill("mod.04");
        await expect(active.getByTestId("mod-row-mod.050")).toHaveCount(0);
        await page.getByTestId("mod-info-show-in-load-order").click();
        const note = page.getByTestId("mods-show-in-load-order-note");
        await expect(note).toBeVisible();
        await expectInside(note, active, "show in load order note");
        await expectNotClipped(note, "show in load order note");
      });

      await test.step("the groups rail and its copy button", async () => {
        const opener = page.getByTestId("groups-open");
        await expectInside(opener, page.locator("body"), "groups button");
        await opener.click();
        const rail = page.getByTestId("group-rail");
        await expect(rail).toBeVisible();
        const copy = page.getByTestId("group-copy-profile");
        await expectInside(copy, rail, "copy from profile button");
        await expectNotClipped(rail, "groups rail");
        await expectNotClipped(copy, "copy from profile button");
      });
    });
  }
});

test.describe("Mods page sort bar at the default 1,280 px window", () => {
  for (const locale of LOCALES) {
    test(`never wraps a label or runs past its pane (${locale})`, async ({ page }) => {
      await page.setViewportSize({ width: 1280, height: 800 });
      await loadScenario(page);
      await page.getByTestId("welcome-card-keep").click();
      await chooseLocale(page, locale);
      await page.getByTestId("nav-mods").click();
      for (const pane of PANES) {
        const bar = modsPane(page, pane).getByTestId("mods-sort-bar");
        await expect(bar).toBeVisible();
        await expectOneLine(bar, `${pane} sort bar`);
      }
    });
  }
});
