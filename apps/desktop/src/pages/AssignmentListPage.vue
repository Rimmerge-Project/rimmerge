<script setup lang="ts">
import Button from "primevue/button";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import { formatList } from "@/i18n/format";
import { useAssignmentsQuery } from "@/queries/assignments";

/** The patch maker's list page — `PatchListPage`'s own layout. */
const { t, locale } = useI18n();
const router = useRouter();
const { data: assignments, isPending, error } = useAssignmentsQuery();

function openNew(): void {
  void router.push({ name: "assignment-new" });
}

function openAssignment(assignmentId: string): void {
  void router.push({ name: "assignment-detail", params: { assignmentId } });
}
</script>

<template>
  <div class="mx-auto flex max-w-6xl flex-col gap-6 p-6">
    <div class="flex items-center justify-between gap-4">
      <h1 class="text-text text-xl font-semibold">
        {{ t("shell.nav.assignments") }}
      </h1>
      <Button
        :label="t('assignments.list.newProjectButton')"
        size="small"
        data-testid="new-assignment-button"
        @click="openNew"
      />
    </div>

    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="assignments-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="assignments-error"
    >
      {{ error instanceof Error ? error.message : t("assignments.list.loadFailed") }}
    </p>
    <p
      v-else-if="(assignments ?? []).length === 0"
      class="text-text-faint text-sm"
      data-testid="assignments-empty"
    >
      {{ t("assignments.list.empty") }}
    </p>
    <table
      v-else
      class="w-full text-left text-sm"
      data-testid="assignments-table"
    >
      <thead>
        <tr class="text-text-muted text-xs uppercase">
          <th class="py-1 pr-2">
            {{ t("assignments.list.nameHeader") }}
          </th>
          <th class="py-1 pr-2">
            {{ t("assignments.list.packageIdHeader") }}
          </th>
          <th class="py-1 pr-2">
            {{ t("assignments.list.sectionsHeader") }}
          </th>
          <th class="py-1 pr-2">
            {{ t("assignments.list.refsHeader") }}
          </th>
          <th class="py-1 pr-2">
            {{ t("assignments.list.targetsHeader") }}
          </th>
          <th class="py-1 pr-2">
            {{ t("assignments.list.rowsHeader") }}
          </th>
          <th class="py-1 pr-2">
            {{ t("assignments.list.uncoveredHeader") }}
          </th>
          <th class="py-1 pr-2">
            {{ t("assignments.list.exportedHeader") }}
          </th>
        </tr>
      </thead>
      <tbody>
        <tr
          v-for="assignment in assignments"
          :key="assignment.id"
          class="border-border-subtle hover:bg-surface-2 cursor-pointer border-t"
          :data-testid="`assignment-row-${assignment.id}`"
          @click="openAssignment(assignment.id)"
        >
          <td class="py-1.5 pr-2">
            {{ assignment.name }}
          </td>
          <td class="py-1.5 pr-2 font-mono text-xs">
            {{ assignment.packageId }}
          </td>
          <td class="py-1.5 pr-2 text-xs">
            {{ formatList(locale, assignment.defTypes) }}
          </td>
          <td class="py-1.5 pr-2 text-xs">
            {{ assignment.refs.length }}
          </td>
          <td class="py-1.5 pr-2 text-xs">
            {{ assignment.targets.length }}
          </td>
          <td class="py-1.5 pr-2 text-xs">
            {{ assignment.rowCount }}
          </td>
          <td class="py-1.5 pr-2 text-xs">
            <span
              v-if="assignment.uncoveredCount === null"
              class="text-text-faint"
            >
              {{ t("assignments.list.notApplicable") }}
            </span>
            <span
              v-else
              :class="assignment.uncoveredCount > 0 ? 'text-status-input' : 'text-text-faint'"
            >
              {{ assignment.uncoveredCount }}
            </span>
          </td>
          <td class="py-1.5 pr-2 text-xs">
            {{
              assignment.exportDir
                ? t("assignments.list.exportedYes")
                : t("assignments.list.exportedNo")
            }}
          </td>
        </tr>
      </tbody>
    </table>
  </div>
</template>
