<script setup lang="ts">
// The "Apply anyway?" panel: shown in place of the action row when the order has an unanswered
// hard problem. Reads its state from the Apply dialog; never sends anything itself.
import Button from "primevue/button";
import { computed, nextTick, onMounted, useId, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";
import { RouterLink } from "vue-router";
import ApplyProblemList from "@/components/apply/ApplyProblemList.vue";
import { useApplyDialogState } from "@/composables/useApplyDialog";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { orderSourceSentenceLabel } from "@/utils/orderSource";

const { t } = useI18n();
const tm = useTranslateMessage();
const { preflight, isLoading, close, confirmProblems, goBackFromConfirm, writeModsConfig } =
  useApplyDialogState();
const emit = defineEmits<{ goBack: [] }>();
const headingId = useId();
const introId = useId();

const unanswered = computed(() => (preflight.value?.items ?? []).filter((i) => !i.acknowledged));
const decided = computed(() => (preflight.value?.items ?? []).filter((i) => i.acknowledged));
const orderLabel = computed(() =>
  preflight.value ? tm(orderSourceSentenceLabel(preflight.value.source)) : "",
);

const goBackButton = useTemplateRef<{ $el: HTMLElement }>("goBackButton");

// Go back takes focus, so a stray Enter backs out; Apply anyway is never the default.
onMounted(async () => {
  await nextTick();
  goBackButton.value?.$el.focus();
});

// With the box forced off meanwhile, confirming does nothing and the panel closes,
// so focus returns to the submit button exactly as Go back does.
async function applyAnyway(): Promise<void> {
  const isNothingToWrite = !writeModsConfig.value;
  await confirmProblems();
  if (isNothingToWrite) {
    emit("goBack");
  }
}

function goBack(): void {
  goBackFromConfirm();
  emit("goBack");
}
</script>

<template>
  <div
    role="group"
    :aria-labelledby="headingId"
    :aria-describedby="introId"
    class="border-border-subtle flex flex-col gap-3 rounded border p-3"
    data-testid="apply-confirm"
  >
    <h3
      :id="headingId"
      class="text-text text-base font-semibold"
      data-testid="apply-confirm-heading"
    >
      {{ t("apply.confirm.heading") }}
    </h3>
    <p
      :id="introId"
      class="text-text text-sm"
    >
      {{ t("apply.confirm.intro", { order: orderLabel, count: unanswered.length }, unanswered.length) }}
    </p>
    <ApplyProblemList
      :items="unanswered"
      data-testid="apply-confirm-problems"
    />
    <section
      v-if="decided.length > 0"
      class="flex flex-col gap-1"
      data-testid="apply-confirm-decided"
    >
      <h4 class="text-text-muted text-xs font-medium">
        {{ t("apply.confirm.acknowledgedHeading") }}
      </h4>
      <ApplyProblemList :items="decided" />
    </section>
    <RouterLink
      to="/inbox"
      class="text-sm underline"
      data-testid="apply-confirm-inbox-link"
      @click="close"
    >
      {{ t("apply.confirm.reviewInInbox") }}
    </RouterLink>
    <div class="flex justify-end gap-2">
      <Button
        ref="goBackButton"
        :label="t('apply.confirm.goBackButton')"
        severity="secondary"
        data-testid="apply-confirm-go-back"
        @click="goBack"
      />
      <Button
        :label="t('apply.confirm.applyAnywayButton')"
        severity="warn"
        :loading="isLoading"
        data-testid="apply-confirm-apply-anyway"
        @click="applyAnyway"
      />
    </div>
  </div>
</template>
