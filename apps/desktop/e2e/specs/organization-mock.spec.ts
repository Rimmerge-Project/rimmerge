import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import { loadScenario } from "./support";

/**
 * These specs drive the mock's organization commands directly (the UI is covered by
 * `mods-groups.spec.ts`) and pin the mock to the backend's rules: cycle refusal,
 * base-id keys, descendant scope with dedupe, one outcome per action, nothing else touched.
 */

type Result = { ok: unknown } | { error: { code: string; message: string } };

/** Invokes one mock IPC command and returns its result or the thrown `{ code, message }`. */
async function call(page: Page, command: string, args?: unknown): Promise<Result> {
  return page.evaluate(
    async ({ command, args }) => {
      try {
        return { ok: await window.__TAURI_INTERNALS__.invoke(command, args) };
      } catch (error) {
        return { error: error as { code: string; message: string } };
      }
    },
    { command, args },
  );
}

async function ok<T>(page: Page, command: string, args?: unknown): Promise<T> {
  const result = await call(page, command, args);
  if (!("ok" in result)) throw new Error(`${command} failed: ${JSON.stringify(result.error)}`);
  return result.ok as T;
}

async function codeOf(page: Page, command: string, args?: unknown): Promise<string> {
  const result = await call(page, command, args);
  if (!("error" in result)) throw new Error(`${command} unexpectedly succeeded`);
  return result.error.code;
}

type Row = {
  modId: string;
  position: number | null;
  groups: { id: number; name: string; color: string | null }[];
  color: string | null;
};
type RowList = { total: number; items: Row[]; reorderable: boolean };
type Created = { kind: "created"; id: number; members: number };
/** `OrganizationDto`: one tagged variant per state, each with only its own fields. */
type Organization =
  | {
      kind: "loaded";
      groups: {
        id: number;
        name: string;
        depth: number;
        directCount: number;
        scopeCount: number;
      }[];
      coloredModCount: number;
    }
  | { kind: "notLoaded" }
  | { kind: "recovered"; issue: { kind: string } };
type Tree = Extract<Organization, { kind: "loaded" }>;

/** The loaded variant, or a failed assertion naming the state it was in instead. */
async function loadedTree(page: Page): Promise<Tree> {
  const organization = await ok<Organization>(page, "get_organization");
  expect(organization.kind).toBe("loaded");
  return organization as Tree;
}

const NO_QUERY = { query: { search: null, tag: null, source: null, sort: null } };

async function activeRows(page: Page, group?: unknown): Promise<RowList> {
  return ok(page, "list_working_mods", { query: { ...NO_QUERY.query, group } });
}

async function inactiveRows(page: Page, group?: unknown): Promise<RowList> {
  return ok(page, "list_inactive_mods", { query: { ...NO_QUERY.query, group } });
}

async function create(page: Page, name: string, parent: number | null = null): Promise<number> {
  const created = await ok<Created>(page, "create_group", { request: { name, parent } });
  return created.id;
}

test.describe("organization (mock)", () => {
  test("creates nested groups, refuses a cycle and a duplicate sibling, and never half-applies", async ({
    page,
  }) => {
    await loadScenario(page);
    const top = await create(page, "Weapons");
    const kid = await create(page, "Blades", top);
    const grandkid = await create(page, "Swords", kid);

    expect(await codeOf(page, "reparent_group", { request: { id: top, parent: grandkid } })).toBe(
      "group_cycle",
    );
    expect(await codeOf(page, "reparent_group", { request: { id: top, parent: top } })).toBe(
      "group_cycle",
    );
    expect(await codeOf(page, "create_group", { request: { name: "weapons", parent: null } })).toBe(
      "group_name_taken",
    );
    expect(await codeOf(page, "create_group", { request: { name: "x", parent: 999 } })).toBe(
      "group_not_found",
    );
    expect(await codeOf(page, "create_group", { request: { name: "   ", parent: null } })).toBe(
      "invalid_input",
    );

    const tree = await loadedTree(page);
    expect(tree.groups.map((group) => [group.name, group.depth])).toEqual([
      ["Weapons", 1],
      ["Blades", 2],
      ["Swords", 3],
    ]);
  });

  test("nests at most eight levels deep", async ({ page }) => {
    await loadScenario(page);
    let parent: number | null = null;
    for (let level = 1; level <= 8; level += 1)
      parent = await create(page, `level ${level}`, parent);

    expect(await codeOf(page, "create_group", { request: { name: "level 9", parent } })).toBe(
      "organization_limit",
    );
  });

  test("a group scope includes descendants, counts each mod once, and keeps load order", async ({
    page,
  }) => {
    await loadScenario(page);
    const active = await activeRows(page);
    const [first, second, third] = active.items.map((row) => row.modId);
    expect(first && second && third).toBeTruthy();
    const parent = await create(page, "Parent");
    const kid = await create(page, "Kid", parent);
    await ok(page, "add_group_members", { request: { id: parent, modIds: [first, second] } });
    await ok(page, "add_group_members", { request: { id: kid, modIds: [second, third] } });

    const tree = await loadedTree(page);
    const parentNode = tree.groups.find((group) => group.id === parent);
    expect([parentNode?.directCount, parentNode?.scopeCount]).toEqual([2, 3]);

    const scoped = await activeRows(page, { kind: "inScope", groupId: parent });
    expect(scoped.items.map((row) => row.modId)).toEqual([first, second, third]);
    expect(scoped.total).toBe(3);
    expect(scoped.reorderable).toBe(true);
    expect(scoped.items.map((row) => row.position)).toEqual(
      active.items.map((row) => row.position).slice(0, 3),
    );
    const secondRow = scoped.items.find((row) => row.modId === second);
    expect(secondRow?.groups.map((label) => label.name)).toEqual(["Parent", "Kid"]);
  });

  test("a row's group labels follow the tree depth first, siblings by name, not creation order", async ({
    page,
  }) => {
    await loadScenario(page);
    const modId = (await activeRows(page)).items[0]?.modId ?? "";
    const zeta = await create(page, "Zeta");
    const alpha = await create(page, "Alpha");
    const child = await create(page, "Child", alpha);
    const beta = await create(page, "Beta");
    for (const id of [zeta, alpha, child, beta]) {
      await ok(page, "add_group_members", { request: { id, modIds: [modId] } });
    }

    const rows = await activeRows(page);

    const row = rows.items.find((candidate) => candidate.modId === modId);
    expect(row?.groups.map((label) => label.name)).toEqual(["Alpha", "Child", "Beta", "Zeta"]);
  });

  test("members are keyed by base id: a _steam copy and its base are one mod", async ({ page }) => {
    await loadScenario(page);
    const active = await activeRows(page);
    const modId = active.items[0]?.modId ?? "";
    const group = await create(page, "G");

    const added = await ok(page, "add_group_members", {
      request: { id: group, modIds: [modId, `${modId}_steam`] },
    });
    const again = await ok(page, "add_group_members", {
      request: { id: group, modIds: [`${modId}_steam`] },
    });

    expect(added).toEqual({ kind: "added", added: 1, already: 0 });
    expect(again).toEqual({ kind: "added", added: 0, already: 1 });
    const labelled = await activeRows(page, { kind: "inScope", groupId: group });
    expect(labelled.items.map((row) => row.modId)).toEqual([modId]);
  });

  test("ungrouped lists mods in no group on both lists, and a stale group id is an error", async ({
    page,
  }) => {
    await loadScenario(page);
    const active = await activeRows(page);
    const inactive = await inactiveRows(page);
    const group = await create(page, "G");
    await ok(page, "add_group_members", {
      request: { id: group, modIds: [active.items[0]?.modId, inactive.items[0]?.modId] },
    });

    const ungroupedActive = await activeRows(page, { kind: "ungrouped" });
    const ungroupedInactive = await inactiveRows(page, { kind: "ungrouped" });

    expect(ungroupedActive.total).toBe(active.total - 1);
    expect(ungroupedInactive.total).toBe(inactive.total - 1);
    expect(
      await codeOf(page, "list_working_mods", {
        query: { ...NO_QUERY.query, group: { kind: "inScope", groupId: 999 } },
      }),
    ).toBe("group_not_found");
  });

  test("move replaces only the source membership; add and remove report their counts", async ({
    page,
  }) => {
    await loadScenario(page);
    const [first, second] = (await activeRows(page)).items.map((row) => row.modId);
    const source = await create(page, "Source");
    const target = await create(page, "Target");
    const other = await create(page, "Other");
    await ok(page, "add_group_members", { request: { id: source, modIds: [first, second] } });
    await ok(page, "add_group_members", { request: { id: other, modIds: [first] } });

    const moved = await ok(page, "move_group_members", {
      request: { from: source, to: target, modIds: [first, "not.in.source"] },
    });
    const removed = await ok(page, "remove_group_members", {
      request: { id: source, modIds: [second, "never.there"] },
    });

    expect(moved).toEqual({ kind: "moved", moved: 1, alreadyInTarget: 0, notInSource: 1 });
    expect(removed).toEqual({ kind: "removed", removed: 1, absent: 1 });
    const row = (await activeRows(page)).items.find((candidate) => candidate.modId === first);
    expect(row?.groups.map((label) => label.name)).toEqual(["Other", "Target"]);
    expect(
      await codeOf(page, "move_group_members", {
        request: { from: source, to: source, modIds: [first] },
      }),
    ).toBe("invalid_input");
  });

  test("a mod colour is the mod's own, set and reset in bulk, never inherited from a group", async ({
    page,
  }) => {
    await loadScenario(page);
    const [first, second] = (await activeRows(page)).items.map((row) => row.modId);
    const group = await create(page, "Coloured");
    await ok(page, "set_group_color", { request: { id: group, color: "teal" } });
    await ok(page, "add_group_members", { request: { id: group, modIds: [first, second] } });

    const colored = await ok(page, "set_mod_colors", {
      request: { modIds: [first], color: "red" },
    });
    const rows = (await activeRows(page)).items;

    expect(colored).toEqual({ kind: "colored", changed: 1, unchanged: 0 });
    expect(rows.find((row) => row.modId === first)?.color).toBe("red");
    expect(rows.find((row) => row.modId === second)?.color).toBeNull();
    const reset = await ok(page, "set_mod_colors", { request: { modIds: [first], color: null } });
    expect(reset).toEqual({ kind: "colored", changed: 1, unchanged: 0 });
    expect((await activeRows(page)).items.find((row) => row.modId === first)?.color).toBeNull();
  });

  test("deleting a group keeps every mod in both lists; lifting keeps subgroups and their members", async ({
    page,
  }) => {
    await loadScenario(page);
    const before = [(await activeRows(page)).total, (await inactiveRows(page)).total];
    const top = await create(page, "Top");
    const middle = await create(page, "Middle", top);
    const leaf = await create(page, "Leaf", middle);
    const modId = (await activeRows(page)).items[0]?.modId ?? "";
    await ok(page, "add_group_members", { request: { id: leaf, modIds: [modId] } });

    const lifted = await ok(page, "delete_group", {
      request: { id: middle, mode: "liftChildren" },
    });

    expect(lifted).toEqual({ kind: "deleted", groupsRemoved: 1, childrenLifted: 1 });
    const tree = await loadedTree(page);
    expect(tree.groups.map((group) => [group.name, group.depth])).toEqual([
      ["Top", 1],
      ["Leaf", 2],
    ]);
    const all = await ok(page, "delete_group", { request: { id: top, mode: "withSubgroups" } });
    expect(all).toEqual({ kind: "deleted", groupsRemoved: 2, childrenLifted: 0 });
    expect([(await activeRows(page)).total, (await inactiveRows(page)).total]).toEqual(before);
  });

  test("plan_group_scope splits a scope into active, inactive and not-installed ids", async ({
    page,
  }) => {
    await loadScenario(page);
    const active = (await activeRows(page)).items[0]?.modId ?? "";
    const inactive = (await inactiveRows(page)).items[0]?.modId ?? "";
    const group = await create(page, "Plan");
    await ok(page, "add_group_members", {
      request: { id: group, modIds: [active, inactive, "gone.for.good"] },
    });

    const plan = await ok(page, "plan_group_scope", { request: { id: group } });

    expect(plan).toEqual({
      groupCount: 1,
      modCount: 3,
      activeIds: [active],
      inactiveIds: [inactive],
      notInstalled: 1,
      notInstalledIds: ["gone.for.good"],
    });
    expect(await codeOf(page, "plan_group_scope", { request: { id: 999 } })).toBe(
      "group_not_found",
    );
  });

  test("remove_uninstalled_members removes only the requested mods that are still not installed", async ({
    page,
  }) => {
    await loadScenario(page);
    const active = (await activeRows(page)).items[0]?.modId ?? "";
    const group = await create(page, "Wishlist");
    await ok(page, "add_group_members", {
      request: { id: group, modIds: [active, "gone.one", "gone.two"] },
    });

    // `active` is installed (so kept even though it is named) and `gone.two` was never requested.
    const outcome = await ok(page, "remove_uninstalled_members", {
      request: { id: group, modIds: [active, "gone.one"] },
    });
    const plan = await ok<{ notInstalledIds: string[]; modCount: number }>(
      page,
      "plan_group_scope",
      { request: { id: group } },
    );

    expect(outcome).toEqual({ kind: "removed", removed: 1, absent: 0 });
    expect(plan.notInstalledIds).toEqual(["gone.two"]);
    expect(plan.modCount).toBe(2);
  });

  test("organization actions leave the working set, the orders and the pending changes alone", async ({
    page,
  }) => {
    await loadScenario(page);
    const snapshot = async () => ({
      working: (await activeRows(page)).items.map((row) => row.modId),
      pending: await ok(page, "get_pending_active_changes"),
      order: await ok(page, "list_order", { source: "current" }),
    });
    const before = await snapshot();

    const group = await create(page, "Quiet");
    await ok(page, "add_group_members", { request: { id: group, modIds: [before.working[0]] } });
    await ok(page, "set_mod_colors", { request: { modIds: before.working, color: "blue" } });
    await ok(page, "delete_group", { request: { id: group, mode: "liftChildren" } });

    expect(await snapshot()).toEqual(before);
    const calls = await page.evaluate(() => ({
      move: window.__MOVE_MODS_CALLS__ ?? [],
      activate: window.__ACTIVATE_CALLS__ ?? [],
      deactivate: window.__DEACTIVATE_CALLS__ ?? [],
      apply: window.__APPLY_CALLS__ ?? [],
      organization: (window.__ORGANIZATION_CALLS__ ?? []).map((entry) => entry.command),
    }));
    expect([calls.move, calls.activate, calls.deactivate, calls.apply]).toEqual([[], [], [], []]);
    expect(calls.organization).toEqual([
      "create_group",
      "add_group_members",
      "set_mod_colors",
      "delete_group",
    ]);
  });

  test("groups survive a rescan", async ({ page }) => {
    await loadScenario(page);
    const group = await create(page, "Keeps");
    const modId = (await activeRows(page)).items[0]?.modId ?? "";
    await ok(page, "add_group_members", { request: { id: group, modIds: [modId] } });

    await ok(page, "rescan_project");

    const tree = await loadedTree(page);
    expect(tree.groups.map((node) => [node.name, node.directCount])).toEqual([["Keeps", 1]]);
    const row = (await activeRows(page)).items.find((candidate) => candidate.modId === modId);
    expect(row?.groups.map((label) => label.name)).toEqual(["Keeps"]);
  });

  test("a recovered organization is read-only until the user starts over", async ({ page }) => {
    await page.addInitScript(() => {
      window.__ORGANIZATION_STATE__ = "recovered";
    });
    await loadScenario(page);

    const state = await ok<Organization>(page, "get_organization");
    expect(state).toEqual({ kind: "recovered", issue: { kind: "malformed" } });
    expect(await codeOf(page, "create_group", { request: { name: "G", parent: null } })).toBe(
      "organization_unavailable",
    );
    expect(
      await codeOf(page, "list_working_mods", {
        query: { ...NO_QUERY.query, group: { kind: "ungrouped" } },
      }),
    ).toBe("organization_unavailable");
    expect((await activeRows(page)).items.every((row) => row.groups.length === 0)).toBe(true);

    await ok(page, "reset_organization");

    expect((await loadedTree(page)).groups).toEqual([]);
    expect(await create(page, "Fresh")).toBeGreaterThan(0);
  });

  test("a session without a store refuses edits and a reset", async ({ page }) => {
    await page.addInitScript(() => {
      window.__ORGANIZATION_STATE__ = "notLoaded";
    });
    await loadScenario(page);

    expect(await ok<Organization>(page, "get_organization")).toEqual({ kind: "notLoaded" });
    expect(await codeOf(page, "create_group", { request: { name: "G", parent: null } })).toBe(
      "organization_unavailable",
    );
    expect(await codeOf(page, "reset_organization")).toBe("invalid_input");
  });

  test("the total-memberships limit and the per-request id cap bind like the backend's", async ({
    page,
  }) => {
    await loadScenario(page);
    const ids = (from: number, count: number) =>
      Array.from({ length: count }, (_, index) => `limit.mod.${from + index}`);
    for (let index = 0; index < 10; index += 1) {
      const group = await create(page, `full ${index}`);
      await ok(page, "add_group_members", {
        request: { id: group, modIds: ids(index * 5_000, 5_000) },
      });
    }
    const eleventh = await create(page, "one too many");

    expect(
      await codeOf(page, "add_group_members", { request: { id: eleventh, modIds: ["extra.mod"] } }),
    ).toBe("organization_limit");
    expect(
      await codeOf(page, "add_group_members", {
        request: { id: eleventh, modIds: ids(0, 50_001) },
      }),
    ).toBe("invalid_input");
    const tree = await loadedTree(page);
    expect(tree.groups.find((node) => node.id === eleventh)?.directCount).toBe(0);
  });

  test("a grouped list request bounds its raw collapsed array like the command's DTO conversion", async ({
    page,
  }) => {
    await loadScenario(page);
    const request = (count: number) => ({
      query: {
        list: { ...NO_QUERY.query },
        // One repeated key: distinct keys stay at one, so only the raw length can trip the bound.
        collapsed: Array.from({ length: count }, () => ({ kind: "ungrouped" })),
      },
    });

    const atTheBound = await call(page, "list_working_mods_grouped", request(1_001));
    const pastTheBound = await codeOf(page, "list_working_mods_grouped", request(1_002));

    expect(atTheBound).toHaveProperty("ok");
    expect(pastTheBound).toBe("invalid_input");
  });

  test("a C1 control character in a name or an id is refused like the backend's", async ({
    page,
  }) => {
    await loadScenario(page);
    const group = await create(page, "Plain");

    expect(await codeOf(page, "create_group", { request: { name: "badname", parent: null } })).toBe(
      "invalid_input",
    );
    expect(
      await codeOf(page, "add_group_members", {
        request: { id: group, modIds: ["badid"] },
      }),
    ).toBe("invalid_input");
  });

  test("a colour outside the palette fails argument parsing with a bare string and records nothing", async ({
    page,
  }) => {
    await loadScenario(page);
    const group = await create(page, "Plain");

    const result = await call(page, "set_group_color", { request: { id: group, color: "mauve" } });

    expect("error" in result && typeof result.error === "string").toBe(true);
    const commands = await page.evaluate(() =>
      (window.__ORGANIZATION_CALLS__ ?? []).map((entry) => entry.command),
    );
    expect(commands).toEqual(["create_group"]);
  });
});
