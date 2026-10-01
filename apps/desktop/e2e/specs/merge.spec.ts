import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * Searches the inbox down to exactly the finding named `needle`, selects
 * it, and waits for its detail to load — pressing `m` right after relies
 * on `currentDetail` already being resolved.
 */
async function openFindingInInbox(
  page: import("@playwright/test").Page,
  needle: string,
): Promise<void> {
  await page.getByTestId("nav-inbox").click();
  await page.getByTestId("finding-search").fill(needle);
  await expect.poll(async () => page.locator('[data-testid^="finding-card-"]').count()).toBe(1);
  await page.locator('[data-testid^="finding-card-"]').first().click();
  await expect(page.getByTestId("suggestion-panel")).toBeVisible();
}

test.describe("merge editor", () => {
  test("m on a def-override finding navigates to its editor; the header shows owners in order and totals", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "BionicHeart");

    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    await expect(page.getByTestId("merge-header-def-key")).toHaveText("HediffDef/BionicHeart");
    const owners = page.getByTestId("merge-header-owners").locator("> span");
    await expect(owners).toHaveCount(2);
    await expect(owners.nth(0)).toContainText("RimWorld");
    await expect(owners.nth(0)).toContainText("(base)");
    await expect(owners.nth(1)).toContainText("Example Bionics Fork");
    await expect(owners.nth(1)).toContainText("(winner)");
    await expect(page.getByTestId("merge-header-totals")).toContainText("46 fields");

    const decideCalls = await page.evaluate(() => window.__DECIDE_CALLS__ ?? []);
    expect(decideCalls).toEqual([
      {
        key: "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]",
        action: {
          kind: "merge",
          key: { defType: "HediffDef", defName: "BionicHeart" },
          choices: {},
        },
        note: null,
      },
    ]);
  });

  test("keyboard walk: a conflict row's owner pick sends the exact payload, flips the pill, and a reverts it", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "HeadNormal");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    await expect(page.getByTestId("merge-header-state")).toHaveText("2 fields need input");

    // `c` moves the row cursor to the next `Conflict` row (vim-style
    // "next occurrence", inclusive-start-exclusive-current) — from the
    // first conflict this lands on the second, `eyeSize`.
    await page.keyboard.press("c");
    await page.keyboard.press("2"); // owner column 2 = `example.bionicsfork` (1 = base)

    // Asserts the whole call log, not just its last entry — a duplicate
    // send (e.g. a stale debounce timer firing a second time) would still
    // leave the *last* entry correct, so only the full array catches it.
    await expect
      .poll(() => page.evaluate(() => window.__SET_MERGE_CHOICES_CALLS__ ?? []))
      .toEqual([
        {
          key: "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]",
          choices: { eyeSize: { choice: "from", modId: "example.bionicsfork" } },
          patchId: null,
        },
      ]);
    await expect(page.getByTestId("merge-header-state")).toHaveText("1 field needs input");

    await page.keyboard.press("a");
    await expect
      .poll(() => page.evaluate(() => window.__SET_MERGE_CHOICES_CALLS__ ?? []))
      .toEqual([
        {
          key: "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]",
          choices: { eyeSize: { choice: "from", modId: "example.bionicsfork" } },
          patchId: null,
        },
        {
          key: "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]",
          choices: {},
          patchId: null,
        },
      ]);
    await expect(page.getByTestId("merge-header-state")).toHaveText("2 fields need input");
  });

  test("e + typing sends a Value choice", async ({ page }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "HeadNormal");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    // The row cursor starts on the first conflict row (`skinColorOverride`).
    await page.keyboard.press("e");
    const input = page.getByTestId("merge-edit-input");
    await expect(input).toBeFocused();
    await input.fill("(0.5,0.5,0.5)");
    await input.press("Enter");

    // The whole call log, not just its last entry — see the equivalent
    // comment in the keyboard-walk test above.
    await expect
      .poll(() => page.evaluate(() => window.__SET_MERGE_CHOICES_CALLS__ ?? []))
      .toEqual([
        {
          key: "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]",
          choices: { skinColorOverride: { choice: "value", text: "(0.5,0.5,0.5)" } },
          patchId: null,
        },
      ]);
  });

  test("Esc returns to the inbox with the cursor preserved, even at a non-zero index", async ({
    page,
  }) => {
    await loadScenario(page);
    // `HeadNormal`, not `BionicHeart`: an empty `Merge` on a finding with
    // no `Conflict` fields resolves to `Complete` immediately (nothing
    // left to auto-resolve), which removes it from the inbox's default
    // `needsInput` filter — `HeadNormal` still has two unresolved
    // conflicts, so it stays put to prove the cursor itself is untouched.
    const key =
      "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]";

    // "example.bionicsfork" (an owner of both mergeable fixtures, and of
    // nothing else in the scenario) narrows to exactly two cards —
    // `BionicHeart` (confidence 42) then `HeadNormal` (confidence 43),
    // the list's ascending-by-confidence sort — so `j` moves the cursor
    // onto `HeadNormal` at index 1, not 0. A search narrowed to exactly
    // *one* result (as `openFindingInInbox` does) would leave this test
    // passing vacuously regardless of whether the cursor is preserved.
    await page.getByTestId("nav-inbox").click();
    await page.getByTestId("finding-search").fill("example.bionicsfork");
    await expect.poll(async () => page.locator('[data-testid^="finding-card-"]').count()).toBe(2);
    // Click the first card (`BionicHeart`, confidence 42) both to move
    // focus off the search input (so `j` isn't swallowed as text entry)
    // and to put the cursor at a known index 0 before moving it.
    await page.locator('[data-testid^="finding-card-"]').first().click();
    await page.keyboard.press("j");
    await expect(page.getByTestId(`finding-card-${key}`)).toHaveAttribute("aria-current", "true");
    // `m` relies on `currentDetail` already being resolved for the row
    // the cursor just moved to (see this file's own header comment) — the
    // panel is already visible from `BionicHeart`'s own detail, so wait
    // for its header to actually name `HeadNormal`'s key, not merely for
    // the panel element to exist.
    await expect(page.getByTestId("suggestion-panel")).toContainText(key);

    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(/\/inbox$/);
    await expect(page.getByTestId(`finding-card-${key}`)).toHaveAttribute("aria-current", "true");
  });
});

test.describe("keyed-map container grouping", () => {
  test("wildAnimals renders as grouped child rows, not a one-line XML blob, and Accept suggested resolves every contested key in one request", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "wildlife.mod.c");

    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    // Ten keys total (four unchanged, three disjoint adds, one agreed-on,
    // two genuine conflicts) — only the two conflicts need a person's
    // input, matching the finding's own complaint: the three adds "already
    // union", which the grouped rows (rather than one multi-kilobyte blob)
    // make visible.
    await expect(page.getByTestId("merge-header-state")).toHaveText("2 fields need input");

    const header = page.getByTestId("merge-container-conflicts-wildAnimals");
    await expect(header).toContainText("wildAnimals — 10 entries, 2 conflicts");

    const acceptButton = page.getByTestId("merge-accept-union-wildAnimals");
    await expect(acceptButton).toHaveText("Accept suggested (2)");
    await acceptButton.click();

    await expect
      .poll(() => page.evaluate(() => window.__SET_MERGE_CHOICES_CALLS__ ?? []))
      .toEqual([
        {
          key: "patch_collision:BiomeDef/AridShrubland:defName:wildAnimals:[wildlife.mod.a,wildlife.mod.b,wildlife.mod.c]",
          choices: {
            "wildAnimals/Raptor": { choice: "from", modId: "wildlife.mod.c" },
            "wildAnimals/Boar": { choice: "from", modId: "wildlife.mod.c" },
          },
          patchId: null,
        },
      ]);
    await expect(page.getByTestId("merge-header-state")).toHaveText("merged");
  });

  test("the auto-resolved section's own wildAnimals group is manually collapsible, even though it starts open (a genuine conflict elsewhere in the container keeps the default open)", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "wildlife.mod.c");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    await page.getByTestId("merge-section-auto").click();
    const header = page.getByTestId("merge-container-auto-wildAnimals");
    await expect(header).toContainText("wildAnimals — 10 entries, 2 conflicts");
    // Starts open: `wildAnimals` still has two unresolved conflicts
    // elsewhere in the container, so the default-collapse rule (more than
    // 12 entries *and* nothing left to review) never applies here.
    await expect(
      page.getByTestId("merge-field-row").filter({ hasText: "wildAnimals/Cobra" }),
    ).toBeVisible();

    await header.click();
    await expect(
      page.getByTestId("merge-field-row").filter({ hasText: "wildAnimals/Cobra" }),
    ).toHaveCount(0);

    await header.click();
    await expect(
      page.getByTestId("merge-field-row").filter({ hasText: "wildAnimals/Cobra" }),
    ).toBeVisible();
  });
});

test.describe("merge mod page", () => {
  const bionicHeartKey = "def_override:HediffDef/BionicHeart:[ludeon.rimworld,example.bionicsfork]";
  const headNormalKey =
    "def_override:HeadTypeDef/HeadNormal:[exampleanim.mod,ludeon.rimworld,example.bionicsfork]";

  test("lists entries by state and previews About.xml with the fixture's packageId", async ({
    page,
  }) => {
    await loadScenario(page);

    // `BionicHeart` has no `Conflict` fields, so deciding it empty via `m`
    // resolves straight to `Complete` — one entry, no field input needed.
    await openFindingInInbox(page, "BionicHeart");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    // `HeadNormal` still has two `Conflict` fields, so its empty decision
    // stays `NeedsFieldInput` — the second entry, still visible in the
    // inbox's default view to reach it.
    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(/\/inbox$/);
    await openFindingInInbox(page, "HeadNormal");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    await page.getByTestId("nav-merge-mod").click();
    await expect(page).toHaveURL(/\/merge-mod$/);

    await expect(page.getByTestId("merge-mod-package-id")).toHaveText(
      "rimmerge.merge.3f9a1c2b7d5e",
    );
    await expect(page.getByTestId(`merge-mod-entry-${bionicHeartKey}`)).toContainText("merged");
    await expect(page.getByTestId(`merge-mod-entry-${headNormalKey}`)).toContainText(
      "2 fields need input",
    );

    await expect(page.getByTestId("merge-mod-file-About/About.xml")).toBeVisible();
    await expect(page.getByTestId("xml-preview")).toContainText("rimmerge.merge.3f9a1c2b7d5e");
  });

  test("apply dialog's merge checkbox is checked by default once a merge is complete, and shows the returned merge mod path", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "BionicHeart");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    await page.getByTestId("shell-apply-button").click();
    await expect(page.getByTestId("apply-dialog")).toBeVisible();
    // Starts unchecked even with a complete merge entry: the user opts in.
    await expect(page.getByTestId("apply-dialog-write-merge-mod-checkbox")).toHaveAttribute(
      "data-p-checked",
      "false",
    );
    await page.getByTestId("apply-dialog-write-merge-mod-checkbox").click();
    await expect(page.getByTestId("apply-dialog-write-merge-mod-checkbox")).toHaveAttribute(
      "data-p-checked",
      "true",
    );

    // The scenario carries unanswered hard problems, so a write asks first.
    await page.getByTestId("apply-dialog-submit").click();
    await page.getByTestId("apply-confirm-apply-anyway").click();

    await expect(page.getByTestId("apply-dialog-merge-mod-path")).toContainText(
      "rimmerge_merge_3f9a1c2b7d5e",
    );

    const calls = await page.evaluate(() => window.__APPLY_CALLS__ ?? []);
    expect(calls).toEqual([
      { source: "suggested", writeModsConfig: true, writeMergeMod: true, force: false },
    ]);
  });
});

// The structural
// guard, surfaced on the merge editor and the apply dialog.
test.describe("structural guard", () => {
  test("shows one banner naming the triggering field and confirms the winner instead of 'N fields need input'", async ({
    page,
  }) => {
    await loadScenario(page);
    await openFindingInInbox(page, "GuardedGadget");

    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);

    const banner = page.getByTestId("merge-header-structural-guard");
    await expect(banner).toBeVisible();
    await expect(banner).toContainText("thingClass");
    await expect(banner).toContainText("Gadget Guard");
    // Points at accepting the finding from the
    // inbox, never at a "below" this fully-auto-resolved def (zero
    // Conflict rows) has nothing left in.
    await expect(banner).toContainText("Findings page");

    // The pill reads the same "confirm the winner" wording the apply
    // dialog's own skip line uses (`utils/format.ts`'s shared
    // `STRUCTURAL_GUARD_TEXT`) — never a field count, which would
    // contradict the totals line right above it (2 fields, 0 conflicts).
    await expect(page.getByTestId("merge-header-state")).toHaveText(
      "not auto-merged — confirm the winner",
    );
    await expect(page.getByTestId("merge-header-totals")).toContainText("2 fields");
    await expect(page.getByTestId("merge-header-totals")).toContainText("0 conflicts");
  });
});

test.describe("apply summary counts", () => {
  test("the apply dialog's summary line counts patch-collision and def-override merges separately", async ({
    page,
  }) => {
    await loadScenario(page);

    // A def-override merge, decided explicitly — an undecided def override
    // never reaches the merge mod
    // on its own, so `BionicHeart` must be decided via `m` to count.
    await openFindingInInbox(page, "BionicHeart");
    await page.keyboard.press("m");
    await expect(page).toHaveURL(/\/merge\//);
    await page.keyboard.press("Escape");
    await expect(page).toHaveURL(/\/inbox$/);

    // A patch-collision merge — `TemperateForest` is already `status:
    // "auto"`/`effective: merge` in the fixture, deliberately left
    // undecided here to prove the summary's own "auto-suggested where
    // undecided" half: mirroring RimWorld's own sequential composition,
    // it enters the merge mod without any `m`/decide at all.
    await page.getByTestId("shell-apply-button").click();
    await expect(page.getByTestId("apply-dialog")).toBeVisible();
    await expect(page.getByTestId("apply-dialog-merge-mod-groups")).toHaveText(
      "Will write 1 patch-collision merge (auto-suggested where undecided) and " +
        "1 def-override merge (explicitly decided).",
    );
  });
});
