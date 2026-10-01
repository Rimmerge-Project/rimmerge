<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute } from "vue-router";

import ModChanges from "@/components/mods/ModChanges.vue";
import ModEdges from "@/components/mods/ModEdges.vue";
import ScanNoteList from "@/components/mods/ScanNoteList.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import { useModQuery } from "@/queries/mods";
import { useSessionStore } from "@/stores/session";
import { sourceLabel } from "@/utils/source";

const { t, locale } = useI18n();
const tm = useTranslateMessage();
const route = useRoute();
const modId = computed(() => {
  const param = route.params["modId"];
  return typeof param === "string" ? param : null;
});

const { data, isPending, error } = useModQuery(modId);
const modLabel = useModLabel();
const session = useSessionStore();
const scanNotes = computed(() => session.scanNotes.filter((note) => note.modId === modId.value));

// Kept out of the template: a `.map().join()` inline there re-runs (and
// re-allocates) on every render this component does, not just when
// `data` actually changes.
const dependencyNames = computed(
  () =>
    data.value?.declared.dependencies.map((dependency) =>
      modLabel.labelWithFallback(dependency.id, dependency.displayName),
    ) ?? [],
);
const loadAfterNames = computed(() => data.value?.declared.loadAfter.map(modLabel.label) ?? []);
const loadBeforeNames = computed(() => data.value?.declared.loadBefore.map(modLabel.label) ?? []);
const forceLoadAfterNames = computed(
  () => data.value?.declared.forceLoadAfter.map(modLabel.label) ?? [],
);
const forceLoadBeforeNames = computed(
  () => data.value?.declared.forceLoadBefore.map(modLabel.label) ?? [],
);
const incompatibleWithNames = computed(
  () => data.value?.declared.incompatibleWith.map(modLabel.label) ?? [],
);
</script>

<template>
  <div class="mx-auto flex max-w-6xl flex-col gap-6 p-6">
    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="mod-detail-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="mod-detail-error"
    >
      {{ error instanceof Error ? error.message : t("mods.detail.loadFailed") }}
    </p>

    <template v-else-if="data">
      <header>
        <h1
          class="text-text text-xl font-semibold"
          data-testid="mod-detail-name"
        >
          {{ data.name }}
        </h1>
        <p class="text-text-muted text-sm">
          {{ t("mods.detail.idAndSource", { modId: data.modId, source: tm(sourceLabel(data.source)) }) }}
        </p>
        <p
          v-if="data.authors.length > 0"
          class="text-text-muted text-sm"
        >
          {{ t("mods.detail.byAuthors", { authors: formatList(locale, data.authors) }) }}
        </p>
      </header>

      <section>
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("mods.detail.tagsHeading") }}
        </h2>
        <div class="flex flex-wrap gap-1">
          <span
            v-for="tag in data.tags"
            :key="tag"
            class="bg-surface-2 text-text-muted rounded px-2 py-0.5 text-xs"
          >{{ tag }}</span>
          <span
            v-if="data.tags.length === 0"
            class="text-text-muted text-sm"
          >{{ t("mods.edges.none") }}</span>
        </div>
      </section>

      <section>
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("mods.detail.declaredOrderHeading") }}
        </h2>
        <dl class="grid grid-cols-2 gap-x-4 gap-y-1 text-sm">
          <dt class="text-text-muted">
            {{ t("mods.detail.loadAfterLabel") }}
          </dt>
          <dd>{{ formatList(locale, loadAfterNames) || "—" }}</dd>
          <dt class="text-text-muted">
            {{ t("mods.detail.loadBeforeLabel") }}
          </dt>
          <dd>{{ formatList(locale, loadBeforeNames) || "—" }}</dd>
          <dt class="text-text-muted">
            {{ t("mods.detail.forceLoadAfterLabel") }}
          </dt>
          <dd>{{ formatList(locale, forceLoadAfterNames) || "—" }}</dd>
          <dt class="text-text-muted">
            {{ t("mods.detail.forceLoadBeforeLabel") }}
          </dt>
          <dd>{{ formatList(locale, forceLoadBeforeNames) || "—" }}</dd>
          <dt class="text-text-muted">
            {{ t("mods.detail.dependenciesLabel") }}
          </dt>
          <dd>
            {{ formatList(locale, dependencyNames) || "—" }}
          </dd>
          <dt class="text-text-muted">
            {{ t("mods.detail.incompatibleWithLabel") }}
          </dt>
          <dd>{{ formatList(locale, incompatibleWithNames) || "—" }}</dd>
        </dl>
      </section>

      <section>
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("mods.detail.edgesInHeading") }}
        </h2>
        <ModEdges :edges="data.edgesIn" />
      </section>

      <section>
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("mods.detail.edgesOutHeading") }}
        </h2>
        <ModEdges :edges="data.edgesOut" />
      </section>

      <section>
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("mods.detail.changesHeading") }}
        </h2>
        <ModChanges :mod-id="data.modId" />
      </section>

      <section>
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("mods.detail.scanNotesHeading") }}
        </h2>
        <ScanNoteList :notes="scanNotes" />
      </section>

      <section>
        <h2 class="text-text mb-1 text-base font-semibold">
          {{ t("mods.detail.findingsHeading") }}
        </h2>
        <ul
          v-if="data.findingKeys.length > 0"
          class="flex flex-col gap-1"
        >
          <li
            v-for="key in data.findingKeys"
            :key="key"
          >
            <RouterLink
              :to="{ path: '/inbox', query: { search: key } }"
              class="text-accent font-mono text-xs underline"
            >
              {{ key }}
            </RouterLink>
          </li>
        </ul>
        <p
          v-else
          class="text-text-muted text-sm"
          data-testid="mod-detail-no-findings"
        >
          {{ t("mods.detail.noLiveFindings") }}
        </p>
      </section>
    </template>
  </div>
</template>
