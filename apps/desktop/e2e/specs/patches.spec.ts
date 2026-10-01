import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

const bionicHeartKey = "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]";
const headNormalKey =
  "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]";

/** Opens `/patches`, presses `n`, fills the identity fields, and adds `scopeModIds` via the search picker — leaves the create form open, not yet submitted. */
async function fillCreateForm(
  page: import("@playwright/test").Page,
  values: { name: string; packageId: string; displayName: string },
  scopeModIds: string[],
): Promise<void> {
  await page.getByTestId("nav-patches").click();
  await expect(page).toHaveURL(/\/patches$/);
  await page.keyboard.press("n");
  await expect(page.getByTestId("patch-create-form")).toBeVisible();

  await page.getByTestId("patch-name-input").fill(values.name);
  await page.getByTestId("patch-package-id-input").fill(values.packageId);
  await page.getByTestId("patch-display-name-input").fill(values.displayName);

  for (const modId of scopeModIds) {
    await page.getByTestId("scope-search").fill(modId);
    await expect(page.getByTestId(`scope-add-${modId}`)).toBeVisible();
    await page.getByTestId(`scope-add-${modId}`).click();
  }
}

test.describe("patches", () => {
  test("n on /patches, the identity form, and two scope picks record create_patch and land on the detail page", async ({
    page,
  }) => {
    await loadScenario(page);
    await fillCreateForm(
      page,
      {
        name: "Wall compat",
        packageId: "author.wallcompat",
        displayName: "Wall Compatibility Patch",
      },
      ["mod.010", "mod.011"],
    );

    await page.getByTestId("patch-save-button").click();

    await expect(page).toHaveURL(/\/patches\/[0-9a-f]{12}$/);
    await expect(page.getByTestId("patch-detail-header")).toContainText("Wall Compatibility Patch");

    const calls = await page.evaluate(() => window.__CREATE_PATCH_CALLS__ ?? []);
    expect(calls).toEqual([
      {
        name: "Wall compat",
        packageId: "author.wallcompat",
        displayName: "Wall Compatibility Patch",
        scope: ["mod.010", "mod.011"],
      },
    ]);
  });

  test("the scoped inbox lists only findings between scope members, badging a partial one, and never a single-mod finding", async ({
    page,
  }) => {
    await loadScenario(page);
    await fillCreateForm(
      page,
      { name: "Heart compat", packageId: "author.heartcompat", displayName: "Heart Compat" },
      ["ludeon.rimworld", "example.bionicsfork"],
    );
    await page.getByTestId("patch-save-button").click();
    await expect(page).toHaveURL(/\/patches\/[0-9a-f]{12}$/);

    // The scope chip shows the fixture mod's real display name, not its
    // bare id — `nameForModId` delegates to `allModNames()` for a
    // hand-crafted `mergeFixtures` owner like this one.
    await expect(page.getByTestId("scope-member-ludeon.rimworld")).toContainText("RimWorld");

    const patchInbox = page.getByTestId("patch-inbox");
    // `BionicHeart`'s two owners are exactly this scope -> full. `HeadNormal`'s
    // third owner (`exampleanim.mod`) is outside it -> partial.
    await expect(patchInbox.getByTestId(`finding-card-${bionicHeartKey}`)).toBeVisible();
    await expect(
      patchInbox.getByTestId(`finding-card-${bionicHeartKey}`).getByTestId("scope-badge"),
    ).toHaveCount(0);

    await expect(patchInbox.getByTestId(`finding-card-${headNormalKey}`)).toBeVisible();
    await expect(
      patchInbox.getByTestId(`finding-card-${headNormalKey}`).getByTestId("scope-badge"),
    ).toContainText("Example Animation");

    // `missing_mod` is single-mod — `PatchScope::membership` always calls
    // it `Outside`, regardless of scope — never admitted into the scoped
    // ledger at all.
    await expect(patchInbox.locator('[data-testid^="finding-card-missing_mod"]')).toHaveCount(0);
  });

  test("Enter on a partial finding accepts the rewritten suggestion, recording decide_patch with an Ignore", async ({
    page,
  }) => {
    await loadScenario(page);
    await fillCreateForm(
      page,
      { name: "Heart compat", packageId: "author.heartcompat2", displayName: "Heart Compat 2" },
      ["ludeon.rimworld", "example.bionicsfork"],
    );
    await page.getByTestId("patch-save-button").click();
    await expect(page).toHaveURL(/\/patches\/[0-9a-f]{12}$/);

    const patchId = page.url().split("/").pop() ?? "";
    const patchInbox = page.getByTestId("patch-inbox");
    await patchInbox.getByTestId("finding-search").fill("HeadNormal");
    await expect(patchInbox.getByTestId(`finding-card-${headNormalKey}`)).toBeVisible();
    await patchInbox.getByTestId(`finding-card-${headNormalKey}`).click();
    await expect(page.getByTestId("suggestion-panel")).toContainText(headNormalKey);

    await page.getByTestId("accept-button").click();

    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_PATCH_CALLS__ ?? []))
      .toEqual([{ patchId, key: headNormalKey, action: { kind: "ignore" }, note: null }]);
  });

  test("shrinking the scope orphans an out-of-admission decision, shown in the scope-change summary and the header count", async ({
    page,
  }) => {
    await loadScenario(page);
    // The initial scope is exactly `HeadNormal`'s three owners, so it
    // starts fully admitted.
    await fillCreateForm(
      page,
      { name: "Head compat", packageId: "author.headcompat", displayName: "Head Compat" },
      ["ludeon.rimworld", "example.bionicsfork", "exampleanim.mod"],
    );
    await page.getByTestId("patch-save-button").click();
    await expect(page).toHaveURL(/\/patches\/[0-9a-f]{12}$/);
    const patchId = page.url().split("/").pop() ?? "";

    const patchInbox = page.getByTestId("patch-inbox");
    await patchInbox.getByTestId("finding-search").fill("HeadNormal");
    await expect(patchInbox.getByTestId(`finding-card-${headNormalKey}`)).toBeVisible();
    await patchInbox.getByTestId(`finding-card-${headNormalKey}`).click();
    await expect(page.getByTestId("suggestion-panel")).toContainText(headNormalKey);
    await page.keyboard.press("i");
    await expect
      .poll(() => page.evaluate(() => window.__DECIDE_PATCH_CALLS__ ?? []))
      .toEqual([{ patchId, key: headNormalKey, action: { kind: "ignore" }, note: null }]);

    // Add a fourth member first, then remove two of `HeadNormal`'s three
    // owners one at a time — the scope never dips below two members at
    // any intermediate step, matching the real `PatchScope::new` floor.
    // Each removal's `update_patch` round trip is awaited (via the removed
    // chip actually disappearing) before the next click, since `ScopeEditor`
    // derives the next scope from its own `members` prop — a click fired
    // before the previous update refetches would compute the new set from
    // stale data.
    await page.getByTestId("scope-search").fill("mod.010");
    await page.getByTestId("scope-add-mod.010").click();
    await expect(page.getByTestId("scope-member-mod.010")).toBeVisible();

    await page.getByTestId("scope-member-remove-ludeon.rimworld").click();
    await expect(page.getByTestId("scope-member-ludeon.rimworld")).toHaveCount(0);
    await expect(page.getByTestId("scope-change-summary")).toHaveCount(0);

    await page.getByTestId("scope-member-remove-example.bionicsfork").click();
    await expect(page.getByTestId("scope-member-example.bionicsfork")).toHaveCount(0);

    await expect(page.getByTestId("scope-change-summary")).toContainText(headNormalKey);
    await expect(page.getByTestId("patch-orphaned-count")).toContainText("1");

    // Each edit's own `update_patch` sent exactly the scope `ScopeEditor`
    // derived from its own `members` prop at that point — the add
    // appended, each removal dropped one member in place, in order.
    await expect
      .poll(() => page.evaluate(() => window.__UPDATE_PATCH_CALLS__ ?? []))
      .toEqual([
        expect.objectContaining({
          patchId,
          scope: ["ludeon.rimworld", "example.bionicsfork", "exampleanim.mod", "mod.010"],
        }),
        expect.objectContaining({
          patchId,
          scope: ["example.bionicsfork", "exampleanim.mod", "mod.010"],
        }),
        expect.objectContaining({ patchId, scope: ["exampleanim.mod", "mod.010"] }),
      ]);
  });

  test("m opens the scoped finding's editor at the patch route, a pick records set_merge_choices with patchId, and Escape returns to the patch", async ({
    page,
  }) => {
    await loadScenario(page);
    await fillCreateForm(
      page,
      { name: "Heart compat 3", packageId: "author.heartcompat3", displayName: "Heart Compat 3" },
      ["ludeon.rimworld", "example.bionicsfork"],
    );
    await page.getByTestId("patch-save-button").click();
    await expect(page).toHaveURL(/\/patches\/[0-9a-f]{12}$/);
    const patchId = page.url().split("/").pop() ?? "";

    const patchInbox = page.getByTestId("patch-inbox");
    await expect(patchInbox.getByTestId(`finding-card-${bionicHeartKey}`)).toBeVisible();
    await patchInbox.getByTestId(`finding-card-${bionicHeartKey}`).click();
    await expect(page.getByTestId("suggestion-panel")).toContainText(bionicHeartKey);

    await page.keyboard.press("m");
    await expect(page).toHaveURL(new RegExp(`/patches/${patchId}/merge/`));
    await expect(page.getByTestId("merge-header-def-key")).toHaveText("HediffDef/BionicHeart");
    // Both of `BionicHeart`'s owners are inside this patch's own scope,
    // so nothing is excluded from the diff.
    await expect(page.getByTestId("merge-header-out-of-scope")).toHaveCount(0);

    // `x` opens the auto section (collapsed by default) so the row
    // cursor lands on `label`, the first `oneSided` field — then `2`
    // picks owner column 2 (`example.bionicsfork`; 1 is the base,
    // `ludeon.rimworld`) for the row at the cursor.
    await page.keyboard.press("x");
    await page.keyboard.press("2");

    await expect
      .poll(() => page.evaluate(() => window.__SET_MERGE_CHOICES_CALLS__ ?? []))
      .toEqual([
        {
          key: bionicHeartKey,
          choices: { label: { choice: "from", modId: "example.bionicsfork" } },
          patchId,
        },
      ]);

    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(new RegExp(`/patches/${patchId}$`));
    await expect(page.getByTestId("patch-detail-header")).toBeVisible();

    // The cursor survives the merge-editor round trip. Switched to the
    // "All" status filter first — the pick above already decided
    // `BionicHeart` (`userOverridden`), which the default `needsInput`
    // filter would otherwise drop from the list the moment `PatchInbox`
    // remounts, leaving only one row and making a "moved to index 1"
    // check meaningless. Moved to index 1 (`HeadNormal`; `BionicHeart`'s
    // own row is index 0 — the same index a wrongly-reset store would
    // also land back on, so asserting there wouldn't prove anything)
    // before opening the editor a second time.
    await patchInbox.getByTestId("status-chip-all").click();
    await expect(patchInbox.getByTestId(`finding-card-${bionicHeartKey}`)).toBeVisible();
    await expect(patchInbox.getByTestId(`finding-card-${headNormalKey}`)).toBeVisible();
    await page.keyboard.press("j");
    const activeCard = patchInbox.locator('[aria-current="true"]');
    await expect(activeCard).toHaveAttribute("data-testid", `finding-card-${headNormalKey}`);

    await page.keyboard.press("m");
    await expect(page).toHaveURL(new RegExp(`/patches/${patchId}/merge/`));
    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(new RegExp(`/patches/${patchId}$`));

    await expect(patchInbox.locator('[aria-current="true"]')).toHaveAttribute(
      "data-testid",
      `finding-card-${headNormalKey}`,
    );
    // Discriminating, not just "some row is current": a wrongly-reset
    // store would also drop back to the default `needsInput` filter,
    // which would hide `BionicHeart` (already decided above) entirely —
    // so both the filter and the first fixture card surviving the round
    // trip are asserted, not only the cursor.
    await expect(patchInbox.getByTestId("status-chip-all")).toHaveAttribute("aria-pressed", "true");
    await expect(patchInbox.getByTestId(`finding-card-${bionicHeartKey}`)).toBeVisible();
  });

  test("the patch inbox's own conflict view scopes fields to its own members", async ({ page }) => {
    // `get_def_conflict_view` scopes to a compat patch's own
    // preview/decisions, so `DefConflictView` renders here too.
    await loadScenario(page);
    await fillCreateForm(
      page,
      { name: "Head compat", packageId: "author.headcompat", displayName: "Head Compat" },
      ["ludeon.rimworld", "example.bionicsfork"],
    );
    await page.getByTestId("patch-save-button").click();
    await expect(page).toHaveURL(/\/patches\/[0-9a-f]{12}$/);

    // `HeadNormal`'s third owner, `exampleanim.mod`, is outside this
    // patch's own scope — "All" status is needed to find it at all (see
    // the identical note in the test above).
    const patchInbox = page.getByTestId("patch-inbox");
    await patchInbox.getByTestId("status-chip-all").click();
    await expect(patchInbox.getByTestId(`finding-card-${headNormalKey}`)).toBeVisible();
    await patchInbox.getByTestId(`finding-card-${headNormalKey}`).click();

    const view = page.getByTestId("def-conflict-view");
    await expect(view.getByTestId("def-conflict-view-def-ref")).toHaveText(
      "HeadTypeDef/HeadNormal",
    );

    // At the profile level (`summary.spec.ts`'s own HeadNormal test) this
    // is a genuine two-mod `Conflict`. Scoped to just this patch's own
    // members, `exampleanim.mod`'s own candidate drops out of the
    // diff entirely, leaving `example.bionicsfork` as the sole contributor —
    // a clean, single-owner merge, not a conflict, under this scope.
    const skinColor = view.locator('[data-path="skinColorOverride"]');
    await expect(skinColor).toContainText("Clean merge");
    await expect(
      skinColor.locator('[data-testid="def-conflict-value-skinColorOverride-exampleanim.mod"]'),
    ).toHaveCount(0);
    await expect(skinColor.getByTestId("def-conflict-after-merge")).toContainText(
      "Example Bionics Fork",
    );

    // `inGame` is read off the field's own unscoped `changedBy` (this
    // mock's `effectiveFieldOwner`, mirroring the real backend's own
    // "in-game is always the unscoped fold's result") rather than the
    // scope-narrowed one `values`/`afterMerge` above use — for this
    // particular field the two happen to agree (`example.bionicsfork` is
    // `changedBy`'s own last entry either way), so this only confirms
    // `inGame` renders correctly under a patch-scoped request, not that
    // scoping can never move it.
    await expect(skinColor.getByTestId("def-conflict-in-game")).toContainText(
      "Example Bionics Fork",
    );
  });

  test("the export panel: picking a folder and exporting records the exact export_patch payload and shows the returned path", async ({
    page,
  }) => {
    await loadScenario(page);
    await fillCreateForm(
      page,
      { name: "Wall compat 2", packageId: "author.wallcompat2", displayName: "Wall Compat 2" },
      ["mod.010", "mod.011"],
    );
    await page.getByTestId("patch-save-button").click();
    await expect(page).toHaveURL(/\/patches\/[0-9a-f]{12}$/);
    const patchId = page.url().split("/").pop() ?? "";

    await page.evaluate(() => {
      window.__PICK_FOLDER_RESULT__ = "C:/exports/wallcompat2";
    });
    await page.getByTestId("patch-export-choose-button").click();
    await expect(page.getByTestId("patch-export-dir-input")).toHaveValue("C:/exports/wallcompat2");

    await page.getByTestId("patch-export-button").click();

    await expect
      .poll(() => page.evaluate(() => window.__EXPORT_PATCH_CALLS__ ?? []))
      .toEqual([{ patchId, outDir: "C:/exports/wallcompat2", install: false, force: false }]);
    await expect(page.getByTestId("patch-export-last-path")).toContainText(
      "C:/exports/wallcompat2",
    );
  });
});
