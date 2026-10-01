<script setup lang="ts">
import Button from "primevue/button";
import { computed, onMounted, onScopeDispose, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";

import PatchIdentityForm, {
  type PatchIdentityFormValues,
} from "@/components/patches/PatchIdentityForm.vue";
import PatchTable from "@/components/patches/PatchTable.vue";
import ScopeEditor from "@/components/patches/ScopeEditor.vue";
import { isInert, ownsEnterNatively } from "@/composables/shortcutTargets";
import { useCreatePatchMutation, usePatchesQuery } from "@/queries/patches";
import type { ModRefDto } from "@/types/generated/ModRefDto";

const { t } = useI18n();
const router = useRouter();
const { data: patches, isPending, error } = usePatchesQuery();
const { mutateAsync: createPatch } = useCreatePatchMutation();

const cursor = ref(0);
const creating = ref(false);
const createScope = ref<ModRefDto[]>([]);
const createError = ref<unknown>(null);
const creatingSaving = ref(false);

const emptyIdentity: PatchIdentityFormValues = {
  name: "",
  packageId: "",
  displayName: "",
  author: "",
  description: "",
};

function openCreate(): void {
  creating.value = true;
  createScope.value = [];
  createError.value = null;
}

function cancelCreate(): void {
  creating.value = false;
}

function openPatch(patchId: string): void {
  void router.push({ name: "patch-detail", params: { patchId } });
}

/**
 * Keeps `cursor` in sync with whichever row actually has DOM focus (Tab,
 * or a click just before it navigates away) — `PatchTable`'s own rows no
 * longer handle `Enter` themselves (see its `focusRow` emit's doc
 * comment), so the cursor this page tracks is the single source of truth
 * for which patch `Enter` opens.
 */
function focusRow(index: number): void {
  cursor.value = index;
}

async function submitCreate(values: PatchIdentityFormValues): Promise<void> {
  createError.value = null;
  creatingSaving.value = true;
  try {
    const created = await createPatch({
      name: values.name,
      packageId: values.packageId,
      displayName: values.displayName,
      scope: createScope.value.map((member) => member.modId),
    });
    creating.value = false;
    await router.push({ name: "patch-detail", params: { patchId: created.id } });
  } catch (err) {
    createError.value = err;
  } finally {
    creatingSaving.value = false;
  }
}

const rowCount = computed(() => patches.value?.length ?? 0);

function handleKeydown(event: KeyboardEvent): void {
  if (isInert(event.target)) {
    return;
  }
  switch (event.key) {
    case "j":
    case "ArrowDown":
      event.preventDefault();
      cursor.value = Math.min(cursor.value + 1, Math.max(rowCount.value - 1, 0));
      break;
    case "k":
    case "ArrowUp":
      event.preventDefault();
      cursor.value = Math.max(cursor.value - 1, 0);
      break;
    case "Enter": {
      if (ownsEnterNatively(event.target)) {
        break;
      }
      const patch = patches.value?.[cursor.value];
      if (patch) {
        event.preventDefault();
        openPatch(patch.id);
      }
      break;
    }
    case "n":
      event.preventDefault();
      openCreate();
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
    <div class="flex items-center justify-between gap-4">
      <h1 class="text-text text-xl font-semibold">
        {{ t("shell.nav.patches") }}
      </h1>
      <Button
        :label="t('patches.list.newPatchButton')"
        size="small"
        data-testid="new-patch-button"
        @click="openCreate"
      />
    </div>

    <section
      v-if="creating"
      class="surface-card flex flex-col gap-4 p-4"
      data-testid="patch-create-form"
    >
      <h2 class="text-text text-sm font-semibold">
        {{ t("patches.list.newPatchHeading") }}
      </h2>
      <PatchIdentityForm
        mode="create"
        :initial="emptyIdentity"
        :error="createError"
        :saving="creatingSaving"
        @save="submitCreate"
        @cancel="cancelCreate"
      />
      <div>
        <h3 class="text-text-muted mb-1 text-xs font-semibold tracking-wide uppercase">
          {{ t("patches.list.scopeHeading") }}
        </h3>
        <ScopeEditor
          :members="createScope"
          @change="(members) => (createScope = members)"
        />
      </div>
    </section>

    <p
      v-if="isPending"
      class="text-text-muted text-sm"
      data-testid="patches-loading"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="error"
      class="text-status-danger text-sm"
      data-testid="patches-error"
    >
      {{ error instanceof Error ? error.message : t("patches.list.loadFailed") }}
    </p>
    <PatchTable
      v-else
      :patches="patches ?? []"
      :active-index="cursor"
      @open="openPatch"
      @focus-row="focusRow"
    />
  </div>
</template>
