<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import type { DefConflictToucherDto } from "@/types/generated/DefConflictToucherDto";
import type { ProblemDto } from "@/types/generated/ProblemDto";
import type { ToucherRoleDto } from "@/types/generated/ToucherRoleDto";
import { assertNever } from "@/utils/assertNever";

/**
 * The touchers table: every owner and patcher in the selected order,
 * with a role badge, its own top-level op count, and whether the def's
 * sequential fold actually reached it. `DefConflictToucherDto` carries no
 * `reached` flag of its own (unlike `PatcherDto.reached` in the def
 * inspector, `DefPage.vue`) — derived here instead from `problems`' own
 * `UnsupportedOp` stopper: the fold applies each active patcher's ops in
 * load order and stops at the first one it can't replay, so any toucher
 * positioned after the stopper's own mod never had its ops applied.
 * `null` when there's no stopper at all, in which case every toucher
 * reached.
 */
const { touchers, problems } = defineProps<{
  touchers: DefConflictToucherDto[];
  problems: ProblemDto[];
}>();

const { t } = useI18n();
const modLabel = useModLabel();

/** A short, human-readable label for one {@link ToucherRoleDto} — local to this table, the only place it renders. */
function toucherRoleLabel(role: ToucherRoleDto): string {
  switch (role) {
    case "owner":
      return t("inbox.touchersList.roleOwner");
    case "patcher":
      return t("inbox.touchersList.rolePatcher");
    default:
      return assertNever(role);
  }
}

const stopperPosition = computed<number | null>(() => {
  const stopper = problems.find(
    (problem): problem is Extract<ProblemDto, { kind: "unsupportedOp" }> =>
      problem.kind === "unsupportedOp",
  );
  if (!stopper) {
    return null;
  }
  return touchers.find((toucher) => toucher.modId === stopper.modId)?.position ?? null;
});

function reached(toucher: DefConflictToucherDto): boolean {
  return stopperPosition.value === null || toucher.position <= stopperPosition.value;
}
</script>

<template>
  <table
    class="w-full text-left text-xs"
    data-testid="def-conflict-touchers"
  >
    <thead>
      <tr class="text-text-faint uppercase">
        <th class="py-1 font-medium">
          {{ t("inbox.touchersList.mod") }}
        </th>
        <th class="py-1 font-medium">
          {{ t("inbox.touchersList.role") }}
        </th>
        <th class="py-1 font-medium">
          {{ t("inbox.touchersList.ops") }}
        </th>
        <th class="py-1 font-medium">
          {{ t("inbox.touchersList.replay") }}
        </th>
      </tr>
    </thead>
    <tbody>
      <tr
        v-for="toucher in touchers"
        :key="`${toucher.role}-${toucher.modId}`"
        class="border-border-subtle border-t"
        data-testid="def-conflict-toucher-row"
        :data-mod-id="toucher.modId"
      >
        <td class="py-1">
          <span :title="modLabel.titleFor(toucher.modId)">{{ modLabel.label(toucher.modId) }}</span>
          <span
            v-if="toucher.isGenerated"
            class="text-text-faint"
          >{{ t("inbox.touchersList.generatedSuffix") }}</span>
        </td>
        <td class="py-1">
          <span
            class="rounded px-1.5 py-0.5"
            :class="toucher.role === 'owner' ? 'bg-surface-2 text-text-muted' : 'bg-accent/10 text-accent'"
          >{{ toucherRoleLabel(toucher.role) }}</span>
        </td>
        <td class="py-1">
          {{ toucher.opCount }}
        </td>
        <td class="py-1">
          <span
            :class="reached(toucher) ? 'text-status-auto' : 'text-text-faint'"
            data-testid="def-conflict-toucher-reached"
          >{{ reached(toucher) ? t("inbox.touchersList.reached") : t("inbox.touchersList.notReached") }}</span>
        </td>
      </tr>
    </tbody>
  </table>
</template>
