<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import GroupRadioList from "@/components/mods/organization/GroupRadioList.vue";
import GroupSearchCreate from "@/components/mods/organization/GroupSearchCreate.vue";
import OrganizationError from "@/components/mods/organization/OrganizationError.vue";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { CommandErrorDescriptor } from "@/utils/errors";
import { type BulkGroupMode, filterGroupsByName } from "@/utils/organization";

/**
 * Add the selected mods to a group, remove them from a group, or move them from one chosen
 * source group to another. Adding can also create the group: the search field filters the list
 * and offers "Create group" for a name no group has, which creates it and adds the mods in one
 * save (`createAndAdd`). Adding never removes; moving is the only way to replace a membership
 * and only touches mods that are direct members of the source. The selection sentence says how
 * many mods this acts on and how many of them the list's filter hides.
 */
const { mode, nodes, selectionText, counts, countsFailed, error, isSubmitting } = defineProps<{
  mode: BulkGroupMode;
  /** Every group, depth first. */
  nodes: readonly GroupNodeDto[];
  /** "12 selected (3 not shown)". */
  selectionText: string;
  /** Selected mods held directly per group; `null` until the backend answered. */
  counts: ReadonlyMap<number, number> | null;
  /** The count request failed, so the counts are unknown (never read as zero). */
  countsFailed: boolean;
  error: CommandErrorDescriptor | null;
  isSubmitting: boolean;
}>();
const emit = defineEmits<{
  choose: [groupId: number];
  move: [from: number, to: number];
  /** Create a group (inside `parent`, `null` for top level) and add the selected mods to it. */
  createAndAdd: [name: string, parent: number | null];
  /** The dialog has finished leaving the page. */
  afterHide: [];
}>();
const visible = defineModel<boolean>("visible", { required: true });

const { t } = useI18n();

const target = ref<number | null>(null);
const source = ref<number | null>(null);
const query = ref("");

watch(visible, (isOpen) => {
  if (isOpen) {
    target.value = null;
    source.value = null;
    query.value = "";
  }
});

/** The add list, narrowed by the search; a target the search hides is no longer chosen. */
const matchingNodes = computed(() => filterGroupsByName(nodes, query.value));
watch(matchingNodes, (shown) => {
  if (mode === "add" && target.value !== null && !shown.some((node) => node.id === target.value)) {
    target.value = null;
  }
});

// A group chosen as the target is no longer one once it becomes the source.
watch(source, (chosen) => {
  if (chosen !== null && chosen === target.value) {
    target.value = null;
  }
});

const needsCounts = computed(() => mode !== "add");
const isLoading = computed(() => needsCounts.value && counts === null && !countsFailed);

/** Groups that hold at least one selected mod directly: the possible sources of a remove or move. */
const sourceNodes = computed(() =>
  counts === null ? [] : nodes.filter((node) => (counts.get(node.id) ?? 0) > 0),
);

const sameAsSource = computed(() => {
  const reasons = new Map<number, string>();
  if (source.value !== null) {
    reasons.set(source.value, t("mods.bulk.dialog.sameGroup"));
  }
  return reasons;
});

const title = computed(() => {
  switch (mode) {
    case "add":
      return t("mods.bulk.dialog.addTitle");
    case "remove":
      return t("mods.bulk.dialog.removeTitle");
    case "move":
      return t("mods.bulk.dialog.moveTitle");
    default:
      return mode satisfies never;
  }
});

const submitLabel = computed(() => {
  switch (mode) {
    case "add":
      return t("mods.bulk.dialog.addButton");
    case "remove":
      return t("mods.bulk.dialog.removeButton");
    case "move":
      return t("mods.bulk.dialog.moveButton");
    default:
      return mode satisfies never;
  }
});

const canSubmit = computed(() =>
  mode === "move" ? source.value !== null && target.value !== null : target.value !== null,
);

function submit(): void {
  if (mode === "move") {
    if (source.value !== null && target.value !== null) {
      emit("move", source.value, target.value);
    }
    return;
  }
  if (target.value !== null) {
    emit("choose", target.value);
  }
}
</script>

<template>
  <Dialog
    v-model:visible="visible"
    modal
    :header="title"
    :style="{ width: '30rem' }"
    data-testid="bulk-groups-dialog"
    @after-hide="emit('afterHide')"
  >
    <div class="flex flex-col gap-3 text-sm">
      <p data-testid="bulk-groups-selection">
        {{ selectionText }}
      </p>
      <p
        v-if="nodes.length === 0 && mode !== 'add'"
        data-testid="bulk-groups-empty"
      >
        {{ t("mods.bulk.dialog.noGroups") }}
      </p>
      <p
        v-else-if="isLoading"
        class="text-text-muted"
        data-testid="bulk-groups-loading"
      >
        {{ t("mods.bulk.dialog.loading") }}
      </p>
      <p
        v-else-if="needsCounts && countsFailed"
        class="text-status-danger"
        role="alert"
        data-testid="bulk-groups-failed"
      >
        {{ t("mods.bulk.dialog.failed") }}
      </p>
      <p
        v-else-if="needsCounts && sourceNodes.length === 0"
        data-testid="bulk-groups-none-held"
      >
        {{ t("mods.bulk.dialog.noneInGroups") }}
      </p>
      <template v-else-if="mode === 'add'">
        <p class="text-text-muted">
          {{ t("mods.bulk.dialog.addHelp") }}
        </p>
        <GroupSearchCreate
          v-model:query="query"
          :nodes="nodes"
          :is-busy="isSubmitting"
          target="selection"
          @create="(name, parent) => emit('createAndAdd', name, parent)"
        />
        <p
          v-if="nodes.length === 0"
          class="text-text-muted"
          data-testid="bulk-groups-empty"
        >
          {{ t("mods.groups.dialog.modGroupsEmpty") }}
        </p>
        <p
          v-else-if="matchingNodes.length === 0"
          class="text-text-muted"
          data-testid="bulk-groups-no-match"
        >
          {{ t("mods.groups.search.noMatch") }}
        </p>
        <GroupRadioList
          v-else
          v-model="target"
          :nodes="matchingNodes"
          :legend="t('mods.bulk.dialog.targetLegend')"
          name="bulk-target"
        />
      </template>
      <template v-else-if="mode === 'remove'">
        <p class="text-text-muted">
          {{ t("mods.bulk.dialog.removeHelp") }}
        </p>
        <GroupRadioList
          v-model="target"
          :nodes="sourceNodes"
          :legend="t('mods.bulk.dialog.removeLegend')"
          name="bulk-target"
          :counts="counts"
        />
      </template>
      <template v-else>
        <p class="text-text-muted">
          {{ t("mods.bulk.dialog.moveHelp") }}
        </p>
        <GroupRadioList
          v-model="source"
          :nodes="sourceNodes"
          :legend="t('mods.bulk.dialog.fromLegend')"
          is-legend-visible
          name="bulk-source"
          :counts="counts"
        />
        <GroupRadioList
          v-model="target"
          :nodes="nodes"
          :legend="t('mods.bulk.dialog.toLegend')"
          is-legend-visible
          name="bulk-target"
          :disabled-reasons="sameAsSource"
        />
      </template>
      <OrganizationError
        v-if="error"
        :error="error"
      />
    </div>
    <template #footer>
      <Button
        :label="t('common.cancel')"
        severity="secondary"
        data-testid="bulk-groups-cancel"
        @click="visible = false"
      />
      <Button
        :label="submitLabel"
        :loading="isSubmitting"
        :disabled="!canSubmit"
        data-testid="bulk-groups-submit"
        @click="submit"
      />
    </template>
  </Dialog>
</template>
