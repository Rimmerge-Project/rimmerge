import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import { flush } from "@/components/apply/ApplyDialog.test-support";
import SaveImportDialog from "@/components/order/SaveImportDialog.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { SaveListingDto } from "@/types/generated/SaveListingDto";

function mountDialog(
  handlers: Record<string, unknown>,
  props: { visible?: boolean; isBusy?: boolean } = {},
) {
  installMockIpc(handlers);
  return mount(SaveImportDialog, {
    props: { visible: props.visible ?? true, isBusy: props.isBusy ?? false },
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
}

const SEPTEMBER = Date.UTC(2026, 8, 5, 12, 0, 0) / 1000;

const LISTING: SaveListingDto = {
  folder: "C:/Users/x/Saves",
  saves: [
    { path: "C:/Users/x/Saves/Newest.rws", name: "Newest", savedAt: SEPTEMBER },
    { path: "C:/Users/x/Saves/<b>Odd</b>.rws", name: "<b>Odd</b>", savedAt: null },
  ],
  omitted: 0,
};

describe("SaveImportDialog", () => {
  afterEach(() => {
    document.body.innerHTML = "";
    clearMocks();
  });

  it("lists the saves in the order the backend gave, each with a date or 'Date unknown'", async () => {
    const wrapper = mountDialog({ list_saves: LISTING });
    await flush();

    const rows = wrapper.findAll('[data-testid="save-import-row"]');

    expect(rows.map((row) => row.get('[data-testid="save-import-name"]').text())).toEqual([
      "Newest",
      "<b>Odd</b>",
    ]);
    expect(rows[0]?.text()).toContain("Saved ");
    expect(rows[1]?.text()).toContain("Date unknown");
  });

  it("renders a save name as text, never as markup", async () => {
    const wrapper = mountDialog({ list_saves: LISTING });
    await flush();

    expect(wrapper.find('[data-testid="save-import-list"] b').exists()).toBe(false);
  });

  it("emits the exact path of the save whose Preview was clicked", async () => {
    const wrapper = mountDialog({ list_saves: LISTING });
    await flush();

    await wrapper.findAll('[data-testid="save-import-preview"]')[1]?.trigger("click");

    expect(wrapper.emitted("preview")).toEqual([["C:/Users/x/Saves/<b>Odd</b>.rws"]]);
  });

  it("names the save in each Preview button's accessible label", async () => {
    const wrapper = mountDialog({ list_saves: LISTING });
    await flush();

    const button = wrapper.get('[data-testid="save-import-preview"]');

    expect(button.attributes("aria-label")).toBe("Preview the mod list of Newest");
  });

  it("starts Browse in the Saves folder", async () => {
    const wrapper = mountDialog({ list_saves: LISTING });
    await flush();

    await wrapper.get('[data-testid="save-import-browse"]').trigger("click");

    expect(wrapper.emitted("browse")).toEqual([["C:/Users/x/Saves"]]);
  });

  it("says there are no saves when the folder is empty", async () => {
    const wrapper = mountDialog({ list_saves: { folder: "C:/Saves", saves: [], omitted: 0 } });
    await flush();

    expect(wrapper.get('[data-testid="save-import-empty"]').text()).toBe(
      "No saves in RimWorld's Saves folder.",
    );
    expect(wrapper.find('[data-testid="save-import-list"]').exists()).toBe(false);
  });

  it("offers only Browse, without a folder, when the Saves folder was not found", async () => {
    const wrapper = mountDialog({ list_saves: { folder: null, saves: [], omitted: 0 } });
    await flush();

    expect(wrapper.find('[data-testid="save-import-no-folder"]').exists()).toBe(true);
    await wrapper.get('[data-testid="save-import-browse"]').trigger("click");
    expect(wrapper.emitted("browse")).toEqual([[null]]);
  });

  it("counts the older saves that are not shown", async () => {
    const wrapper = mountDialog({ list_saves: { ...LISTING, omitted: 3 } });
    await flush();

    expect(wrapper.get('[data-testid="save-import-omitted"]').text()).toBe(
      "3 older saves not shown.",
    );
  });

  it("words a failed listing with the code's own sentence and still offers Browse", async () => {
    const wrapper = mountDialog({
      list_saves: () => {
        throw { code: "saves_folder_unreadable", message: "C:/Saves: access denied" };
      },
    });
    await flush();

    expect(wrapper.get('[data-testid="save-import-error"]').text()).toContain(
      "Couldn't read the Saves folder",
    );
    expect(
      wrapper.get('[data-testid="save-import-browse"]').attributes("disabled"),
    ).toBeUndefined();
  });

  it("does not list the folder until it is opened", async () => {
    let calls = 0;
    const wrapper = mountDialog(
      {
        list_saves: () => {
          calls += 1;
          return LISTING;
        },
      },
      { visible: false },
    );
    await flush();
    expect(calls).toBe(0);

    await wrapper.setProps({ visible: true });
    await flush();

    expect(calls).toBe(1);
  });

  it("disables Preview and Browse while a preview is running", async () => {
    const wrapper = mountDialog({ list_saves: LISTING }, { isBusy: true });
    await flush();

    expect(wrapper.get('[data-testid="save-import-preview"]').attributes("disabled")).toBeDefined();
    expect(wrapper.get('[data-testid="save-import-browse"]').attributes("disabled")).toBeDefined();
  });
});
