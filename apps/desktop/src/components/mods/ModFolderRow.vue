<script setup lang="ts">
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import GroupFolderIcon from "@/components/mods/GroupFolderIcon.vue";
import type { TreeRow } from "@/utils/groupedEntries";

/**
 * The inside of a Groups view folder header: the chevron, the folder icon in the group's colour,
 * its name, how many mods it holds and its ⋮ button. The table owns the row element (its role,
 * level, focus, keys and drop handling). A click anywhere on the header folds or opens it; the
 * button opens the folder menu and neither its click nor its keys reach the row.
 */
const { row } = defineProps<{
  row: Extract<TreeRow, { kind: "folder" }>;
}>();
const emit = defineEmits<{ openMenu: [event: MouseEvent] }>();
const { t } = useI18n();

const name = computed(() =>
  row.entry.folder.kind === "group" ? row.entry.folder.name : t("mods.groupsView.ungrouped"),
);
const color = computed(() => (row.entry.folder.kind === "group" ? row.entry.folder.color : null));
const menuLabel = computed(() => t("mods.groupsView.folderActions", { name: name.value }));
</script>

<template>
  <div
    role="gridcell"
    class="contents"
  >
    <i
      class="pi text-text-muted w-4 shrink-0 text-xs"
      :class="row.entry.isCollapsed ? 'pi-chevron-right' : 'pi-chevron-down'"
      aria-hidden="true"
      data-testid="folder-chevron"
    />
    <i
      v-if="row.entry.folder.kind === 'ungrouped'"
      class="pi pi-inbox text-text-muted shrink-0"
      aria-hidden="true"
    />
    <GroupFolderIcon
      v-else
      :color="color"
    />
    <span
      class="text-text min-w-12 flex-1 truncate font-semibold"
      :title="name"
      data-testid="folder-name"
    >{{ name }}</span>
    <span
      class="text-text-muted shrink-0 text-xs"
      data-testid="folder-count"
    >{{ t("mods.groupsView.modCount", { count: row.entry.matched }, row.entry.matched) }}</span>
    <Button
      class="shrink-0"
      icon="pi pi-ellipsis-v"
      severity="secondary"
      text
      rounded
      size="small"
      tabindex="-1"
      aria-haspopup="menu"
      :aria-label="menuLabel"
      :title="menuLabel"
      data-testid="folder-menu-button"
      @click.stop="(event: MouseEvent) => emit('openMenu', event)"
      @dblclick.stop
      @keydown.stop
    />
  </div>
</template>
