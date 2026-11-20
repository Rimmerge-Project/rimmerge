import { PiniaColada } from "@pinia/colada";
import Aura from "@primevue/themes/aura";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it, vi } from "vitest";

import BulkTagsDialog from "@/components/mods/organization/BulkTagsDialog.vue";
import { installMockIpc } from "@/services/ipc.mock";

/**
 * The bulk Tags dialog sends one `set_manual_tags` call with exactly the selected ids, never
 * rewrites what was typed, keeps its buttons off until the tag is a valid slug, and shows a
 * refusal inside itself instead of closing.
 */

const toastAdd = vi.hoisted(() => vi.fn());
vi.mock("primevue/usetoast", () => ({ useToast: () => ({ add: toastAdd }) }));

const IDS = ["a", "b", "c"];
const requests: unknown[] = [];
let wrappers: ReturnType<typeof mount>[] = [];

function mountDialog(handler: (payload: unknown) => unknown) {
  requests.length = 0;
  installMockIpc({
    set_manual_tags: (payload: unknown) => {
      requests.push(payload);
      return handler(payload);
    },
  });
  const wrapper = mount(BulkTagsDialog, {
    props: {
      visible: true,
      modIds: IDS,
      selectionText: "3 selected (1 not shown)",
      "onUpdate:visible": (value: boolean) => {
        void wrapper.setProps({ visible: value });
      },
    },
    attachTo: document.body,
    global: {
      plugins: [createPinia(), PiniaColada, [PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
  wrappers.push(wrapper);
  return wrapper;
}

function wrapperOf() {
  const wrapper = wrappers.at(-1);
  if (!wrapper) {
    throw new Error("mount the dialog first");
  }
  return wrapper;
}

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
  clearMocks();
  toastAdd.mockClear();
});

describe("BulkTagsDialog", () => {
  it("says how many mods it acts on, hidden ones included", async () => {
    const wrapper = mountDialog(() => ({ changed: 0, keptByDecision: 0, unchanged: 0 }));
    await flushPromises();

    expect(wrapper.get('[data-testid="bulk-tags-selection"]').text()).toBe(
      "3 selected (1 not shown)",
    );
  });

  it("keeps both buttons off until the tag is a valid slug", async () => {
    const wrapper = mountDialog(() => ({ changed: 0, keptByDecision: 0, unchanged: 0 }));
    await flushPromises();
    expect(wrapper.get('[data-testid="bulk-tags-add"]').attributes("disabled")).toBeDefined();

    await wrapper.get('[data-testid="bulk-tags-input"]').setValue("Not A Slug");
    expect(wrapper.get('[data-testid="bulk-tags-add"]').attributes("disabled")).toBeDefined();
    expect(wrapper.get('[data-testid="bulk-tags-remove"]').attributes("disabled")).toBeDefined();

    await wrapper.get('[data-testid="bulk-tags-input"]').setValue("framework");
    expect(wrapper.get('[data-testid="bulk-tags-add"]').attributes("disabled")).toBeUndefined();
  });

  it("adds the tag to exactly the selected ids with one call and reports the counts", async () => {
    const wrapper = mountDialog(() => ({ changed: 2, keptByDecision: 0, unchanged: 1 }));
    await flushPromises();

    await wrapper.get('[data-testid="bulk-tags-input"]').setValue("framework");
    await wrapper.get('[data-testid="bulk-tags-add"]').trigger("click");
    await flushPromises();

    expect(requests).toEqual([{ request: { modIds: IDS, tag: "framework", mode: "add" } }]);
    expect(toastAdd).toHaveBeenCalledWith(
      expect.objectContaining({
        summary: "Tag framework added to 2 mods",
        detail: "1 did not change",
      }),
    );
    expect(wrapper.props("visible")).toBe(false);
  });

  it("removes the tag with the remove button", async () => {
    const wrapper = mountDialog(() => ({ changed: 3, keptByDecision: 0, unchanged: 0 }));
    await flushPromises();

    await wrapper.get('[data-testid="bulk-tags-input"]').setValue("framework");
    await wrapper.get('[data-testid="bulk-tags-remove"]').trigger("click");
    await flushPromises();

    expect(requests).toEqual([{ request: { modIds: IDS, tag: "framework", mode: "remove" } }]);
    expect(toastAdd).toHaveBeenCalledWith(
      expect.objectContaining({ summary: "Tag framework removed from 3 mods" }),
    );
  });

  it("says on separate lines which mods keep the tag because of a decision and which did not change", async () => {
    mountDialog(() => ({ changed: 1, keptByDecision: 2, unchanged: 1 }));
    await flushPromises();

    await wrapperOf().get('[data-testid="bulk-tags-input"]').setValue("framework");
    await wrapperOf().get('[data-testid="bulk-tags-remove"]').trigger("click");
    await flushPromises();

    expect(toastAdd).toHaveBeenCalledWith(
      expect.objectContaining({
        summary: "Tag framework removed from 1 mod",
        detail: "2 mods keep it because of a decision on the Findings page\n1 did not change",
      }),
    );
  });

  it("stays open and shows the refusal when the backend refuses", async () => {
    const wrapper = mountDialog(() => {
      throw { code: "invalid_input", message: "invalid tag" };
    });
    await flushPromises();

    await wrapper.get('[data-testid="bulk-tags-input"]').setValue("framework");
    await wrapper.get('[data-testid="bulk-tags-add"]').trigger("click");
    await flushPromises();

    expect(wrapper.props("visible")).toBe(true);
    expect(wrapper.find('[data-testid="organization-error"]').exists()).toBe(true);
    expect(toastAdd).not.toHaveBeenCalled();
  });
});
