import { expect, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * The patch maker's mock-IPC spec: create -> confirm schema -> set one row -> coverage flips to
 * "this project" -> export shows the mocked `skipped` row. Uses the
 * scenario's own `fixture.framework`/`fixture.parts`/`fixture.target`
 * assignment fixture (mirrors `crates/rim-io/tests/fixtures/assign_game`),
 * asserting the exact IPC payloads `e2e/fixtures/scenario.ts` records —
 * never presence alone, per `apps/desktop/CLAUDE.md`.
 */
test.describe("assignments (patch maker)", () => {
  test("create -> confirm schema -> set a row -> coverage shows it -> export", async ({ page }) => {
    await loadScenario(page);

    await page.getByTestId("nav-assignments").click();
    await expect(page).toHaveURL(/\/assignments$/);
    await expect(page.getByTestId("assignments-empty")).toBeVisible();

    await page.getByTestId("new-assignment-button").click();
    await expect(page).toHaveURL(/\/assignments\/new$/);

    await page.getByTestId("assignment-name-input").fill("Example race patch");
    await page.getByTestId("assignment-package-id-input").fill("mypatch.parts");
    await page.getByTestId("assignment-display-name-input").fill("Sample Part Patch");

    await page.getByTestId("refs-search").fill("fixture.framework");
    await expect(page.getByTestId("refs-add-fixture.framework")).toBeVisible();
    await page.getByTestId("refs-add-fixture.framework").click();

    await page.getByTestId("targets-search").fill("fixture.target");
    await expect(page.getByTestId("targets-add-fixture.target")).toBeVisible();
    await page.getByTestId("targets-add-fixture.target").click();

    await page.getByTestId("propose-button").click();
    await expect(page.getByTestId("candidates-section")).toBeVisible();
    await expect(page.getByTestId("candidate-radio-example.PartAssignmentDef")).toBeVisible();
    await page.getByTestId("candidate-radio-example.PartAssignmentDef").click();
    await expect(page.getByTestId("schema-row-speciesNames")).toBeVisible();
    await expect(page.getByTestId("schema-row-parts")).toBeVisible();

    await page.getByTestId("assignment-create-button").click();
    await expect(page).toHaveURL(/\/assignments\/[0-9a-f]{12}$/);

    const createCalls = await page.evaluate(() => window.__CREATE_ASSIGNMENT_CALLS__ ?? []);
    expect(createCalls).toHaveLength(1);
    expect(createCalls[0]).toMatchObject({
      name: "Example race patch",
      packageId: "mypatch.parts",
      displayName: "Sample Part Patch",
      refs: ["fixture.framework"],
      excludedRefs: [],
      targets: ["fixture.target"],
    });

    await expect(page.getByTestId("coverage-row-Race0")).toBeVisible();
    await expect(page.getByTestId("coverage-cell-Race0")).toHaveText("uncovered");

    await page.getByTestId("coverage-row-Race0").click();
    await expect(page.getByTestId("row-editor")).toBeVisible();
    await page
      .getByTestId("item-picker-checkbox-example.PartDef-Part0")
      .waitFor({ state: "visible" });
    await page.getByTestId("item-picker-checkbox-example.PartDef-Part0").check();

    // `enabled` (`ScalarKind::Bool`) is a real toggle, not
    // free text — starts on the schema's own default ("true") and flips
    // to the literal text `"false"` RimWorld's XML deserializer expects.
    await expect(page.getByTestId("row-editor-scalar-enabled")).toBeChecked();
    await page.getByTestId("row-editor-scalar-enabled").click();
    await expect(page.getByTestId("row-editor-scalar-enabled")).not.toBeChecked();

    await page.getByTestId("row-editor-save-button").click();

    const rowCalls = await page.evaluate(() => window.__SET_ASSIGNMENT_ROW_CALLS__ ?? []);
    expect(rowCalls).toHaveLength(1);
    expect(rowCalls[0]?.target).toEqual({
      keyField: "speciesNames",
      def: { defType: "ThingDef", defName: "Race0" },
    });
    expect(rowCalls[0]?.row.values).toMatchObject({
      parts: { kind: "names", names: ["Part0"] },
      enabled: { kind: "text", text: "false" },
    });

    // The coverage list refetches after the mutation invalidates every
    // query — the cell flips from "uncovered" to "this project".
    await expect(page.getByTestId("coverage-cell-Race0")).toHaveText("this project");

    await page.getByTestId("assignment-export-dir-input").fill("C:/exports/example");
    await page.getByTestId("assignment-export-button").click();
    await expect(page.getByTestId("assignment-export-last-path")).toBeVisible();

    const exportCalls = await page.evaluate(() => window.__EXPORT_ASSIGNMENT_CALLS__ ?? []);
    expect(exportCalls).toEqual([
      {
        assignmentId: rowCalls[0]?.assignmentId,
        outDir: "C:/exports/example",
        install: false,
        force: false,
      },
    ]);
  });

  test("export shows every skipped field", async ({ page }) => {
    await loadScenario(page);
    await page.evaluate(() => {
      window.__ASSIGNMENT_EXPORT_FORCE_SKIP__ = true;
    });

    await page.getByTestId("nav-assignments").click();
    await page.getByTestId("new-assignment-button").click();
    await page.getByTestId("assignment-name-input").fill("Example race patch");
    await page.getByTestId("assignment-package-id-input").fill("mypatch.parts2");
    await page.getByTestId("assignment-display-name-input").fill("Sample Part Patch 2");
    await page.getByTestId("refs-search").fill("fixture.framework");
    await page.getByTestId("refs-add-fixture.framework").click();
    await page.getByTestId("targets-search").fill("fixture.target");
    await page.getByTestId("targets-add-fixture.target").click();
    await page.getByTestId("propose-button").click();
    await expect(page.getByTestId("candidates-section")).toBeVisible();
    await page.getByTestId("candidate-radio-example.PartAssignmentDef").click();
    await expect(page.getByTestId("schema-row-speciesNames")).toBeVisible();
    await page.getByTestId("assignment-create-button").click();
    await expect(page).toHaveURL(/\/assignments\/[0-9a-f]{12}$/);

    await page.getByTestId("coverage-row-Race0").click();
    await page
      .getByTestId("item-picker-checkbox-example.PartDef-Part0")
      .waitFor({ state: "visible" });
    await page.getByTestId("item-picker-checkbox-example.PartDef-Part0").check();
    await page.getByTestId("row-editor-save-button").click();
    await expect(page.getByTestId("coverage-cell-Race0")).toHaveText("this project");

    await page.getByTestId("assignment-export-dir-input").fill("C:/exports/example2");
    await page.getByTestId("assignment-export-button").click();

    const skipped = page.getByTestId("assignment-export-skipped");
    await expect(skipped).toBeVisible();
    await expect(skipped).toContainText("ThingDef/Race0");
    await expect(skipped).toContainText("no longer exist in the active list");
  });

  test("a candidate with no target-key field can be created with an empty target set", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("nav-assignments").click();
    await page.getByTestId("new-assignment-button").click();
    await page.getByTestId("assignment-name-input").fill("New Part");
    await page.getByTestId("assignment-package-id-input").fill("sample.newpart");
    await page.getByTestId("assignment-display-name-input").fill("Sample's New Part");

    await page.getByTestId("refs-search").fill("fixture.parts");
    await expect(page.getByTestId("refs-add-fixture.parts")).toBeVisible();
    await page.getByTestId("refs-add-fixture.parts").click();
    // Targets (T) is deliberately left empty — the whole point of this
    // "good to have" extension: a candidate with no TargetKey field is
    // only offered when T is empty.

    await page.getByTestId("propose-button").click();
    await expect(page.getByTestId("candidates-section")).toBeVisible();
    await expect(page.getByTestId("candidate-radio-example.PartDef")).toBeVisible();
    await page.getByTestId("candidate-radio-example.PartDef").click();

    await expect(page.getByTestId("schema-row-effect")).toBeVisible();
    await expect(page.getByTestId("standalone-note")).toBeVisible();

    await page.getByTestId("assignment-create-button").click();
    await expect(page).toHaveURL(/\/assignments\/[0-9a-f]{12}$/);

    const createCalls = await page.evaluate(() => window.__CREATE_ASSIGNMENT_CALLS__ ?? []);
    expect(createCalls.at(-1)).toMatchObject({
      name: "New Part",
      packageId: "sample.newpart",
      refs: ["fixture.parts"],
      targets: [],
    });
  });

  /**
   * A free-standing section's own rows are creatable and editable
   * after creation — this exercises the full round trip through the real
   * UI (not the direct mock-function calls the contract test below uses
   * for its own, narrower purpose).
   */
  test("free-standing rows: 'New row' creates one, selecting an existing row edits it, and 'Clear row' removes it", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("nav-assignments").click();
    await page.getByTestId("new-assignment-button").click();
    await page.getByTestId("assignment-name-input").fill("New Part Rows");
    await page.getByTestId("assignment-package-id-input").fill("sample.newpartrows");
    await page.getByTestId("assignment-display-name-input").fill("Sample's New Part Rows");
    await page.getByTestId("refs-search").fill("fixture.parts");
    await page.getByTestId("refs-add-fixture.parts").click();
    await page.getByTestId("propose-button").click();
    await expect(page.getByTestId("candidates-section")).toBeVisible();
    await page.getByTestId("candidate-radio-example.PartDef").click();
    await expect(page.getByTestId("schema-row-effect")).toBeVisible();
    await page.getByTestId("assignment-create-button").click();
    await expect(page).toHaveURL(/\/assignments\/[0-9a-f]{12}$/);

    await expect(page.getByTestId("standalone-section-empty")).toBeVisible();
    await expect(page.getByTestId("row-editor")).toHaveCount(0);

    await page.getByTestId("standalone-new-row-button").click();
    await expect(page.getByTestId("row-editor")).toContainText("New row");
    await page.getByTestId("row-editor-def-name-input").fill("sample_newpartrows_Alpha");
    await page.getByTestId("row-editor-scalar-effect").fill("A shiny new part");
    await page.getByTestId("row-editor-save-button").click();

    const setCalls = await page.evaluate(() => window.__SET_ASSIGNMENT_ROW_CALLS__ ?? []);
    expect(setCalls).toHaveLength(1);
    expect(setCalls[0]).toMatchObject({
      section: "example.PartDef",
      target: null,
      row: {
        defName: "sample_newpartrows_Alpha",
        values: { effect: { kind: "text", text: "A shiny new part" } },
      },
    });
    const savedRow = page.getByTestId("standalone-row-sample_newpartrows_Alpha");
    await expect(savedRow).toBeVisible();
    await expect(savedRow).toHaveAttribute("aria-current", "true");

    // Re-selecting it from the list (not the "just saved" state above)
    // proves an *existing* row loads correctly, not only a freshly-saved
    // one still sitting in the draft.
    await page.getByTestId("standalone-new-row-button").click();
    await expect(page.getByTestId("row-editor-def-name-input")).toHaveValue("");
    await savedRow.click();
    await expect(page.getByTestId("row-editor-def-name-input")).toHaveValue(
      "sample_newpartrows_Alpha",
    );
    await expect(page.getByTestId("row-editor-scalar-effect")).toHaveValue("A shiny new part");

    await page.getByTestId("row-editor-clear-button").click();
    await expect(page.getByTestId("standalone-row-sample_newpartrows_Alpha")).toHaveCount(0);
    await expect(page.getByTestId("standalone-section-empty")).toBeVisible();
    await expect(page.getByTestId("row-editor")).toHaveCount(0);
  });

  test("multi-section: create -> add section -> set a row referencing an own instance -> export shows per-section skips", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("nav-assignments").click();
    await page.getByTestId("new-assignment-button").click();

    await page.getByTestId("assignment-name-input").fill("Example multi-section patch");
    await page.getByTestId("assignment-package-id-input").fill("sample.multisection");
    await page.getByTestId("assignment-display-name-input").fill("Sample's Multi-Section Patch");

    // Both fixture mods as refs — the created project's own reference set
    // must already own `example.PartDef` for "Add section" to ever offer
    // it later (`list_assignment_candidates` only offers a def type owned
    // by a member of the project's own effective refs).
    await page.getByTestId("refs-search").fill("fixture.framework");
    await page.getByTestId("refs-add-fixture.framework").click();
    await page.getByTestId("refs-search").fill("fixture.parts");
    await page.getByTestId("refs-add-fixture.parts").click();

    await page.getByTestId("targets-search").fill("fixture.target");
    await page.getByTestId("targets-add-fixture.target").click();

    await page.getByTestId("propose-button").click();
    await expect(page.getByTestId("candidates-section")).toBeVisible();
    await page.getByTestId("candidate-radio-example.PartAssignmentDef").click();
    await expect(page.getByTestId("schema-row-speciesNames")).toBeVisible();

    await page.getByTestId("assignment-create-button").click();
    await expect(page).toHaveURL(/\/assignments\/[0-9a-f]{12}$/);
    await expect(page.getByTestId("section-tab-example.PartAssignmentDef")).toBeVisible();

    // Add a free-standing `example.PartDef` section.
    await page.getByTestId("open-add-section-button").click();
    await expect(page.getByTestId("add-section-candidate-example.PartDef")).toBeVisible();
    // Already a section on this project — never offered again.
    await expect(page.getByTestId("add-section-candidate-example.PartAssignmentDef")).toHaveCount(
      0,
    );
    await page.getByTestId("add-section-candidate-example.PartDef").click();
    await page.getByTestId("add-section-confirm").click();
    await expect(page.getByTestId("add-section-dialog")).toBeHidden();

    const addSectionCalls = await page.evaluate(
      () => window.__ADD_ASSIGNMENT_SECTION_CALLS__ ?? [],
    );
    expect(addSectionCalls).toHaveLength(1);
    expect(addSectionCalls[0]).toMatchObject({ defType: "example.PartDef" });
    await expect(page.getByTestId("section-tab-example.PartDef")).toBeVisible();

    // Back on the target-keyed section, reference the new free-standing
    // section's own row through its item picker's "This project" group.
    await page.getByTestId("section-tab-example.PartAssignmentDef").click();
    await expect(page.getByTestId("coverage-row-Race0")).toBeVisible();
    await page.getByTestId("coverage-row-Race0").click();
    await expect(page.getByTestId("item-picker-own-heading-example.PartDef")).toBeVisible();
    const ownCheckbox = page.getByTestId(
      "item-picker-checkbox-example.PartDef-sample_multisection_OwnPart",
    );
    await ownCheckbox.waitFor({ state: "visible" });
    await ownCheckbox.check();
    await page.getByTestId("row-editor-save-button").click();

    const rowCalls = await page.evaluate(() => window.__SET_ASSIGNMENT_ROW_CALLS__ ?? []);
    expect(rowCalls).toHaveLength(1);
    expect(rowCalls[0]?.section).toBe("example.PartAssignmentDef");
    expect(rowCalls[0]?.row.values).toMatchObject({
      parts: { kind: "names", names: ["sample_multisection_OwnPart"] },
    });

    await page.evaluate(() => {
      window.__ASSIGNMENT_EXPORT_FORCE_SKIP__ = true;
    });
    await page.getByTestId("assignment-export-dir-input").fill("C:/exports/multi");
    await page.getByTestId("assignment-export-button").click();

    const skipped = page.getByTestId("assignment-export-skipped");
    await expect(skipped).toBeVisible();
    await expect(skipped).toContainText("example.PartAssignmentDef");
    await expect(skipped).toContainText("ThingDef/Race0");
    await expect(skipped).toContainText("example.PartDef");
    await expect(skipped).toContainText("sample_multisection_OwnPart");
  });

  /**
   * Two mock contracts: `list_assignment_items` applies
   * `filter.search` to a project's own free-standing rows the way
   * `rim_session::use_cases::own_items` does, and `set_assignment_row`/
   * `clear_assignment_row` refuse a target-keyed key into a
   * free-standing section (or vice versa) with the domain's own
   * `RowKeyMismatch` refusal. Both are exercised by calling the mock's
   * own fixture functions directly (the same `window.__E2E_MOCK_IPC__`
   * pattern `dashboard.spec.ts`'s "ranked by count" test already uses).
   * The "free-standing rows" spec above exercises `RowKeyMismatch`
   * through the real UI, but the real UI never *constructs* a mismatched
   * request (it always threads the section's own kind through correctly),
   * so a genuinely malformed payload — the shape this test's own two
   * `mismatched*` cases send — has no click-path that produces it; only
   * the mock's own contract under direct test can cover that
   * malformed-input case at all.
   */
  test("mock list_assignment_items/set_assignment_row/clear_assignment_row match the real backend's own contract", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("nav-assignments").click();
    await page.getByTestId("new-assignment-button").click();
    await page.getByTestId("assignment-name-input").fill("Mock contract check");
    await page.getByTestId("assignment-package-id-input").fill("sample.mockcontract");
    await page.getByTestId("assignment-display-name-input").fill("Sample's Mock Contract Check");
    await page.getByTestId("refs-search").fill("fixture.framework");
    await page.getByTestId("refs-add-fixture.framework").click();
    await page.getByTestId("refs-search").fill("fixture.parts");
    await page.getByTestId("refs-add-fixture.parts").click();
    await page.getByTestId("targets-search").fill("fixture.target");
    await page.getByTestId("targets-add-fixture.target").click();
    await page.getByTestId("propose-button").click();
    await expect(page.getByTestId("candidates-section")).toBeVisible();
    await page.getByTestId("candidate-radio-example.PartAssignmentDef").click();
    await expect(page.getByTestId("schema-row-speciesNames")).toBeVisible();
    await page.getByTestId("assignment-create-button").click();
    await expect(page).toHaveURL(/\/assignments\/[0-9a-f]{12}$/);

    // Add the free-standing `example.PartDef` section — the mock seeds one
    // real own row (`sample_mockcontract_OwnPart`) the moment it's added.
    await page.getByTestId("open-add-section-button").click();
    await page.getByTestId("add-section-candidate-example.PartDef").click();
    await page.getByTestId("add-section-confirm").click();
    await expect(page.getByTestId("add-section-dialog")).toBeHidden();

    const assignmentId = new URL(page.url()).pathname.split("/").pop();
    expect(assignmentId).toMatch(/^[0-9a-f]{12}$/);

    // `filter.search` must apply to the project's own free-standing rows
    // too, not only the active/external list.
    const names = await page.evaluate(
      async ({ id, search }) => {
        const listItems = window.__E2E_MOCK_IPC__?.["list_assignment_items"];
        if (typeof listItems !== "function") {
          throw new Error("list_assignment_items fixture missing or not a function");
        }
        const result = (await listItems({
          request: {
            defType: "example.PartDef",
            filter: { search, offset: 0, limit: 20 },
            assignmentId: id,
          },
        })) as { items: { def: { defName: string } }[] };
        return result.items.map((item) => item.def.defName);
      },
      { id: assignmentId, search: "Part0" },
    );
    expect(names).toContain("Part0");
    expect(names).not.toContain("sample_mockcontract_OwnPart");

    // A target-keyed row into the free-standing section is refused.
    const mismatchedTarget = await page.evaluate(async (id) => {
      const setRow = window.__E2E_MOCK_IPC__?.["set_assignment_row"];
      if (typeof setRow !== "function") {
        throw new Error("set_assignment_row fixture missing or not a function");
      }
      try {
        await setRow({
          request: {
            assignmentId: id,
            section: "example.PartDef",
            target: { keyField: "speciesNames", def: { defType: "ThingDef", defName: "Race0" } },
            row: { values: {}, defName: "Bogus", note: null },
          },
        });
        return null;
      } catch (error) {
        return error;
      }
    }, assignmentId);
    expect(mismatchedTarget).toMatchObject({
      code: "invalid_input",
      message: expect.stringContaining("does not match section"),
    });

    // A free-standing (no-target) row into the target-keyed section is
    // refused the same way.
    const mismatchedOwn = await page.evaluate(async (id) => {
      const setRow = window.__E2E_MOCK_IPC__?.["set_assignment_row"];
      if (typeof setRow !== "function") {
        throw new Error("set_assignment_row fixture missing or not a function");
      }
      try {
        await setRow({
          request: {
            assignmentId: id,
            section: "example.PartAssignmentDef",
            target: null,
            row: { values: {}, defName: "Bogus", note: null },
          },
        });
        return null;
      } catch (error) {
        return error;
      }
    }, assignmentId);
    expect(mismatchedOwn).toMatchObject({
      code: "invalid_input",
      message: expect.stringContaining("does not match section"),
    });

    // `clear_assignment_row` enforces the identical refusal.
    const clearMismatch = await page.evaluate(async (id) => {
      const clearRow = window.__E2E_MOCK_IPC__?.["clear_assignment_row"];
      if (typeof clearRow !== "function") {
        throw new Error("clear_assignment_row fixture missing or not a function");
      }
      try {
        await clearRow({
          request: {
            assignmentId: id,
            section: "example.PartDef",
            target: { keyField: "speciesNames", def: { defType: "ThingDef", defName: "Race0" } },
            defName: null,
          },
        });
        return null;
      } catch (error) {
        return error;
      }
    }, assignmentId);
    expect(clearMismatch).toMatchObject({ code: "invalid_input" });
  });

  test("removing a still-referenced section shows the referencing rows, then 'remove anyway' forces it", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("nav-assignments").click();
    await page.getByTestId("new-assignment-button").click();
    await page.getByTestId("assignment-name-input").fill("Example section removal");
    await page.getByTestId("assignment-package-id-input").fill("sample.removal");
    await page.getByTestId("assignment-display-name-input").fill("Sample's Removal Patch");
    await page.getByTestId("refs-search").fill("fixture.framework");
    await page.getByTestId("refs-add-fixture.framework").click();
    await page.getByTestId("refs-search").fill("fixture.parts");
    await page.getByTestId("refs-add-fixture.parts").click();
    await page.getByTestId("targets-search").fill("fixture.target");
    await page.getByTestId("targets-add-fixture.target").click();
    await page.getByTestId("propose-button").click();
    await page.getByTestId("candidate-radio-example.PartAssignmentDef").click();
    await expect(page.getByTestId("schema-row-speciesNames")).toBeVisible();
    await page.getByTestId("assignment-create-button").click();
    await expect(page).toHaveURL(/\/assignments\/[0-9a-f]{12}$/);

    await page.getByTestId("open-add-section-button").click();
    await page.getByTestId("add-section-candidate-example.PartDef").click();
    await page.getByTestId("add-section-confirm").click();
    await expect(page.getByTestId("add-section-dialog")).toBeHidden();

    // Reference the free-standing section's own row before trying to
    // remove it, so the removal actually hits the in-use refusal.
    await page.getByTestId("section-tab-example.PartAssignmentDef").click();
    await page.getByTestId("coverage-row-Race0").click();
    await page
      .getByTestId("item-picker-checkbox-example.PartDef-sample_removal_OwnPart")
      .waitFor({ state: "visible" });
    await page.getByTestId("item-picker-checkbox-example.PartDef-sample_removal_OwnPart").check();
    await page.getByTestId("row-editor-save-button").click();
    await expect(page.getByTestId("coverage-cell-Race0")).toHaveText("this project");

    await page.getByTestId("section-tab-example.PartDef").click();
    await page.getByTestId("open-remove-section-button").click();
    await page.getByTestId("remove-section-confirm").click();

    await expect(page.getByTestId("remove-section-in-use")).toBeVisible();
    await expect(page.getByTestId("remove-section-reference-0")).toContainText(
      "example.PartAssignmentDef",
    );

    await page.getByTestId("remove-section-force").click();
    await expect(page.getByTestId("remove-section-dialog")).toBeHidden();
    await expect(page.getByTestId("section-tab-example.PartDef")).toHaveCount(0);

    const removeCalls = await page.evaluate(() => window.__REMOVE_ASSIGNMENT_SECTION_CALLS__ ?? []);
    expect(removeCalls).toEqual([
      { assignmentId: removeCalls[0]?.assignmentId, defType: "example.PartDef", force: false },
      { assignmentId: removeCalls[0]?.assignmentId, defType: "example.PartDef", force: true },
    ]);
  });

  /**
   * `EditReferencesTargetsDialog.vue` is `useUpdateAssignmentMutation`'s
   * UI caller — this exercises the real R/T-edit round trip end to end
   * (opening the dialog shows the project's own current R/T, adding a ref
   * records the exact `update_assignment` payload, and the "what changed"
   * disclosure renders afterward).
   */
  test("editing references records update_assignment and shows the what-changed disclosure", async ({
    page,
  }) => {
    await loadScenario(page);

    await page.getByTestId("nav-assignments").click();
    await page.getByTestId("new-assignment-button").click();
    await page.getByTestId("assignment-name-input").fill("Example race patch");
    await page.getByTestId("assignment-package-id-input").fill("sample.editrt");
    await page.getByTestId("assignment-display-name-input").fill("Sample's Edit RT Patch");
    await page.getByTestId("refs-search").fill("fixture.framework");
    await page.getByTestId("refs-add-fixture.framework").click();
    await page.getByTestId("targets-search").fill("fixture.target");
    await page.getByTestId("targets-add-fixture.target").click();
    await page.getByTestId("propose-button").click();
    await expect(page.getByTestId("candidates-section")).toBeVisible();
    await page.getByTestId("candidate-radio-example.PartAssignmentDef").click();
    await expect(page.getByTestId("schema-row-speciesNames")).toBeVisible();
    await page.getByTestId("assignment-create-button").click();
    await expect(page).toHaveURL(/\/assignments\/[0-9a-f]{12}$/);

    await page.getByTestId("open-edit-refs-targets-button").click();
    await expect(page.getByTestId("edit-refs-targets-dialog")).toBeVisible();
    await expect(page.getByTestId("edit-refs-member-fixture.framework")).toBeVisible();
    await expect(page.getByTestId("edit-targets-member-fixture.target")).toBeVisible();

    await page.getByTestId("edit-refs-search").fill("fixture.parts");
    await expect(page.getByTestId("edit-refs-add-fixture.parts")).toBeVisible();
    await page.getByTestId("edit-refs-add-fixture.parts").click();
    await page.getByTestId("edit-refs-targets-save").click();

    await expect(page.getByTestId("edit-refs-targets-outcome")).toBeVisible();
    await expect(page.getByTestId("schema-change-example.PartAssignmentDef")).toBeVisible();

    const updateCalls = await page.evaluate(() => window.__UPDATE_ASSIGNMENT_CALLS__ ?? []);
    expect(updateCalls.at(-1)).toMatchObject({
      refs: ["fixture.framework", "fixture.parts"],
      excludedRefs: null,
      targets: ["fixture.target"],
    });

    await page.getByTestId("edit-refs-targets-cancel").click();
    await expect(page.getByTestId("edit-refs-targets-dialog")).toBeHidden();
  });
});
