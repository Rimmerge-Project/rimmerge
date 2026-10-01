<script setup lang="ts">
import Button from "primevue/button";
import InputText from "primevue/inputtext";
import Textarea from "primevue/textarea";
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import { RimmergeError } from "@/services/ipc";

/** The identity fields this form collects — a subset of `PatchDetailDto` shared by both modes. */
export interface PatchIdentityFormValues {
  name: string;
  packageId: string;
  displayName: string;
  author: string;
  description: string;
}

const {
  mode,
  initial,
  error = null,
  saving = false,
} = defineProps<{
  /**
   * `"create"` hides author/description — `create_patch` has no fields
   * for them (the backend defaults author to `"Rimmerge"` and generates a
   * description from the scope); `"edit"` shows every field.
   */
  mode: "create" | "edit";
  initial: PatchIdentityFormValues;
  /** The last save attempt's failure, if any — shown inline. */
  error?: unknown;
  /** Disables the save button while a save is in flight. */
  saving?: boolean;
}>();
const emit = defineEmits<{ save: [values: PatchIdentityFormValues]; cancel: [] }>();
const { t } = useI18n();

const name = ref(initial.name);
const packageId = ref(initial.packageId);
const displayName = ref(initial.displayName);
const author = ref(initial.author);
const description = ref(initial.description);

/** Field-by-field comparison — `initial` is a plain object of primitives, never worth a deep-equal dependency for. */
function identityValuesEqual(a: PatchIdentityFormValues, b: PatchIdentityFormValues): boolean {
  return (
    a.name === b.name &&
    a.packageId === b.packageId &&
    a.displayName === b.displayName &&
    a.author === b.author &&
    a.description === b.description
  );
}

// `initial` changes when a different patch's detail loads (edit mode) or
// when the create form is reopened — either way the draft must follow it,
// not keep stale text from whatever was being edited before. But
// `PatchDetailPage`'s `identityValues` is a `computed` that builds a new
// object on every `usePatchQuery` refetch even when every field is
// unchanged — resetting on every new *reference* would wipe an unsaved,
// in-progress edit each time the query silently refetches in the
// background. Only a real change to the values themselves resets the
// draft.
watch(
  () => initial,
  (value, previous) => {
    if (previous && identityValuesEqual(value, previous)) {
      return;
    }
    name.value = value.name;
    packageId.value = value.packageId;
    displayName.value = value.displayName;
    author.value = value.author;
    description.value = value.description;
  },
);

/**
 * `create_patch`/`update_patch` map a rejected name/package id to
 * `patch_identity_invalid` — shown
 * right under the package id field, the one this error is almost always
 * about. Any other error code gets a generic banner instead.
 */
const packageIdError = computed(() =>
  error instanceof RimmergeError && error.code === "patch_identity_invalid" ? error.message : null,
);
const generalError = computed(() => {
  if (!(error instanceof RimmergeError) || error.code === "patch_identity_invalid") {
    return null;
  }
  return error.message;
});

function submit(): void {
  emit("save", {
    name: name.value,
    packageId: packageId.value,
    displayName: displayName.value,
    author: author.value,
    description: description.value,
  });
}
</script>

<template>
  <form
    class="flex flex-col gap-3"
    data-testid="patch-identity-form"
    @submit.prevent="submit"
  >
    <label class="flex flex-col gap-1 text-sm">
      <span class="text-text-muted">{{ t("patches.identityForm.nameLabel") }}</span>
      <InputText
        v-model="name"
        data-testid="patch-name-input"
        required
      />
    </label>
    <label class="flex flex-col gap-1 text-sm">
      <span class="text-text-muted">{{ t("patches.identityForm.packageIdLabel") }}</span>
      <InputText
        v-model="packageId"
        :placeholder="t('patches.identityForm.packageIdPlaceholder')"
        data-testid="patch-package-id-input"
        :invalid="packageIdError !== null"
        required
      />
      <span
        v-if="packageIdError"
        class="text-status-danger text-xs"
        data-testid="patch-package-id-error"
      >{{ packageIdError }}</span>
    </label>
    <label class="flex flex-col gap-1 text-sm">
      <span class="text-text-muted">{{ t("patches.identityForm.displayNameLabel") }}</span>
      <InputText
        v-model="displayName"
        data-testid="patch-display-name-input"
        required
      />
    </label>
    <template v-if="mode === 'edit'">
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-text-muted">{{ t("patches.identityForm.authorLabel") }}</span>
        <InputText
          v-model="author"
          data-testid="patch-author-input"
        />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        <span class="text-text-muted">{{ t("patches.identityForm.descriptionLabel") }}</span>
        <Textarea
          v-model="description"
          rows="3"
          auto-resize
          data-testid="patch-description-input"
        />
      </label>
    </template>
    <p
      v-if="generalError"
      class="text-status-danger text-xs"
      data-testid="patch-identity-error"
    >
      {{ generalError }}
    </p>
    <div class="flex gap-2">
      <Button
        type="submit"
        :label="t('patches.identityForm.save')"
        size="small"
        :loading="saving"
        data-testid="patch-save-button"
      />
      <Button
        v-if="mode === 'create'"
        type="button"
        :label="t('common.cancel')"
        size="small"
        severity="secondary"
        data-testid="patch-cancel-button"
        @click="emit('cancel')"
      />
    </div>
  </form>
</template>
