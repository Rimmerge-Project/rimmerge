import Aura from "@primevue/themes/aura";
import { mount } from "@vue/test-utils";
import PrimeVue from "primevue/config";
import { describe, expect, it } from "vitest";

import PatchIdentityForm, {
  type PatchIdentityFormValues,
} from "@/components/patches/PatchIdentityForm.vue";
import { RimmergeError } from "@/services/ipc";

const VALUES: PatchIdentityFormValues = {
  name: "Wall compat",
  packageId: "author.wallcompat",
  displayName: "Wall Compatibility Patch",
  author: "Rimmerge",
  description: "Compatibility patch for Mod A, Mod B.",
};

/** `InputText`/`Textarea` read injected PrimeVue config. */
function mountForm(props: InstanceType<typeof PatchIdentityForm>["$props"]) {
  return mount(PatchIdentityForm, {
    props,
    global: { plugins: [[PrimeVue, { theme: { preset: Aura } }]] },
  });
}

describe("PatchIdentityForm", () => {
  it("create mode shows name/package id/display name but not author/description", () => {
    const wrapper = mountForm({ mode: "create", initial: VALUES });

    expect(wrapper.find('[data-testid="patch-name-input"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="patch-package-id-input"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="patch-display-name-input"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="patch-author-input"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="patch-description-input"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="patch-cancel-button"]').exists()).toBe(true);
  });

  it("edit mode shows every field and no cancel button", () => {
    const wrapper = mountForm({ mode: "edit", initial: VALUES });

    expect(wrapper.find('[data-testid="patch-author-input"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="patch-description-input"]').exists()).toBe(true);
    expect(wrapper.find('[data-testid="patch-cancel-button"]').exists()).toBe(false);
  });

  it("emits save with the current field values on submit", async () => {
    const wrapper = mountForm({ mode: "edit", initial: VALUES });

    await wrapper.get('[data-testid="patch-name-input"]').setValue("Renamed");
    await wrapper.get('[data-testid="patch-identity-form"]').trigger("submit");

    expect(wrapper.emitted("save")).toEqual([[{ ...VALUES, name: "Renamed" }]]);
  });

  it("emits cancel from the create-mode cancel button", async () => {
    const wrapper = mountForm({ mode: "create", initial: VALUES });

    await wrapper.get('[data-testid="patch-cancel-button"]').trigger("click");
    expect(wrapper.emitted("cancel")).toHaveLength(1);
  });

  it("resets the draft when a different patch's initial values arrive", async () => {
    const wrapper = mountForm({ mode: "edit", initial: VALUES });
    await wrapper.get('[data-testid="patch-name-input"]').setValue("Dirty draft");

    await wrapper.setProps({ initial: { ...VALUES, name: "Someone else's patch" } });

    expect(
      (wrapper.get('[data-testid="patch-name-input"]').element as HTMLInputElement).value,
    ).toBe("Someone else's patch");
  });

  it("keeps a dirty draft when initial is a new object with the same values (a refetch, not a different patch)", async () => {
    const wrapper = mountForm({ mode: "edit", initial: VALUES });
    await wrapper.get('[data-testid="patch-name-input"]').setValue("Dirty draft");

    await wrapper.setProps({ initial: { ...VALUES } });

    expect(
      (wrapper.get('[data-testid="patch-name-input"]').element as HTMLInputElement).value,
    ).toBe("Dirty draft");
  });

  it("shows the inline error under the package id field for patchIdentityInvalid", () => {
    const error = new RimmergeError({
      code: "patch_identity_invalid",
      message: "author.wallcompat is already used by another patch",
    });
    const wrapper = mountForm({ mode: "edit", initial: VALUES, error });

    expect(wrapper.get('[data-testid="patch-package-id-error"]').text()).toBe(
      "author.wallcompat is already used by another patch",
    );
    expect(wrapper.find('[data-testid="patch-identity-error"]').exists()).toBe(false);
  });

  it("shows a general error banner for any other error code", () => {
    const error = new RimmergeError({ code: "profile_io_failed", message: "disk is full" });
    const wrapper = mountForm({ mode: "edit", initial: VALUES, error });

    expect(wrapper.get('[data-testid="patch-identity-error"]').text()).toBe("disk is full");
    expect(wrapper.find('[data-testid="patch-package-id-error"]').exists()).toBe(false);
  });
});
