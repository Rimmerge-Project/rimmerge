import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

test.describe("load order", () => {
  test("virtualizes the 60-mod list and scrolls to reveal later rows", async ({ page }) => {
    await loadScenario(page);
    await page.getByTestId("nav-order").click();
    await expect(page).toHaveURL(/\/order$/);

    const table = page.getByTestId("order-table");
    await expect(table).toBeVisible();

    const renderedBeforeScroll = await page.locator('[data-testid^="order-row-"]').count();
    expect(renderedBeforeScroll).toBeGreaterThan(0);
    expect(renderedBeforeScroll).toBeLessThan(60);
    await expect(page.getByTestId("order-row-mod.059")).not.toBeAttached();

    await table.evaluate((element) => {
      element.scrollTop = element.scrollHeight;
    });

    await expect(page.getByTestId("order-row-mod.059")).toBeVisible();
  });

  test("clicking a row opens the why-panel, and its edge chips navigate to the other mod", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-order").click();

    await page.getByTestId("order-row-mod.005").click();
    await expect(page).toHaveURL(/\/order\/mod\.005$/);
    await expect(page.getByTestId("why-panel-content")).toBeVisible();
    await expect(page.getByTestId("why-panel-lower-bounds")).toBeVisible();

    const modLink = page.locator('[data-testid^="edge-chip-mod-link-"]').first();
    const targetModId = (await modLink.getAttribute("data-testid"))?.replace(
      "edge-chip-mod-link-",
      "",
    );
    await modLink.click();

    await expect(page).toHaveURL(new RegExp(`/order/${targetModId}$`));
    await expect(page.getByTestId("why-panel-content")).toBeVisible();
  });

  test("shows which edge overruled a dropped one, and which settings produced the order", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-order").click();

    await page.getByTestId("order-row-mod.005").click();
    await expect(page.getByTestId("why-panel-content")).toBeVisible();

    const winnerLine = page.getByTestId("why-panel-dropped-winner");
    await expect(winnerLine).toContainText("Overruled by");
    await expect(winnerLine.getByTestId("edge-chip-mod-link-mod.002")).toBeVisible();

    await expect(page.getByTestId("why-panel-sort-settings")).toContainText(
      "tie-break: rebuild · imported pairs: off · imported placements: on",
    );
  });

  test("the finding link on an edge chip opens the inbox filtered to that pair", async ({
    page,
  }) => {
    await loadScenario(page);
    await page.getByTestId("nav-order").click();
    await page.getByTestId("order-row-mod.005").click();

    const findingLink = page.locator('[data-testid^="edge-chip-finding-link-"]').first();
    await findingLink.click();

    await expect(page).toHaveURL(/\/inbox\?search=/);
    await expect(page.getByTestId("finding-search")).toHaveValue(/mod\.005/);
  });
});
