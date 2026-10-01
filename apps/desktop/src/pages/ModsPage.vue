<script setup lang="ts">
import { refDebounced, useDebounceFn } from "@vueuse/core";
import Button from "primevue/button";
import Checkbox from "primevue/checkbox";
import Dialog from "primevue/dialog";
import InputText from "primevue/inputtext";
import Message from "primevue/message";
import Select from "primevue/select";
import Tab from "primevue/tab";
import TabList from "primevue/tablist";
import Tabs from "primevue/tabs";
import { useToast } from "primevue/usetoast";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import DefSearchBox from "@/components/mods/DefSearchBox.vue";
import ModInfoPanel from "@/components/mods/ModInfoPanel.vue";
import ModTable from "@/components/mods/ModTable.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import {
  useActivateModsMutation,
  useDeactivateModsMutation,
  useInactiveModsQuery,
  usePlanActivateModsMutation,
  usePlanDeactivateModsMutation,
} from "@/queries/activeSet";
import { useModsQuery } from "@/queries/mods";
import { asRecord } from "@/types/dtoMaps";
import type { ActivatePlanDto } from "@/types/generated/ActivatePlanDto";
import type { DeactivatePlanDto } from "@/types/generated/DeactivatePlanDto";
import type { SourceDto } from "@/types/generated/SourceDto";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";
import { sourceLabel } from "@/utils/source";

const { t, locale } = useI18n();
const tm = useTranslateMessage();
const router = useRouter();
const route = useRoute();
const modLabel = useModLabel();
const toast = useToast();

const selectedModId = computed(() => {
  const param = route.params["modId"];
  return typeof param === "string" ? param : null;
});

function closePanel(): void {
  void router.push({ name: "mods" });
}

/** Which of the two tabs is showing. */
const tab = ref<"active" | "inactive">("active");

// The input stays responsive on every keystroke; only the value that
// actually drives the query is debounced, so typing a search term
// doesn't fire a `list_mods`/`list_inactive_mods` query per character.
const search = ref("");
const debouncedSearch = refDebounced(search, 300);
const source = ref<SourceDto | null>(null);
const tag = ref("");

const SOURCE_VALUES: readonly SourceDto[] = ["core", "dlc", "local", "workshop"];
/** `sourceLabel`'s own descriptors, resolved through `t()` reactively — plus the "Any" option, which isn't a real `SourceDto`. */
const sourceOptions = computed(() => [
  { label: t("mods.page.sourceAny"), value: null },
  ...SOURCE_VALUES.map((value) => ({ label: tm(sourceLabel(value)), value })),
]);

const {
  data: activeData,
  isPending: activePending,
  error: activeError,
} = useModsQuery(
  () => ({
    search: debouncedSearch.value.length > 0 ? debouncedSearch.value : null,
    tag: tag.value.length > 0 ? tag.value : null,
    source: source.value,
    offset: 0,
    limit: 200,
  }),
  () => tab.value === "active",
);

const {
  data: inactiveData,
  isPending: inactivePending,
  error: inactiveError,
} = useInactiveModsQuery(
  () => ({
    search: debouncedSearch.value.length > 0 ? debouncedSearch.value : null,
    tag: null,
    source: source.value,
    offset: 0,
    limit: 200,
  }),
  () => tab.value === "inactive",
);

const selectedActiveIds = ref<string[]>([]);
const selectedInactiveIds = ref<string[]>([]);
const withDependencies = ref(false);

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

// -- Activation --

const { mutateAsync: planActivate, isLoading: planningActivate } = usePlanActivateModsMutation();
const { mutateAsync: commitActivate, isLoading: activating } = useActivateModsMutation();
const activatePlan = ref<ActivatePlanDto | null>(null);
const activateDialogVisible = ref(false);
const activateError = ref<CommandErrorDescriptor | null>(null);

async function openActivateDialog(): Promise<void> {
  activateError.value = null;
  try {
    activatePlan.value = await planActivate({
      ids: selectedInactiveIds.value,
      withDependencies: withDependencies.value,
    });
    activateDialogVisible.value = true;
  } catch (error: unknown) {
    activateError.value = describeCommandError(error);
  }
}

async function confirmActivate(): Promise<void> {
  activateError.value = null;
  try {
    await commitActivate({
      ids: selectedInactiveIds.value,
      withDependencies: withDependencies.value,
    });
    activateDialogVisible.value = false;
    selectedInactiveIds.value = [];
    toast.add({ severity: "success", summary: t("mods.page.activatedToast"), life: 4000 });
  } catch (error: unknown) {
    activateError.value = describeCommandError(error);
  }
}

// -- Deactivation --

const { mutateAsync: planDeactivate, isLoading: planningDeactivate } =
  usePlanDeactivateModsMutation();
const { mutateAsync: commitDeactivate, isLoading: deactivating } = useDeactivateModsMutation();
const deactivatePlan = ref<DeactivatePlanDto | null>(null);
const deactivateDialogVisible = ref(false);
const deactivateError = ref<CommandErrorDescriptor | null>(null);

async function openDeactivateDialog(): Promise<void> {
  deactivateError.value = null;
  try {
    deactivatePlan.value = await planDeactivate({ ids: selectedActiveIds.value });
    deactivateDialogVisible.value = true;
  } catch (error: unknown) {
    deactivateError.value = describeCommandError(error);
  }
}

async function confirmDeactivate(): Promise<void> {
  deactivateError.value = null;
  try {
    await commitDeactivate({ ids: selectedActiveIds.value });
    deactivateDialogVisible.value = false;
    selectedActiveIds.value = [];
    toast.add({ severity: "success", summary: t("mods.page.deactivatedToast"), life: 4000 });
  } catch (error: unknown) {
    deactivateError.value = describeCommandError(error);
  }
}
</script>

<template>
  <div class="flex h-full min-h-0 gap-4 p-4">
    <section class="flex min-h-0 min-w-0 flex-1 flex-col gap-4">
      <div class="flex shrink-0 items-center justify-between gap-2">
        <h1 class="text-text text-xl font-semibold">
          {{ t("mods.page.heading") }}
        </h1>
        <DefSearchBox />
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

      <div class="flex shrink-0 flex-wrap items-center gap-2">
        <InputText
          v-model="search"
          :placeholder="t('mods.page.searchPlaceholder')"
          data-testid="mods-search"
        />
        <InputText
          v-if="tab === 'active'"
          v-model="tag"
          :placeholder="t('mods.page.tagFilterPlaceholder')"
          data-testid="mods-tag-filter"
        />
        <Select
          v-model="source"
          :options="sourceOptions"
          option-label="label"
          option-value="value"
          :placeholder="t('mods.page.sourcePlaceholder')"
          data-testid="mods-source-filter"
        />

        <div class="ml-auto flex items-center gap-2">
          <template v-if="tab === 'active'">
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
          <template v-else>
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
        </div>
      </div>

      <template v-if="tab === 'active'">
        <p
          v-if="activePending"
          class="text-text-muted text-sm"
          data-testid="mods-loading"
        >
          {{ t("common.loading") }}
        </p>
        <p
          v-else-if="activeError"
          class="text-status-danger text-sm"
          data-testid="mods-error"
        >
          {{ activeError instanceof Error ? activeError.message : t("mods.page.loadFailed") }}
        </p>
        <template v-else-if="activeData">
          <p
            class="text-text-muted shrink-0 text-sm"
            data-testid="mods-total"
          >
            {{ t("mods.page.totalMods", { count: activeData.total }, activeData.total) }}
            <span
              v-if="activeData.total > activeData.items.length"
              class="text-text-faint"
              data-testid="mods-page-cap-hint"
            >{{ t("mods.page.pageCapHint", { count: activeData.items.length }) }}</span>
          </p>
          <ModTable
            :mods="activeData.items"
            selectable
            :selected-ids="selectedActiveIds"
            @update:selected-ids="(ids: string[]) => (selectedActiveIds = ids)"
            @select-mod="openMod"
            @focus-mod="followFocus"
          />
        </template>
      </template>
      <template v-else>
        <p
          v-if="inactivePending"
          class="text-text-muted text-sm"
          data-testid="mods-loading"
        >
          {{ t("common.loading") }}
        </p>
        <p
          v-else-if="inactiveError"
          class="text-status-danger text-sm"
          data-testid="mods-error"
        >
          {{ inactiveError instanceof Error ? inactiveError.message : t("mods.page.loadFailed") }}
        </p>
        <template v-else-if="inactiveData">
          <p
            class="text-text-muted shrink-0 text-sm"
            data-testid="mods-total"
          >
            {{ t("mods.page.totalMods", { count: inactiveData.total }, inactiveData.total) }}
            <span
              v-if="inactiveData.total > inactiveData.items.length"
              class="text-text-faint"
              data-testid="mods-page-cap-hint"
            >{{ t("mods.page.pageCapHint", { count: inactiveData.items.length }) }}</span>
          </p>
          <ModTable
            :mods="inactiveData.items"
            selectable
            :show-active-columns="false"
            :selected-ids="selectedInactiveIds"
            @update:selected-ids="(ids: string[]) => (selectedInactiveIds = ids)"
            @select-mod="openMod"
            @focus-mod="followFocus"
          />
        </template>
      </template>
    </section>

    <ModInfoPanel
      :mod-id="selectedModId"
      @close="closePanel"
    />

    <Dialog
      v-model:visible="activateDialogVisible"
      modal
      :header="t('mods.page.activateDialogHeader')"
      data-testid="activate-confirm-dialog"
    >
      <div
        v-if="activatePlan"
        class="flex w-96 flex-col gap-3 text-sm"
      >
        <div v-if="activatePlan.toAdd.length > 0">
          <p class="text-text-muted">
            {{ t("mods.page.willActivateHeading") }}
          </p>
          <ul class="list-inside list-disc">
            <li
              v-for="id in activatePlan.toAdd"
              :key="id"
              data-testid="activate-plan-to-add"
            >
              {{ modLabel.label(id) }}
            </li>
          </ul>
        </div>
        <div v-if="activatePlan.alreadyActive.length > 0">
          <p class="text-text-muted">
            {{ t("mods.page.alreadyActiveHeading") }}
          </p>
          <ul class="list-inside list-disc">
            <li
              v-for="id in activatePlan.alreadyActive"
              :key="id"
            >
              {{ modLabel.label(id) }}
            </li>
          </ul>
        </div>
        <Message
          v-for="(deps, id) in asRecord(activatePlan.unresolvableDependencies)"
          :key="id"
          severity="warn"
          data-testid="activate-plan-unresolvable"
        >
          {{
            t("mods.page.unresolvableDependency", {
              mod: modLabel.label(id),
              deps: formatList(
                locale,
                deps.map((dep) => modLabel.label(dep)),
              ),
              count: deps.length,
            }, deps.length)
          }}
        </Message>
        <Message
          v-if="activateError"
          severity="error"
          data-testid="activate-confirm-error"
        >
          <div class="font-medium">
            {{ tm(activateError.title) }}
          </div>
          <div>{{ tm(activateError.detail) }}</div>
          <div
            v-if="activateError.technicalDetail"
            class="text-xs opacity-75"
          >
            {{ activateError.technicalDetail }}
          </div>
        </Message>
      </div>
      <template #footer>
        <Button
          :label="t('common.cancel')"
          severity="secondary"
          data-testid="activate-confirm-cancel"
          @click="activateDialogVisible = false"
        />
        <Button
          :label="t('mods.page.activateButton')"
          :loading="activating"
          data-testid="activate-confirm-submit"
          @click="confirmActivate"
        />
      </template>
    </Dialog>

    <Dialog
      v-model:visible="deactivateDialogVisible"
      modal
      :header="t('mods.page.deactivateDialogHeader')"
      data-testid="deactivate-confirm-dialog"
    >
      <div
        v-if="deactivatePlan"
        class="flex w-96 flex-col gap-3 text-sm"
      >
        <div v-if="deactivatePlan.toRemove.length > 0">
          <p class="text-text-muted">
            {{ t("mods.page.willDeactivateHeading") }}
          </p>
          <ul class="list-inside list-disc">
            <li
              v-for="id in deactivatePlan.toRemove"
              :key="id"
              data-testid="deactivate-plan-to-remove"
            >
              {{ modLabel.label(id) }}
            </li>
          </ul>
        </div>
        <div v-if="deactivatePlan.refused.length > 0">
          <p class="text-text-muted">
            {{ t("mods.page.refusedHeading") }}
          </p>
          <ul class="list-inside list-disc">
            <li
              v-for="id in deactivatePlan.refused"
              :key="id"
            >
              {{ modLabel.label(id) }}
            </li>
          </ul>
        </div>
        <Message
          v-for="(dependents, id) in asRecord(deactivatePlan.dependentsStillActive)"
          :key="id"
          severity="warn"
          data-testid="deactivate-plan-dependents"
        >
          {{
            t(
              "mods.page.dependentsStillActive",
              {
                count: dependents.length,
                deps: formatList(
                  locale,
                  dependents.map((dep) => modLabel.label(dep)),
                ),
                mod: modLabel.label(id),
              },
              dependents.length,
            )
          }}
        </Message>
        <Message
          v-if="deactivateError"
          severity="error"
          data-testid="deactivate-confirm-error"
        >
          <div class="font-medium">
            {{ tm(deactivateError.title) }}
          </div>
          <div>{{ tm(deactivateError.detail) }}</div>
          <div
            v-if="deactivateError.technicalDetail"
            class="text-xs opacity-75"
          >
            {{ deactivateError.technicalDetail }}
          </div>
        </Message>
      </div>
      <template #footer>
        <Button
          :label="t('common.cancel')"
          severity="secondary"
          data-testid="deactivate-confirm-cancel"
          @click="deactivateDialogVisible = false"
        />
        <Button
          :label="t('mods.page.deactivateButton')"
          severity="danger"
          :loading="deactivating"
          data-testid="deactivate-confirm-submit"
          @click="confirmDeactivate"
        />
      </template>
    </Dialog>
  </div>
</template>
