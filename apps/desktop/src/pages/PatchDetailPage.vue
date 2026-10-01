<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import { useToast } from "primevue/usetoast";
import { computed, onMounted, onScopeDispose, ref, useTemplateRef, watch } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";

import PatchExportPanel from "@/components/patches/PatchExportPanel.vue";
import PatchIdentityForm, {
  type PatchIdentityFormValues,
} from "@/components/patches/PatchIdentityForm.vue";
import PatchInbox from "@/components/patches/PatchInbox.vue";
import ScopeEditor from "@/components/patches/ScopeEditor.vue";
import { isInert } from "@/composables/shortcutTargets";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import {
  useDeletePatchMutation,
  useImportProfileDecisionsMutation,
  usePatchQuery,
  usePrunePatchDecisionsMutation,
  useUpdatePatchMutation,
} from "@/queries/patches";
import type { ModRefDto } from "@/types/generated/ModRefDto";
import type { ScopeChangeDto } from "@/types/generated/ScopeChangeDto";
import { describeDecisionRejection } from "@/utils/decisionRejection";

const { t } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();
const route = useRoute();
const router = useRouter();
const toast = useToast();

const patchId = computed(() => String(route.params["patchId"] ?? ""));
const { data: patch, isPending, error } = usePatchQuery(patchId);

const { mutateAsync: updatePatch } = useUpdatePatchMutation();
const { mutateAsync: deletePatch } = useDeletePatchMutation();
const { mutateAsync: importProfileDecisions } = useImportProfileDecisionsMutation();
const { mutateAsync: prunePatchDecisions } = usePrunePatchDecisionsMutation();

const identityError = ref<unknown>(null);
const savingIdentity = ref(false);
/**
 * The last scope edit's own report, when it shrank the scope enough to
 * orphan or affect a decision — cleared on navigating to a different
 * patch, never left over from the one viewed before.
 */
const scopeChange = ref<ScopeChangeDto | null>(null);
const deleteDialogVisible = ref(false);

watch(patchId, () => {
  identityError.value = null;
  scopeChange.value = null;
  deleteDialogVisible.value = false;
});

const identityValues = computed<PatchIdentityFormValues>(() => ({
  name: patch.value?.name ?? "",
  packageId: patch.value?.packageId ?? "",
  displayName: patch.value?.displayName ?? "",
  author: patch.value?.author ?? "",
  description: patch.value?.description ?? "",
}));

async function saveIdentity(values: PatchIdentityFormValues): Promise<void> {
  identityError.value = null;
  savingIdentity.value = true;
  try {
    await updatePatch({
      patchId: patchId.value,
      name: values.name,
      packageId: values.packageId,
      displayName: values.displayName,
      author: values.author,
      description: values.description,
      scope: null,
    });
  } catch (err) {
    identityError.value = err;
  } finally {
    savingIdentity.value = false;
  }
}

async function changeScope(members: ModRefDto[]): Promise<void> {
  const result = await updatePatch({
    patchId: patchId.value,
    name: null,
    packageId: null,
    displayName: null,
    author: null,
    description: null,
    scope: members.map((member) => member.modId),
  });
  scopeChange.value = result.scopeChange;
}

/**
 * The "N imported; M skipped: reasons" toast detail for a profile import
 * that skipped at least one decision — the imported and skipped counts
 * pluralize independently (`importedWithSkips.{imported,skipped}`),
 * rather than one message sharing a single plural selector across two
 * numbers, which a translation can't reorder or repluralize
 * independently (the same reasoning as `EditReferencesTargetsDialog.vue`'s
 * `schemaChangeCountsText`). The reasons are whole clauses that can
 * contain commas and "and", so they are joined with the locale's own
 * clause separator (`reasonSeparator`), never a list conjunction.
 */
function importedWithSkipsText(
  importedCount: number,
  skippedCount: number,
  reasons: string,
): string {
  const imported = t(
    "patches.detail.importedWithSkips.imported",
    { count: importedCount },
    importedCount,
  );
  const skipped = t(
    "patches.detail.importedWithSkips.skipped",
    { count: skippedCount, reasons },
    skippedCount,
  );
  return t("patches.detail.importedWithSkips.combined", { imported, skipped });
}

async function importFromProfile(): Promise<void> {
  const report = await importProfileDecisions({ patchId: patchId.value, keys: null });
  const detail =
    report.skipped.length === 0
      ? t(
          "patches.detail.importedNoSkips",
          { count: report.imported.length },
          report.imported.length,
        )
      : importedWithSkipsText(
          report.imported.length,
          report.skipped.length,
          report.skipped
            .map((entry) => tm(describeDecisionRejection(entry.cause, modLabel.label)))
            .join(t("patches.detail.importedWithSkips.reasonSeparator")),
        );
  toast.add({
    severity: "success",
    summary: t("patches.detail.importedToastSummary"),
    detail,
    life: 8000,
  });
}

async function pruneOrphaned(): Promise<void> {
  const before = patch.value?.orphaned.length ?? 0;
  const result = await prunePatchDecisions(patchId.value);
  // `result.orphaned` is the count *after* pruning (normally `0`), not
  // assumed to be — `before - result.orphaned.length` is the real number
  // removed, rather than the count of everything orphaned before the
  // call, which would overstate a prune that (for whatever future reason)
  // left some behind.
  const prunedCount = before - result.orphaned.length;
  toast.add({
    severity: "success",
    summary: t("patches.detail.prunedToastSummary"),
    detail: t("patches.detail.prunedToastDetail", { count: prunedCount }, prunedCount),
    life: 6000,
  });
}

async function confirmDelete(): Promise<void> {
  await deletePatch(patchId.value);
  deleteDialogVisible.value = false;
  await router.push({ name: "patches" });
}

/**
 * Where `Escape` from the scoped inbox below returns focus — the patch's
 * own header (never the profile findings, and this page has nowhere else
 * to "exit" to).
 */
const headerRef = useTemplateRef<HTMLElement>("header");
function focusHeader(): void {
  headerRef.value?.focus();
}

interface ScopeEditorInstance {
  focusSearch: () => void;
}
interface PatchExportPanelInstance {
  focusOutDir: () => void;
}
const scopeEditorRef = useTemplateRef<ScopeEditorInstance>("scopeEditor");
const exportPanelRef = useTemplateRef<PatchExportPanelInstance>("exportPanel");

/**
 * `s`/`x`: focus the scope search / the export directory field from
 * anywhere on this page. Same `isInert` inert-target
 * rule every other shortcut in the app uses — a text-entry control, a
 * dialog, or `contentEditable` swallows the keystroke instead.
 */
function handleKeydown(event: KeyboardEvent): void {
  if (isInert(event.target)) {
    return;
  }
  switch (event.key) {
    case "s":
      event.preventDefault();
      scopeEditorRef.value?.focusSearch();
      break;
    case "x":
      event.preventDefault();
      exportPanelRef.value?.focusOutDir();
      break;
    default:
      break;
  }
}
onMounted(() => window.addEventListener("keydown", handleKeydown));
onScopeDispose(() => window.removeEventListener("keydown", handleKeydown));
</script>

<template>
  <div class="mx-auto flex max-w-6xl flex-col gap-6 p-6">
    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="patch-detail-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="patch-detail-error"
    >
      {{ error instanceof Error ? error.message : t("patches.detail.loadFailed") }}
    </p>
    <template v-else-if="patch">
      <header
        ref="header"
        tabindex="-1"
        class="flex items-start justify-between gap-4 focus:outline-none"
        data-testid="patch-detail-header"
      >
        <div>
          <h1 class="text-text text-xl font-semibold">
            {{ patch.displayName }}
          </h1>
          <p class="text-text-muted font-mono text-sm">
            {{ patch.packageId }}
          </p>
          <p
            v-if="patch.orphaned.length > 0"
            class="text-status-input text-xs"
            data-testid="patch-orphaned-count"
          >
            {{
              t(
                "patches.detail.orphanedCount",
                { count: patch.orphaned.length },
                patch.orphaned.length,
              )
            }}
          </p>
        </div>
        <div class="flex shrink-0 gap-2">
          <Button
            :label="t('patches.detail.importFromProfileButton')"
            size="small"
            severity="secondary"
            data-testid="import-profile-decisions-button"
            @click="importFromProfile"
          />
          <Button
            v-if="patch.orphaned.length > 0"
            :label="t('patches.detail.pruneOrphanedButton')"
            size="small"
            severity="secondary"
            data-testid="prune-orphaned-button"
            @click="pruneOrphaned"
          />
          <Button
            :label="t('patches.detail.deleteButton')"
            size="small"
            severity="danger"
            outlined
            data-testid="delete-patch-button"
            @click="deleteDialogVisible = true"
          />
        </div>
      </header>

      <section class="surface-card p-4">
        <h2 class="text-text mb-2 text-sm font-semibold">
          {{ t("patches.detail.identityHeading") }}
        </h2>
        <PatchIdentityForm
          mode="edit"
          :initial="identityValues"
          :error="identityError"
          :saving="savingIdentity"
          @save="saveIdentity"
        />
      </section>

      <section class="surface-card p-4">
        <h2 class="text-text mb-2 text-sm font-semibold">
          {{ t("patches.list.scopeHeading") }}
        </h2>
        <ScopeEditor
          ref="scopeEditor"
          :members="patch.scope"
          :scope-change="scopeChange"
          @change="changeScope"
        />
      </section>

      <section class="surface-card p-4">
        <h2 class="text-text mb-2 text-sm font-semibold">
          {{ t("patches.detail.exportHeading") }}
        </h2>
        <PatchExportPanel
          ref="exportPanel"
          :patch-id="patchId"
          :export-dir="patch.exportDir"
        />
      </section>

      <section>
        <h2 class="text-text mb-2 text-sm font-semibold">
          {{ t("patches.detail.findingsHeading") }}
        </h2>
        <PatchInbox
          :patch-id="patchId"
          @escape="focusHeader"
        />
      </section>
    </template>

    <Dialog
      v-model:visible="deleteDialogVisible"
      modal
      :header="t('patches.detail.deleteDialogHeader')"
      data-testid="delete-patch-dialog"
    >
      <p class="text-sm">
        {{ t("patches.detail.deleteDialogBody") }}
      </p>
      <div class="mt-4 flex justify-end gap-2">
        <Button
          :label="t('common.cancel')"
          size="small"
          severity="secondary"
          data-testid="delete-patch-cancel"
          @click="deleteDialogVisible = false"
        />
        <Button
          :label="t('patches.detail.deleteButton')"
          size="small"
          severity="danger"
          data-testid="delete-patch-confirm"
          @click="confirmDelete"
        />
      </div>
    </Dialog>
  </div>
</template>
