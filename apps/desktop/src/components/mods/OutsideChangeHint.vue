<script setup lang="ts">
// A small, non-blocking note under the pending banner on every page except the Mods page (which
// has `FileChangeNote`, the one place an outside edit is explained): the active mod list on disk
// was changed outside Rimmerge, and applying would overwrite it. It reads the live comparison, so
// it says nothing about your own unsaved work; it goes away by itself once Rimmerge reloads or
// writes the file (the backend's outside-change fact resets then). It is not shown while the file
// already holds the selected order (`fileMatchesSelected`): an Apply then overwrites nothing, so
// there is nothing to warn about on every page; the Mods page's `FileChangeNote` still explains it.
import Message from "primevue/message";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { RouterLink, useRoute } from "vue-router";

import { useBannerAnnouncement } from "@/composables/useBannerAnnouncement";
import { useFileBaselineQuery } from "@/queries/fileBaseline";
import { QUIET_MESSAGE_PT } from "@/utils/quietMessage";

const { t } = useI18n();
const route = useRoute();
const { data: baseline } = useFileBaselineQuery();

/** The Mods page and its mod info panel: both already show `FileChangeNote`. */
const isModsPage = computed(() => route.name === "mods" || route.name === "mod-detail");
const isOverwriteAtRisk = computed(
  () =>
    baseline.value !== undefined &&
    baseline.value.externalChange.kind !== "none" &&
    !baseline.value.fileMatchesSelected,
);
const isShown = computed(() => isOverwriteAtRisk.value && !isModsPage.value);
// Hidden on the Mods page is not gone: only a change that settled the matter ends the banner, so
// coming back from the Mods page with the same text says nothing again.
useBannerAnnouncement("outsideHint", () => {
  if (!isOverwriteAtRisk.value) {
    return null;
  }
  return isModsPage.value ? undefined : t("shell.outsideChange.text");
});
</script>

<template>
  <Message
    v-if="isShown"
    severity="info"
    class="shrink-0"
    :pt="QUIET_MESSAGE_PT"
    data-testid="outside-change-hint"
  >
    <div class="flex flex-wrap items-center gap-x-3 gap-y-1 text-sm">
      <span>{{ t("shell.outsideChange.text") }}</span>
      <RouterLink
        :to="{ name: 'mods' }"
        class="underline"
        data-testid="outside-change-hint-link"
      >
        {{ t("shell.outsideChange.link") }}
      </RouterLink>
    </div>
  </Message>
</template>
