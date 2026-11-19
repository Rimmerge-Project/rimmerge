<script setup lang="ts">
// "Now using your own order": shown after the first reorder (or an anchored activation) switched
// the selected order from Suggested to Current, so the page never silently changes what Apply
// writes. "View suggested" selects the suggested order again; the suggested order itself is
// untouched by the switch.
import Button from "primevue/button";
import Message from "primevue/message";
import { computed, watch } from "vue";
import { useI18n } from "vue-i18n";

import { useOrderSource } from "@/composables/useOrderSource";

const isRequested = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();
const { selected, select } = useOrderSource();

const isShown = computed(() => isRequested.value && selected.value === "current");

// Selecting the suggested order, by this button or any other route (the shell's switch), ends the
// note's claim for good: a later switch back to Current must not bring a stale note back.
watch(selected, (source) => {
  if (source === "suggested") {
    isRequested.value = false;
  }
});

function viewSuggested(): void {
  select("suggested");
}
</script>

<template>
  <Message
    v-if="isShown"
    severity="info"
    class="shrink-0"
    data-testid="order-switched-note"
  >
    <div class="flex flex-wrap items-center justify-between gap-3">
      <span>{{ t("mods.move.switchedNote") }}</span>
      <Button
        :label="t('mods.move.viewSuggested')"
        size="small"
        severity="secondary"
        data-testid="order-switched-view-suggested"
        @click="viewSuggested"
      />
    </div>
  </Message>
</template>
