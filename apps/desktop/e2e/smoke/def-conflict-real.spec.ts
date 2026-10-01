import { chromium, expect, test } from "@playwright/test";

const CDP_URL = "http://localhost:9222";
// The real Vite dev server this config's `webServer` starts (matches
// `src-tauri/tauri.conf.json`'s `devUrl`) — a full navigation here, not a
// click through the SPA's own router, so this spec starts from a clean
// session regardless of what another spec left loaded in the same
// shared, already-running app (`workers: 1` runs every smoke spec one
// after another against one `tauri dev` process).
const APP_URL = "http://localhost:5173";

/**
 * The desktop-tier real-install check:
 * loads the machine's real, read-only RimWorld install (never a scratch
 * fixture — the point is the two *named* real contested defs, which only
 * exist on a real install with the right real mods active)
 * against a scratch *profile* directory, then opens
 * `BiomeDef/AridShrubland`'s `plantDensity` patch collision and
 * `HediffDef/BionicHeart`'s `comps` list in the real `DefConflictView`.
 *
 * Real installs vary in size and this repo's own convention is never to
 * assert exact counts that drift with the Workshop — every assertion
 * below is qualitative (a row exists, both mods appear, a value is
 * present), not a specific mod/finding count.
 *
 * **Neither real def produces a `Conflict` row on a typical install**:
 * `apps/cli/tests/real_install_defs.rs`'s own Rust-tier cases show that
 * the two real contributors happen to replace `plantDensity` to the
 * identical value (`Agreeing`/`CleanMerge`, not `Conflict`), and
 * BionicHeart's `DefOverride` has only two owners, one of which is
 * definitionally the diff's own base (so it can only ever be
 * `CleanMerge`/`ListEntry`, never `Conflict`). This spec therefore
 * asserts the real `CleanMerge`/`ListEntry` shapes end to end instead of
 * a `Conflict` row that does not exist for either case.
 */
test("AridShrubland's plantDensity collision and BionicHeart's comps list render against the real install", async () => {
  // The real install's full scan runs the actual Rust backend in a
  // `tauri dev` (debug) build — far slower than the release-profile
  // Rust-tier tests — so this one spec gets a generous budget well above
  // the config's own 90s default.
  test.setTimeout(300_000);

  const profileDir = process.env.RIMMERGE_SMOKE_REAL_PROFILE_DIR;
  if (!profileDir) {
    throw new Error(
      "RIMMERGE_SMOKE_REAL_PROFILE_DIR was not set by e2e/smoke/playwright.config.ts",
    );
  }

  const browser = await chromium.connectOverCDP(CDP_URL);
  const context = browser.contexts()[0];
  if (!context) {
    throw new Error("WebView2 exposed no browser context over CDP");
  }
  const page = context.pages()[0] ?? (await context.waitForEvent("page"));
  await page.waitForLoadState();

  try {
    await page.goto(`${APP_URL}/setup`);

    // `game-dir-input`/`workshop-dir-input`/`mods-config-input` are left
    // exactly as `get_default_paths` prefills them — the machine's real,
    // read-only RimWorld/Workshop/ModsConfig.xml paths — deliberately
    // never filled by hand, unlike every scratch-game spec in this
    // suite. Only `profile-dir-input` is overridden, so this run never
    // reads or writes the real profile store.
    await expect(page.getByTestId("game-dir-input")).not.toHaveValue("", { timeout: 30_000 });
    await expect(page.getByTestId("mods-config-input")).not.toHaveValue("", { timeout: 30_000 });
    await page.getByTestId("profile-dir-input").fill(profileDir);

    await page.getByTestId("load-project-button").click();

    const modCount = page.getByTestId("mod-count");
    await expect(modCount).toBeVisible({ timeout: 240_000 });
    await expect
      .poll(async () => Number.parseInt((await modCount.textContent()) ?? "0", 10))
      .toBeGreaterThan(0);

    await page.getByTestId("nav-inbox").click();
    await expect(page.getByTestId("finding-list")).toBeVisible({ timeout: 30_000 });
    // The default status filter is "needs input" — a real def-shaped
    // finding can just as well auto-resolve (BionicHeart's own two-owner
    // override does), so every search below runs against "all" statuses
    // rather than assuming either finding needs a decision.
    await page.getByTestId("status-chip-all").click();

    // --- BiomeDef/AridShrubland's plantDensity collision ---
    await page.getByTestId("finding-search").fill("AridShrubland");
    const aridCard = page.locator(
      '[data-testid^="finding-card-"][data-testid*="AridShrubland:def_name:plantDensity"]',
    );
    await expect(aridCard).toHaveCount(1, { timeout: 15_000 });
    await aridCard.click();

    const conflictView = page.getByTestId("def-conflict-view");
    await expect(conflictView).toBeVisible({ timeout: 15_000 });
    await expect(conflictView.getByTestId("def-conflict-view-def-ref")).toHaveText(
      "BiomeDef/AridShrubland",
    );

    const plantDensityRow = conflictView.locator('[data-path="plantDensity"]');
    await expect(plantDensityRow).toBeVisible();
    await expect(
      plantDensityRow.locator('[data-testid^="def-conflict-value-plantDensity-"]'),
    ).toHaveCount(2);
    // Named only through this env var — format "<mod a>,<mod b>", the same one the Rust
    // tier's own `real_install_defs.rs` reads. Unset, the count check
    // above is this block's own assertion.
    const plantDensityPair = process.env.RIMMERGE_EXPECTED_PLANT_DENSITY_PAIR?.split(",");
    if (plantDensityPair) {
      for (const modId of plantDensityPair) {
        await expect(
          plantDensityRow.locator(`[data-testid="def-conflict-value-plantDensity-${modId}"]`),
        ).toBeVisible();
      }
    }
    await expect(plantDensityRow.getByTestId("def-conflict-in-game")).toBeVisible();
    await expect(plantDensityRow.getByTestId("def-conflict-after-merge")).toBeVisible();

    // --- BiomeDef/AridShrubland's wildAnimals collision, rendered as a
    // per-key list rather than a single-line string ---
    // `AridShrubland` has four separate `PatchCollision` findings
    // (`plantDensity`/`pollutionWildAnimals`/`wildAnimals`/`wildPlants`)
    // on this install, so the search above (which matched all of them)
    // isn't enough here — this card is the `wildAnimals`-specific one.
    await page.getByTestId("finding-search").fill("AridShrubland");
    const wildAnimalsCard = page.locator(
      '[data-testid^="finding-card-"][data-testid*="AridShrubland:def_name:wildAnimals"]',
    );
    await expect(wildAnimalsCard).toHaveCount(1, { timeout: 15_000 });
    await wildAnimalsCard.click();

    await expect(conflictView).toBeVisible({ timeout: 15_000 });
    await expect(conflictView.getByTestId("def-conflict-view-def-ref")).toHaveText(
      "BiomeDef/AridShrubland",
    );

    // (a) per-key rows under one collapsible group header, never a
    // single-line XML blob for the whole container: three real content
    // mods each add their own disjoint animals — dozens of entries, all
    // grouped under one `wildAnimals` header (`FieldRowsTable.vue`'s own
    // `mapEntry` grouping bucket). Anchored to the *start* of the header
    // text, not a bare substring: `AridShrubland` also has its own
    // `coastalWildAnimals` keyed map on this real install, whose own
    // header (`"coastalWildAnimals (17 entries)"`) is a case-insensitive
    // substring match for a plain `hasText: "wildAnimals"` filter and
    // resolved to two elements the first time this spec ran.
    const wildAnimalsHeader = conflictView.locator('[data-testid="def-conflict-group-header"]', {
      hasText: /^wildAnimals \(/,
    });
    await expect(wildAnimalsHeader).toBeVisible({ timeout: 15_000 });
    await expect(wildAnimalsHeader).toHaveText(/^wildAnimals \(\d+ entries\)/);

    const wildAnimalsRows = conflictView.locator('[data-path^="wildAnimals/"]');
    await expect.poll(async () => wildAnimalsRows.count()).toBeGreaterThan(1);
    // Spot-check one real key naming its own contributor, not a shared
    // multi-kilobyte blob — named only through this env var, format
    // "<wildAnimals key>:<mod id>,...". Unset, the row-count check above
    // is this block's own assertion.
    const wildAnimalsSample = process.env.RIMMERGE_EXPECTED_WILD_ANIMALS_SAMPLE?.split(",");
    if (wildAnimalsSample) {
      for (const entry of wildAnimalsSample) {
        const [animal, modId] = entry.split(":");
        const row = conflictView.locator(`[data-path="wildAnimals/${animal}"]`);
        await expect(row).toBeVisible();
        await expect(
          row.locator(`[data-testid="def-conflict-value-wildAnimals/${animal}-${modId}"]`),
        ).toBeVisible();
      }
    }

    // --- HediffDef/BionicHeart's comps list ---
    await page.getByTestId("finding-search").fill("BionicHeart");
    const bionicCard = page.locator(
      '[data-testid^="finding-card-"][data-testid*="HediffDef/BionicHeart"]',
    );
    await expect(bionicCard).toHaveCount(1, { timeout: 15_000 });
    await bionicCard.click();

    await expect(conflictView).toBeVisible({ timeout: 15_000 });
    await expect(conflictView.getByTestId("def-conflict-view-def-ref")).toHaveText(
      "HediffDef/BionicHeart",
    );

    const compsEntries = conflictView.locator('[data-path^="comps/li["]');
    await expect.poll(async () => compsEntries.count()).toBeGreaterThan(0);
    // Every comps list entry names at least one source mod as its own
    // value chip — the "list entries with mod chips" shape.
    const firstEntry = compsEntries.first();
    await expect(firstEntry.locator('[data-testid^="def-conflict-value-"]')).not.toHaveCount(0);
  } finally {
    await browser.close();
  }
});
