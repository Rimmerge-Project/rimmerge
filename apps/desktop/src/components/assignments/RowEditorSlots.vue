<script setup lang="ts">
// The row editor's item-slot pickers and their paired chance inputs.
import InputNumber from "primevue/inputnumber";
import { useI18n } from "vue-i18n";
import ItemPicker from "@/components/assignments/ItemPicker.vue";
import { useRowEditorContext } from "@/composables/rowEditorContext";
import { formatList } from "@/i18n/format";
import type { AssignmentDetailDto } from "@/types/generated/AssignmentDetailDto";

const { assignment } = defineProps<{ assignment: AssignmentDetailDto }>();
const { t, locale } = useI18n();

const {
  modLabel,
  itemSlotFields,
  chanceFields,
  prefillSource,
  clampedFrom,
  droppedFromCopy,
  namesFor,
  setNames,
  chanceAt,
  setChanceAt,
} = useRowEditorContext();
</script>

<template>
  <div
    v-for="[path, spec] in itemSlotFields"
    :key="path"
    class="flex flex-col gap-1"
  >
    <span class="text-text-muted text-xs">
      {{ path }} ({{ spec.role.type === "itemSlot" ? spec.role.defType : "" }})
      <span
        v-if="prefillSource[path]"
        class="text-status-input"
        :data-testid="`row-editor-prefill-${path}`"
      >
        {{ t("assignments.rowEditor.prefillFrom", { mod: modLabel.label(prefillSource[path]) }) }}
      </span>
      <span
        v-if="clampedFrom[path]"
        class="text-status-danger"
        :data-testid="`row-editor-clamped-${path}`"
      >
        {{
          t("assignments.rowEditor.clampedTo", {
            name: namesFor(path)[0],
            count: clampedFrom[path].length,
            joined: formatList(locale, clampedFrom[path]),
          })
        }}
      </span>
      <span
        v-if="droppedFromCopy[path]"
        class="text-status-danger"
        :data-testid="`row-editor-copy-dropped-${path}`"
      >
        {{
          t(
            "assignments.rowEditor.droppedFromCopy",
            {
              count: droppedFromCopy[path].length,
              names: formatList(
                locale,
                droppedFromCopy[path].map((value) => value.name),
              ),
            },
            droppedFromCopy[path].length,
          )
        }}
      </span>
    </span>
    <ItemPicker
      :def-type="spec.role.type === 'itemSlot' ? spec.role.defType : ''"
      :selected="namesFor(path)"
      :cardinality="spec.cardinality"
      :assignment-id="assignment.id"
      @update:selected="(names) => setNames(path, names)"
    />
  </div>

  <div
    v-for="[path, spec] in chanceFields"
    :key="path"
    class="flex flex-col gap-1"
  >
    <span class="text-text-muted text-xs">
      {{ path }} ({{
        t("assignments.rowEditor.pairedTo", {
          slot: spec.role.type === "chances" ? spec.role.forSlot : "",
        })
      }})
      <span
        v-if="prefillSource[path]"
        class="text-status-input"
        :data-testid="`row-editor-prefill-${path}`"
      >
        {{ t("assignments.rowEditor.prefillFrom", { mod: modLabel.label(prefillSource[path]) }) }}
      </span>
    </span>
    <div class="flex flex-wrap gap-1">
      <InputNumber
        v-for="(_, index) in namesFor(spec.role.type === 'chances' ? spec.role.forSlot : '')"
        :key="index"
        :model-value="chanceAt(path, index)"
        size="small"
        class="w-20"
        locale="en-US"
        :use-grouping="false"
        :min-fraction-digits="0"
        :max-fraction-digits="6"
        :data-testid="`row-editor-chance-${path}-${index}`"
        @update:model-value="(value: number | null) => setChanceAt(path, index, value)"
      />
    </div>
  </div>
</template>
