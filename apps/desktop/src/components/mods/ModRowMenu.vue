<script setup lang="ts">
import Menu, { type MenuMethods } from "primevue/menu";
import type { MenuItem } from "primevue/menuitem";
import { computed, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import { isKeyboardOpen, markFirstMenuItem } from "@/composables/popupMenuFocus";
import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import { anchorForStep, moveSetFor } from "@/utils/modMove";
import { isDirectActivateFor, type MenuAction, type RowMenu } from "@/utils/modRowMenu";
import { COLOR_NAME_KEY, COLOR_SWATCH_CLASS, COLOR_TOKENS } from "@/utils/organization";

/**
 * The one popup menu a mod table shares between all its rows (a menu per virtualized row would be a
 * menu per row): the position entries (Move, or "Activate at…"), the mod's own "Groups…" and
 * "Colour…", and the colour swatches. The table opens it for a row with {@link open}; every choice
 * goes out as an event and the table's owner decides what it means.
 */
const {
  mods,
  selectedIds = [],
  rowMenu = "none",
  hasOrganizationActions = false,
  reorderBlockedReason = null,
} = defineProps<{
  /** The rows the table shows, to read a row's colour and to find the neighbour a step moves past. */
  mods: readonly ModListRowDto[];
  selectedIds?: readonly string[];
  /** The per-row position actions: Move entries (Active), "Activate at…" (Inactive) or none. */
  rowMenu?: RowMenu;
  /** Whether the menu also offers the mod's own "Groups…" and "Colour…" (organization is usable). */
  hasOrganizationActions?: boolean;
  /** Why moving is off; `null` when it is on. */
  reorderBlockedReason?: string | null;
}>();
const emit = defineEmits<{
  moveStep: [modId: string, direction: "up" | "down"];
  menuAction: [modId: string, action: MenuAction];
  setColor: [modId: string, color: ColorTokenDto | null];
}>();
const { t } = useI18n();

const menu = useTemplateRef<MenuMethods>("moveMenu");
const menuModId = ref<string | null>(null);
const menuModColor = computed<ColorTokenDto | null>(
  () => mods.find((mod) => mod.modId === menuModId.value)?.color ?? null,
);

const isDirectActivate = computed(() => isDirectActivateFor(rowMenu, hasOrganizationActions));

/** A swatch in the menu: set the colour in one click and close the menu. */
function pickColor(color: ColorTokenDto | null): void {
  const modId = menuModId.value;
  menu.value?.hide();
  if (modId !== null) {
    emit("setColor", modId, color);
  }
}

const menuItems = computed<MenuItem[]>(() => {
  const modId = menuModId.value;
  if (modId === null) {
    return [];
  }
  // `testId` rides along on the item (a MenuItem accepts extra keys) for the item template.
  const positionItems = rowMenu === "move" ? moveItems(modId) : activateItems(modId);
  if (!hasOrganizationActions) {
    return positionItems;
  }
  const color = mods.find((mod) => mod.modId === modId)?.color ?? null;
  const organizationItems: MenuItem[] = [
    {
      label: t("mods.groups.rowMenu.groups"),
      testId: "mod-menu-groups",
      command: () => emit("menuAction", modId, "groups"),
    },
    {
      label:
        color === null
          ? t("mods.groups.rowMenu.color")
          : t("mods.groups.rowMenu.colorCurrent", { name: t(COLOR_NAME_KEY[color]) }),
      testId: "mod-menu-color",
      command: () => emit("menuAction", modId, "color"),
    },
  ];
  return positionItems.length === 0
    ? organizationItems
    : [...positionItems, { separator: true }, ...organizationItems];
});

function activateItems(modId: string): MenuItem[] {
  if (rowMenu !== "activate") {
    return [];
  }
  return [
    {
      label: t("mods.move.activateAtItem"),
      testId: "mod-activate-at",
      command: () => emit("menuAction", modId, "activateAt"),
    },
  ];
}

function moveItems(modId: string): MenuItem[] {
  const moving = new Set(moveSetFor(modId, selectedIds, mods));
  const isBlocked = reorderBlockedReason !== null;
  return [
    {
      label: t("mods.move.up"),
      testId: "mod-move-up",
      disabled: isBlocked || anchorForStep(mods, moving, "up") === null,
      command: () => emit("moveStep", modId, "up"),
    },
    {
      label: t("mods.move.down"),
      testId: "mod-move-down",
      disabled: isBlocked || anchorForStep(mods, moving, "down") === null,
      command: () => emit("moveStep", modId, "down"),
    },
    {
      label: t("mods.move.top"),
      testId: "mod-move-top",
      disabled: isBlocked,
      command: () => emit("menuAction", modId, "top"),
    },
    {
      label: t("mods.move.bottom"),
      testId: "mod-move-bottom",
      disabled: isBlocked,
      command: () => emit("menuAction", modId, "bottom"),
    },
    {
      label: t("mods.move.after"),
      testId: "mod-move-after",
      disabled: isBlocked,
      command: () => emit("menuAction", modId, "after"),
    },
  ];
}

/** Whether the open menu came from the keyboard, so its first item is marked once it shows. */
let isMenuFromKeyboard = false;

/**
 * The row button (and Shift+F10): opens the menu for this row, or, with a single possible entry,
 * runs it.
 */
function open(event: Event, modId: string): void {
  if (isDirectActivate.value) {
    emit("menuAction", modId, "activateAt");
    return;
  }
  menuModId.value = modId;
  isMenuFromKeyboard = isKeyboardOpen(event);
  menu.value?.toggle(event);
}
defineExpose({ open });

function onMenuShown(): void {
  if (isMenuFromKeyboard) {
    markFirstMenuItem();
  }
}
</script>

<template>
  <Menu
    ref="moveMenu"
    popup
    :model="menuItems"
    data-testid="mod-move-menu"
    @show="onMenuShown"
  >
    <template
      v-if="hasOrganizationActions"
      #start
    >
      <div
        role="group"
        class="flex flex-wrap items-center gap-1 px-3 pt-2 pb-1"
        :aria-label="t('mods.colors.swatchesLabel')"
        data-testid="mod-menu-swatches"
      >
        <button
          v-for="color in COLOR_TOKENS"
          :key="color"
          type="button"
          class="focus-visible:outline-accent size-5 cursor-pointer rounded-full focus-visible:outline-2"
          :class="[
            COLOR_SWATCH_CLASS[color],
            menuModColor === color ? 'ring-text ring-2 ring-offset-1' : '',
          ]"
          :aria-label="t('mods.colors.swatch', { name: t(COLOR_NAME_KEY[color]) })"
          :title="t(COLOR_NAME_KEY[color])"
          :aria-pressed="menuModColor === color"
          :data-testid="`mod-menu-swatch-${color}`"
          @click="pickColor(color)"
        />
        <button
          type="button"
          class="border-border-strong text-text-muted focus-visible:outline-accent flex size-5 cursor-pointer items-center justify-center rounded-full border border-dashed focus-visible:outline-2"
          :aria-label="t('mods.colors.swatchNone')"
          :title="t('mods.colors.swatchNone')"
          :aria-pressed="menuModColor === null"
          data-testid="mod-menu-swatch-none"
          @click="pickColor(null)"
        >
          <i
            class="pi pi-times text-[0.6rem]"
            aria-hidden="true"
          />
        </button>
      </div>
    </template>
    <template #item="{ item, props }">
      <a
        v-bind="props.action"
        :data-testid="item['testId']"
      >
        {{ item.label }}
      </a>
    </template>
    <template
      v-if="rowMenu === 'move' && reorderBlockedReason !== null"
      #end
    >
      <p
        class="text-text-muted max-w-64 px-3 py-2 text-xs"
        data-testid="mod-move-menu-reason"
      >
        {{ reorderBlockedReason }}
      </p>
    </template>
  </Menu>
</template>
