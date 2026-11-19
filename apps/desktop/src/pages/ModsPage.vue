<script setup lang="ts">
import { useDebounceFn, useMediaQuery } from "@vueuse/core";
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import { computed, nextTick, ref, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import ActivateDialog from "@/components/mods/ActivateDialog.vue";
import DeactivateDialog from "@/components/mods/DeactivateDialog.vue";
import DefSearchBox from "@/components/mods/DefSearchBox.vue";
import FileChangeNote from "@/components/mods/FileChangeNote.vue";
import ModFilterBar from "@/components/mods/ModFilterBar.vue";
import ModInfoPanel from "@/components/mods/ModInfoPanel.vue";
import ModListPane from "@/components/mods/ModListPane.vue";
import ModPositionDialog from "@/components/mods/ModPositionDialog.vue";
import ModSortBar from "@/components/mods/ModSortBar.vue";
import ModsRescanButton from "@/components/mods/ModsRescanButton.vue";
import OrderSwitchedNote from "@/components/mods/OrderSwitchedNote.vue";
import OrderNotInFileNote from "@/components/order/OrderNotInFileNote.vue";
import { useModActivation } from "@/composables/useModActivation";
import { useModDeactivation } from "@/composables/useModDeactivation";
import { useModListFilters } from "@/composables/useModListFilters";
import { useModListSort } from "@/composables/useModListSort";
import { type ListKind, useModReorder } from "@/composables/useModReorder";
import { useOrderSource } from "@/composables/useOrderSource";
import { useInactiveModsQuery, useWorkingModsQuery } from "@/queries/activeSet";
import { ACTIVE_SORT_KEYS, INACTIVE_SORT_KEYS } from "@/utils/modListSort";

const { t } = useI18n();
const router = useRouter();
const route = useRoute();

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

const activeFilters = useModListFilters({ sort: activeSort, hasTag: true });
const inactiveFilters = useModListFilters({ sort: inactiveSort, hasTag: false });

const {
  data: activeData,
  isPending: activePending,
  error: activeError,
} = useWorkingModsQuery(activeFilters.query);

const {
  data: inactiveData,
  isPending: inactivePending,
  error: inactiveError,
} = useInactiveModsQuery(inactiveFilters.query);

const selectedActiveIds = ref<string[]>([]);
const selectedInactiveIds = ref<string[]>([]);
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
  deactivateDropped,
  confirm: confirmDeactivate,
} = useModDeactivation({ selectedIds: selectedActiveIds });

const reorder = useModReorder({
  activeList: activeData,
  inactiveList: inactiveData,
  isActiveFiltered: activeFilters.isFiltered,
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
    <section class="flex min-h-0 min-w-0 flex-1 flex-col gap-4">
      <div class="flex shrink-0 items-start justify-between gap-2">
        <h1 class="text-text text-xl font-semibold">
          {{ t("mods.page.heading") }}
        </h1>
        <div class="flex items-start gap-2">
          <ModsRescanButton />
          <DefSearchBox />
        </div>
      </div>

      <FileChangeNote />

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
          :is-pending="inactivePending"
          :error="inactiveError"
          :is-filtered="inactiveFilters.isFiltered.value"
          :sort-key="inactiveSort.key"
          :show-active-columns="false"
          draggable
          :dragging-ids="reorder.drag.value?.ids ?? []"
          :drop-mode="reorder.dropModeFor('inactive')"
          :refused-text="reorder.refusalFor('inactive')"
          row-menu="activate"
          @select-mod="(modId: string) => openMod(modId, 'inactive')"
          @focus-mod="followFocus"
          @drag-start="(modId: string) => reorder.startDrag('inactive', modId)"
          @drag-end="reorder.endDrag"
          @drop="(slot: number) => reorder.drop('inactive', slot)"
          @menu-action="reorder.menuAction"
        >
          <template #toolbar>
            <ModFilterBar
              v-model:search="inactiveFilters.search.value"
              v-model:tag="inactiveFilters.tag.value"
              v-model:source="inactiveFilters.source.value"
              :has-tag-filter="false"
              :search-label="t('mods.page.searchInactiveLabel')"
            />
            <ModSortBar
              v-model:sort="inactiveSort"
              :keys="INACTIVE_SORT_KEYS"
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
          :is-pending="activePending"
          :error="activeError"
          :is-filtered="activeFilters.isFiltered.value"
          :sort-key="activeSort.key"
          :show-position="activeData?.reorderable === false"
          draggable
          :dragging-ids="reorder.drag.value?.ids ?? []"
          :drop-mode="reorder.dropModeFor('active')"
          :refused-text="reorder.refusalFor('active')"
          row-menu="move"
          :reorder-blocked-reason="reorder.reorderBlockedReason.value"
          @select-mod="(modId: string) => openMod(modId, 'active')"
          @focus-mod="followFocus"
          @drag-start="(modId: string) => reorder.startDrag('active', modId)"
          @drag-end="reorder.endDrag"
          @drop="(slot: number) => reorder.drop('active', slot)"
          @move-step="reorder.step"
          @menu-action="reorder.menuAction"
        >
          <template #toolbar>
            <ModFilterBar
              v-model:search="activeFilters.search.value"
              v-model:tag="activeFilters.tag.value"
              v-model:source="activeFilters.source.value"
              :has-tag-filter="true"
              :search-label="t('mods.page.searchActiveLabel')"
            />
            <ModSortBar
              v-model:sort="activeSort"
              :keys="ACTIVE_SORT_KEYS"
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
      :is-overlay="!hasPanelColumn"
      @close="() => void closePanel()"
    />

    <ActivateDialog
      v-model:visible="activateDialogVisible"
      :plan="activatePlan"
      :error="activateError"
      :is-submitting="activating"
      @confirm="confirmActivate"
    />

    <DeactivateDialog
      v-model:visible="deactivateDialogVisible"
      :plan="deactivatePlan"
      :error="deactivateError"
      :is-submitting="deactivating"
      @confirm="confirmDeactivate"
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
