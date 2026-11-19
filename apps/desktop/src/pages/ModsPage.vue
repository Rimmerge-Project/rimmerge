<script setup lang="ts">
import { useDebounceFn } from "@vueuse/core";
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import Tab from "primevue/tab";
import TabList from "primevue/tablist";
import Tabs from "primevue/tabs";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import ActivateDialog from "@/components/mods/ActivateDialog.vue";
import DeactivateDialog from "@/components/mods/DeactivateDialog.vue";
import DefSearchBox from "@/components/mods/DefSearchBox.vue";
import ModFilterBar from "@/components/mods/ModFilterBar.vue";
import ModInfoPanel from "@/components/mods/ModInfoPanel.vue";
import ModListPane from "@/components/mods/ModListPane.vue";
import ModsRescanButton from "@/components/mods/ModsRescanButton.vue";
import { useModActivation } from "@/composables/useModActivation";
import { useModDeactivation } from "@/composables/useModDeactivation";
import { useModListFilters } from "@/composables/useModListFilters";
import { useInactiveModsQuery, useWorkingModsQuery } from "@/queries/activeSet";

const { t } = useI18n();
const router = useRouter();
const route = useRoute();

const selectedModId = computed(() => {
  const param = route.params["modId"];
  return typeof param === "string" ? param : null;
});

function closePanel(): void {
  void router.push({ name: "mods" });
}

/** Which of the two tabs is showing. */
const tab = ref<"active" | "inactive">("active");

const { search, source, tag, activeQuery, inactiveQuery, isActiveFiltered, isInactiveFiltered } =
  useModListFilters();

const {
  data: activeData,
  isPending: activePending,
  error: activeError,
} = useWorkingModsQuery(activeQuery, () => tab.value === "active");

const {
  data: inactiveData,
  isPending: inactivePending,
  error: inactiveError,
} = useInactiveModsQuery(inactiveQuery, () => tab.value === "inactive");

const selectedActiveIds = ref<string[]>([]);
const selectedInactiveIds = ref<string[]>([]);
const withDependencies = ref(false);

const {
  plan: activatePlan,
  isDialogVisible: activateDialogVisible,
  error: activateError,
  isPlanning: planningActivate,
  isCommitting: activating,
  openDialog: openActivateDialog,
  confirm: confirmActivate,
} = useModActivation({ selectedIds: selectedInactiveIds, withDependencies });

const {
  plan: deactivatePlan,
  isDialogVisible: deactivateDialogVisible,
  error: deactivateError,
  isPlanning: planningDeactivate,
  isCommitting: deactivating,
  openDialog: openDeactivateDialog,
  confirm: confirmDeactivate,
} = useModDeactivation({ selectedIds: selectedActiveIds });

function openMod(modId: string): void {
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

      <Tabs
        :value="tab"
        class="shrink-0"
        @update:value="(value: string | number) => (tab = value === 'inactive' ? 'inactive' : 'active')"
      >
        <TabList>
          <Tab
            value="active"
            data-testid="mods-tab-active"
          >
            {{ t("mods.page.activeTab") }}
          </Tab>
          <Tab
            value="inactive"
            data-testid="mods-tab-inactive"
          >
            {{ t("mods.page.inactiveTab") }}
          </Tab>
        </TabList>
      </Tabs>

      <ModFilterBar
        v-model:search="search"
        v-model:tag="tag"
        v-model:source="source"
        :has-tag-filter="tab === 'active'"
      />

      <ModListPane
        v-if="tab === 'active'"
        v-model:selected-ids="selectedActiveIds"
        :mod-list="activeData"
        :is-pending="activePending"
        :error="activeError"
        :is-filtered="isActiveFiltered"
        @select-mod="openMod"
        @focus-mod="followFocus"
      >
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
      <ModListPane
        v-else
        v-model:selected-ids="selectedInactiveIds"
        :mod-list="inactiveData"
        :is-pending="inactivePending"
        :error="inactiveError"
        :is-filtered="isInactiveFiltered"
        :show-active-columns="false"
        @select-mod="openMod"
        @focus-mod="followFocus"
      >
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
    </section>

    <ModInfoPanel
      :mod-id="selectedModId"
      @close="closePanel"
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
  </div>
</template>
