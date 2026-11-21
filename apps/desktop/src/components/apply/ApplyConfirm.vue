<script setup lang="ts">
// The "Apply anyway?" panel: shown in place of the action row when the order has an unanswered
// hard problem, when the file was changed outside Rimmerge (read fresh when Apply was pressed), or
// both: one panel lists everything, so "Go back" stays the one default. Reads its state from the
// Apply dialog; never sends anything itself.
import Button from "primevue/button";
import { computed, nextTick, onMounted, useId, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";
import { RouterLink } from "vue-router";
import ApplyProblemList from "@/components/apply/ApplyProblemList.vue";
import { useApplyDialogState } from "@/composables/useApplyDialog";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { assertNever } from "@/utils/assertNever";
import { orderSourceSentenceLabel } from "@/utils/orderSource";

const { t } = useI18n();
const tm = useTranslateMessage();
const {
  confirmPreflight: preflight,
  isLoading,
  close,
  confirmProblems,
  goBackFromConfirm,
  writeModsConfig,
} = useApplyDialogState();
const emit = defineEmits<{ goBack: [] }>();
const headingId = useId();
const introId = useId();
const outsideId = useId();

const unanswered = computed(() => (preflight.value?.items ?? []).filter((i) => !i.acknowledged));
const decided = computed(() => (preflight.value?.items ?? []).filter((i) => i.acknowledged));
const outsideText = computed<string | null>(() => {
  const change = preflight.value?.outsideChange;
  if (change === undefined) {
    return null;
  }
  switch (change.kind) {
    case "none":
      return null;
    case "membership":
      return t("apply.confirm.outsideMembership", {
        added: change.added,
        removed: change.removed,
        order: orderLabel.value,
      });
    case "reordered":
      return t("apply.confirm.outsideReordered", { order: orderLabel.value });
    default:
      return assertNever(change);
  }
});
const describedBy = computed(() =>
  [outsideText.value !== null ? outsideId : null, unanswered.value.length > 0 ? introId : null]
    .filter((id) => id !== null)
    .join(" "),
);
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
    :aria-describedby="describedBy"
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
    <section
      v-if="outsideText !== null"
      class="flex flex-col gap-1"
      data-testid="apply-confirm-outside"
    >
      <h4 class="text-text text-sm font-medium">
        {{ t("apply.confirm.outsideHeading") }}
      </h4>
      <p
        :id="outsideId"
        class="text-text text-sm"
        data-testid="apply-confirm-outside-text"
      >
        {{ outsideText }}
      </p>
    </section>
    <p
      v-if="unanswered.length > 0"
      :id="introId"
      class="text-text text-sm"
    >
      {{ t("apply.confirm.intro", { order: orderLabel, count: unanswered.length }, unanswered.length) }}
    </p>
    <ApplyProblemList
      v-if="unanswered.length > 0"
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
      v-if="unanswered.length > 0"
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
