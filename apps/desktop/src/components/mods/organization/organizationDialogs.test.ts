import Aura from "@primevue/themes/aura";
import { flushPromises, mount } from "@vue/test-utils";
import PrimeVue from "primevue/config";
import { afterEach, describe, expect, it } from "vitest";

import ColorPickerDialog from "@/components/mods/organization/ColorPickerDialog.vue";
import GroupDeleteDialog from "@/components/mods/organization/GroupDeleteDialog.vue";
import GroupMoveDialog from "@/components/mods/organization/GroupMoveDialog.vue";
import GroupNameDialog from "@/components/mods/organization/GroupNameDialog.vue";
import ModGroupsDialog from "@/components/mods/organization/ModGroupsDialog.vue";
import { descriptor } from "@/i18n/messageDescriptor";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";

function node(
  id: number,
  name: string,
  parent: number | null,
  depth: number,
  extra: Partial<GroupNodeDto> = {},
): GroupNodeDto {
  return {
    id,
    name,
    color: null,
    parent,
    depth,
    childCount: 0,
    directCount: 0,
    scopeCount: 0,
    ...extra,
  };
}

const WEAPONS = node(1, "Weapons", null, 1, { childCount: 1, scopeCount: 7 });
const MELEE = node(2, "Melee", 1, 2, { scopeCount: 3 });
const ARMOR = node(3, "Armor", null, 1, { color: "teal" });
const NODES = [WEAPONS, MELEE, ARMOR];

let wrappers: ReturnType<typeof mount>[] = [];

async function mountDialog(component: object, props: Record<string, unknown>) {
  const wrapper = mount(component, {
    props: { visible: true, ...props },
    attachTo: document.body,
    global: {
      plugins: [[PrimeVue, { theme: { preset: Aura } }]],
      // PrimeVue Dialog renders through a real Teleport.
      stubs: { teleport: true },
    },
  });
  wrappers.push(wrapper);
  await flushPromises();
  return wrapper;
}

afterEach(() => {
  for (const wrapper of wrappers) {
    wrapper.unmount();
  }
  wrappers = [];
});

describe("GroupDeleteDialog", () => {
  it("offers both subgroup modes with the keep option chosen, and says no mod is deleted", async () => {
    const wrapper = await mountDialog(GroupDeleteDialog, {
      group: WEAPONS,
      error: null,
      isSubmitting: false,
    });

    expect(wrapper.text()).toContain("Delete Weapons?");
    expect(wrapper.text()).toContain("7 mods are in this group, including its subgroups.");
    expect(wrapper.text()).toContain("What happens to its 1 subgroup");
    expect(wrapper.get('[data-testid="group-delete-safety"]').text()).toBe(
      "No mod is deleted or uninstalled. Mods stay in any other group they belong to.",
    );
    expect(wrapper.get<HTMLInputElement>('[data-testid="group-delete-lift"]').element.checked).toBe(
      true,
    );

    await wrapper.get('[data-testid="group-delete-submit"]').trigger("click");
    expect(wrapper.emitted("confirm")).toEqual([["liftChildren"]]);
  });

  it("confirms with the subgroups deleted too when that is chosen", async () => {
    const wrapper = await mountDialog(GroupDeleteDialog, {
      group: WEAPONS,
      error: null,
      isSubmitting: false,
    });

    await wrapper.get('[data-testid="group-delete-with-subgroups"]').setValue(true);
    await wrapper.get('[data-testid="group-delete-submit"]').trigger("click");

    expect(wrapper.emitted("confirm")).toEqual([["withSubgroups"]]);
  });

  it("asks no question about subgroups for a group that has none", async () => {
    const wrapper = await mountDialog(GroupDeleteDialog, {
      group: ARMOR,
      error: null,
      isSubmitting: false,
    });

    expect(wrapper.find('[data-testid="group-delete-lift"]').exists()).toBe(false);
    expect(wrapper.text()).toContain("No mods are in this group.");
  });

  it("shows a refusal inside the dialog", async () => {
    const wrapper = await mountDialog(GroupDeleteDialog, {
      group: ARMOR,
      error: {
        title: descriptor("error.code.group_not_found.title"),
        detail: descriptor("error.code.group_not_found.detail"),
        technicalDetail: null,
      },
      isSubmitting: false,
    });

    expect(wrapper.get('[data-testid="organization-error"]').text()).toContain(
      "That group no longer exists",
    );
  });
});

describe("GroupNameDialog", () => {
  it("submits the trimmed name and never an empty one", async () => {
    const wrapper = await mountDialog(GroupNameDialog, {
      mode: "create",
      initialName: "",
      parentName: null,
      error: null,
      isSubmitting: false,
    });

    expect(wrapper.get('[data-testid="group-name-submit"]').attributes("disabled")).toBeDefined();
    await wrapper.get('[data-testid="group-name-input"]').setValue("  Mods I like  ");
    await wrapper.get('[data-testid="group-name-submit"]').trigger("click");

    expect(wrapper.emitted("submit")).toEqual([["Mods I like"]]);
  });

  it("starts a rename from the current name and a subgroup shows where it goes", async () => {
    const rename = await mountDialog(GroupNameDialog, {
      mode: "rename",
      initialName: "Weapons",
      parentName: null,
      error: null,
      isSubmitting: false,
    });
    const subgroup = await mountDialog(GroupNameDialog, {
      mode: "create",
      initialName: "",
      parentName: "Weapons",
      error: null,
      isSubmitting: false,
    });

    expect(rename.get<HTMLInputElement>('[data-testid="group-name-input"]').element.value).toBe(
      "Weapons",
    );
    expect(rename.text()).toContain("Rename group");
    expect(subgroup.text()).toContain("Inside Weapons");
    expect(subgroup.text()).toContain("New subgroup");
  });
});

describe("GroupMoveDialog", () => {
  it("disables the group itself and its subgroups with the reason, and the current place", async () => {
    const wrapper = await mountDialog(GroupMoveDialog, {
      group: WEAPONS,
      nodes: NODES,
      error: null,
      isSubmitting: false,
    });

    const own = wrapper.get<HTMLInputElement>('[data-testid="group-move-target-1"]');
    const below = wrapper.get<HTMLInputElement>('[data-testid="group-move-target-2"]');
    const elsewhere = wrapper.get<HTMLInputElement>('[data-testid="group-move-target-3"]');
    expect(own.element.disabled).toBe(true);
    expect(below.element.disabled).toBe(true);
    expect(elsewhere.element.disabled).toBe(false);
    expect(wrapper.get('[data-testid="group-move-reason-2"]').text()).toBe(
      "A group can't go inside itself or its own subgroups",
    );
    expect(wrapper.get<HTMLInputElement>('[data-testid="group-move-top"]').element.disabled).toBe(
      true,
    );
  });

  it("moves to the chosen group, or to the top level", async () => {
    const wrapper = await mountDialog(GroupMoveDialog, {
      group: MELEE,
      nodes: NODES,
      error: null,
      isSubmitting: false,
    });

    expect(wrapper.get('[data-testid="group-move-submit"]').attributes("disabled")).toBeDefined();
    await wrapper.get('[data-testid="group-move-target-3"]').setValue(true);
    await wrapper.get('[data-testid="group-move-submit"]').trigger("click");
    await wrapper.get('[data-testid="group-move-top"]').setValue(true);
    await wrapper.get('[data-testid="group-move-submit"]').trigger("click");

    expect(wrapper.emitted("move")).toEqual([[3], [null]]);
  });
});

describe("ColorPickerDialog", () => {
  it("lists every colour by name with a no-colour choice and starts from the current one", async () => {
    const wrapper = await mountDialog(ColorPickerDialog, {
      subject: "mod",
      current: "teal",
      error: null,
      isSubmitting: false,
    });

    const names = wrapper.findAll("label").map((label) => label.text());
    expect(names).toEqual([
      "Red",
      "Orange",
      "Amber",
      "Lime",
      "Green",
      "Teal",
      "Cyan",
      "Blue",
      "Indigo",
      "Violet",
      "Pink",
      "Slate",
      "No colour",
    ]);
    expect(wrapper.get<HTMLInputElement>('[data-testid="color-option-teal"]').element.checked).toBe(
      true,
    );
    expect(wrapper.find('[role="radiogroup"]').exists()).toBe(true);
  });

  it("applies the chosen colour, and null for no colour", async () => {
    const wrapper = await mountDialog(ColorPickerDialog, {
      subject: "group",
      current: null,
      error: null,
      isSubmitting: false,
    });

    await wrapper.get('[data-testid="color-option-violet"]').setValue(true);
    await wrapper.get('[data-testid="color-apply"]').trigger("click");
    await wrapper.get('[data-testid="color-option-none"]').setValue(true);
    await wrapper.get('[data-testid="color-apply"]').trigger("click");

    expect(wrapper.emitted("choose")).toEqual([["violet"], [null]]);
  });
});

describe("ModGroupsDialog", () => {
  it("checks the groups the mod is in and asks for the opposite on a click without changing the box itself", async () => {
    const wrapper = await mountDialog(ModGroupsDialog, {
      modName: "Some Mod",
      nodes: NODES,
      memberOf: [3],
      error: null,
      isBusy: false,
    });

    const armor = wrapper.get<HTMLInputElement>('[data-testid="mod-groups-toggle-3"]');
    const weapons = wrapper.get<HTMLInputElement>('[data-testid="mod-groups-toggle-1"]');
    expect(armor.element.checked).toBe(true);
    expect(weapons.element.checked).toBe(false);

    await weapons.trigger("click");
    await armor.trigger("click");

    expect(wrapper.emitted("toggle")).toEqual([
      [1, true],
      [3, false],
    ]);
    // Controlled: the box only changes once the parent saves and passes the new membership.
    expect(weapons.element.checked).toBe(false);
    expect(wrapper.text()).toContain("Groups for Some Mod");
  });

  it("keeps a focused checkbox enabled and focused while a save runs, and ignores clicks on it", async () => {
    const wrapper = await mountDialog(ModGroupsDialog, {
      modName: "Some Mod",
      nodes: NODES,
      memberOf: [3],
      error: null,
      isBusy: false,
    });
    const weapons = wrapper.get<HTMLInputElement>('[data-testid="mod-groups-toggle-1"]');
    weapons.element.focus();

    await wrapper.setProps({ isBusy: true } as Record<string, unknown>);
    await weapons.trigger("click");

    // A `disabled` input would drop keyboard focus; the user keeps their place in the list.
    expect(weapons.attributes("disabled")).toBeUndefined();
    expect(weapons.attributes("aria-disabled")).toBe("true");
    expect(document.activeElement).toBe(weapons.element);
    expect(wrapper.emitted("toggle")).toBeUndefined();
  });

  it("says so when there are no groups to choose from", async () => {
    const wrapper = await mountDialog(ModGroupsDialog, {
      modName: "Some Mod",
      nodes: [],
      memberOf: [],
      error: null,
      isBusy: false,
    });

    expect(wrapper.get('[data-testid="mod-groups-empty"]').text()).toContain("no groups yet");
  });
});
