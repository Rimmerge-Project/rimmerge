import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { createGroup, expectNothingMoved, node, openMods } from "./organization-support";

/**
 * Copy groups and colours from another profile (#53): pick the other profile's folder, read the
 * preview for Merge or Replace, confirm a Replace with Cancel focused, read the result. The mock
 * mirrors the backend's merge rule, clashes and refusals, and organization never moves a mod, so
 * every spec ends with `expectNothingMoved`.
 */

const OTHER = "C:/Profiles/other";
const OWN = "C:/Profile/rimmerge";

type SourceGroup = {
  id: number;
  name: string;
  parent: number | null;
  color: string | null;
  members: string[];
};

/** Registers the other profiles' folders the mock can read (before the page opens). */
async function withSources(page: Page, sources: Record<string, unknown>): Promise<void> {
  await page.addInitScript((value) => {
    window.__COPY_PROFILE_SOURCES__ = value as NonNullable<typeof window.__COPY_PROFILE_SOURCES__>;
  }, sources);
}

function organization(groups: SourceGroup[], modColors: Record<string, string> = {}) {
  return { kind: "organization", groups, modColors };
}

async function pick(page: Page, folder: string | null): Promise<void> {
  await page.evaluate((value) => {
    window.__PICK_FOLDER_RESULT__ = value;
  }, folder);
}

async function openCopyDialog(page: Page, folder: string): Promise<void> {
  await pick(page, folder);
  await page.getByTestId("group-copy-profile").click();
  await expect(page.getByTestId("copy-profile-dialog")).toBeVisible();
  await page.getByTestId("copy-choose").click();
}

async function copyCalls(page: Page): Promise<{ command: string; request: unknown }[]> {
  return page.evaluate(() =>
    (window.__ORGANIZATION_CALLS__ ?? []).filter((call) => call.command.includes("copy")),
  );
}

function groupsNamed(count: number): SourceGroup[] {
  return Array.from({ length: count }, (_, index) => ({
    id: index + 1,
    name: `group ${index}`,
    parent: null,
    color: null,
    members: [],
  }));
}

const TARGET_SOURCE = organization(
  [
    { id: 1, name: "weapons", parent: null, color: "red", members: ["mod.003", "gone.mod"] },
    { id: 2, name: "Melee", parent: 1, color: null, members: ["mod.004"] },
    { id: 3, name: "Armor", parent: null, color: "teal", members: [] },
  ],
  { "mod.003": "pink" },
);

test.describe("copy groups from another profile", () => {
  test("merges into a group with the same path, keeps this profile's colours and lists the result", async ({
    page,
  }) => {
    await withSources(page, { [OTHER]: TARGET_SOURCE });
    const before = await openMods(page);
    await createGroup(page, "Weapons");

    await openCopyDialog(page, OTHER);

    const preview = page.getByTestId("copy-preview");
    await expect(preview).toContainText("2 groups added");
    await expect(preview).toContainText(
      "1 group merged into the group here with the same name in the same place",
    );
    await expect(preview).toContainText("1 mod in these groups isn't installed");
    await page.getByTestId("copy-submit").click();
    await expect(page.getByTestId("copy-result")).toContainText("Merged the other profile's");
    await page.getByTestId("copy-close").click();

    await expect(node(page, "Weapons")).toBeVisible();
    await expect(node(page, "Melee")).toBeVisible();
    await expect(node(page, "Armor")).toBeVisible();
    expect(await copyCalls(page)).toEqual([
      { command: "preview_copy_organization", request: { sourceDir: OTHER, mode: "merge" } },
      { command: "copy_organization", request: { sourceDir: OTHER, mode: "merge" } },
    ]);
    await expectNothingMoved(page, before);
  });

  test("a colour set on both sides stays this profile's and is listed as a clash", async ({
    page,
  }) => {
    await withSources(page, { [OTHER]: TARGET_SOURCE });
    await openMods(page);
    await createGroup(page, "Weapons");
    // Give this profile's Weapons its own colour first.
    await node(page, "Weapons").click({ button: "right" });
    await page.getByTestId("group-menu-color").click();
    await page.getByTestId("color-option-blue").check();
    await page.getByTestId("color-apply").click();
    await expect(page.getByTestId("color-dialog")).toBeHidden();

    await openCopyDialog(page, OTHER);

    await expect(page.getByTestId("copy-clash")).toHaveText(
      "Weapons: yours is Blue, theirs is Red",
    );
  });

  test("a replace is confirmed with Cancel focused, and Escape leaves everything as it was", async ({
    page,
  }) => {
    await withSources(page, { [OTHER]: TARGET_SOURCE });
    const before = await openMods(page);
    await createGroup(page, "Mine");

    await openCopyDialog(page, OTHER);
    await page.getByTestId("copy-mode-replace").focus();
    await page.keyboard.press("Space");
    await expect(page.getByTestId("copy-submit")).toHaveText("Replace…");
    await expect(page.getByTestId("copy-preview")).toContainText("1 group");
    await page.getByTestId("copy-submit").press("Enter");

    await expect(page.getByTestId("copy-confirm")).toContainText("This cannot be undone.");
    await expect(page.getByTestId("copy-confirm-cancel")).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("copy-profile-dialog")).toBeHidden();

    await expect(node(page, "Mine")).toBeVisible();
    await expect(node(page, "Armor")).toHaveCount(0);
    expect((await copyCalls(page)).map((call) => call.command)).not.toContain("copy_organization");
    await expectNothingMoved(page, before);
  });

  test("a confirmed replace swaps the whole organization for the other profile's", async ({
    page,
  }) => {
    await withSources(page, { [OTHER]: TARGET_SOURCE });
    const before = await openMods(page);
    await createGroup(page, "Mine");

    await openCopyDialog(page, OTHER);
    await page.getByTestId("copy-mode-replace").focus();
    await page.keyboard.press("Space");
    await expect(page.getByTestId("copy-submit")).toHaveText("Replace…");
    await page.getByTestId("copy-submit").press("Enter");
    await page.keyboard.press("Tab");
    await expect(page.getByTestId("copy-confirm-replace")).toBeFocused();
    await page.keyboard.press("Enter");

    await expect(page.getByTestId("copy-result")).toContainText("Replaced your groups");
    await page.getByTestId("copy-close").click();
    await expect(node(page, "Armor")).toBeVisible();
    await expect(node(page, "Mine")).toHaveCount(0);
    expect(await copyCalls(page)).toEqual([
      { command: "preview_copy_organization", request: { sourceDir: OTHER, mode: "merge" } },
      { command: "preview_copy_organization", request: { sourceDir: OTHER, mode: "replace" } },
      { command: "copy_organization", request: { sourceDir: OTHER, mode: "replace" } },
    ]);
    await expectNothingMoved(page, before);
  });

  test("refuses this profile's own folder, a folder without a file and a damaged file, changing nothing", async ({
    page,
  }) => {
    await withSources(page, {
      [OWN]: organization([]),
      "C:/Profiles/empty": { kind: "missing" },
      "C:/Profiles/damaged": { kind: "unusable", issue: { kind: "malformed" } },
    });
    const before = await openMods(page);
    await createGroup(page, "Mine");

    await openCopyDialog(page, OWN);
    await expect(page.getByTestId("copy-refusal")).toContainText("own folder");
    await expect(page.getByTestId("copy-submit")).toBeDisabled();

    await pick(page, "C:/Profiles/empty");
    await page.getByTestId("copy-choose").click();
    await expect(page.getByTestId("copy-refusal")).toContainText("no groups and colours file");

    await pick(page, "C:/Profiles/damaged");
    await page.getByTestId("copy-choose").click();
    await expect(page.getByTestId("copy-refusal")).toContainText("is damaged");

    await pick(page, "C:/Profiles/nowhere");
    await page.getByTestId("copy-choose").click();
    await expect(page.getByTestId("copy-refusal")).toContainText("not a folder");
    await page.getByTestId("copy-cancel").click();

    await expect(node(page, "Mine")).toBeVisible();
    expect((await copyCalls(page)).map((call) => call.command)).toEqual([
      "preview_copy_organization",
      "preview_copy_organization",
      "preview_copy_organization",
      "preview_copy_organization",
    ]);
    await expectNothingMoved(page, before);
  });

  test("a copy that would pass the group limit only when combined is refused as a limit, in Merge only", async ({
    page,
  }) => {
    // Valid on its own (exactly the limit); one more group here makes a Merge overflow.
    await withSources(page, { [OTHER]: organization(groupsNamed(1000)) });
    const before = await openMods(page);
    await createGroup(page, "Mine");

    await openCopyDialog(page, OTHER);
    const refusal = page.getByTestId("copy-refusal");
    await expect(refusal).toContainText("The result would have more groups than Rimmerge allows.");
    await expect(refusal).not.toContainText("don't fit together");
    await expect(page.getByTestId("copy-submit")).toBeDisabled();

    // A Replace drops "Mine", so the same source fits.
    await page.getByTestId("copy-mode-replace").check();
    await expect(page.getByTestId("copy-discardGroups")).toHaveText("1 group");
    await expect(page.getByTestId("copy-refusal")).toHaveCount(0);
    await page.getByTestId("copy-cancel").click();

    await expect(node(page, "Mine")).toBeVisible();
    expect((await copyCalls(page)).map((call) => call.command)).not.toContain("copy_organization");
    await expectNothingMoved(page, before);
  });

  test("a source that breaks a rule on its own is unusable, whatever this profile holds", async ({
    page,
  }) => {
    const tooMany = "C:/Profiles/too-many";
    const sameName = "C:/Profiles/same-name";
    await withSources(page, {
      [tooMany]: organization(groupsNamed(1001)),
      // Siblings that differ only in case are one name to the app.
      [sameName]: organization([
        { id: 1, name: "Gear", parent: null, color: null, members: [] },
        { id: 2, name: "gear", parent: null, color: null, members: [] },
      ]),
    });
    const before = await openMods(page);
    await createGroup(page, "Mine");

    await openCopyDialog(page, tooMany);
    const refusal = page.getByTestId("copy-refusal");
    await expect(refusal).toContainText("has groups that don't fit together");
    await expect(refusal).not.toContainText("The result would");
    await expect(page.getByTestId("copy-submit")).toBeDisabled();

    await pick(page, sameName);
    await page.getByTestId("copy-choose").click();
    await expect(refusal).toContainText("has groups that don't fit together");
    await expect(page.getByTestId("copy-preview")).toHaveCount(0);
    await page.getByTestId("copy-mode-replace").check();
    await expect(refusal).toContainText("has groups that don't fit together");
    await page.getByTestId("copy-cancel").click();

    await expect(node(page, "Mine")).toBeVisible();
    expect((await copyCalls(page)).map((call) => call.command)).not.toContain("copy_organization");
    await expectNothingMoved(page, before);
  });

  test("a folder holding exactly this profile's groups and colours has nothing to copy", async ({
    page,
  }) => {
    await withSources(page, {
      [OTHER]: organization([{ id: 1, name: "Mine", parent: null, color: null, members: [] }]),
    });
    const before = await openMods(page);
    await createGroup(page, "Mine");

    await openCopyDialog(page, OTHER);

    await expect(page.getByTestId("copy-refusal")).toContainText("there is nothing to copy");
    await expect(page.getByTestId("copy-submit")).toBeDisabled();
    await page.getByTestId("copy-cancel").click();
    await expectNothingMoved(page, before);
  });

  test("a network share is refused by name and a drive letter still works", async ({ page }) => {
    await withSources(page, { [OTHER]: TARGET_SOURCE });
    const before = await openMods(page);

    await openCopyDialog(page, "\\\\server\\share\\profiles");
    await expect(page.getByTestId("organization-error")).toContainText(
      "Network folders aren't supported",
    );
    await expect(page.getByTestId("copy-submit")).toBeDisabled();

    await pick(page, "\\\\server@SSL\\DavWWWRoot\\profiles");
    await page.getByTestId("copy-choose").click();
    await expect(page.getByTestId("organization-error")).toContainText(
      "Network folders aren't supported",
    );

    await pick(page, "\\\\.\\pipe\\profiles");
    await page.getByTestId("copy-choose").click();
    await expect(page.getByTestId("organization-error")).toContainText("Invalid input");

    await pick(page, OTHER);
    await page.getByTestId("copy-choose").click();
    await expect(page.getByTestId("copy-preview")).toBeVisible();
    await expect(page.getByTestId("organization-error")).toHaveCount(0);
    await page.getByTestId("copy-cancel").click();
    await expectNothingMoved(page, before);
  });

  test("the folder picker opens beside this profile's own folder", async ({ page }) => {
    await withSources(page, { [OTHER]: TARGET_SOURCE });
    await openMods(page);

    await openCopyDialog(page, OTHER);

    const options = await page.evaluate(() => window.__PICK_FOLDER_OPTIONS__ ?? []);
    expect(options).toEqual([
      expect.objectContaining({ directory: true, defaultPath: "C:/Profile" }),
    ]);
  });

  test("cancelling the folder picker leaves the dialog on its first step", async ({ page }) => {
    await withSources(page, { [OTHER]: TARGET_SOURCE });
    const before = await openMods(page);

    await openCopyDialog(page, OTHER);
    await expect(page.getByTestId("copy-preview")).toBeVisible();
    await page.getByTestId("copy-cancel").click();
    await page.getByTestId("group-copy-profile").click();
    await pick(page, null);
    await page.getByTestId("copy-choose").click();

    await expect(page.getByTestId("copy-folder")).toHaveCount(0);
    await expect(page.getByTestId("copy-submit")).toBeDisabled();
    await expectNothingMoved(page, before, { editsOrganization: true });
  });
});
