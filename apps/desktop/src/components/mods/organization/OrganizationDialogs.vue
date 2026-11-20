<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import { useToast } from "primevue/usetoast";
import { computed, ref, type WritableComputedRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import BulkGroupsDialog from "@/components/mods/organization/BulkGroupsDialog.vue";
import ColorPickerDialog from "@/components/mods/organization/ColorPickerDialog.vue";
import GroupDeleteDialog from "@/components/mods/organization/GroupDeleteDialog.vue";
import GroupMoveDialog from "@/components/mods/organization/GroupMoveDialog.vue";
import GroupNameDialog from "@/components/mods/organization/GroupNameDialog.vue";
import ModGroupsDialog from "@/components/mods/organization/ModGroupsDialog.vue";
import OrganizationError from "@/components/mods/organization/OrganizationError.vue";
import StartOverDialog from "@/components/mods/organization/StartOverDialog.vue";
import { useModLabel } from "@/composables/useModLabel";
import {
  useGroupMembershipCountsQuery,
  useGroupScopePlanQuery,
  useOrganizationMutations,
} from "@/queries/organization";
import type { ColorTokenDto } from "@/types/generated/ColorTokenDto";
import type { DeleteModeDto } from "@/types/generated/DeleteModeDto";
import type { GroupNodeDto } from "@/types/generated/GroupNodeDto";
import type { OrganizationEditOutcomeDto } from "@/types/generated/OrganizationEditOutcomeDto";
import { assertNever } from "@/utils/assertNever";
import { type CommandErrorDescriptor, describeCommandError } from "@/utils/errors";
import { findNode, type OrganizationDialog } from "@/utils/organization";
import { selectionSummary } from "@/utils/selectionText";

/**
 * Every organization dialog, and the one edit each makes. The page only says which dialog to open
 * (`dialog`); this runs the edit, shows its refusal inside the dialog, toasts the outcome and
 * closes. None of these edits touches the selection, the working set, the load order or
 * `ModsConfig.xml`: they call only the organization commands.
 */
const { nodes } = defineProps<{
  /** Every group, depth first. */
  nodes: readonly GroupNodeDto[];
}>();
const emit = defineEmits<{
  /** A group was deleted and its dialog has finished closing: focus can go back to the tree. */
  groupDeleted: [groupId: number];
}>();
const dialog = defineModel<OrganizationDialog | null>("dialog", { required: true });

const { t } = useI18n();
const toast = useToast();
const modLabel = useModLabel();
const edits = useOrganizationMutations();

const error = ref<CommandErrorDescriptor | null>(null);
const isBusy = computed(() => Object.values(edits).some((mutation) => mutation.isLoading.value));

function close(): void {
  dialog.value = null;
  error.value = null;
}

/** Runs one edit; a refusal stays in the dialog, a success closes it unless `keepOpen`. */
async function run(edit: () => Promise<void>, options: { keepOpen?: boolean } = {}): Promise<void> {
  error.value = null;
  try {
    await edit();
    if (options.keepOpen !== true) {
      close();
    }
  } catch (caught: unknown) {
    error.value = describeCommandError(caught);
  }
}

function say(summary: string, detail?: string): void {
  toast.add({
    severity: "success",
    summary,
    ...(detail === undefined ? {} : { detail }),
    life: 4000,
  });
}

/** A dialog flag: open while `dialog` is of that kind, and closing it clears `dialog`. */
function isOpen(kind: OrganizationDialog["kind"]): WritableComputedRef<boolean> {
  return computed({
    get: () => dialog.value?.kind === kind,
    set: (open) => {
      if (!open) {
        close();
      }
    },
  });
}

const isCreateOpen = isOpen("createGroup");
const isRenameOpen = isOpen("renameGroup");
const isDeleteOpen = isOpen("deleteGroup");
const isGroupColorOpen = isOpen("groupColor");
const isMoveOpen = isOpen("moveGroup");
const isRemoveUninstalledOpen = isOpen("removeUninstalled");
const isModColorOpen = isOpen("modColor");
const isModGroupsOpen = isOpen("modGroups");
const isStartOverOpen = isOpen("startOver");
const isBulkGroupsOpen = isOpen("bulkGroups");
const isBulkColorOpen = isOpen("bulkColor");

/** The group a dialog about one group is open for. */
const groupId = computed(() => {
  const current = dialog.value;
  if (current === null) {
    return null;
  }
  switch (current.kind) {
    case "renameGroup":
    case "deleteGroup":
    case "groupColor":
    case "moveGroup":
    case "removeUninstalled":
      return current.id;
    case "createGroup":
    case "modColor":
    case "modGroups":
    case "startOver":
    case "bulkGroups":
    case "bulkColor":
      return null;
    default:
      return assertNever(current);
  }
});
const group = computed(() => findNode(nodes, groupId.value));
const parentOfNew = computed(() =>
  dialog.value?.kind === "createGroup" ? (findNode(nodes, dialog.value.parent) ?? null) : null,
);

const modName = computed(() => {
  const current = dialog.value;
  return current?.kind === "modColor" || current?.kind === "modGroups"
    ? modLabel.label(current.modId)
    : "";
});
const modGroupIds = computed(() =>
  dialog.value?.kind === "modGroups" ? dialog.value.groupIds : [],
);
const modColorNow = computed<ColorTokenDto | null>(() =>
  dialog.value?.kind === "modColor" ? dialog.value.current : null,
);

type BulkDialog = Extract<OrganizationDialog, { kind: "bulkGroups" | "bulkColor" }>;

/**
 * The bulk dialogs' selection, kept until the dialog has finished leaving: closing sets `dialog`
 * to `null` at once, and a dialog that read it directly would flip to its empty state (the
 * "add" layout, no selection sentence) for the length of the leave animation.
 */
const bulk = ref<BulkDialog | null>(null);
watch(
  dialog,
  (current) => {
    if (current?.kind === "bulkGroups" || current?.kind === "bulkColor") {
      bulk.value = current;
    }
  },
  { immediate: true },
);

function onBulkDialogGone(): void {
  const current = dialog.value;
  if (current?.kind !== "bulkGroups" && current?.kind !== "bulkColor") {
    bulk.value = null;
  }
}

const bulkText = computed(() =>
  bulk.value === null ? "" : selectionSummary(t, bulk.value.modIds.length, bulk.value.hiddenCount),
);
const bulkMode = computed(() => (bulk.value?.kind === "bulkGroups" ? bulk.value.mode : "add"));
// Remove and move list the groups that hold a selected mod, so they ask the backend which do.
const { counts: bulkCounts, hasFailed: bulkCountsFailed } = useGroupMembershipCountsQuery(() =>
  bulk.value?.kind === "bulkGroups" && bulk.value.mode !== "add" ? bulk.value.modIds : null,
);

const { data: scopePlan, error: scopePlanError } = useGroupScopePlanQuery(() =>
  dialog.value?.kind === "removeUninstalled" ? dialog.value.id : null,
);

/** The mods the "remove uninstalled" dialog lists, by the names the lists use; `null` until known. */
const uninstalledNames = computed(
  () => scopePlan.value?.notInstalledIds.map((id) => modLabel.label(id)) ?? null,
);

function createGroup(name: string): Promise<void> {
  const current = dialog.value;
  const parent = current?.kind === "createGroup" ? current.parent : null;
  return run(async () => {
    await edits.createGroup.mutateAsync({ name, parent });
    say(t("mods.groups.toast.created"));
  });
}

function renameGroup(name: string): Promise<void> {
  const id = groupId.value;
  if (id === null) {
    return Promise.resolve();
  }
  return run(async () => {
    await edits.renameGroup.mutateAsync({ id, name });
    say(t("mods.groups.toast.renamed"));
  });
}

/** Set by a successful delete, read once the delete dialog has finished leaving the page. */
const deletedGroupId = ref<number | null>(null);

function deleteGroup(mode: DeleteModeDto): Promise<void> {
  const id = groupId.value;
  if (id === null) {
    return Promise.resolve();
  }
  return run(async () => {
    await edits.deleteGroup.mutateAsync({ id, mode });
    deletedGroupId.value = id;
    say(t("mods.groups.toast.deleted"));
  });
}

function onDeleteDialogGone(): void {
  const id = deletedGroupId.value;
  deletedGroupId.value = null;
  if (id !== null) {
    emit("groupDeleted", id);
  }
}

function colorGroup(color: ColorTokenDto | null): Promise<void> {
  const id = groupId.value;
  if (id === null) {
    return Promise.resolve();
  }
  return run(async () => {
    await edits.setGroupColor.mutateAsync({ id, color });
    say(t("mods.colors.toast.groupChanged"));
  });
}

function moveGroup(parent: number | null): Promise<void> {
  const id = groupId.value;
  if (id === null) {
    return Promise.resolve();
  }
  return run(async () => {
    await edits.reparentGroup.mutateAsync({ id, parent });
    say(t("mods.groups.toast.moved"));
  });
}

function colorMod(color: ColorTokenDto | null): Promise<void> {
  const current = dialog.value;
  if (current?.kind !== "modColor") {
    return Promise.resolve();
  }
  return run(async () => {
    await edits.setModColors.mutateAsync({ modIds: [current.modId], color });
    say(
      color === null
        ? t("mods.colors.toast.modCleared", { name: modName.value })
        : t("mods.colors.toast.modChanged", { name: modName.value }),
    );
  });
}

function toggleMembership(id: number, isMember: boolean): Promise<void> {
  const current = dialog.value;
  if (current?.kind !== "modGroups") {
    return Promise.resolve();
  }
  const request = { id, modIds: [current.modId] };
  return run(
    async () => {
      await (isMember ? edits.addMembers : edits.removeMembers).mutateAsync(request);
      // The checkbox follows what was saved. The dialog may have closed meanwhile.
      const latest = dialog.value;
      if (latest?.kind === "modGroups") {
        const others = latest.groupIds.filter((groupIdValue) => groupIdValue !== id);
        dialog.value = { ...latest, groupIds: isMember ? [...others, id] : others };
      }
    },
    { keepOpen: true },
  );
}

function removedCounts(outcome: OrganizationEditOutcomeDto): { removed: number; absent: number } {
  switch (outcome.kind) {
    case "removed":
      return { removed: outcome.removed, absent: outcome.absent };
    case "created":
    case "done":
    case "deleted":
    case "added":
    case "moved":
    case "colored":
      return { removed: 0, absent: 0 };
    default:
      return assertNever(outcome);
  }
}

function removeUninstalled(): Promise<void> {
  const id = groupId.value;
  const planned = scopePlan.value?.notInstalledIds;
  if (id === null || planned === undefined) {
    return Promise.resolve();
  }
  return run(async () => {
    // Exactly the mods the dialog lists; the backend removes only those still not installed.
    const outcome = await edits.removeUninstalled.mutateAsync({ id, modIds: planned });
    const { removed, absent } = removedCounts(outcome);
    // Listed mods neither removed nor already out of the group were installed again since the plan.
    const kept = planned.length - removed - absent;
    say(
      t("mods.groups.toast.removedUninstalled", { count: removed }, removed),
      kept > 0 ? t("mods.groups.toast.keptInstalledAgain", { count: kept }, kept) : undefined,
    );
  });
}

/**
 * One toast per action: the summary, and any further sentences as its detail, one per line (a
 * sentence never holds two numbers, so each count is its own line; the toast host keeps the line
 * breaks).
 */
function sayCounts(summary: string, details: readonly (string | null)[]): void {
  const lines = details.filter((line): line is string => line !== null);
  say(summary, lines.length === 0 ? undefined : lines.join("\n"));
}

function bulkChoose(id: number): Promise<void> {
  const current = dialog.value;
  const target = findNode(nodes, id);
  if (current?.kind !== "bulkGroups" || target === undefined) {
    return Promise.resolve();
  }
  const request = { id, modIds: [...current.modIds] };
  return run(async () => {
    if (current.mode === "add") {
      const outcome = await edits.addMembers.mutateAsync(request);
      if (outcome.kind === "added") {
        sayCounts(
          t("mods.bulk.toast.added", { count: outcome.added, name: target.name }, outcome.added),
          [
            outcome.already > 0
              ? t("mods.bulk.toast.alreadyIn", { count: outcome.already }, outcome.already)
              : null,
          ],
        );
      }
      return;
    }
    const outcome = await edits.removeMembers.mutateAsync(request);
    if (outcome.kind === "removed") {
      sayCounts(
        t(
          "mods.bulk.toast.removed",
          { count: outcome.removed, name: target.name },
          outcome.removed,
        ),
        [
          outcome.absent > 0
            ? t("mods.bulk.toast.notIn", { count: outcome.absent }, outcome.absent)
            : null,
        ],
      );
    }
  });
}

function bulkMove(from: number, to: number): Promise<void> {
  const current = dialog.value;
  const source = findNode(nodes, from);
  const target = findNode(nodes, to);
  if (current?.kind !== "bulkGroups" || source === undefined || target === undefined) {
    return Promise.resolve();
  }
  return run(async () => {
    const outcome = await edits.moveMembers.mutateAsync({
      from,
      to,
      modIds: [...current.modIds],
    });
    if (outcome.kind !== "moved") {
      return;
    }
    const left = outcome.moved + outcome.alreadyInTarget;
    sayCounts(
      t("mods.bulk.toast.moved", { count: left, from: source.name, to: target.name }, left),
      [
        outcome.notInSource > 0
          ? t(
              "mods.bulk.toast.notInSource",
              { count: outcome.notInSource, name: source.name },
              outcome.notInSource,
            )
          : null,
        outcome.alreadyInTarget > 0
          ? t(
              "mods.bulk.toast.alreadyInTarget",
              { count: outcome.alreadyInTarget, name: target.name },
              outcome.alreadyInTarget,
            )
          : null,
      ],
    );
  });
}

function bulkColor(color: ColorTokenDto | null): Promise<void> {
  const current = dialog.value;
  if (current?.kind !== "bulkColor") {
    return Promise.resolve();
  }
  return run(async () => {
    const outcome = await edits.setModColors.mutateAsync({ modIds: [...current.modIds], color });
    if (outcome.kind !== "colored") {
      return;
    }
    const summary =
      color === null
        ? t("mods.bulk.toast.colorCleared", { count: outcome.changed }, outcome.changed)
        : t("mods.bulk.toast.colorSet", { count: outcome.changed }, outcome.changed);
    sayCounts(summary, [
      outcome.unchanged > 0
        ? t("mods.bulk.toast.unchanged", { count: outcome.unchanged }, outcome.unchanged)
        : null,
    ]);
  });
}

function startOver(): Promise<void> {
  return run(async () => {
    await edits.resetOrganization.mutateAsync();
    say(t("mods.groups.toast.startedOver"));
  });
}
</script>

<template>
  <GroupNameDialog
    v-model:visible="isCreateOpen"
    mode="create"
    initial-name=""
    :parent-name="parentOfNew?.name ?? null"
    :error="error"
    :is-submitting="isBusy"
    @submit="createGroup"
  />
  <GroupNameDialog
    v-model:visible="isRenameOpen"
    mode="rename"
    :initial-name="group?.name ?? ''"
    :parent-name="null"
    :error="error"
    :is-submitting="isBusy"
    @submit="renameGroup"
  />
  <GroupDeleteDialog
    v-model:visible="isDeleteOpen"
    :group="group ?? null"
    :error="error"
    :is-submitting="isBusy"
    @confirm="deleteGroup"
    @after-hide="onDeleteDialogGone"
  />
  <ColorPickerDialog
    v-model:visible="isGroupColorOpen"
    subject="group"
    :current="group?.color ?? null"
    :error="error"
    :is-submitting="isBusy"
    @choose="colorGroup"
  />
  <ColorPickerDialog
    v-model:visible="isModColorOpen"
    subject="mod"
    :current="modColorNow"
    :error="error"
    :is-submitting="isBusy"
    @choose="colorMod"
  />
  <GroupMoveDialog
    v-model:visible="isMoveOpen"
    :group="group ?? null"
    :nodes="nodes"
    :error="error"
    :is-submitting="isBusy"
    @move="moveGroup"
  />
  <ModGroupsDialog
    v-model:visible="isModGroupsOpen"
    :mod-name="modName"
    :nodes="nodes"
    :member-of="modGroupIds"
    :error="error"
    :is-busy="isBusy"
    @toggle="toggleMembership"
  />
  <BulkGroupsDialog
    v-model:visible="isBulkGroupsOpen"
    :mode="bulkMode"
    :nodes="nodes"
    :selection-text="bulkText"
    :counts="bulkCounts"
    :counts-failed="bulkCountsFailed"
    :error="error"
    :is-submitting="isBusy"
    @choose="bulkChoose"
    @move="bulkMove"
    @after-hide="onBulkDialogGone"
  />
  <ColorPickerDialog
    v-model:visible="isBulkColorOpen"
    subject="mods"
    :current="null"
    :scope-text="bulkText"
    :error="error"
    :is-submitting="isBusy"
    @choose="bulkColor"
    @after-hide="onBulkDialogGone"
  />
  <StartOverDialog
    v-model:visible="isStartOverOpen"
    :error="error"
    :is-submitting="isBusy"
    @confirm="startOver"
  />
  <Dialog
    v-model:visible="isRemoveUninstalledOpen"
    modal
    :header="t('mods.groups.dialog.removeUninstalledTitle')"
    :style="{ width: '28rem' }"
    data-testid="remove-uninstalled-dialog"
  >
    <div class="flex flex-col gap-3 text-sm">
      <p
        v-if="scopePlan === undefined && scopePlanError === null"
        class="text-text-muted"
        data-testid="remove-uninstalled-loading"
      >
        {{ t("mods.groups.dialog.removeUninstalledLoading") }}
      </p>
      <p
        v-else-if="scopePlan === undefined"
        class="text-status-danger"
        role="alert"
        data-testid="remove-uninstalled-failed"
      >
        {{ t("mods.groups.dialog.removeUninstalledFailed") }}
      </p>
      <p
        v-else-if="scopePlan.notInstalled === 0"
        data-testid="remove-uninstalled-none"
      >
        {{ t("mods.groups.menu.noUninstalled") }}
      </p>
      <template v-else-if="group">
        <p>
          <i18n-t
            keypath="mods.groups.dialog.removeUninstalledBody"
            tag="span"
            :plural="scopePlan.notInstalled"
          >
            <template #count>
              {{ scopePlan.notInstalled }}
            </template>
            <template #name>
              <strong>{{ group.name }}</strong>
            </template>
          </i18n-t>
        </p>
        <ul
          class="border-border-subtle bg-surface-1 max-h-40 overflow-y-auto rounded border px-3 py-1"
          :aria-label="t('mods.groups.dialog.removeUninstalledList')"
          data-testid="remove-uninstalled-list"
        >
          <li
            v-for="(name, index) in uninstalledNames"
            :key="scopePlan.notInstalledIds[index]"
            class="truncate py-0.5"
          >
            {{ name }}
          </li>
        </ul>
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
        data-testid="remove-uninstalled-cancel"
        @click="isRemoveUninstalledOpen = false"
      />
      <Button
        :label="t('mods.groups.dialog.removeUninstalledButton')"
        severity="danger"
        :loading="isBusy"
        :disabled="scopePlan === undefined || scopePlan.notInstalled === 0"
        data-testid="remove-uninstalled-confirm"
        @click="removeUninstalled"
      />
    </template>
  </Dialog>
</template>
