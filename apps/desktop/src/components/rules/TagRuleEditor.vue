<script setup lang="ts">
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Select from "primevue/select";
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { formatList } from "@/i18n/format";
import type { SetManualTagRequestDto } from "@/types/generated/SetManualTagRequestDto";
import type { TagAssignmentDto } from "@/types/generated/TagAssignmentDto";
import type { TagModeDto } from "@/types/generated/TagModeDto";
import { tagProvenanceKindLabel } from "@/utils/tagProvenance";
import { tagSignalLabel } from "@/utils/tagSignal";

/**
 * A signal-based "tag rule" editor (url, def-namespace, def-prefix,
 * assembly-ref, and depends-on signals) was designed, but the backend
 * exposes only `list_tags` (inference plus manual overrides, already
 * computed) and `set_manual_tag` — there is no `upsert_tag_rule`
 * command, so inference signals aren't editable from here. This instead
 * lists every current assignment, inferred (with its matched signals and
 * confidence) or manual, and lets the user add or remove a manual
 * override — the whole editable surface the backend actually offers.
 */
const { assignments } = defineProps<{ assignments: TagAssignmentDto[] }>();
const emit = defineEmits<{ save: [request: SetManualTagRequestDto] }>();
const { t, locale } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();

// A plain (non-`readonly`) mutable array: PrimeVue's `Select` types its
// `options` prop as `any[]`, which a `const`-asserted tuple can't
// satisfy. Resolved through `t()` reactively, so a locale switch
// updates the labels.
const modes = computed<{ label: string; value: TagModeDto }[]>(() => [
  { label: t("rules.tagEditor.modeAdd"), value: "add" },
  { label: t("rules.tagEditor.modeRemove"), value: "remove" },
]);

const modId = ref("");
const tag = ref("");
const mode = ref<TagModeDto>("add");

function submit(): void {
  emit("save", { modId: modId.value, tag: tag.value, mode: mode.value });
  modId.value = "";
  tag.value = "";
}
</script>

<template>
  <div
    class="flex flex-col gap-4"
    data-testid="tag-rule-editor"
  >
    <div class="surface-card overflow-hidden">
      <table class="w-full border-collapse text-sm">
        <thead>
          <tr class="table-head text-left">
            <th
              scope="col"
              class="py-1 font-medium"
            >
              {{ t("rules.tagEditor.modHeader") }}
            </th>
            <th
              scope="col"
              class="py-1 font-medium"
            >
              {{ t("rules.tagEditor.tagHeader") }}
            </th>
            <th
              scope="col"
              class="py-1 font-medium"
            >
              {{ t("rules.tagEditor.sourceHeader") }}
            </th>
            <th
              scope="col"
              class="py-1 font-medium"
            >
              {{ t("rules.tagEditor.confidenceHeader") }}
            </th>
            <th
              scope="col"
              class="py-1 font-medium"
            >
              {{ t("rules.tagEditor.signalsHeader") }}
            </th>
          </tr>
        </thead>
        <tbody>
          <tr
            v-for="assignment in assignments"
            :key="`${assignment.modId}|${assignment.tag}`"
            class="border-border-subtle border-b"
            :data-testid="`tag-assignment-${assignment.modId}-${assignment.tag}`"
          >
            <td :title="modLabel.titleFor(assignment.modId)">
              {{ modLabel.label(assignment.modId) }}
            </td>
            <td>{{ assignment.tag }}</td>
            <td>{{ tm(tagProvenanceKindLabel(assignment.provenance.kind)) }}</td>
            <td>
              <span v-if="assignment.provenance.kind === 'inferred'">{{ assignment.provenance.confidence }}%</span>
            </td>
            <td>
              <span v-if="assignment.provenance.kind === 'inferred'">{{
                formatList(
                  locale,
                  assignment.provenance.matched.map((signal) => tagSignalLabel(signal, t, modLabel.label)),
                )
              }}</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <form
      class="flex flex-wrap items-end gap-2"
      @submit.prevent="submit"
    >
      <label class="flex flex-col gap-1 text-sm">
        {{ t("rules.tagEditor.modIdLabel") }}
        <InputText
          v-model="modId"
          data-testid="manual-tag-mod-id"
          required
        />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        {{ t("rules.tagEditor.tagLabel") }}
        <InputText
          v-model="tag"
          data-testid="manual-tag-tag"
          required
        />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        {{ t("rules.tagEditor.modeLabel") }}
        <Select
          v-model="mode"
          :options="modes"
          option-label="label"
          option-value="value"
          data-testid="manual-tag-mode"
        />
      </label>
      <Button
        type="submit"
        :label="t('rules.tagEditor.saveButton')"
        data-testid="manual-tag-submit"
      />
    </form>
  </div>
</template>
