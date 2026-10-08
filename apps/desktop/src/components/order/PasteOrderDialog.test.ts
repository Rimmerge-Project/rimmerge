import Aura from "@primevue/themes/aura";
import { mount } from "@vue/test-utils";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import { flush } from "@/components/apply/ApplyDialog.test-support";
import PasteOrderDialog from "@/components/order/PasteOrderDialog.vue";

function mountDialog(props: { isBusy?: boolean } = {}) {
  return mount(PasteOrderDialog, {
    props: { visible: true, isBusy: props.isBusy ?? false },
    attachTo: document.body,
    global: {
      plugins: [[PrimeVue, { theme: { preset: Aura } }]],
      stubs: { teleport: true },
    },
  });
}

describe("PasteOrderDialog", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  it("keeps Preview disabled until something other than whitespace is pasted", async () => {
    const wrapper = mountDialog();
    await flush();
    const preview = wrapper.get('[data-testid="paste-order-preview"]');

    expect(preview.attributes("disabled")).toBeDefined();
    await wrapper.get('[data-testid="paste-order-text"]').setValue("   \n ");
    expect(preview.attributes("disabled")).toBeDefined();
    await wrapper.get('[data-testid="paste-order-text"]').setValue("example.framework");
    expect(preview.attributes("disabled")).toBeUndefined();
  });

  it("previews the text exactly as pasted", async () => {
    const wrapper = mountDialog();
    await flush();
    const text = "1. Example Framework [example.framework]\n2. Some Local Mod [someone.localmod]\n";

    await wrapper.get('[data-testid="paste-order-text"]').setValue(text);
    await wrapper.get("form").trigger("submit");

    expect(wrapper.emitted("preview")).toEqual([[text]]);
  });

  it("sends a paste over the size limit whole, so the backend answers tooLarge", async () => {
    const wrapper = mountDialog();
    await flush();
    const overLong = "a.b\n".repeat(1024 * 1024 + 1);

    await wrapper.get('[data-testid="paste-order-text"]').setValue(overLong);
    await wrapper.get("form").trigger("submit");

    expect(wrapper.get('[data-testid="paste-order-text"]').attributes("maxlength")).toBeUndefined();
    expect(wrapper.emitted("preview")).toEqual([[overLong]]);
  });

  it("cannot be cancelled or closed while a preview is running", async () => {
    const wrapper = mountDialog({ isBusy: true });
    await flush();

    expect(wrapper.get('[data-testid="paste-order-cancel"]').attributes("disabled")).toBeDefined();
    expect(wrapper.find(".p-dialog-close-button").exists()).toBe(false);
  });

  it("does not preview twice while a preview is already running", async () => {
    const wrapper = mountDialog({ isBusy: true });
    await flush();

    await wrapper.get('[data-testid="paste-order-text"]').setValue("example.framework");
    await wrapper.get("form").trigger("submit");

    expect(wrapper.emitted("preview")).toBeUndefined();
  });
});
