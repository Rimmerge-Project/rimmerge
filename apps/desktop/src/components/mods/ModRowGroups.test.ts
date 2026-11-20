import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import { createI18n } from "vue-i18n";

import ModRowGroups from "@/components/mods/ModRowGroups.vue";
import type { GroupLabelDto } from "@/types/generated/GroupLabelDto";

function label(id: number, name: string): GroupLabelDto {
  return { id, name, color: null };
}

function mountFor(locale: string, groups: GroupLabelDto[]) {
  const i18n = createI18n({
    legacy: false,
    locale,
    messages: { [locale]: { mods: { groups: { allGroups: "{groups}" } } } },
  });
  return mount(ModRowGroups, { props: { groups }, global: { plugins: [i18n] } });
}

describe("ModRowGroups", () => {
  it("joins every group name with the locale's own list wording", () => {
    const groups = [label(1, "Weapons"), label(2, "Armor"), label(3, "Loot")];

    expect(mountFor("en", groups).get('[data-testid="mod-row-groups-text"]').text()).toBe(
      "Weapons, Armor, and Loot",
    );
    expect(mountFor("pt-BR", groups).get('[data-testid="mod-row-groups-text"]').text()).toBe(
      "Weapons, Armor e Loot",
    );
  });
});
