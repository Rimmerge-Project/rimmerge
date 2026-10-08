<script setup lang="ts">
// The Load order page header's Export and Import menus, and the dialogs they lead to.
// Export writes the order in ModsConfig.xml (what RimWorld loads), not the selected order.
import Button from "primevue/button";
import Menu, { type MenuMethods } from "primevue/menu";
import type { MenuItem } from "primevue/menuitem";
import { computed, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import ImportPreviewDialog from "@/components/order/ImportPreviewDialog.vue";
import PasteOrderDialog from "@/components/order/PasteOrderDialog.vue";
import { useOrderShare } from "@/composables/useOrderShare";
import { useDashboardQuery } from "@/queries/dashboard";

const { t } = useI18n();
const {
  isBusy,
  isImporting,
  isPasteOpen,
  isPreviewOpen,
  outcome,
  saveAsFile,
  copyAsText,
  openFile,
  openPaste,
  previewPasted,
  confirmImport,
  openWorkshop,
  copyMissing,
} = useOrderShare();
const { data: dashboard } = useDashboardQuery();

const exportMenu = useTemplateRef<MenuMethods>("exportMenu");
const importMenu = useTemplateRef<MenuMethods>("importMenu");

function toggleExport(event: Event): void {
  exportMenu.value?.toggle(event);
}

function toggleImport(event: Event): void {
  importMenu.value?.toggle(event);
}

// Unknown until the dashboard answers: no claim is made about an order we cannot see.
const replacesUnwritten = computed(() => dashboard.value?.fileMatchesCurrent === false);

// `testId` rides along on the item (a MenuItem accepts extra keys) for the item template.
const exportItems = computed<MenuItem[]>(() => [
  {
    label: t("orderShare.export.saveFile"),
    testId: "order-export-save",
    command: () => void saveAsFile(),
  },
  {
    label: t("orderShare.export.copyText"),
    testId: "order-export-copy",
    command: () => void copyAsText(),
  },
]);

const importItems = computed<MenuItem[]>(() => [
  {
    label: t("orderShare.import.openFile"),
    testId: "order-import-file",
    command: () => void openFile(),
  },
  {
    label: t("orderShare.import.paste"),
    testId: "order-import-paste",
    command: openPaste,
  },
]);
</script>

<template>
  <div
    class="flex shrink-0 items-center gap-2"
    data-testid="order-share-menus"
  >
    <Button
      :label="t('orderShare.export.button')"
      severity="secondary"
      size="small"
      icon="pi pi-chevron-down"
      icon-pos="right"
      :disabled="isBusy"
      aria-haspopup="menu"
      data-testid="order-export-button"
      @click="toggleExport"
    />
    <Menu
      ref="exportMenu"
      popup
      :model="exportItems"
      data-testid="order-export-menu"
    >
      <template #item="{ item, props }">
        <a
          v-bind="props.action"
          :data-testid="item['testId']"
        >
          {{ item.label }}
        </a>
      </template>
      <template #end>
        <p
          class="text-text-muted max-w-64 px-3 py-2 text-xs"
          data-testid="order-export-hint"
        >
          {{ t("orderShare.export.hint") }}
        </p>
      </template>
    </Menu>

    <Button
      :label="t('orderShare.import.button')"
      severity="secondary"
      size="small"
      icon="pi pi-chevron-down"
      icon-pos="right"
      :disabled="isBusy"
      aria-haspopup="menu"
      data-testid="order-import-button"
      @click="toggleImport"
    />
    <Menu
      ref="importMenu"
      popup
      :model="importItems"
      data-testid="order-import-menu"
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

    <PasteOrderDialog
      v-model:visible="isPasteOpen"
      :is-busy="isBusy"
      @preview="previewPasted"
    />
    <ImportPreviewDialog
      v-model:visible="isPreviewOpen"
      :outcome="outcome"
      :is-importing="isImporting"
      :replaces-unwritten="replacesUnwritten"
      @confirm="confirmImport"
      @open-workshop="openWorkshop"
      @copy-missing="copyMissing"
    />
  </div>
</template>
