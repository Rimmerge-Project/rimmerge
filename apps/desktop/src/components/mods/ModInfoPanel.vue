<script setup lang="ts">
import Message from "primevue/message";
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import BaseEmpty from "@/components/base/BaseEmpty.vue";
import ModDescription from "@/components/mods/ModDescription.vue";
import ModInfoFacts from "@/components/mods/ModInfoFacts.vue";
import ModPreview from "@/components/mods/ModPreview.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import { useModIconQuery, useModInfoQuery, useOpenModLinkMutation } from "@/queries/mods";
import { RimmergeError } from "@/services/ipc";
import type { HomepageLinkDto } from "@/types/generated/HomepageLinkDto";
import type { ModInfoDto } from "@/types/generated/ModInfoDto";
import type { PendingChangeDto } from "@/types/generated/PendingChangeDto";
import { describeAboutUnreadable } from "@/utils/aboutUnreadable";
import { assertNever } from "@/utils/assertNever";
import { describeCommandError } from "@/utils/errors";
import { sourceLabel } from "@/utils/source";

/** `null` closes the panel; a mod id opens it and loads that mod's info. */
const { modId } = defineProps<{ modId: string | null }>();
const emit = defineEmits<{ close: [] }>();

const { t, locale } = useI18n();
const tm = useTranslateMessage();
const router = useRouter();
const modLabel = useModLabel();

const { data, isPending, error } = useModInfoQuery(() => modId);
const { data: icon } = useModIconQuery(() => modId);
const { mutateAsync: openLink } = useOpenModLinkMutation();

const isKnownNotFound = computed(
  () => error.value instanceof RimmergeError && error.value.code === "mod_not_found",
);
const loadError = computed(() => (error.value ? describeCommandError(error.value) : null));

/**
 * An active mod's own `homepage` (scan-time, immediately known) is
 * preferred; an inactive mod carries none of its own, so its homepage
 * only ever comes from the lazily-read `about.homepage`.
 */
const homepage = computed<HomepageLinkDto | null>(() => {
  const value = data.value;
  if (!value || value.kind === "missing") {
    return null;
  }
  if (value.kind === "active") {
    return value.homepage;
  }
  return value.about.kind === "read" ? value.about.homepage : null;
});

/** Narrows `data` to the Active/Inactive shape both the header and {@link ModInfoFacts} need. */
const activeOrInactive = computed(() => {
  const value = data.value;
  return value && (value.kind === "active" || value.kind === "inactive")
    ? (value as Extract<ModInfoDto, { kind: "active" | "inactive" }>)
    : null;
});

function openWorkshopLink(): void {
  if (modId) {
    void openLink({ modId, link: "workshop" });
  }
}

function openHomepageLink(): void {
  if (modId) {
    void openLink({ modId, link: "homepage" });
  }
}

function openAllDetails(): void {
  if (modId) {
    void router.push({ name: "mod-detail-page", params: { modId } });
  }
}

function close(): void {
  emit("close");
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") {
    close();
  }
}

/** Exhaustive over {@link PendingChangeDto} so a third pending state is a compile error here, not a silently-wrong label. */
function pendingChangeText(pending: PendingChangeDto): string {
  switch (pending) {
    case "activationPending":
      return t("modInfo.panel.pendingActivation");
    case "deactivationPending":
      return t("modInfo.panel.pendingDeactivation");
    default:
      return assertNever(pending);
  }
}
</script>

<template>
  <aside
    v-if="modId !== null"
    class="surface-card flex min-h-0 w-[26rem] shrink-0 flex-col gap-4 overflow-y-auto p-4"
    :aria-label="t('modInfo.panel.regionLabel')"
    data-testid="mod-info-panel"
    @keydown="onKeydown"
  >
    <div class="flex items-start justify-between gap-2">
      <h2 class="sr-only">
        {{ t("modInfo.panel.regionLabel") }}
      </h2>
      <button
        type="button"
        class="text-text-muted hover:text-text ml-auto cursor-pointer text-sm"
        :aria-label="t('modInfo.panel.close')"
        data-testid="mod-info-panel-close"
        @click="close"
      >
        <i
          class="pi pi-times"
          aria-hidden="true"
        />
      </button>
    </div>

    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="mod-info-panel-loading"
    >
      {{ t("common.loading") }}
    </p>

    <BaseEmpty
      v-else-if="isKnownNotFound"
      :message="t('modInfo.panel.notFoundTitle')"
      data-testid="mod-info-panel-not-found"
    />

    <p
      v-else-if="loadError"
      class="text-status-danger text-sm"
      data-testid="mod-info-panel-error"
    >
      {{ tm(loadError.detail) }}
    </p>

    <template v-else-if="data?.kind === 'missing'">
      <header data-testid="mod-info-panel-missing-header">
        <h3 class="text-status-danger text-lg font-semibold">
          {{ t("modInfo.panel.missingHeading") }}
        </h3>
        <p class="text-text-muted text-sm">
          {{ data.modId }} — {{ t("modInfo.panel.missingBody") }}
        </p>
      </header>
      <section v-if="data.requiredBy.length > 0">
        <h3 class="text-text mb-1 text-sm font-semibold">
          {{ t("modInfo.panel.requiredByHeading") }}
        </h3>
        <ul class="text-text-muted flex flex-col gap-0.5 text-sm">
          <li
            v-for="id in data.requiredBy"
            :key="id"
          >
            {{ modLabel.label(id) }}
          </li>
        </ul>
      </section>
    </template>

    <template v-else-if="activeOrInactive">
      <ModPreview :mod-id="modId" />

      <header class="flex flex-col gap-1">
        <div class="flex items-center gap-2">
          <img
            v-if="icon?.kind === 'image'"
            :src="icon.dataUrl"
            alt=""
            class="h-8 w-8 shrink-0 rounded object-contain"
            data-testid="mod-info-icon"
          >
          <h3
            class="text-text truncate text-lg font-semibold"
            data-testid="mod-info-name"
          >
            {{ activeOrInactive.name }}
          </h3>
        </div>
        <p class="text-text-muted text-xs">
          {{ t("modInfo.panel.packageIdLine", { id: activeOrInactive.modId }) }}
          <span
            v-if="activeOrInactive.modId.endsWith('_steam')"
            class="text-text-faint"
          >({{ t("modInfo.panel.workshopCopyNote") }})</span>
        </p>
        <p
          v-if="activeOrInactive.authors.length > 0"
          class="text-text-muted text-sm"
        >
          {{ t("mods.detail.byAuthors", { authors: formatList(locale, activeOrInactive.authors) }) }}
        </p>
        <p class="text-text-muted text-xs">
          {{ tm(sourceLabel(activeOrInactive.source)) }}
        </p>
        <p
          v-if="activeOrInactive.kind === 'active'"
          class="text-xs"
          :class="activeOrInactive.supportsGameVersion ? 'text-status-auto' : 'text-text-faint'"
        >
          {{
            activeOrInactive.supportsGameVersion
              ? t("modInfo.panel.supportsGameVersion")
              : t("modInfo.panel.doesNotSupportGameVersion")
          }}
        </p>
        <p
          v-if="activeOrInactive.pending"
          class="text-status-danger text-xs"
          data-testid="mod-info-pending"
        >
          {{ pendingChangeText(activeOrInactive.pending) }}
        </p>
      </header>

      <div class="flex flex-wrap items-center gap-2">
        <button
          v-if="activeOrInactive.workshopId !== null"
          type="button"
          class="border-border-subtle hover:bg-surface-2 cursor-pointer rounded border px-2 py-1 text-xs"
          :aria-label="t('modInfo.panel.openWorkshopButton')"
          data-testid="mod-info-workshop-link"
          @click="openWorkshopLink"
        >
          {{ t("modInfo.panel.openWorkshopButton") }}
        </button>
        <button
          v-if="homepage?.kind === 'openable'"
          type="button"
          class="border-border-subtle hover:bg-surface-2 cursor-pointer rounded border px-2 py-1 text-xs"
          :aria-label="t('modInfo.panel.openHomepageButton')"
          data-testid="mod-info-homepage-link"
          @click="openHomepageLink"
        >
          {{ t("modInfo.panel.openHomepageButton") }}
        </button>
        <span
          v-else-if="homepage?.kind === 'text'"
          class="text-text-faint text-xs"
        >{{ homepage.text }}</span>
      </div>

      <section data-testid="mod-info-about">
        <h3 class="text-text mb-1 text-sm font-semibold">
          {{ t("modInfo.panel.descriptionHeading") }}
        </h3>
        <Message
          v-if="activeOrInactive.about.kind === 'changed'"
          severity="warn"
          data-testid="mod-info-about-changed"
        >
          {{ t("modInfo.panel.aboutChanged") }}
        </Message>
        <Message
          v-else-if="activeOrInactive.about.kind === 'unreadable'"
          severity="warn"
          data-testid="mod-info-about-unreadable"
        >
          {{
            t("modInfo.panel.aboutUnreadable", {
              reason: tm(describeAboutUnreadable(activeOrInactive.about.cause)),
            })
          }}
          <span
            v-if="activeOrInactive.about.detail"
            class="text-text-faint mt-1 block font-mono text-xs break-all select-text"
            data-testid="mod-info-about-unreadable-detail"
          >
            {{ t("common.technicalDetail", { detail: activeOrInactive.about.detail }) }}
          </span>
        </Message>
        <Message
          v-else-if="activeOrInactive.about.kind === 'notOnDisk'"
          severity="warn"
          data-testid="mod-info-about-not-on-disk"
        >
          {{ t("modInfo.panel.aboutNotOnDisk") }}
        </Message>
        <template v-else-if="activeOrInactive.about.kind === 'read'">
          <p
            v-if="activeOrInactive.about.modVersion"
            class="text-text-faint mb-1 text-xs"
          >
            {{ t("modInfo.panel.versionLine", { version: activeOrInactive.about.modVersion }) }}
          </p>
          <ModDescription :description="activeOrInactive.about.description" />
        </template>
      </section>

      <ModInfoFacts :info="activeOrInactive" />

      <RouterLink
        v-if="activeOrInactive.kind === 'active'"
        :to="{ name: 'mod-detail-page', params: { modId } }"
        class="text-accent text-sm underline"
        data-testid="mod-info-all-details-link"
        @click.prevent="openAllDetails"
      >
        {{ t("modInfo.panel.allDetailsLink") }}
      </RouterLink>
    </template>
  </aside>
</template>
