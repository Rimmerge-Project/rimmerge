<script setup lang="ts">
// The "Import from a save…" dialog: RimWorld's Saves folder as a list (name and date, newest
// first, as the backend ordered it) with a Preview button per save and a native "Browse…"
// picker. Nothing here opens a save: the picked path goes to the existing preview, which reads
// only the save's mod list.
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import Message from "primevue/message";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useSavesQuery } from "@/queries/orderShare";
import { describeCommandError } from "@/utils/errors";

const { isBusy } = defineProps<{ isBusy: boolean }>();
const visible = defineModel<boolean>("visible", { required: true });
const emit = defineEmits<{
  preview: [path: string];
  /** `defaultPath` is the Saves folder when the backend found it, else `null`. */
  browse: [defaultPath: string | null];
}>();

const { t, locale } = useI18n();
const tm = useTranslateMessage();
const { data: listing, error, isPending } = useSavesQuery(visible);

const dateFormat = computed(
  () => new Intl.DateTimeFormat(locale.value, { dateStyle: "medium", timeStyle: "short" }),
);

const failure = computed(() => (error.value === null ? null : describeCommandError(error.value)));

function savedAtLabel(savedAt: number | null): string {
  return savedAt === null
    ? t("orderShare.saves.unknownDate")
    : t("orderShare.saves.savedAt", { date: dateFormat.value.format(new Date(savedAt * 1000)) });
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="t('orderShare.saves.title')"
    :closable="!isBusy"
    :close-on-escape="!isBusy"
    class="w-[36rem] max-w-[95vw]"
    data-testid="save-import-dialog"
  >
    <div class="flex flex-col gap-3">
      <p
        class="text-text-muted text-sm"
        data-testid="save-import-intro"
      >
        {{ t("orderShare.saves.intro") }}
      </p>

      <p
        v-if="isPending"
        role="status"
        class="text-text-muted text-sm"
      >
        {{ t("common.loading") }}
      </p>
      <Message
        v-else-if="failure !== null"
        severity="error"
        :closable="false"
        data-testid="save-import-error"
      >
        <p class="font-medium">
          {{ tm(failure.title) }}
        </p>
        <p>{{ tm(failure.detail) }}</p>
      </Message>
      <p
        v-else-if="listing?.folder === null"
        class="text-text-muted text-sm"
        data-testid="save-import-no-folder"
      >
        {{ t("orderShare.saves.noFolder") }}
      </p>
      <p
        v-else-if="listing?.saves.length === 0"
        class="text-text-muted text-sm"
        data-testid="save-import-empty"
      >
        {{ t("orderShare.saves.empty") }}
      </p>
      <template v-else-if="listing !== undefined">
        <ul
          class="border-border-subtle divide-border-subtle max-h-80 divide-y overflow-auto rounded border"
          data-testid="save-import-list"
        >
          <li
            v-for="save in listing.saves"
            :key="save.path"
            class="flex items-center justify-between gap-3 px-3 py-2"
            data-testid="save-import-row"
          >
            <div class="min-w-0">
              <p
                class="text-text truncate text-sm font-medium"
                data-testid="save-import-name"
              >
                {{ save.name }}
              </p>
              <p class="text-text-muted text-xs">
                {{ savedAtLabel(save.savedAt) }}
              </p>
            </div>
            <Button
              :label="t('orderShare.saves.preview')"
              :aria-label="t('orderShare.saves.previewLabel', { name: save.name })"
              severity="secondary"
              size="small"
              class="shrink-0"
              :disabled="isBusy"
              data-testid="save-import-preview"
              @click="emit('preview', save.path)"
            />
          </li>
        </ul>
        <p
          v-if="listing.omitted > 0"
          class="text-text-muted text-xs"
          data-testid="save-import-omitted"
        >
          {{ t("orderShare.saves.omitted", { count: listing.omitted }, listing.omitted) }}
        </p>
      </template>

      <div class="flex justify-between gap-2">
        <Button
          :label="t('orderShare.saves.browse')"
          severity="secondary"
          :disabled="isBusy"
          data-testid="save-import-browse"
          @click="emit('browse', listing?.folder ?? null)"
        />
        <Button
          :label="t('orderShare.preview.close')"
          severity="secondary"
          :disabled="isBusy"
          data-testid="save-import-close"
          @click="visible = false"
        />
      </div>
    </div>
  </Dialog>
</template>
