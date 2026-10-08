<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import OrderNotInFileNote from "@/components/order/OrderNotInFileNote.vue";
import OrderShareMenus from "@/components/order/OrderShareMenus.vue";
import OrderTable from "@/components/order/OrderTable.vue";
import WhyPanel from "@/components/order/WhyPanel.vue";
import { useOrderSource } from "@/composables/useOrderSource";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useOrderQuery } from "@/queries/order";
import { orderSourceLabel } from "@/utils/orderSource";

const { t } = useI18n();
const tm = useTranslateMessage();
const route = useRoute();
const router = useRouter();
const { selected } = useOrderSource();

const { data, isPending, error } = useOrderQuery(selected);

const selectedModId = computed(() => {
  const param = route.params["modId"];
  return typeof param === "string" ? param : null;
});

function openMod(modId: string): void {
  void router.push({ name: "order-mod", params: { modId } });
}

function closePanel(): void {
  void router.push({ name: "order" });
}
</script>

<template>
  <div class="mx-auto flex h-full max-w-6xl min-h-0 flex-col gap-6 p-6">
    <div class="flex shrink-0 flex-wrap items-center justify-between gap-3">
      <h1 class="text-text text-xl font-semibold">
        {{ t("order.heading", { source: tm(orderSourceLabel(selected)) }) }}
      </h1>
      <OrderShareMenus />
    </div>

    <OrderNotInFileNote />

    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="order-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="order-error"
    >
      {{ error instanceof Error ? error.message : t("order.loadFailed") }}
    </p>
    <OrderTable
      v-else-if="data"
      :rows="data"
      @select-row="openMod"
    />

    <WhyPanel
      :mod-id="selectedModId"
      @close="closePanel"
    />
  </div>
</template>
