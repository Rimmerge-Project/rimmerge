<script setup lang="ts">
import { useEventListener } from "@vueuse/core";
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Menu, { type MenuMethods } from "primevue/menu";
import type { MenuItem } from "primevue/menuitem";
import { computed, nextTick, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import GroupTree from "@/components/mods/GroupTree.vue";
import { isInsideMenu, isKeyboardOpen, markFirstMenuItem } from "@/composables/popupMenuFocus";
import { isInert } from "@/composables/shortcutTargets";
import { useGroupScopePlanQuery } from "@/queries/organization";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { GroupScopePlanDto } from "@/types/generated/GroupScopePlanDto";
import { type GroupScopeAction, scopeIds } from "@/utils/groupScopeText";
import { findNode, type GroupScopeChoice, searchTree } from "@/utils/organization";

/**
 * The groups panel: a rail beside the lists on a very wide window, or a drawer over the left edge
 * (`isOverlay`) below that. It holds the search, the tree and the actions on the group in focus;
 * the dialogs and every edit live with the page, so this only says what the user asked for.
 */
const {
  nodes,
  isOverlay = false,
  rowDrag = null,
} = defineProps<{
  /** Every group, depth first, siblings by name. */
  nodes: readonly GroupNodeDto[];
  /** Whether the panel floats over the page (a drawer) instead of taking its own column. */
  isOverlay?: boolean;
  /** The mods being dragged from a list, so the tree can take them as a drop. */
  rowDrag?: { readonly count: number; readonly hiddenCount: number } | null;
}>();
const emit = defineEmits<{
  createGroup: [parent: number | null];
  renameGroup: [groupId: number];
  changeColor: [groupId: number];
  moveGroup: [groupId: number];
  deleteGroup: [groupId: number];
  removeUninstalled: [groupId: number];
  /** Activate every installed mod of the group and its subgroups (opens the Activate dialog). */
  activateGroup: [groupId: number];
  /** Deactivate every active mod of the group and its subgroups (opens the Deactivate dialog). */
  deactivateGroup: [groupId: number];
  dropMods: [groupId: number];
  dropGroup: [groupId: number, parent: number | null];
  close: [];
}>();
const scope = defineModel<GroupScopeChoice>("scope", { required: true });

const { t } = useI18n();

const search = ref("");
const searchResult = computed(() => searchTree(nodes, search.value));
const shown = computed(() => searchResult.value?.shown ?? null);
const hasNoMatch = computed(
  () => searchResult.value !== null && searchResult.value.matches.size === 0,
);

const expanded = ref<ReadonlySet<number>>(new Set());

// A group that gains a subgroup opens, so a new subgroup is visible where it was just made.
watch(
  () => nodes,
  (current, previous) => {
    const before = new Map(previous.map((node) => [node.id, node.childCount]));
    const opened = current.filter(
      (node) => node.childCount > (before.get(node.id) ?? node.childCount),
    );
    if (opened.length > 0) {
      expanded.value = new Set([...expanded.value, ...opened.map((node) => node.id)]);
    }
  },
);

const menu = useTemplateRef<MenuMethods>("groupMenu");
const menuGroupId = ref<number | null>(null);
const { data: scopePlan, error: scopePlanError } = useGroupScopePlanQuery(menuGroupId);

const scopedGroupId = computed(() => (scope.value.kind === "group" ? scope.value.id : null));

const menuItems = computed<MenuItem[]>(() => {
  const id = menuGroupId.value;
  if (id === null) {
    return [];
  }
  return [
    {
      label: t("mods.groups.menu.newSubgroup"),
      testId: "group-menu-new-subgroup",
      command: () => emit("createGroup", id),
    },
    {
      label: t("mods.groups.menu.rename"),
      testId: "group-menu-rename",
      command: () => emit("renameGroup", id),
    },
    {
      label: t("mods.groups.menu.color"),
      testId: "group-menu-color",
      command: () => emit("changeColor", id),
    },
    {
      label: t("mods.groups.menu.move"),
      testId: "group-menu-move",
      command: () => emit("moveGroup", id),
    },
    { separator: true },
    ...scopeActionItems(id),
    { separator: true },
    removeUninstalledItem(id),
    { separator: true },
    {
      label: t("mods.groups.menu.delete"),
      testId: "group-menu-delete",
      command: () => emit("deleteGroup", id),
    },
  ];
});

/**
 * The Activate and Deactivate entries for a whole group. The plan answers how many mods each
 * covers; until it does (or when it failed) the counts are unknown, and one disabled line says so
 * for both instead of offering an action over an unknown number of mods. Choosing an entry plans
 * over the same mods again, fresh, in the page.
 */
function scopeActionItems(id: number): MenuItem[] {
  const plan = scopePlan.value;
  if (plan === undefined) {
    return [
      {
        label:
          scopePlanError.value === null
            ? t("mods.groups.menu.checkingScope")
            : t("mods.groups.menu.scopeUnavailable"),
        testId: "group-menu-scope-status",
        disabled: true,
      },
    ];
  }
  return [scopeActionItem(id, "activate", plan), scopeActionItem(id, "deactivate", plan)];
}

function scopeActionItem(id: number, action: GroupScopeAction, plan: GroupScopePlanDto): MenuItem {
  const testId = `group-menu-${action}-all`;
  const count = scopeIds(plan, action).length;
  if (count === 0) {
    return {
      label: t(
        action === "activate"
          ? "mods.groups.menu.nothingToActivate"
          : "mods.groups.menu.nothingToDeactivate",
      ),
      testId,
      disabled: true,
    };
  }
  return {
    label: t(
      action === "activate" ? "mods.groups.menu.activateAll" : "mods.groups.menu.deactivateAll",
      { count },
      count,
    ),
    testId,
    command: () => {
      if (action === "activate") {
        emit("activateGroup", id);
      } else {
        emit("deactivateGroup", id);
      }
    },
  };
}

/**
 * The "remove mods that aren't installed" entry. Until the plan answers (or when it failed) the
 * count is unknown, and an unknown count is never worded as "all installed".
 */
function removeUninstalledItem(id: number): MenuItem {
  const testId = "group-menu-remove-uninstalled";
  const plan = scopePlan.value;
  if (plan === undefined) {
    const label =
      scopePlanError.value === null
        ? t("mods.groups.menu.checkingUninstalled")
        : t("mods.groups.menu.uninstalledUnavailable");
    return { label, testId, disabled: true };
  }
  if (plan.notInstalled === 0) {
    return { label: t("mods.groups.menu.noUninstalled"), testId, disabled: true };
  }
  return {
    label: t("mods.groups.menu.removeUninstalled", { count: plan.notInstalled }, plan.notInstalled),
    testId,
    command: () => emit("removeUninstalled", id),
  };
}

/** Whether the open menu came from the keyboard, so its first item is marked once it shows. */
let isMenuFromKeyboard = false;

function openMenu(event: Event, groupId: number): void {
  menuGroupId.value = groupId;
  isMenuFromKeyboard = isKeyboardOpen(event);
  menu.value?.toggle(event);
}

function onMenuShown(): void {
  if (isMenuFromKeyboard) {
    markFirstMenuItem();
  }
}

function openMenuForScope(event: Event): void {
  if (scopedGroupId.value !== null) {
    openMenu(event, scopedGroupId.value);
  }
}

const scopedNode = computed(() => findNode(nodes, scopedGroupId.value));

const panel = useTemplateRef<HTMLElement>("panel");
const tree = useTemplateRef<{ focusCurrent: () => Promise<void> }>("tree");

/** Puts keyboard focus back on the tree, for after a dialog about a group closed. */
function focusTree(): Promise<void> {
  return tree.value?.focusCurrent() ?? Promise.resolve();
}
defineExpose({ focusTree });

// A drawer closes on Escape from anywhere on the page, like the mod info drawer; a dialog, a text
// field or the group menu keeps its own Escape (closing the menu must leave the drawer open).
useEventListener(window, "keydown", (event: KeyboardEvent) => {
  if (
    isOverlay &&
    event.key === "Escape" &&
    !isInert(event.target) &&
    !isInsideMenu(event.target)
  ) {
    emit("close");
  }
});

// A drawer covers the lists, so opening it moves keyboard focus into it.
watch(
  () => isOverlay,
  async (isDrawer) => {
    if (isDrawer) {
      await nextTick();
      panel.value?.focus();
    }
  },
  { immediate: true },
);
</script>

<template>
  <aside
    id="group-rail"
    ref="panel"
    tabindex="-1"
    class="surface-card flex min-h-0 w-72 flex-col gap-3 p-3 focus:outline-none"
    :class="isOverlay ? 'fixed inset-y-0 left-0 z-30 max-w-full shadow-xl' : 'shrink-0'"
    :aria-label="t('mods.groups.regionLabel')"
    data-testid="group-rail"
  >
    <div class="flex items-center justify-between gap-2">
      <h2 class="text-text text-lg font-semibold">
        {{ t("mods.groups.heading") }}
      </h2>
      <Button
        v-if="isOverlay"
        icon="pi pi-times"
        severity="secondary"
        text
        rounded
        size="small"
        :aria-label="t('mods.groups.close')"
        data-testid="group-rail-close"
        @click="emit('close')"
      />
    </div>
    <p class="text-text-muted text-xs">
      {{ t("mods.groups.help") }}
    </p>
    <div class="flex flex-wrap gap-2">
      <Button
        :label="t('mods.groups.newGroup')"
        icon="pi pi-plus"
        size="small"
        severity="secondary"
        outlined
        data-testid="group-new"
        @click="emit('createGroup', null)"
      />
      <Button
        :label="t('mods.groups.actions')"
        icon="pi pi-ellipsis-h"
        size="small"
        severity="secondary"
        text
        aria-haspopup="menu"
        :aria-label="
          scopedNode === undefined
            ? t('mods.groups.actions')
            : t('mods.groups.actionsFor', { name: scopedNode.name })
        "
        :disabled="scopedNode === undefined"
        data-testid="group-actions"
        @click="openMenuForScope"
      />
    </div>
    <InputText
      v-model="search"
      size="small"
      :placeholder="t('mods.groups.searchPlaceholder')"
      :aria-label="t('mods.groups.searchLabel')"
      data-testid="group-search"
    />
    <p
      v-if="nodes.length === 0"
      class="text-text-muted text-sm"
      data-testid="group-empty"
    >
      {{ t("mods.groups.empty") }}
    </p>
    <p
      v-else-if="hasNoMatch"
      class="text-text-muted text-sm"
      data-testid="group-no-match"
    >
      {{ t("mods.groups.noMatches") }}
    </p>
    <GroupTree
      ref="tree"
      v-model:scope="scope"
      v-model:expanded="expanded"
      :nodes="nodes"
      :shown="shown"
      :row-drag="rowDrag"
      @menu="openMenu"
      @rename="(id: number) => emit('renameGroup', id)"
      @delete="(id: number) => emit('deleteGroup', id)"
      @drop-mods="(id: number) => emit('dropMods', id)"
      @drop-group="(id: number, parent: number | null) => emit('dropGroup', id, parent)"
    />
    <Menu
      ref="groupMenu"
      popup
      :model="menuItems"
      data-testid="group-menu"
      @show="onMenuShown"
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
  </aside>
</template>
