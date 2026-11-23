<script setup lang="ts">
import Button from "primevue/button";
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useGroupScopePlanQuery } from "@/queries/organization";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import { findNode, type GroupScopeChoice, pathTo } from "@/utils/organization";

/**
 * What both lists are narrowed to, shown above them while a group (or "Ungrouped") is the scope:
 * the group path, its mod count with subgroups, how many of those are not installed, and a way
 * back to every mod. Search and filters work inside the scope; the Active list stays in load
 * order. Renders nothing for "All mods".
 */
const { scope, nodes } = defineProps<{
  scope: GroupScopeChoice;
  /** Every group, depth first. */
  nodes: readonly GroupNodeDto[];
}>();
const emit = defineEmits<{ clear: [] }>();

const { t } = useI18n();

const groupId = computed(() => (scope.kind === "group" ? scope.id : null));
const group = computed(() => findNode(nodes, groupId.value));
const path = computed(() =>
  groupId.value === null
    ? ""
    : pathTo(nodes, groupId.value)
        .map((node) => node.name)
        .join(" / "),
);
const { data: plan } = useGroupScopePlanQuery(groupId);
const notInstalled = computed(() => plan.value?.notInstalled ?? 0);
</script>

<template>
  <div
    v-if="scope.kind !== 'all'"
    class="bg-accent-soft text-text flex shrink-0 flex-wrap items-center gap-x-3 gap-y-1 rounded px-3 py-2 text-sm"
    data-testid="group-scope-bar"
  >
    <template v-if="scope.kind === 'ungrouped'">
      <span data-testid="group-scope-text">{{ t("mods.groups.scope.ungrouped") }}</span>
    </template>
    <template v-else-if="group">
      <span data-testid="group-scope-text">
        <i18n-t
          scope="global"
          keypath="mods.groups.scope.group"
          tag="span"
          :plural="group.scopeCount"
        >
          <template #name>
            <strong>{{ path }}</strong>
          </template>
          <template #count>
            {{ group.scopeCount }}
          </template>
        </i18n-t>
      </span>
      <span
        v-if="notInstalled > 0"
        class="text-text-muted"
        data-testid="group-scope-not-installed"
      >{{ t("mods.groups.scope.notInstalled", { count: notInstalled }, notInstalled) }}</span>
    </template>
    <span class="text-text-muted text-xs">{{ t("mods.groups.scope.note") }}</span>
    <Button
      class="ms-auto"
      :label="t('mods.groups.scope.showAll')"
      size="small"
      severity="secondary"
      text
      data-testid="group-scope-clear"
      @click="emit('clear')"
    />
  </div>
</template>
