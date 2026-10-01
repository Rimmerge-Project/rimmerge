import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";
import { createMemoryHistory, createRouter } from "vue-router";

import PatchListPage from "@/pages/PatchListPage.vue";
import { installMockIpc } from "@/services/ipc.mock";
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

async function mountPatchListPage() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: "/patches", name: "patches", component: { template: "<div />" } },
      { path: "/patches/:patchId", name: "patch-detail", component: { template: "<div />" } },
    ],
  });
  await router.push({ name: "patches" });

  return mount(PatchListPage, {
    // Attached to the real document — the double-fire regression this
    // file's own "Enter" test guards against only reproduces when a
    // keydown dispatched on a focused row actually bubbles up to
    // `window`, which a detached (the default) render tree never does.
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, router, [PrimeVue, { theme: { preset: Aura } }]],
    },
  });
}

async function flush(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("PatchListPage", () => {
  afterEach(() => {
    clearMocks();
    document.body.innerHTML = "";
  });

  it("renders aria-current on the cursor row and moves it with j/k", async () => {
    installMockIpc({ list_patches: () => [patch(), patch({ id: "other" })] });
    const wrapper = await mountPatchListPage();
    await flush();
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="patch-row-abc123def456"]').attributes("aria-current")).toBe(
      "true",
    );

    document.body.dispatchEvent(
      new KeyboardEvent("keydown", { key: "j", cancelable: true, bubbles: true }),
    );
    await wrapper.vm.$nextTick();

    expect(wrapper.get('[data-testid="patch-row-other"]').attributes("aria-current")).toBe("true");
    expect(wrapper.get('[data-testid="patch-row-abc123def456"]').attributes("aria-current")).toBe(
      "false",
    );

    wrapper.unmount();
  });

  it("opens the cursor's patch exactly once on Enter, even when the row itself has native DOM focus", async () => {
    installMockIpc({ list_patches: () => [patch()] });
    const wrapper = await mountPatchListPage();
    await flush();
    await wrapper.vm.$nextTick();

    const router = wrapper.vm.$router;
    const pushSpy = router.push;
    const calls: unknown[] = [];
    router.push = ((...args: unknown[]) => {
      calls.push(args[0]);
      return pushSpy.apply(router, args as never);
    }) as typeof router.push;

    // A real browser focuses a clicked `tabindex="0"` row; simulated here
    // via a direct `.focus()` call so the row is the actual
    // `document.activeElement` — the condition the double-fire bug
    // (the row's own `@keydown.enter` firing *in addition to* the page's
    // `window` listener, since the event also bubbles there) needed.
    const row = wrapper.get('[data-testid="patch-row-abc123def456"]')
      .element as HTMLTableRowElement;
    row.focus();

    row.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", cancelable: true, bubbles: true }),
    );
    await wrapper.vm.$nextTick();

    expect(calls).toHaveLength(1);
    expect(calls[0]).toMatchObject({ name: "patch-detail", params: { patchId: "abc123def456" } });

    wrapper.unmount();
  });

  it("leaves Enter on a focused button to native activation instead of navigating", async () => {
    installMockIpc({ list_patches: () => [patch()] });
    const wrapper = await mountPatchListPage();
    await flush();
    await wrapper.vm.$nextTick();

    const router = wrapper.vm.$router;
    const pushSpy = router.push;
    const calls: unknown[] = [];
    router.push = ((...args: unknown[]) => {
      calls.push(args[0]);
      return pushSpy.apply(router, args as never);
    }) as typeof router.push;

    const newPatchButton = wrapper.get('[data-testid="new-patch-button"]')
      .element as HTMLButtonElement;
    newPatchButton.focus();

    newPatchButton.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Enter", cancelable: true, bubbles: true }),
    );
    await wrapper.vm.$nextTick();

    expect(calls).toHaveLength(0);

    wrapper.unmount();
  });
});
