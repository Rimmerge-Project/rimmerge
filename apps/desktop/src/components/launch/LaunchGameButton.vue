<script setup lang="ts">
// The Launch RimWorld action. One component, two placements (the sidebar under Apply, the
// strip's done line); each instance owns its Apply-first prompt and its own `ApplyDialog`.
// Only the sidebar instance polls the shared status query, and the in-flight and Starting
// state is shared through `stores/gameLaunch.ts`, so a click on one disables the other.
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import { computed, useId } from "vue";
import { useI18n } from "vue-i18n";
import ApplyDialog from "@/components/apply/ApplyDialog.vue";
import LaunchApplyFirstPrompt from "@/components/launch/LaunchApplyFirstPrompt.vue";
import { useGameLaunch } from "@/composables/useGameLaunch";

const { variant } = defineProps<{ variant: "sidebar" | "strip" }>();

const { t } = useI18n();
const {
  label,
  hint,
  isDisabled,
  isBusy,
  isPromptOpen,
  promptSentence,
  isApplyOpen,
  announcement,
  request,
  applyFirst,
  launchAnyway,
  cancelPrompt,
  handleApplied,
} = useGameLaunch({ pollsStatus: variant === "sidebar" });

const hintId = useId();
const isSidebar = computed(() => variant === "sidebar");
const buttonTestId = computed(() =>
  isSidebar.value ? "shell-launch-button" : "guide-launch-button",
);
</script>

<template>
  <div
    class="flex flex-col gap-1"
    data-testid="launch-game"
  >
    <Button
      :label="label"
      :fluid="isSidebar"
      :severity="isSidebar ? 'secondary' : undefined"
      :disabled="isDisabled"
      :loading="isBusy"
      :aria-describedby="hint ? hintId : undefined"
      :data-testid="buttonTestId"
      @click="request"
    />
    <p
      v-if="hint"
      :id="hintId"
      :class="['text-text-muted', isSidebar ? 'text-xs' : 'text-sm']"
      data-testid="launch-game-hint"
    >
      {{ hint }}
    </p>

    <Dialog
      v-model:visible="isPromptOpen"
      modal
      :header="t('gameLaunch.applyFirst.header')"
      data-testid="launch-apply-first-dialog"
    >
      <LaunchApplyFirstPrompt
        :sentence="promptSentence"
        @apply-first="applyFirst"
        @launch-anyway="launchAnyway"
        @cancel="cancelPrompt"
      />
    </Dialog>

    <ApplyDialog
      v-model:visible="isApplyOpen"
      @applied="handleApplied"
    />

    <div
      class="sr-only"
      role="status"
      aria-live="polite"
      data-testid="launch-game-live-region"
    >
      {{ announcement }}
    </div>
  </div>
</template>
