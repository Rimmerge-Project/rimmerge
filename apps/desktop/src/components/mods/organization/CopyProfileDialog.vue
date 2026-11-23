<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import Message from "primevue/message";
import { computed, nextTick, useId, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import OrganizationError from "@/components/mods/organization/OrganizationError.vue";
import { useCopyProfile } from "@/composables/useCopyProfile";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { CopyModeDto } from "@/types/generated/CopyModeDto";
import { COLOR_NAME_KEY } from "@/utils/organization";
import { copyLines, copyRefusalMessage, discardLines } from "@/utils/organizationCopy";

/**
 * Copy groups and colours from another profile. One dialog, four steps: choose the other profile's
 * folder, read the preview for Merge or Replace, confirm a Replace (Cancel is focused: it is
 * destructive), read the result. It only calls the two copy commands; it never writes where the
 * folder is and never touches the load order, the active mods or the rules.
 */
const visible = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();
const tm = useTranslateMessage();
const flow = useCopyProfile();
const legendId = useId();

const report = computed(() =>
  flow.answer.value?.kind === "report" ? flow.answer.value.report : null,
);
const refusal = computed(() =>
  flow.answer.value?.kind === "refused" ? flow.answer.value.reason : null,
);
const MODE_TEXT = [
  {
    mode: "merge",
    label: "mods.groups.copy.mode.merge",
    help: "mods.groups.copy.mode.mergeHelp",
  },
  {
    mode: "replace",
    label: "mods.groups.copy.mode.replace",
    help: "mods.groups.copy.mode.replaceHelp",
  },
] as const satisfies readonly { mode: CopyModeDto; label: string; help: string }[];

const title = computed(() => {
  switch (flow.step.value) {
    case "confirmReplace":
      return t("mods.groups.copy.confirm.title");
    case "done":
      return t("mods.groups.copy.done.title");
    case "pick":
    case "preview":
      return t("mods.groups.copy.title");
    default:
      return flow.step.value satisfies never;
  }
});

const cancelButton = useTemplateRef<{ $el: HTMLElement }>("confirmCancel");
const closeButton = useTemplateRef<{ $el: HTMLElement }>("doneClose");
const chooseButton = useTemplateRef<{ $el: HTMLElement }>("choose");

watch(visible, (isOpen) => {
  if (isOpen) {
    flow.reset();
  }
});

// The dialog's own autofocus runs once, on open; a step that swaps the footer moves focus itself.
watch(flow.step, async (step) => {
  await nextTick();
  if (step === "confirmReplace") {
    cancelButton.value?.$el.focus();
  } else if (step === "done") {
    closeButton.value?.$el.focus();
  }
});

// A copy that ends back on the preview (refused at the copy, or failed) removes the button that was
// focused (Replace everything, or a Merge that is now disabled): focus goes to "Choose" so the
// keyboard user lands on a live control next to the message.
watch(
  () => flow.isCopying.value,
  async (isCopying, wasCopying) => {
    if (!wasCopying || isCopying) {
      return;
    }
    await nextTick();
    if (flow.step.value === "preview") {
      chooseButton.value?.$el.focus();
    }
  },
);

function pathText(path: readonly string[]): string {
  return path.join(" / ");
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="title"
    :style="{ width: '36rem' }"
    :closable="!flow.isCopying.value"
    :close-on-escape="!flow.isCopying.value"
    data-testid="copy-profile-dialog"
  >
    <div
      v-if="flow.step.value === 'pick' || flow.step.value === 'preview'"
      class="flex flex-col gap-3 text-sm"
    >
      <p v-if="flow.step.value === 'pick'">
        {{ t("mods.groups.copy.intro") }}
      </p>
      <div class="flex flex-wrap items-center gap-2">
        <Button
          ref="choose"
          :label="
            flow.sourceDir.value === null
              ? t('mods.groups.copy.choose')
              : t('mods.groups.copy.chooseAnother')
          "
          icon="pi pi-folder-open"
          severity="secondary"
          size="small"
          :disabled="flow.isLoadingPreview.value || flow.isCopying.value"
          autofocus
          data-testid="copy-choose"
          @click="flow.choose()"
        />
        <p
          v-if="flow.sourceDir.value !== null"
          class="text-text-muted min-w-0 break-all"
          data-testid="copy-folder"
        >
          {{ t("mods.groups.copy.folder", { path: flow.sourceDir.value }) }}
        </p>
      </div>

      <template v-if="flow.step.value === 'preview'">
        <fieldset
          role="radiogroup"
          :aria-labelledby="legendId"
          class="flex flex-col gap-2"
        >
          <legend
            :id="legendId"
            class="font-medium"
          >
            {{ t("mods.groups.copy.modeLegend") }}
          </legend>
          <label
            v-for="option in MODE_TEXT"
            :key="option.mode"
            class="flex items-start gap-2"
          >
            <input
              type="radio"
              name="copy-mode"
              class="mt-0.5"
              :value="option.mode"
              :checked="flow.mode.value === option.mode"
              :disabled="flow.isCopying.value"
              :data-testid="`copy-mode-${option.mode}`"
              @change="flow.setMode(option.mode)"
            >
            <span>
              <span class="font-medium">{{ t(option.label) }}</span>
              <span class="text-text-muted block">{{ t(option.help) }}</span>
            </span>
          </label>
        </fieldset>

        <p
          v-if="flow.isLoadingPreview.value"
          class="text-text-muted"
          role="status"
          data-testid="copy-loading"
        >
          {{ t("common.loading") }}
        </p>
        <OrganizationError
          v-if="flow.error.value"
          :error="flow.error.value"
        />
        <Message
          v-if="refusal"
          severity="warn"
          data-testid="copy-refusal"
        >
          {{ tm(copyRefusalMessage(refusal)) }}
        </Message>
        <div
          v-if="report"
          class="flex flex-col gap-2"
          data-testid="copy-preview"
        >
          <p class="font-medium">
            {{ t("mods.groups.copy.previewHeading") }}
          </p>
          <ul
            v-if="copyLines(report).length > 0"
            class="list-disc ps-5"
          >
            <li
              v-for="entry in copyLines(report)"
              :key="entry.id"
              :data-testid="`copy-line-${entry.id}`"
            >
              {{ tm(entry.message) }}
            </li>
          </ul>
          <p
            v-else
            data-testid="copy-nothing"
          >
            {{ t("mods.groups.copy.nothingChanges") }}
          </p>
          <div v-if="report.clashes.length > 0">
            <p
              class="font-medium"
              data-testid="copy-clashes-heading"
            >
              {{ t("mods.groups.copy.clashesHeading", { count: report.clashes.length }, report.clashes.length) }}
            </p>
            <ul class="list-disc ps-5">
              <li
                v-for="(clash, index) in report.clashes"
                :key="index"
                data-testid="copy-clash"
              >
                {{
                  t("mods.groups.copy.clash", {
                    path: pathText(clash.path),
                    kept: t(COLOR_NAME_KEY[clash.kept]),
                    offered: t(COLOR_NAME_KEY[clash.offered]),
                  })
                }}
              </li>
            </ul>
          </div>
          <template v-if="report.mode === 'replace'">
            <p class="font-medium">
              {{ t("mods.groups.copy.discard.heading") }}
            </p>
            <ul class="list-disc ps-5">
              <li
                v-for="entry in discardLines(report)"
                :key="entry.id"
                :data-testid="`copy-${entry.id}`"
              >
                {{ tm(entry.message) }}
              </li>
            </ul>
          </template>
        </div>
      </template>
    </div>

    <div
      v-else-if="flow.step.value === 'confirmReplace' && report"
      class="flex flex-col gap-3 text-sm"
      data-testid="copy-confirm"
    >
      <p>{{ t("mods.groups.copy.confirm.body") }}</p>
      <ul class="list-disc ps-5">
        <li
          v-for="entry in discardLines(report)"
          :key="entry.id"
        >
          {{ tm(entry.message) }}
        </li>
      </ul>
      <OrganizationError
        v-if="flow.error.value"
        :error="flow.error.value"
      />
    </div>

    <div
      v-else-if="flow.step.value === 'done' && report"
      class="flex flex-col gap-2 text-sm"
      role="status"
      data-testid="copy-result"
    >
      <p>
        {{
          report.mode === "replace"
            ? t("mods.groups.copy.done.replaced")
            : t("mods.groups.copy.done.merged")
        }}
      </p>
      <ul
        v-if="copyLines(report).length > 0"
        class="list-disc ps-5"
      >
        <li
          v-for="entry in copyLines(report)"
          :key="entry.id"
          :data-testid="`copy-result-line-${entry.id}`"
        >
          {{ tm(entry.message) }}
        </li>
      </ul>
      <div v-if="report.clashes.length > 0">
        <p class="font-medium">
          {{ t("mods.groups.copy.clashesHeading", { count: report.clashes.length }, report.clashes.length) }}
        </p>
        <ul class="list-disc ps-5">
          <li
            v-for="(clash, index) in report.clashes"
            :key="index"
          >
            {{
              t("mods.groups.copy.clash", {
                path: pathText(clash.path),
                kept: t(COLOR_NAME_KEY[clash.kept]),
                offered: t(COLOR_NAME_KEY[clash.offered]),
              })
            }}
          </li>
        </ul>
      </div>
    </div>

    <template #footer>
      <template v-if="flow.step.value === 'confirmReplace'">
        <Button
          ref="confirmCancel"
          :label="t('common.cancel')"
          severity="secondary"
          autofocus
          :disabled="flow.isCopying.value"
          data-testid="copy-confirm-cancel"
          @click="visible = false"
        />
        <Button
          :label="t('mods.groups.copy.confirm.button')"
          severity="danger"
          :loading="flow.isCopying.value"
          data-testid="copy-confirm-replace"
          @click="flow.runCopy()"
        />
      </template>
      <template v-else-if="flow.step.value === 'done'">
        <Button
          ref="doneClose"
          :label="t('mods.groups.copy.close')"
          data-testid="copy-close"
          @click="visible = false"
        />
      </template>
      <template v-else>
        <Button
          :label="t('common.cancel')"
          severity="secondary"
          :disabled="flow.isCopying.value"
          data-testid="copy-cancel"
          @click="visible = false"
        />
        <Button
          :label="flow.mode.value === 'replace' ? t('mods.groups.copy.replaceButton') : t('mods.groups.copy.mergeButton')"
          :severity="flow.mode.value === 'replace' ? 'danger' : 'primary'"
          :loading="flow.isCopying.value && flow.step.value === 'preview' && report !== null"
          :disabled="!flow.canCopy.value || flow.step.value !== 'preview'"
          data-testid="copy-submit"
          @click="flow.requestCopy()"
        />
      </template>
    </template>
  </Dialog>
</template>
