import { expect, test } from "@playwright/test";

import { installScenario } from "../fixtures/scenario";
import { failOnSameOriginRequestFailure } from "./support";

// The tier's server must speak HTTP/2 (#58): over HTTP/1.1 every per-test context opens about
// six keep-alive sockets, and on Windows the TIME_WAIT entries they leave behind now and then
// make the OS refuse a module request, leaving a blank page. A run without `RIMMERGE_E2E_HTTPS=1`
// fails at start-up (the https URL never answers); this spec catches a revert to http, or h2
// being lost while TLS stays on (see `vite.config.ts` and the Playwright configs).
test.describe("e2e dev server", () => {
  test("serves the app over HTTP/2", async ({ page }) => {
    failOnSameOriginRequestFailure(page);
    // The mock backend, so the app boots as in every other spec instead of logging a missing Tauri.
    await page.addInitScript(installScenario);
    await page.goto("/setup");

    const protocol = await page.evaluate(() => {
      const [entry] = performance.getEntriesByType("navigation");
      return entry instanceof PerformanceNavigationTiming
        ? entry.nextHopProtocol
        : "no navigation entry";
    });

    expect(protocol).toBe("h2");
  });
});
