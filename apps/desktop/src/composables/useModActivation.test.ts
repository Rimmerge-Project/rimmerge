import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";
import { createI18n } from "vue-i18n";

import { EMPTY_PENDING_ACTIVE_CHANGES } from "@/components/apply/ApplyDialog.test-support";
import { useModActivation } from "@/composables/useModActivation";
import { installMockIpc } from "@/services/ipc.mock";
import type { ActivateModsResultDto } from "@/types/generated/ActivateModsResultDto";

vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: vi.fn() }) }));

function activateResult(activated: string[], alreadyActive: string[]): ActivateModsResultDto {
  return {
    pending: EMPTY_PENDING_ACTIVE_CHANGES,
    activated,
    alreadyActive,
    selected: "current",
    didSwitchToCurrent: false,
  };
}

/** Mounts the composable over `selected` and returns its confirm flow, with the IPC answering `activated`. */
async function setup(selected: string[], activated: string[], alreadyActive: string[] = []) {
  installMockIpc({
    plan_activate_mods: { toAdd: activated, unresolvableDependencies: {}, alreadyActive },
    activate_mods: activateResult(activated, alreadyActive),
  });
  const selectedIds = ref(selected);
  let flow!: ReturnType<typeof useModActivation>;
  const Harness = defineComponent({
    setup() {
      flow = useModActivation({
        selectedIds,
        withDependencies: true,
        onSwitchedToCurrent: () => {},
      });
      return {};
    },
    template: "<div />",
  });
  mount(Harness, {
    global: {
      plugins: [
        createPinia(),
        PiniaColada,
        createI18n({ legacy: false, locale: "en", missingWarn: false, fallbackWarn: false }),
      ],
    },
  });
  await flushPromises();
  return { selectedIds, flow };
}

describe("useModActivation", () => {
  afterEach(() => {
    clearMocks();
  });

  it("drops a pulled-in dependency the filter hides from the selection (#47)", async () => {
    // "base" is selected but hidden by a filter; "needs" is the shown row the user activates.
    const { selectedIds, flow } = await setup(["needs", "base"], ["base", "needs"]);

    await flow.openForIds(["needs"]);
    await flow.confirm();

    expect(selectedIds.value).toEqual([]);
  });

  it("drops a requested mod the commit found already active, and keeps unrelated ones", async () => {
    // The commit re-plans: "needs" was already active by then, so only "base" was added.
    const { selectedIds, flow } = await setup(["needs", "base", "other"], ["base"], ["needs"]);

    await flow.openForIds(["needs"]);
    await flow.confirm();

    expect(selectedIds.value).toEqual(["other"]);
  });

  it("keeps a selected mod the commit neither activated nor found active", async () => {
    const { selectedIds, flow } = await setup(["needs", "other"], ["needs"]);

    await flow.openForIds(["needs"]);
    await flow.confirm();

    expect(selectedIds.value).toEqual(["other"]);
  });
});
