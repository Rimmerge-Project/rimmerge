import { expect, type Page, test } from "@playwright/test";
// Type-only: pulls in `Window.__E2E_MOCK_IPC__`'s ambient declaration.
import type {} from "../../src/e2e-mock-bootstrap";
import type { ModListDto } from "../../src/types/generated/ModListDto";
import { loadScenario } from "./support";

/**
 * Mock-fidelity checks for the reorder commands (`move_mods`, an anchored `activate_mods`,
 * `discard_pending_changes`, `revert_order_to_file`) and `reorderable`: they call the mock
 * handlers directly and pin the semantics `rim_session` has (the settle rule in both directions,
 * the selection switch, stale anchors), so a UI spec built on them cannot pass against behaviour
 * the backend does not have.
 */

type Outcome<T> = { ok: true; value: T } | { ok: false; code: string };

async function call<T>(page: Page, command: string, args: unknown): Promise<Outcome<T>> {
  return page.evaluate(
    async ([name, payload]) => {
      try {
        const value = await window.__TAURI_INTERNALS__.invoke(
          name as string,
          payload as Record<string, unknown>,
        );
        return { ok: true as const, value: value as never };
      } catch (error) {
        return { ok: false as const, code: (error as { code: string }).code };
      }
    },
    [command, args] as const,
  );
}

function expectOk<T>(outcome: Outcome<T>): T {
  if (!outcome.ok) throw new Error(`expected success, got ${outcome.code}`);
  return outcome.value;
}

const NO_QUERY = { search: null, tag: null, source: null, sort: null };

async function activeIds(page: Page): Promise<string[]> {
  const list = expectOk(await call<ModListDto>(page, "list_working_mods", { query: NO_QUERY }));
  return list.items.map((row) => row.modId);
}

async function fileMatchesCurrent(page: Page): Promise<boolean> {
  const dashboard = expectOk(
    await call<{ fileMatchesCurrent: boolean }>(page, "get_dashboard", {}),
  );
  return dashboard.fileMatchesCurrent;
}

async function currentIds(page: Page): Promise<string[]> {
  const rows = expectOk(await call<{ modId: string }[]>(page, "list_order", { source: "current" }));
  return rows.map((row) => row.modId);
}

/**
 * The order `ModsConfig.xml` holds in the mock, read through the order export (which reads the
 * file, never Current): the ids in the `N. Name [id]` lines.
 */
async function fileIds(page: Page): Promise<string[]> {
  const exported = expectOk(await call<{ text: string }>(page, "export_order_text", {}));
  return [...exported.text.matchAll(/\[([^\]]+)\]$/gm)].map((match) => match[1] ?? "");
}

/**
 * A mod the mock's Current holds. The mock's Current lists only the fixture's own mods (the
 * working set also holds fixture-only owners and missing mods), so a move must pick from it.
 */
async function currentMover(page: Page): Promise<string> {
  const ids = await currentIds(page);
  return ids.at(-1) as string;
}

test.describe("reorder mocks mirror the backend", () => {
  test.beforeEach(async ({ page }) => {
    await loadScenario(page);
  });

  test("only the Active list in load order is reorderable, and a filter keeps it on", async ({
    page,
  }) => {
    const query = (over: object) => ({ query: { ...NO_QUERY, ...over } });

    const plain = expectOk(await call<ModListDto>(page, "list_working_mods", query({})));
    const searching = expectOk(
      await call<ModListDto>(page, "list_working_mods", query({ search: "a" })),
    );
    const byName = expectOk(
      await call<ModListDto>(
        page,
        "list_working_mods",
        query({ sort: { key: "name", direction: "ascending" } }),
      ),
    );
    const inactive = expectOk(await call<ModListDto>(page, "list_inactive_mods", query({})));

    expect(plain.reorderable).toBe(true);
    expect(searching.reorderable).toBe(true);
    expect(byName.reorderable).toBe(false);
    expect(inactive.reorderable).toBe(false);
  });

  test("a move over unchanged membership settles into Current and leaves the file alone", async ({
    page,
  }) => {
    const before = await activeIds(page);
    const last = await currentMover(page);

    expectOk(
      await call(page, "move_mods", { request: { ids: [last], anchor: { kind: "start" } } }),
    );

    const after = await activeIds(page);
    expect(after).toEqual([last, ...before.filter((id) => id !== last)]);
    // Current follows (the mock's Current lists only the fixture's own mods).
    expect((await currentIds(page))[0]).toBe(last);
    expect(await fileMatchesCurrent(page)).toBe(false);
    const pending = expectOk(
      await call<{
        unscanned: { added: string[]; removed: string[] };
        unapplied: { added: string[]; removed: string[] };
        orderDiffersFromFile: boolean;
      }>(page, "get_pending_active_changes", {}),
    );
    expect(pending.unscanned).toEqual({ added: [], removed: [] });
    // The membership diffs cannot see a reorder; only this field does (the close guard reads it).
    expect(pending.unapplied).toEqual({ added: [], removed: [] });
    expect(pending.orderDiffersFromFile).toBe(true);
  });

  test("a stale anchor, an inactive mover and an anchor inside the moved set are refused", async ({
    page,
  }) => {
    const [first, second] = await activeIds(page);

    const stale = await call(page, "move_mods", {
      request: { ids: [first], anchor: { kind: "after", modId: "no.such.mod" } },
    });
    const inactive = await call(page, "move_mods", {
      request: { ids: ["no.such.mod"], anchor: { kind: "start" } },
    });
    const inside = await call(page, "move_mods", {
      request: { ids: [first, second], anchor: { kind: "before", modId: second } },
    });

    expect(stale).toEqual({ ok: false, code: "move_anchor_not_active" });
    expect(inactive).toEqual({ ok: false, code: "invalid_input" });
    expect(inside).toEqual({ ok: false, code: "invalid_input" });
  });

  test("the first reorder while Suggested is selected switches to Current", async ({ page }) => {
    const ids = await activeIds(page);
    const mover = await currentMover(page);
    await call(page, "select_order", { source: "suggested" });

    const first = expectOk(
      await call<{ selected: string; didSwitchToCurrent: boolean }>(page, "move_mods", {
        request: { ids: [mover], anchor: { kind: "start" } },
      }),
    );
    const second = expectOk(
      await call<{ selected: string; didSwitchToCurrent: boolean }>(page, "move_mods", {
        request: { ids: [ids.at(-2)], anchor: { kind: "start" } },
      }),
    );

    expect(first).toEqual({ selected: "current", didSwitchToCurrent: true });
    expect(second).toEqual({ selected: "current", didSwitchToCurrent: false });
  });

  test("an anchored activation while Suggested is selected switches to Current; an un-anchored one does not", async ({
    page,
  }) => {
    type ActivateResult = {
      pending: { unscanned: { added: string[]; removed: string[] } };
      selected: string;
      didSwitchToCurrent: boolean;
    };
    const inactive = expectOk(
      await call<ModListDto>(page, "list_inactive_mods", { query: NO_QUERY }),
    );
    const extra = inactive.items[0]?.modId as string;
    await call(page, "select_order", { source: "suggested" });

    const plain = expectOk(
      await call<ActivateResult>(page, "activate_mods", {
        request: { ids: [extra], withDependencies: false, anchor: null },
      }),
    );
    expectOk(await call(page, "deactivate_mods", { request: { ids: [extra] } }));
    const anchored = expectOk(
      await call<ActivateResult>(page, "activate_mods", {
        request: { ids: [extra], withDependencies: false, anchor: { kind: "start" } },
      }),
    );
    const again = expectOk(
      await call<ActivateResult>(page, "activate_mods", {
        request: { ids: [extra], withDependencies: false, anchor: { kind: "start" } },
      }),
    );

    expect(plain.selected).toBe("suggested");
    expect(plain.didSwitchToCurrent).toBe(false);
    expect(plain.pending.unscanned.added).toEqual([extra]);
    expect(anchored.selected).toBe("current");
    expect(anchored.didSwitchToCurrent).toBe(true);
    expect(again.didSwitchToCurrent).toBe(false);
  });

  test("a move with a pending activation stays out of Current until a rescan carries it", async ({
    page,
  }) => {
    const inactive = expectOk(
      await call<ModListDto>(page, "list_inactive_mods", { query: NO_QUERY }),
    );
    const extra = inactive.items[0]?.modId as string;
    const mover = await currentMover(page);
    const currentBefore = await currentIds(page);
    expectOk(
      await call(page, "activate_mods", {
        request: { ids: [extra], withDependencies: false, anchor: null },
      }),
    );

    expectOk(
      await call(page, "move_mods", {
        request: { ids: [mover], anchor: { kind: "start" } },
      }),
    );

    expect(await currentIds(page)).toEqual(currentBefore);
    expect((await activeIds(page))[0]).toBe(mover);
    expectOk(await call(page, "rescan_project", {}));
    expect((await currentIds(page))[0]).toBe(mover);
  });

  test("an anchored activation lands at the anchor; an un-anchored one returns to its Current slot", async ({
    page,
  }) => {
    const ids = await activeIds(page);
    const held = await currentIds(page);
    const middle = held[Math.floor(held.length / 2)] as string;
    expectOk(await call(page, "deactivate_mods", { request: { ids: [middle] } }));

    expectOk(
      await call(page, "activate_mods", {
        request: { ids: [middle], withDependencies: false, anchor: null },
      }),
    );

    expect(await activeIds(page)).toEqual(ids);
    expect(await fileMatchesCurrent(page)).toBe(true);

    expectOk(await call(page, "deactivate_mods", { request: { ids: [middle] } }));
    expectOk(
      await call(page, "activate_mods", {
        request: { ids: [middle], withDependencies: false, anchor: { kind: "start" } },
      }),
    );
    expect((await activeIds(page))[0]).toBe(middle);
    expect(await fileMatchesCurrent(page)).toBe(false);
  });

  test("a stale anchor refuses an activation at plan time and at commit", async ({ page }) => {
    const inactive = expectOk(
      await call<ModListDto>(page, "list_inactive_mods", { query: NO_QUERY }),
    );
    const request = {
      ids: [inactive.items[0]?.modId],
      withDependencies: false,
      anchor: { kind: "before", modId: "no.such.mod" },
    };

    expect(await call(page, "plan_activate_mods", { request })).toEqual({
      ok: false,
      code: "move_anchor_not_active",
    });
    expect(await call(page, "activate_mods", { request })).toEqual({
      ok: false,
      code: "move_anchor_not_active",
    });
  });

  test("revert restores the file's order, and is refused with pending membership changes", async ({
    page,
  }) => {
    const ids = await activeIds(page);
    const mover = await currentMover(page);
    expectOk(
      await call(page, "move_mods", { request: { ids: [mover], anchor: { kind: "start" } } }),
    );
    expect(await fileMatchesCurrent(page)).toBe(false);

    expectOk(await call(page, "revert_order_to_file", {}));

    expect(await activeIds(page)).toEqual(ids);
    expect(await fileMatchesCurrent(page)).toBe(true);

    expectOk(await call(page, "deactivate_mods", { request: { ids: [mover] } }));
    expect(await call(page, "revert_order_to_file", {})).toEqual({
      ok: false,
      code: "revert_order_pending_changes",
    });
  });

  test("discard drops pending activations without a rescan but keeps a settled reorder", async ({
    page,
  }) => {
    const ids = await activeIds(page);
    const mover = await currentMover(page);
    expectOk(
      await call(page, "move_mods", { request: { ids: [mover], anchor: { kind: "start" } } }),
    );
    expectOk(await call(page, "deactivate_mods", { request: { ids: [ids[1]] } }));

    expectOk(await call(page, "discard_pending_changes", {}));

    const after = await activeIds(page);
    expect(after).toHaveLength(ids.length);
    expect(after[0]).toBe(mover);
    expect(await page.evaluate(() => window.__RESCAN_CALLS__ ?? 0)).toBe(0);
  });

  test("applying after a move writes the moved order", async ({ page }) => {
    const mover = await currentMover(page);
    expectOk(
      await call(page, "move_mods", { request: { ids: [mover], anchor: { kind: "start" } } }),
    );
    expect(await fileMatchesCurrent(page)).toBe(false);
    const fileBefore = await fileIds(page);
    expect(fileBefore[0]).not.toBe(mover);

    expectOk(
      await call(page, "apply", {
        request: {
          source: "current",
          writeModsConfig: true,
          writeMergeMod: false,
          force: false,
        },
      }),
    );

    expect(await fileMatchesCurrent(page)).toBe(true);
    expect((await activeIds(page))[0]).toBe(mover);
    // The file now holds exactly Current's sequence, not merely a list that starts with the mover.
    const written = await fileIds(page);
    expect(written).toEqual((await currentIds(page)).map((id) => id.replace(/_steam$/, "")));
    expect(written).not.toEqual(fileBefore);
  });
});
