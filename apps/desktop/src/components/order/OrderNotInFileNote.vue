<script setup lang="ts">
// "This order isn't in ModsConfig.xml yet" with an Apply button: shown on the Load order page
// while the Current order is selected and the file does not hold it (after an import, or after
// activation changes were rescanned). Apply is the normal dialog; nothing is written here.
import Button from "primevue/button";
import Message from "primevue/message";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import ApplyDialog from "@/components/apply/ApplyDialog.vue";
import { useOrderSource } from "@/composables/useOrderSource";
import { useDashboardQuery } from "@/queries/dashboard";

const { t } = useI18n();
const { selected } = useOrderSource();
const { data: dashboard } = useDashboardQuery();

const isApplyOpen = ref(false);
// Unknown (not loaded yet) shows nothing rather than a claim about the file.
const isShown = computed(
  () => selected.value === "current" && dashboard.value?.fileMatchesCurrent === false,
);
</script>

<template>
  <Message
    v-if="isShown"
    severity="info"
    class="shrink-0"
    data-testid="order-not-in-file"
  >
    <div class="flex flex-wrap items-center justify-between gap-3">
      <span>{{ t("orderShare.notInFile") }}</span>
      <Button
        :label="t('orderShare.notInFileApply')"
        size="small"
        data-testid="order-not-in-file-apply"
        @click="isApplyOpen = true"
      />
    </div>
  </Message>
  <ApplyDialog v-model:visible="isApplyOpen" />
</template>
