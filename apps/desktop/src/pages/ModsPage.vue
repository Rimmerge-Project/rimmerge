<script setup lang="ts">
import { useDebounceFn, useMediaQuery } from "@vueuse/core";
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import { useToast } from "primevue/usetoast";
import { type ComponentPublicInstance, computed, nextTick, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import ActivateDialog from "@/components/mods/ActivateDialog.vue";
import DeactivateDialog from "@/components/mods/DeactivateDialog.vue";
import DefSearchBox from "@/components/mods/DefSearchBox.vue";
import FileChangeNote from "@/components/mods/FileChangeNote.vue";
import GroupRail from "@/components/mods/GroupRail.vue";
import GroupScopeBar from "@/components/mods/GroupScopeBar.vue";
import ModFilterBar from "@/components/mods/ModFilterBar.vue";
import ModInfoPanel from "@/components/mods/ModInfoPanel.vue";
import ModListPane from "@/components/mods/ModListPane.vue";
import ModPositionDialog from "@/components/mods/ModPositionDialog.vue";
import ModSortBar from "@/components/mods/ModSortBar.vue";
import ModsRescanButton from "@/components/mods/ModsRescanButton.vue";
import ModViewSwitch from "@/components/mods/ModViewSwitch.vue";
import OrderSwitchedNote from "@/components/mods/OrderSwitchedNote.vue";
import BulkTagsDialog from "@/components/mods/organization/BulkTagsDialog.vue";
import OrganizationDialogs from "@/components/mods/organization/OrganizationDialogs.vue";
import OrganizationNotice from "@/components/mods/organization/OrganizationNotice.vue";
import OrderNotInFileNote from "@/components/order/OrderNotInFileNote.vue";
import { useGroupDrop } from "@/composables/useGroupDrop";
import { useGroupScope } from "@/composables/useGroupScope";
import { useGroupScopeActions } from "@/composables/useGroupScopeActions";
import { useGroupsView } from "@/composables/useGroupsView";
import { useModActivation } from "@/composables/useModActivation";
import { useModDeactivation } from "@/composables/useModDeactivation";
import { useModLabel } from "@/composables/useModLabel";
import { useModListFilters } from "@/composables/useModListFilters";
import { useModListSort } from "@/composables/useModListSort";
import { type ListKind, useModReorder } from "@/composables/useModReorder";
import { useOrderSource } from "@/composables/useOrderSource";
import { useOrganizationMutations } from "@/queries/organization";
import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import type { FolderKeyDto } from "@/types/generated/FolderKeyDto";
import type { ModListDto } from "@/types/generated/ModListDto";
import { assertNever } from "@/utils/assertNever";
import { ACTIVE_SORT_KEYS, INACTIVE_SORT_KEYS } from "@/utils/modListSort";
import type { FolderAction, MenuAction } from "@/utils/modRowMenu";
import { ALL_MODS, type BulkAction, type OrganizationDialog } from "@/utils/organization";
import { selectionSummary } from "@/utils/selectionText";

const { t } = useI18n();
const router = useRouter();
const route = useRoute();
const modLabel = useModLabel();

const selectedModId = computed(() => {
  const param = route.params["modId"];
  return typeof param === "string" ? param : null;
});

/** The row that opened the drawer, so closing it can hand keyboard focus back there. */
let drawerOrigin: { readonly from: ListKind; readonly modId: string } | null = null;

async function closePanel(): Promise<void> {
  const origin = drawerOrigin;
  drawerOrigin = null;
  await router.push({ name: "mods" });
  if (origin === null) {
    return;
  }
  await nextTick();
  await (origin.from === "active" ? activePane : inactivePane).value?.focusRow(origin.modId);
}

/**
 * Whether the mod info panel has a column of its own. Only a very wide window has the room: the
 * two lists share the width otherwise, so the panel floats over them as a drawer, and following
 * keyboard focus is off (an arrow key through a list must not pop a drawer over it).
 */
const hasPanelColumn = useMediaQuery("(min-width: 1800px)");

// The display sort is page-local state and only shapes the list queries; it never
// touches the load order or a pending change. Whether the Active list may be
// reordered is the backend's answer (`reorderable` on the list), never restated here.
const { activeSort, inactiveSort } = useModListSort();

// The group scope narrows both lists (a filter, never a reorder); the search runs inside it.
const groups = useGroupScope();
const activeFilters = useModListFilters({ sort: activeSort, group: groups.scope });
const inactiveFilters = useModListFilters({ sort: inactiveSort, group: groups.scope });

const selectedActiveIds = ref<string[]>([]);
const selectedInactiveIds = ref<string[]>([]);

// Each pane is a flat list or the Groups view; a view's own query is the only one that runs. The
// callbacks reach composables set up further down (read only when a menu choice is made).
const groupsView = useGroupsView({
  groups,
  queries: { active: activeFilters.query, inactive: inactiveFilters.query },
  selections: { active: selectedActiveIds, inactive: selectedInactiveIds },
  announce: (text) => reorder.announce(text),
  activateGroup: (groupId) => groupActions.activateGroup(groupId),
  deactivateGroup: (groupId) => groupActions.deactivateGroup(groupId),
});
const {
  data: activeData,
  grouped: activeGrouped,
  isPending: activePending,
  isPlaceholder: activePlaceholder,
  error: activeError,
} = groupsView.active;
const {
  data: inactiveData,
  grouped: inactiveGrouped,
  isPending: inactivePending,
  isPlaceholder: inactivePlaceholder,
  error: inactiveError,
} = groupsView.inactive;

// A list that fails because its group scope is gone drops the scope instead of showing an error.
watch(activeError, (failure) => groups.dropScopeIfStale(failure));
watch(inactiveError, (failure) => groups.dropScopeIfStale(failure));

/** The group panel: a column of its own on a very wide window, a left drawer otherwise. */
const hasGroupColumn = hasPanelColumn;
const isGroupDrawerOpen = ref(false);
const isGroupPanelShown = computed(
  () => groups.isUsable.value && (hasGroupColumn.value || isGroupDrawerOpen.value),
);
const groupsOpenButton = useTemplateRef<ComponentPublicInstance>("groupsOpenButton");
const groupRail = useTemplateRef<{ focusTree: () => Promise<void> }>("groupRail");

/**
 * Closes the group drawer and hands keyboard focus back to the button that opened it: the drawer
 * leaves the page (`v-if`), and focus on a removed element would fall to the document body.
 */
async function closeGroupDrawer(): Promise<void> {
  if (!isGroupDrawerOpen.value) {
    return;
  }
  isGroupDrawerOpen.value = false;
  await nextTick();
  const button = groupsOpenButton.value?.$el;
  if (button instanceof HTMLElement) {
    button.focus();
  }
}

// Picking a scope in the drawer answers the question it was opened for, so it closes.
watch(groups.scope, () => void closeGroupDrawer());

// A drawer left open must not outlive what it shows: groups turning unusable (a rescan that came
// back damaged) or the window growing wide enough for the column would otherwise leave it armed.
watch([groups.isUsable, hasGroupColumn], ([isUsable, hasColumn]) => {
  if (!isUsable || hasColumn) {
    isGroupDrawerOpen.value = false;
  }
});

const organizationDialog = ref<OrganizationDialog | null>(null);

/** The list a row is in, to read its groups and colour when its row menu asks for them. */
function rowIn(list: ModListDto | undefined, modId: string) {
  return list?.items.find((row) => row.modId === modId);
}

/** The shown row of the mod open in the info panel, for the colour and groups it lists. */
const selectedRow = computed(() => {
  const modId = selectedModId.value;
  return modId === null
    ? undefined
    : (rowIn(activeData.value, modId) ?? rowIn(inactiveData.value, modId));
});

/** The groups the open mod is a direct member of, for the groups panel to highlight. */
const openModGroupIds = computed<ReadonlySet<number>>(
  () => new Set(selectedRow.value?.groups.map((group) => group.id) ?? []),
);

/** A group chip or dot on a row: the same scope the groups panel sets. */
function scopeToGroup(groupId: number): void {
  groups.scope.value = { kind: "group", id: groupId };
}

const toast = useToast();
const organizationEdits = useOrganizationMutations();

/**
 * A swatch in a row menu: one click sets (or clears) the mod's own colour through the same
 * command as the "Colour..." dialog. A failure is already toasted by the app-wide mutation handler.
 */
async function setRowColor(modId: string, color: ColorTokenDto | null): Promise<void> {
  try {
    await organizationEdits.setModColors.mutateAsync({ modIds: [modId], color });
  } catch {
    return;
  }
  const name = modLabel.label(modId);
  toast.add({
    severity: "success",
    summary:
      color === null
        ? t("mods.colors.toast.modCleared", { name })
        : t("mods.colors.toast.modChanged", { name }),
    life: 4000,
  });
}

function onMenuAction(modId: string, action: MenuAction, from: ListKind): void {
  if (action !== "groups" && action !== "color") {
    reorder.menuAction(modId, action);
    return;
  }
  const row = rowIn(from === "active" ? activeData.value : inactiveData.value, modId);
  if (row === undefined) {
    return;
  }
  organizationDialog.value =
    action === "groups"
      ? { kind: "modGroups", modId, groupIds: row.groups.map((group) => group.id) }
      : { kind: "modColor", modId, current: row.color };
}

const withDependencies = ref(false);
const { selected: selectedOrder } = useOrderSource();
const isSwitchNoteShown = ref(false);

const activePane = useTemplateRef<{ focusRow: (modId: string) => Promise<void> }>("activePane");
const inactivePane = useTemplateRef<{ focusRow: (modId: string) => Promise<void> }>("inactivePane");

const {
  plan: activatePlan,
  isDialogVisible: activateDialogVisible,
  error: activateError,
  isPlanning: planningActivate,
  isCommitting: activating,
  openDialog: openActivateDialog,
  openForIds: openActivateFor,
  activateAt,
  confirm: confirmActivate,
} = useModActivation({
  selectedIds: selectedInactiveIds,
  withDependencies,
  onSwitchedToCurrent: () => {
    isSwitchNoteShown.value = true;
  },
});

const {
  plan: deactivatePlan,
  isDialogVisible: deactivateDialogVisible,
  error: deactivateError,
  isPlanning: planningDeactivate,
  isCommitting: deactivating,
  openDialog: openDeactivateDialog,
  openForIds: openDeactivateFor,
  deactivateDropped,
  confirm: confirmDeactivate,
} = useModDeactivation({ selectedIds: selectedActiveIds });

const shownIds = (list: ModListDto | undefined): ReadonlySet<string> =>
  new Set(list?.items.map((row) => row.modId));

const groupActions = useGroupScopeActions({
  openActivate: openActivateFor,
  openDeactivate: openDeactivateFor,
  shownInactiveIds: () => shownIds(inactiveData.value),
  shownActiveIds: () => shownIds(activeData.value),
});

// A group's scope lines belong to the one dialog it opened: any later dialog (the selection
// button, a drop) starts without them.
watch(activateDialogVisible, (isVisible) => {
  if (!isVisible) {
    groupActions.clearScope("activate");
  }
});
watch(deactivateDialogVisible, (isVisible) => {
  if (!isVisible) {
    groupActions.clearScope("deactivate");
  }
});

// Bulk organization and tag actions act on one list's whole selection, hidden-by-filter mods
// included (each list keeps its own selection). They open dialogs and never touch the selection.
// The selection stays after the dialog closes so its text does not blank during the leave
// animation; the next open replaces it.
const bulkTags = ref<{ readonly modIds: readonly string[]; readonly hiddenCount: number } | null>(
  null,
);
const isBulkTagsVisible = ref(false);
const bulkTagsText = computed(() =>
  bulkTags.value === null
    ? ""
    : selectionSummary(t, bulkTags.value.modIds.length, bulkTags.value.hiddenCount),
);

function onBulkAction(from: ListKind, action: BulkAction): void {
  const modIds = [
    ...new Set(from === "active" ? selectedActiveIds.value : selectedInactiveIds.value),
  ];
  const shown = shownIds(from === "active" ? activeData.value : inactiveData.value);
  const hiddenCount = modIds.filter((id) => !shown.has(id)).length;
  switch (action) {
    case "addToGroup":
      organizationDialog.value = { kind: "bulkGroups", mode: "add", modIds, hiddenCount };
      return;
    case "removeFromGroup":
      organizationDialog.value = { kind: "bulkGroups", mode: "remove", modIds, hiddenCount };
      return;
    case "moveToGroup":
      organizationDialog.value = { kind: "bulkGroups", mode: "move", modIds, hiddenCount };
      return;
    case "color":
      organizationDialog.value = { kind: "bulkColor", modIds, hiddenCount };
      return;
    case "tags":
      bulkTags.value = { modIds, hiddenCount };
      isBulkTagsVisible.value = true;
      return;
    default:
      assertNever(action);
  }
}

const reorder = useModReorder({
  activeList: activeData,
  inactiveList: inactiveData,
  isActiveFiltered: activeFilters.isFiltered,
  isActiveGrouped: () => groupsView.isActiveGrouped.value,
  selectedActiveIds,
  selectedInactiveIds,
  activation: { activateAt },
  deactivation: { deactivateDropped },
  focusActiveRow: async (modId) => {
    await nextTick();
    await activePane.value?.focusRow(modId);
  },
  onSwitchedToCurrent: () => {
    isSwitchNoteShown.value = true;
  },
});

// Mods dropped on a group join it and a group dropped on a group nests; both are organization
// edits only (D14: a drop never replaces a membership).
const groupDrop = useGroupDrop({
  nodes: groups.nodes,
  drag: reorder.drag,
  endDrag: reorder.endDrag,
  announce: reorder.announce,
});

/**
 * Mods dropped on a group, on its rail entry or on a Groups view header. Not awaited on purpose:
 * `dropMods` never rejects (a failed edit is toasted by the app-wide mutation handler, the result
 * is announced) and nothing here waits for it.
 */
function dropMods(groupId: number): void {
  void groupDrop.dropMods(groupId);
}

/** The tree's view of a drag from a list: how many mods it carries, `null` when none. */
const rowDrag = computed(() =>
  reorder.drag.value === null
    ? null
    : { count: reorder.drag.value.ids.length, hiddenCount: reorder.drag.value.hiddenCount },
);

const isPositionDialogVisible = computed({
  get: () => reorder.positionRequest.value !== null,
  set: (isVisible) => {
    if (!isVisible) {
      reorder.positionRequest.value = null;
    }
  },
});

function openMod(modId: string, from: ListKind): void {
  // Only the drawer covers the lists: a panel with a column of its own leaves focus on the row.
  drawerOrigin = hasPanelColumn.value ? null : { from, modId };
  void router.push({ name: "mod-detail", params: { modId } });
}

/**
 * The panel follows keyboard focus, but only after 150ms of it settling
 * — holding an arrow key through a long list must not fire a
 * `get_mod_info`/`read_mod_preview` request per row. `router.replace`,
 * not `push`: an auto-follow while arrowing through rows isn't a
 * "selection" the back button should have to step through one row at a
 * time — only {@link openMod}'s own immediate Enter/click navigation is.
 *
 * The `route.name` check guards against a stale timer firing after the
 * user has already navigated elsewhere from *inside* the panel (the
 * "Findings"/"All details"/"Why here?" links, all in a child component
 * with no handle on this debounce to cancel directly) — a plain click
 * on a row focuses it natively *and* fires this same debounce, so
 * without the guard a 150ms-old timer can silently replace an explicit,
 * just-clicked navigation back to this mod's own route.
 */
const followFocus = useDebounceFn((modId: string) => {
  if (!hasPanelColumn.value) {
    return;
  }
  if (route.name !== "mods" && route.name !== "mod-detail") {
    return;
  }
  void router.replace({ name: "mod-detail", params: { modId } });
}, 150);
</script>

<template>
  <div class="flex h-full min-h-0 gap-4 p-4">
    <GroupRail
      v-if="isGroupPanelShown"
      ref="groupRail"
      v-model:scope="groups.scope.value"
      :nodes="groups.nodes.value"
      :is-overlay="!hasGroupColumn"
      :row-drag="rowDrag"
      :open-mod-group-ids="openModGroupIds"
      @create-group="(parent: number | null) => (organizationDialog = { kind: 'createGroup', parent })"
      @rename-group="(id: number) => (organizationDialog = { kind: 'renameGroup', id })"
      @change-color="(id: number) => (organizationDialog = { kind: 'groupColor', id })"
      @move-group="(id: number) => (organizationDialog = { kind: 'moveGroup', id })"
      @delete-group="(id: number) => (organizationDialog = { kind: 'deleteGroup', id })"
      @remove-uninstalled="(id: number) => (organizationDialog = { kind: 'removeUninstalled', id })"
      @copy-from-profile="organizationDialog = { kind: 'copyProfile' }"
      @activate-group="(id: number) => void groupActions.activateGroup(id)"
      @deactivate-group="(id: number) => void groupActions.deactivateGroup(id)"
      @drop-mods="dropMods"
      @drop-group="(id: number, parent: number | null) => void groupDrop.dropGroup(id, parent)"
      @close="() => void closeGroupDrawer()"
    />
    <section class="flex min-h-0 min-w-0 flex-1 flex-col gap-4">
      <div class="flex shrink-0 items-start justify-between gap-2">
        <h1 class="text-text text-xl font-semibold">
          {{ t("mods.page.heading") }}
        </h1>
        <div class="flex items-start gap-2">
          <Button
            v-if="groups.isUsable.value && !hasGroupColumn"
            ref="groupsOpenButton"
            :label="t('mods.groups.openButton')"
            icon="pi pi-sitemap"
            severity="secondary"
            outlined
            size="small"
            aria-controls="group-rail"
            :aria-expanded="isGroupDrawerOpen"
            data-testid="groups-open"
            @click="isGroupDrawerOpen = true"
          />
          <ModsRescanButton />
          <DefSearchBox />
        </div>
      </div>

      <FileChangeNote />

      <OrganizationNotice
        :organization="groups.organization.value"
        @start-over="organizationDialog = { kind: 'startOver' }"
      />
      <GroupScopeBar
        :scope="groups.scope.value"
        :nodes="groups.nodes.value"
        @clear="groups.scope.value = ALL_MODS"
      />

      <!-- A move settles into Current without writing the file: say so, with the Apply path. -->
      <OrderNotInFileNote allow-revert />
      <OrderSwitchedNote v-model:visible="isSwitchNoteShown" />

      <div
        class="grid min-h-0 flex-1 grid-cols-1 grid-rows-2 gap-4 lg:grid-cols-2 lg:grid-rows-1"
        data-testid="mods-lists"
      >
        <ModListPane
          ref="inactivePane"
          v-model:selected-ids="selectedInactiveIds"
          kind="inactive"
          :mod-list="inactiveData"
          :grouped="inactiveGrouped"
          :is-pending="inactivePending"
          :is-placeholder="inactivePlaceholder"
          :error="inactiveError"
          :is-filtered="inactiveFilters.isFiltered.value"
          :sort-key="inactiveSort.key"
          :show-active-columns="false"
          :open-mod-id="selectedModId"
          draggable
          :dragging-ids="reorder.drag.value?.ids ?? []"
          :drop-mode="reorder.dropModeFor('inactive')"
          :refused-text="reorder.refusalFor('inactive')"
          row-menu="activate"
          :has-organization-actions="groups.isUsable.value"
          @select-mod="(modId: string) => openMod(modId, 'inactive')"
          @focus-mod="followFocus"
          @drag-start="(modId: string) => reorder.startDrag('inactive', modId)"
          @drag-end="reorder.endDrag"
          @drop="(slot: number) => reorder.drop('inactive', slot)"
          @menu-action="(modId: string, action: MenuAction) => onMenuAction(modId, action, 'inactive')"
          @toggle-active="(modId: string) => openActivateFor([modId])"
          @set-color="setRowColor"
          @scope-group="scopeToGroup"
          @bulk-action="(action: BulkAction) => onBulkAction('inactive', action)"
          @set-folded="(folder: FolderKeyDto, isCollapsed: boolean) => groupsView.setFolded('inactive', folder, isCollapsed)"
          @drop-on-folder="dropMods"
          @folder-action="(folder: FolderKeyDto, action: FolderAction) => groupsView.onFolderAction('inactive', folder, action)"
        >
          <template #toolbar>
            <ModFilterBar
              v-model:search="inactiveFilters.search.value"
              v-model:tag="inactiveFilters.tag.value"
              v-model:source="inactiveFilters.source.value"
              :search-label="t('mods.page.searchInactiveLabel')"
            />
            <ModSortBar
              v-model:sort="inactiveSort"
              :keys="INACTIVE_SORT_KEYS"
            />
            <ModViewSwitch
              v-model:view="groupsView.inactiveView.value"
              :is-groups-available="groups.isUsable.value"
              @collapse-all="groupsView.collapseAll('inactive')"
              @expand-all="groupsView.expandAll('inactive')"
            />
          </template>
          <template #actions>
            <label class="flex items-center gap-1.5 text-sm">
              <Checkbox
                v-model="withDependencies"
                binary
                data-testid="activate-with-dependencies-checkbox"
              />
              {{ t("mods.page.alsoActivateDependenciesLabel") }}
            </label>
            <Button
              :label="t('mods.page.activateSelectedButton')"
              size="small"
              :disabled="selectedInactiveIds.length === 0"
              :loading="planningActivate"
              data-testid="activate-selected-button"
              @click="openActivateDialog"
            />
          </template>
        </ModListPane>

        <ModListPane
          ref="activePane"
          v-model:selected-ids="selectedActiveIds"
          kind="active"
          :mod-list="activeData"
          :grouped="activeGrouped"
          :is-pending="activePending"
          :is-placeholder="activePlaceholder"
          :error="activeError"
          :is-filtered="activeFilters.isFiltered.value"
          :sort-key="activeSort.key"
          show-position
          :open-mod-id="selectedModId"
          draggable
          :dragging-ids="reorder.drag.value?.ids ?? []"
          :drop-mode="reorder.dropModeFor('active')"
          :refused-text="reorder.refusalFor('active')"
          row-menu="move"
          :has-organization-actions="groups.isUsable.value"
          :reorder-blocked-reason="reorder.reorderBlockedReason.value"
          @select-mod="(modId: string) => openMod(modId, 'active')"
          @focus-mod="followFocus"
          @drag-start="(modId: string) => reorder.startDrag('active', modId)"
          @drag-end="reorder.endDrag"
          @drop="(slot: number) => reorder.drop('active', slot)"
          @move-step="reorder.step"
          @menu-action="(modId: string, action: MenuAction) => onMenuAction(modId, action, 'active')"
          @toggle-active="(modId: string) => openDeactivateFor([modId])"
          @set-color="setRowColor"
          @scope-group="scopeToGroup"
          @bulk-action="(action: BulkAction) => onBulkAction('active', action)"
          @set-folded="(folder: FolderKeyDto, isCollapsed: boolean) => groupsView.setFolded('active', folder, isCollapsed)"
          @drop-on-folder="dropMods"
          @folder-action="(folder: FolderKeyDto, action: FolderAction) => groupsView.onFolderAction('active', folder, action)"
        >
          <template #toolbar>
            <ModFilterBar
              v-model:search="activeFilters.search.value"
              v-model:tag="activeFilters.tag.value"
              v-model:source="activeFilters.source.value"
              :search-label="t('mods.page.searchActiveLabel')"
            />
            <ModSortBar
              v-model:sort="activeSort"
              :keys="ACTIVE_SORT_KEYS"
            />
            <ModViewSwitch
              v-model:view="groupsView.activeView.value"
              :is-groups-available="groups.isUsable.value"
              @collapse-all="groupsView.collapseAll('active')"
              @expand-all="groupsView.expandAll('active')"
            />
          </template>
          <template #notes>
            <p
              v-if="reorder.reorderBlockedReason.value !== null"
              class="text-text-muted shrink-0 text-xs"
              data-testid="mods-reorder-blocked"
            >
              {{ reorder.reorderBlockedReason.value }}
            </p>
            <p
              v-else-if="selectedOrder === 'suggested'"
              class="text-text-muted shrink-0 text-xs"
              data-testid="mods-suggested-note"
            >
              {{ t("mods.move.suggestedNote") }}
            </p>
          </template>
          <template #actions>
            <Button
              :label="t('mods.page.deactivateSelectedButton')"
              severity="danger"
              outlined
              size="small"
              :disabled="selectedActiveIds.length === 0"
              :loading="planningDeactivate"
              data-testid="deactivate-selected-button"
              @click="openDeactivateDialog"
            />
          </template>
        </ModListPane>
      </div>
    </section>

    <ModInfoPanel
      :mod-id="selectedModId"
      :color="selectedRow?.color ?? null"
      :groups="selectedRow?.groups ?? []"
      :is-overlay="!hasPanelColumn"
      @close="() => void closePanel()"
    />

    <ActivateDialog
      v-model:visible="activateDialogVisible"
      :plan="activatePlan"
      :scope-lines="groupActions.activateLines.value"
      :error="activateError"
      :is-submitting="activating"
      @confirm="confirmActivate"
    />

    <DeactivateDialog
      v-model:visible="deactivateDialogVisible"
      :plan="deactivatePlan"
      :scope-lines="groupActions.deactivateLines.value"
      :error="deactivateError"
      :is-submitting="deactivating"
      @confirm="confirmDeactivate"
    />

    <OrganizationDialogs
      v-model:dialog="organizationDialog"
      :nodes="groups.nodes.value"
      @group-deleted="() => void groupRail?.focusTree()"
    />

    <BulkTagsDialog
      v-model:visible="isBulkTagsVisible"
      :mod-ids="bulkTags?.modIds ?? []"
      :selection-text="bulkTagsText"
    />

    <ModPositionDialog
      v-model:visible="isPositionDialogVisible"
      :mode="reorder.positionRequest.value?.mode ?? 'move'"
      :exclude-ids="reorder.positionRequest.value?.ids ?? []"
      @choose="reorder.positionChosen"
    />

    <!-- One live region for every move, always mounted so a screen reader announces its changes. -->
    <p
      class="sr-only"
      role="status"
      aria-live="polite"
      data-testid="mods-move-status"
    >
      {{ reorder.announcement.value }}
    </p>
  </div>
</template>
