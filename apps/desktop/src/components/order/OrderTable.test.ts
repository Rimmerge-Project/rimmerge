import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import OrderTable from "@/components/order/OrderTable.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { OrderRowDto } from "@/types/generated/OrderRowDto";

/**
 * `useModLabel` (via `OrderTable`) needs Pinia + Pinia Colada, and
 * `list_mod_names` needs *some* registered fixture — an unregistered
 * command's rejection reaches Vitest as an unhandled rejection that
 * fails the whole run, not just this file (see the identical note in
 * `InboxPage.test.ts`). No name resolves either way, which is fine:
 * every assertion below is on a testid or an emitted id, never on the
 * rendered name text.
 */
function mountTable(rows: OrderRowDto[]) {
  installMockIpc({ list_mod_names: {} });
  return mount(OrderTable, {
    props: { rows },
    global: { plugins: [createPinia(), PiniaColada] },
  });
}

function buildRows(count: number): OrderRowDto[] {
  return Array.from({ length: count }, (_, index) => ({
    modId: `mod.${index}`,
    name: `Mod ${index}`,
    position: index,
    previousPosition: index === 5 ? 50 : null,
    tier: "body",
    tags: [],
    hardDependents: 0,
    needsInputCount: index === 3 ? 2 : 0,
  }));
}

describe("OrderTable", () => {
  // `@tanstack/vue-virtual` measures the scroll container via
  // `offsetWidth`/`offsetHeight`, which happy-dom always reports as `0`
  // with no layout engine behind it — stubbing a real-looking viewport
  // height is what makes the virtualizer compute a bounded visible range
  // instead of the "no size, no visible items" default.
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
      configurable: true,
      value: 400,
    });
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
      configurable: true,
      value: 600,
    });
  });

  afterEach(() => {
    clearMocks();
    Reflect.deleteProperty(HTMLElement.prototype, "offsetHeight");
    Reflect.deleteProperty(HTMLElement.prototype, "offsetWidth");
  });

  it("renders far fewer DOM rows than the full 1000-row dataset", async () => {
    const wrapper = mountTable(buildRows(1000));
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    const renderedRows = wrapper.findAll('[role="listitem"][data-testid^="order-row-"]');
    expect(renderedRows.length).toBeGreaterThan(0);
    expect(renderedRows.length).toBeLessThan(100);
  });

  it("shows a moved-from marker only for a row whose position changed", async () => {
    const wrapper = mountTable(buildRows(20));
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    expect(wrapper.find('[data-testid="order-row-moved-mod.5"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="order-row-moved-mod.0"]').exists()).toBe(false);
  });

  it("emits selectRow with the clicked row's modId", async () => {
    const wrapper = mountTable(buildRows(20));
    await wrapper.vm.$nextTick();
    await wrapper.vm.$nextTick();

    await wrapper.get('[data-testid="order-row-mod.0"]').trigger("click");

    expect(wrapper.emitted("selectRow")).toEqual([["mod.0"]]);
  });
});
