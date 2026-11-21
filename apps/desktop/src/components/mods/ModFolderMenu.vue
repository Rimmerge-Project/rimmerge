<script setup lang="ts">
import Menu, { type MenuMethods } from "primevue/menu";
import type { MenuItem } from "primevue/menuitem";
import { computed, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import { isKeyboardOpen, markFirstMenuItem } from "@/composables/popupMenuFocus";
import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import type { FolderAction } from "@/utils/modRowMenu";

/**
 * The one popup menu the Groups view shares between its folder headers: select the folder's mods
 * and, on a group, activate or deactivate them. The table opens it for a header with {@link open};
 * a choice goes out as an event and the page decides what it means.
 */
const emit = defineEmits<{
  action: [folder: FolderKeyDto, action: FolderAction];
}>();
const { t } = useI18n();

const menu = useTemplateRef<MenuMethods>("folderMenu");
const menuFolder = ref<FolderKeyDto | null>(null);
/** Whether the open menu came from the keyboard, so its first item is marked once it shows. */
let isMenuFromKeyboard = false;

const items = computed<MenuItem[]>(() => {
  const folder = menuFolder.value;
  if (folder === null) {
    return [];
  }
  const entries: MenuItem[] = [
    {
      label: t("mods.groupsView.menu.select"),
      testId: "folder-menu-select",
      command: () => emit("action", folder, "select"),
    },
  ];
  if (folder.kind === "group") {
    entries.push(
      {
        label: t("mods.groupsView.menu.activate"),
        testId: "folder-menu-activate",
        command: () => emit("action", folder, "activate"),
      },
      {
        label: t("mods.groupsView.menu.deactivate"),
        testId: "folder-menu-deactivate",
        command: () => emit("action", folder, "deactivate"),
      },
    );
  }
  return entries;
});

/** Opens (or closes, on a second press) the menu for `folder`, below the event's target. */
function open(event: Event, folder: FolderKeyDto): void {
  menuFolder.value = folder;
  isMenuFromKeyboard = isKeyboardOpen(event);
  menu.value?.toggle(event);
}
defineExpose({ open });

function onShown(): void {
  if (isMenuFromKeyboard) {
    markFirstMenuItem();
  }
}
</script>

<template>
  <Menu
    ref="folderMenu"
    popup
    :model="items"
    data-testid="mods-folder-menu"
    @show="onShown"
  >
    <template #item="{ item, props }">
      <a
        v-bind="props.action"
        :data-testid="item['testId']"
      >
        {{ item.label }}
      </a>
    </template>
  </Menu>
</template>
