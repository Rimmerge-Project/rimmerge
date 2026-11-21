<script setup lang="ts">
// A small, non-blocking note under the pending banner on every page except the Mods page (which
// has `FileChangeNote`, the one place an outside edit is explained): the active mod list on disk
// was changed outside Rimmerge, and applying would overwrite it. It reads the live comparison, so
// it says nothing about your own unsaved work; it goes away by itself once Rimmerge reloads or
// writes the file (the backend's outside-change fact resets then).
import Message from "primevue/message";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { RouterLink, useRoute } from "vue-router";

import { useFileBaselineQuery } from "@/queries/fileBaseline";

const { t } = useI18n();
const route = useRoute();
const { data: baseline } = useFileBaselineQuery();

/** The Mods page and its mod info panel: both already show `FileChangeNote`. */
const isModsPage = computed(() => route.name === "mods" || route.name === "mod-detail");
const hasOutsideChange = computed(
  () => baseline.value !== undefined && baseline.value.externalChange.kind !== "none",
);
</script>

<template>
  <Message
    v-if="hasOutsideChange && !isModsPage"
    severity="info"
    class="shrink-0"
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
