import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, describe, expect, it, vi } from "vitest";
import { defineComponent, ref } from "vue";
import { createI18n } from "vue-i18n";

import { EMPTY_PENDING_ACTIVE_CHANGES } from "@/components/apply/ApplyDialog.test-support";
import { useModDeactivation } from "@/composables/useModDeactivation";
import { installMockIpc } from "@/services/ipc.mock";

vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: vi.fn() }) }));

/** Mounts the composable over `selected`, with the IPC removing exactly `removed`. */
async function setup(selected: string[], removed: string[], notActive: string[] = []) {
  installMockIpc({
    plan_deactivate_mods: { toRemove: removed, dependentsStillActive: {}, refused: [] },
    deactivate_mods: { pending: EMPTY_PENDING_ACTIVE_CHANGES, removed, notActive },
  });
  const selectedIds = ref(selected);
  let flow!: ReturnType<typeof useModDeactivation>;
  const Harness = defineComponent({
    setup() {
      flow = useModDeactivation({ selectedIds });
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

describe("useModDeactivation", () => {
  afterEach(() => {
    clearMocks();
  });

  it("keeps a refused Core selected and drops what the backend removed (#47)", async () => {
    const { selectedIds, flow } = await setup(["ludeon.rimworld", "a", "hidden"], ["a"]);

    await flow.openForIds(["ludeon.rimworld", "a"]);
    await flow.confirm();

    expect(selectedIds.value).toEqual(["ludeon.rimworld", "hidden"]);
  });

  it("drops a requested mod that was no longer active at commit, and keeps unrelated ones", async () => {
    const { selectedIds, flow } = await setup(["a", "gone", "other"], ["a"], ["gone"]);

    await flow.openForIds(["a", "gone"]);
    await flow.confirm();

    expect(selectedIds.value).toEqual(["other"]);
  });
});
