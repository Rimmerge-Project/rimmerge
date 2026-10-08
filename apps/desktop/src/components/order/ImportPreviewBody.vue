<script setup lang="ts">
// The ready half of the import preview: what the list would do on this install. Every row is
// text (a sender's name is untrusted); installed mods are named from this install's own
// inventory through the shell-wide label toggle.
import Button from "primevue/button";
import Message from "primevue/message";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import ImportPreviewSection from "@/components/order/ImportPreviewSection.vue";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { OrderImportPreviewDto } from "@/types/generated/OrderImportPreviewDto";
import {
  corePlacementNote,
  groupImportEntries,
  importBlockedMessage,
  missingKindNote,
  skippedMessage,
  versionNote,
} from "@/utils/orderShare";

const { preview, replacesUnwritten } = defineProps<{
  preview: OrderImportPreviewDto;
  /** This import replaces the Current order, which is not in ModsConfig.xml yet. */
  replacesUnwritten: boolean;
}>();
const emit = defineEmits<{ openWorkshop: [workshopId: number] }>();

const { t } = useI18n();
const tm = useTranslateMessage();
const { labelWithFallback, titleFor } = useModLabel();

const groups = computed(() => groupImportEntries(preview.entries));
const version = computed(() => versionNote(preview.version));
const core = computed(() => corePlacementNote(preview.core));
// The sender's name is untrusted text; it still follows the shell's id/name toggle.
const missingRows = computed(() =>
  groups.value.notInstalled.map((row) => {
    const shown = labelWithFallback(row.listed, row.name);
    return {
      row,
      shown,
      showsId: shown !== row.listed,
      note: missingKindNote(row.missing),
    };
  }),
);
// The blocking id is the sender's, so it has no installed name to show: the fallback keeps the
// id, and still follows the shell's id/name toggle for a mod this install does know.
const blocked = computed(() =>
  preview.importBlocked === null
    ? null
    : importBlockedMessage(preview.importBlocked, (id) => labelWithFallback(id, null)),
);
</script>

<template>
  <div
    class="flex flex-col gap-3"
    data-testid="import-preview-ready"
  >
    <p
      class="text-text text-sm font-medium"
      data-testid="import-preview-summary"
    >
      {{ t("orderShare.preview.summary", { count: preview.listed }, preview.listed) }}
    </p>
    <p
      v-if="version"
      class="text-text-muted text-sm"
      data-testid="import-preview-version"
    >
      {{ tm(version) }}
    </p>
    <Message
      v-if="preview.replacesPendingChanges"
      severity="warn"
      data-testid="import-preview-replaces-pending"
    >
      {{ t("orderShare.preview.replacesPending") }}
    </Message>
    <Message
      v-if="replacesUnwritten"
      severity="warn"
      data-testid="import-preview-replaces-unwritten"
    >
      {{ t("orderShare.preview.replacesUnwritten") }}
    </Message>
    <p
      v-if="core"
      class="text-text-muted text-sm"
      data-testid="import-preview-core"
    >
      {{ tm(core) }}
    </p>
    <p
      v-if="blocked"
      class="text-status-danger text-sm"
      data-testid="import-preview-blocked"
    >
      {{ tm(blocked) }}
    </p>
    <p
      class="text-text-muted text-sm"
      data-testid="import-preview-moved"
    >
      {{
        preview.moved > 0
          ? t("orderShare.preview.moved", { count: preview.moved }, preview.moved)
          : t("orderShare.preview.sameOrder")
      }}
    </p>

    <div class="flex max-h-[45vh] flex-col gap-2 overflow-auto">
      <ImportPreviewSection
        v-if="groups.activated.length > 0"
        :heading="t('orderShare.preview.activatedHeading', { count: groups.activated.length })"
        test-id="import-preview-activated"
      >
        <ul class="flex flex-col gap-1 text-sm">
          <li
            v-for="row in groups.activated"
            :key="row.id"
            :title="titleFor(row.id)"
          >
            {{ labelWithFallback(row.id, row.name) }}
          </li>
        </ul>
      </ImportPreviewSection>

      <ImportPreviewSection
        v-if="preview.deactivated.length > 0"
        :heading="t('orderShare.preview.deactivatedHeading', { count: preview.deactivated.length })"
        test-id="import-preview-deactivated"
      >
        <ul class="flex flex-col gap-1 text-sm">
          <li
            v-for="row in preview.deactivated"
            :key="row.modId"
            :title="titleFor(row.modId)"
          >
            {{ labelWithFallback(row.modId, row.name) }}
          </li>
        </ul>
      </ImportPreviewSection>

      <ImportPreviewSection
        v-if="groups.notInstalled.length > 0"
        :heading="t('orderShare.preview.missingHeading', { count: groups.notInstalled.length })"
        test-id="import-preview-missing"
        is-open
      >
        <p class="text-text-muted mb-2 text-sm">
          {{ t("orderShare.preview.missingExplanation") }}
        </p>
        <ul class="flex flex-col gap-2 text-sm">
          <li
            v-for="({ row, shown, showsId, note }, index) in missingRows"
            :key="`${row.listed}-${index}`"
            class="flex flex-wrap items-center justify-between gap-2"
            data-testid="import-preview-missing-row"
          >
            <span class="flex flex-col">
              <span>{{ shown }}</span>
              <span
                v-if="showsId"
                class="text-text-faint text-xs"
              >{{ row.listed }}</span>
              <span
                v-if="note"
                class="text-text-muted text-xs"
              >{{ tm(note) }}</span>
            </span>
            <Button
              v-if="row.missing.kind === 'workshop'"
              :label="t('orderShare.preview.openWorkshop')"
              :aria-label="t('orderShare.preview.openWorkshopLabel', { name: shown })"
              severity="secondary"
              size="small"
              :data-testid="`import-preview-workshop-${row.missing.workshopId}`"
              @click="emit('openWorkshop', row.missing.workshopId)"
            />
          </li>
        </ul>
      </ImportPreviewSection>

      <ImportPreviewSection
        v-if="groups.duplicates.length > 0"
        :heading="t('orderShare.preview.duplicatesHeading', { count: groups.duplicates.length })"
        test-id="import-preview-duplicates"
      >
        <ul class="flex flex-col gap-1 text-sm">
          <li
            v-for="(row, index) in groups.duplicates"
            :key="`${row.id}-${index}`"
          >
            {{
              t("orderShare.preview.duplicateRow", {
                id: labelWithFallback(row.id, null),
                position: row.firstPosition,
              })
            }}
          </li>
        </ul>
      </ImportPreviewSection>

      <ImportPreviewSection
        v-if="groups.otherCopies.length > 0"
        :heading="t('orderShare.preview.otherCopyHeading', { count: groups.otherCopies.length })"
        test-id="import-preview-other-copies"
      >
        <ul class="flex flex-col gap-1 text-sm">
          <li
            v-for="(row, index) in groups.otherCopies"
            :key="`${row.listed}-${index}`"
          >
            {{
              t("orderShare.preview.otherCopyRow", {
                listed: labelWithFallback(row.listed, null),
                installed: labelWithFallback(row.installed, row.name),
              })
            }}
          </li>
        </ul>
      </ImportPreviewSection>

      <ImportPreviewSection
        v-if="preview.skipped.length > 0"
        :heading="t('orderShare.preview.skippedHeading', { count: preview.skipped.length + preview.omittedSkipped })"
        test-id="import-preview-skipped"
      >
        <ul class="flex flex-col gap-1 text-sm">
          <li
            v-for="(row, index) in preview.skipped"
            :key="index"
          >
            {{ tm(skippedMessage(row)) }}
          </li>
        </ul>
        <p
          v-if="preview.omittedSkipped > 0"
          class="text-text-muted mt-1 text-sm"
          data-testid="import-preview-skipped-omitted"
        >
          {{ t("orderShare.preview.skippedOmitted", { count: preview.omittedSkipped }, preview.omittedSkipped) }}
        </p>
      </ImportPreviewSection>
    </div>

    <p class="text-text-muted text-sm">
      {{ t("orderShare.preview.whatHappens") }}
    </p>
  </div>
</template>
