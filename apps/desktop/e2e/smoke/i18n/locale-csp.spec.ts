import { chromium, expect, test } from "@playwright/test";

const CDP_URL = "http://localhost:9223";
const APP_URL = "http://localhost:5173";

test("detects the WebView2-reported OS language and compiles messages under the real CSP", async () => {
  const browser = await chromium.connectOverCDP(CDP_URL);
  const context = browser.contexts()[0];
  if (!context) {
    throw new Error("WebView2 exposed no browser context over CDP");
  }
  const page = context.pages()[0] ?? (await context.waitForEvent("page"));
  await page.waitForLoadState();

  const cspViolations: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error" && /content security policy/i.test(message.text())) {
      cspViolations.push(message.text());
    }
  });

  try {
    await page.goto(`${APP_URL}/setup`);
    await expect(page.getByTestId("game-dir-input")).toBeVisible({ timeout: 30_000 });

    // `--lang=zh-CN` (this config's own `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS`)
    // is what WebView2 reports through `navigator.languages`, and
    // `detectLocale` (`src/i18n/locales.ts`) picks it up with no
    // `tauri-plugin-os` dependency — `setAppLocale` then syncs
    // `document.documentElement.lang` to match (`src/i18n/i18n.ts`).
    const htmlLang = await page.evaluate(() => document.documentElement.lang);
    expect(htmlLang).toBe("zh-CN");

    // `htmlLang` alone could pass on a locale a previous run persisted to
    // this profile's `localStorage`, with the page itself still rendering
    // English (a stale/failed detection) — this asserts the page actually
    // rendered in Chinese, not just that the `<html lang>` attribute says
    // so. `setup.title`'s own rendered text ("设置 Rimmerge").
    await expect(page.getByText("设置 Rimmerge")).toBeVisible();

    // vue-i18n's runtime message compiler ran to render this very page
    // (the Setup page's own chrome, and every `t()` call `TheShell.vue`
    // would use once mounted) under the app's real `script-src 'self'`
    // CSP — a JIT-compile failure there surfaces as a CSP console
    // violation, not a thrown JS error, which is why this asserts on
    // console output rather than a try/catch around rendering.
    expect(cspViolations).toEqual([]);
  } finally {
    await browser.close();
  }
});
