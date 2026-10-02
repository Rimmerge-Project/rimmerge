import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";

import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { LOCALE_OPTIONS, type LocaleOption } from "../../src/i18n/locales";
import { loadScenario } from "../specs/support";
import { waitForQuietMain } from "./settle";

/**
 * The per-locale screenshot and overflow pass (see `playwright.config.ts`
 * in this folder). For every locale it switches the language through the
 * real picker, visits every shell route, and for each stop
 *
 * - screenshots the page light and dark (plus the shell's inner scroll
 *   containers scrolled to their end, which a full-page shot of the
 *   document never reaches), and
 * - audits the DOM for text that does not fit: clipped or ellipsized
 *   elements and horizontal page scroll,
 *
 * writing PNGs and one `overflow.json` per locale under
 * `RIMMERGE_SCREENSHOT_DIR/<locale>/`. The audit is a report for a human
 * to read next to the screenshots, not an assertion; the spec itself
 * fails only on an uncaught page error or a route that mounts nothing.
 * `RIMMERGE_SCREENSHOT_LOCALES=ru,ja` limits the run to those locales
 * (a delta round re-checks only what changed).
 */

const OUTPUT_DIR = process.env["RIMMERGE_SCREENSHOT_DIR"];
const ONLY_LOCALES = (process.env["RIMMERGE_SCREENSHOT_LOCALES"] ?? "")
  .split(",")
  .map((locale) => locale.trim())
  .filter((locale) => locale !== "");

const ROUTES = [
  "nav-dashboard",
  "nav-inbox",
  "nav-merge-mod",
  "nav-patches",
  "nav-assignments",
  "nav-startup",
  "nav-order",
  "nav-rules",
  "nav-mods",
  "nav-settings",
] as const;

const COLOR_SCHEMES = ["light", "dark"] as const;

type OverflowFinding = {
  readonly route: string;
  readonly kind: "clipped" | "ellipsized" | "pageScroll" | "pastViewport";
  readonly element: string;
  readonly text: string;
  readonly scrollWidth: number;
  readonly clientWidth: number;
};

function localesToTour(): readonly LocaleOption[] {
  const selected = LOCALE_OPTIONS.filter(
    (option) => ONLY_LOCALES.length === 0 || ONLY_LOCALES.includes(option.locale),
  );
  if (ONLY_LOCALES.length > 0 && selected.length !== ONLY_LOCALES.length) {
    throw new Error(`RIMMERGE_SCREENSHOT_LOCALES names an unknown locale: ${ONLY_LOCALES.join()}`);
  }
  return selected;
}

/** Reads the DOM for text that does not fit. Runs in the page; takes and returns plain data only. */
async function auditOverflow(page: Page, route: string): Promise<OverflowFinding[]> {
  return page.evaluate((routeName) => {
    const findings: OverflowFinding[] = [];
    const describe = (element: Element): string => {
      const testId = element.getAttribute("data-testid");
      return testId ? `${element.tagName.toLowerCase()}[${testId}]` : element.tagName.toLowerCase();
    };
    const push = (
      kind: OverflowFinding["kind"],
      element: Element,
      scrollWidth: number,
      clientWidth: number,
    ): void => {
      findings.push({
        route: routeName,
        kind,
        element: describe(element),
        text: (element.textContent ?? "").trim().replace(/\s+/g, " ").slice(0, 80),
        scrollWidth,
        clientWidth,
      });
    };
    const root = document.documentElement;
    if (root.scrollWidth > window.innerWidth + 1) {
      push("pageScroll", root, root.scrollWidth, window.innerWidth);
    }
    for (const element of document.body.querySelectorAll("*")) {
      const box = element.getBoundingClientRect();
      if (box.width === 0 || box.height === 0) {
        continue;
      }
      const style = getComputedStyle(element);
      const isTruncating = element.scrollWidth > element.clientWidth + 1;
      if (isTruncating && style.textOverflow === "ellipsis") {
        push("ellipsized", element, element.scrollWidth, element.clientWidth);
      } else if (isTruncating && (style.overflowX === "hidden" || style.overflowX === "clip")) {
        push("clipped", element, element.scrollWidth, element.clientWidth);
      } else if (box.right > window.innerWidth + 1 && style.position !== "fixed") {
        push("pastViewport", element, Math.round(box.right), window.innerWidth);
      }
    }
    return findings;
  }, route);
}

/** Scrolls every inner scroll container (the shell scrolls inside, not the document) to its end. */
async function scrollContainersToEnd(page: Page): Promise<boolean> {
  return page.evaluate(() => {
    let didScroll = false;
    for (const element of document.querySelectorAll("*")) {
      const { overflowY } = getComputedStyle(element);
      const isScrollable = overflowY === "auto" || overflowY === "scroll";
      if (isScrollable && element.scrollHeight > element.clientHeight + 1) {
        element.scrollTop = element.scrollHeight;
        didScroll = true;
      }
    }
    return didScroll;
  });
}

async function scrollContainersToStart(page: Page): Promise<void> {
  await page.evaluate(() => {
    for (const element of document.querySelectorAll("*")) {
      element.scrollTop = 0;
    }
  });
}

async function photographStop(page: Page, directory: string, stop: string): Promise<void> {
  for (const scheme of COLOR_SCHEMES) {
    await page.emulateMedia({ colorScheme: scheme });
    await scrollContainersToStart(page);
    await page.screenshot({ path: path.join(directory, `${stop}-${scheme}.png`), fullPage: true });
    if (await scrollContainersToEnd(page)) {
      await page.screenshot({
        path: path.join(directory, `${stop}-${scheme}-scrolled.png`),
        fullPage: true,
      });
    }
  }
  await scrollContainersToStart(page);
}

async function chooseLanguage(page: Page, option: LocaleOption): Promise<void> {
  await page.getByTestId("nav-settings").click();
  await page.getByTestId("language-picker-select").click();
  const name = option.isPreview ? `${option.nativeName} (preview)` : option.nativeName;
  await page.getByRole("option", { name, exact: true }).click();
  await expect(page.locator("html")).toHaveAttribute("lang", option.locale);
}

/**
 * Clicks a shell nav item and waits until its route is the active one and
 * has had a moment to render its lazily loaded content. Without this the
 * "not empty" check passes on the *previous* page and every screenshot is
 * one route behind.
 */
async function visitRoute(page: Page, navTestId: string): Promise<void> {
  const navItem = page.getByTestId(navTestId);
  await navItem.click();
  await expect(navItem).toHaveAttribute("aria-current", "page");
  await expect(page.locator("main")).not.toBeEmpty();
  await waitForQuietMain(page);
}

/** The Apply dialog is the densest layout in the app; photographed when the scenario offers it. */
async function photographApplyDialog(
  page: Page,
  directory: string,
  findings: OverflowFinding[],
): Promise<void> {
  await visitRoute(page, "nav-dashboard");
  const applyButton = page.getByTestId("dashboard-apply-button");
  if (!(await applyButton.isVisible()) || !(await applyButton.isEnabled())) {
    return;
  }
  await applyButton.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog).toBeVisible();
  // A mid-fade shot hides what it should show: wait for the open transition.
  await dialog.evaluate((element) =>
    Promise.all(element.getAnimations({ subtree: true }).map((animation) => animation.finished)),
  );
  findings.push(...(await auditOverflow(page, "apply-dialog")));
  await photographStop(page, directory, "apply-dialog");
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
}

/**
 * The finding list under its tallest cards: every status shown and the
 * Current order selected, so wrapped titles meet the decided/resolved chips.
 * The list sizes its rows from their content, so none may overlap the next.
 */
async function photographInboxWithChips(
  page: Page,
  directory: string,
  findings: OverflowFinding[],
): Promise<void> {
  await visitRoute(page, "nav-inbox");
  await page.getByTestId("order-source-current").click();
  await page.getByTestId("status-chip-all").click();
  // Deciding the selected finding gives its card the "decided" chip.
  await page.getByTestId("accept-button").click();
  await waitForQuietMain(page);
  findings.push(...(await auditOverflow(page, "inbox-all-current")));
  await photographStop(page, directory, "inbox-all-current");
  await page.getByTestId("order-source-suggested").click();
}

test.describe("locale tour", () => {
  test.skip(OUTPUT_DIR === undefined, "set RIMMERGE_SCREENSHOT_DIR to a scratch directory to run");

  for (const option of localesToTour()) {
    test(`tour ${option.locale}`, async ({ page }) => {
      const pageErrors: Error[] = [];
      page.on("pageerror", (error) => pageErrors.push(error));
      const directory = path.join(OUTPUT_DIR ?? "", option.locale);
      mkdirSync(directory, { recursive: true });
      const findings: OverflowFinding[] = [];

      await loadScenario(page);
      await chooseLanguage(page, option);

      for (const navTestId of ROUTES) {
        await visitRoute(page, navTestId);
        const stop = navTestId.replace(/^nav-/, "");
        findings.push(...(await auditOverflow(page, stop)));
        await photographStop(page, directory, stop);
      }

      await photographInboxWithChips(page, directory, findings);
      await photographApplyDialog(page, directory, findings);
      writeFileSync(
        path.join(directory, "overflow.json"),
        `${JSON.stringify(findings, null, 2)}\n`,
      );
      expect(pageErrors, `uncaught page error(s) under ${option.locale}`).toEqual([]);
    });
  }
});
