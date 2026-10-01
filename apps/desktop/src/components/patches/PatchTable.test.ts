import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import PatchTable from "@/components/patches/PatchTable.vue";
import type { PatchSummaryDto } from "@/types/generated/PatchSummaryDto";

function patch(overrides: Partial<PatchSummaryDto> = {}): PatchSummaryDto {
  return {
    id: "abc123def456",
    name: "Wall compat",
    packageId: "author.wallcompat",
    displayName: "Wall Compatibility Patch",
    scope: [
      { modId: "a.mod", name: "Mod A" },
      { modId: "b.mod", name: "Mod B" },
    ],
    decisionCount: 1,
    completeCount: 1,
    needsInputCount: 0,
    exportDir: null,
    updatedAt: "2026-09-05T00:00:00Z",
    ...overrides,
  };
}

describe("PatchTable", () => {
  it("shows the empty state when there are no patches", () => {
    const wrapper = mount(PatchTable, { props: { patches: [], activeIndex: -1 } });
    expect(wrapper.find('[data-testid="empty-state"]').exists()).toBe(true);
  });

  it("renders a row per patch with its name, package id, scope, and counts", () => {
    const wrapper = mount(PatchTable, {
      props: {
        patches: [patch(), patch({ id: "other", needsInputCount: 3, completeCount: 0 })],
        activeIndex: -1,
      },
    });

    const row = wrapper.get('[data-testid="patch-row-abc123def456"]');
    expect(row.text()).toContain("Wall compat");
    expect(row.text()).toContain("author.wallcompat");
    expect(row.text()).toContain("Mod A");
    expect(row.text()).toContain("Mod B");

    expect(wrapper.find('[data-testid="patch-row-other"]').text()).toContain("3");
  });

  it("shows the export directory, or 'Not exported' when never exported", () => {
    const wrapper = mount(PatchTable, {
      props: {
        patches: [
          patch({ id: "exported", exportDir: "C:/exports/wallcompat" }),
          patch({ id: "never", exportDir: null }),
        ],
        activeIndex: -1,
      },
    });

    expect(wrapper.get('[data-testid="patch-row-exported"]').text()).toContain(
      "C:/exports/wallcompat",
    );
    expect(wrapper.get('[data-testid="patch-row-never"]').text()).toContain("Not exported");
  });

  it("emits open when a row is clicked", async () => {
    const wrapper = mount(PatchTable, { props: { patches: [patch()], activeIndex: -1 } });
    const row = wrapper.get('[data-testid="patch-row-abc123def456"]');

    await row.trigger("click");
    expect(wrapper.emitted("open")).toEqual([["abc123def456"]]);
  });

  it("emits focusRow when a row receives DOM focus, and never open — Enter navigation is the page's own job, so the row must not double-fire it", async () => {
    const wrapper = mount(PatchTable, { props: { patches: [patch()], activeIndex: -1 } });
    const row = wrapper.get('[data-testid="patch-row-abc123def456"]');

    await row.trigger("focus");
    expect(wrapper.emitted("focusRow")).toEqual([[0]]);

    await row.trigger("keydown.enter");
    expect(wrapper.emitted("open")).toBeUndefined();
  });

  it("renders aria-current and the accent background only on the active row", () => {
    const wrapper = mount(PatchTable, {
      props: {
        patches: [patch(), patch({ id: "other" })],
        activeIndex: 1,
      },
    });

    const activeRow = wrapper.get('[data-testid="patch-row-other"]');
    expect(activeRow.attributes("aria-current")).toBe("true");
    expect(activeRow.classes()).toContain("bg-accent-soft");

    const inactiveRow = wrapper.get('[data-testid="patch-row-abc123def456"]');
    expect(inactiveRow.attributes("aria-current")).toBe("false");
    expect(inactiveRow.classes()).not.toContain("bg-accent-soft");
  });
});
