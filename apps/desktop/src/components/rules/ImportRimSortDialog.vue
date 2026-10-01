<script setup lang="ts">
import Button from "primevue/button";
import Dialog from "primevue/dialog";
import InputText from "primevue/inputtext";
import { ref, watchEffect } from "vue";
import { useI18n } from "vue-i18n";

import { useDefaultRimSortPathsQuery } from "@/queries/rules";
import type { RimSortPathsDto } from "@/types/generated/RimSortPathsDto";

const { t } = useI18n();
const { visible } = defineProps<{ visible: boolean }>();
const emit = defineEmits<{
  "update:visible": [value: boolean];
  import: [paths: RimSortPathsDto];
}>();

const { data: defaults } = useDefaultRimSortPathsQuery();

const userRules = ref("");
const communityRules = ref("");
const steamDb = ref("");

// Prefills once the defaults resolve, without clobbering anything the
// user has already typed. A `null` default (no RimSort install found)
// simply leaves the field blank rather than writing the literal string
// "null" into it.
watchEffect(() => {
  if (!defaults.value) {
    return;
  }
  if (!userRules.value) {
    userRules.value = defaults.value.userRules ?? "";
  }
  if (!communityRules.value) {
    communityRules.value = defaults.value.communityRules ?? "";
  }
  if (!steamDb.value) {
    steamDb.value = defaults.value.steamDb ?? "";
  }
});

// A blank field means "no file for this source" — submitted as `null`,
// never an empty-string path (`None` on a `RimSortPathsDto` field is a
// supported "not part of this import" state, distinct from a path that
// fails to read). `.trim()` first so a field containing only whitespace
// is treated as blank too, not as a literal (and certainly-nonexistent)
// path.
function submit(): void {
  emit("import", {
    userRules: userRules.value.trim() || null,
    communityRules: communityRules.value.trim() || null,
    steamDb: steamDb.value.trim() || null,
  });
}
</script>

<template>
  <Dialog
    :visible="visible"
    modal
    :header="t('rules.importDialog.header')"
    data-testid="import-rimsort-dialog"
    @update:visible="(value) => emit('update:visible', value)"
  >
    <form
      class="flex flex-col gap-3"
      data-testid="import-rimsort-form"
      @submit.prevent="submit"
    >
      <label class="flex flex-col gap-1 text-sm">
        {{ t("rules.importDialog.userRulesLabel") }}
        <InputText
          v-model="userRules"
          data-testid="import-user-rules-path"
        />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        {{ t("rules.importDialog.communityRulesLabel") }}
        <InputText
          v-model="communityRules"
          data-testid="import-community-rules-path"
        />
      </label>
      <label class="flex flex-col gap-1 text-sm">
        {{ t("rules.importDialog.steamDbLabel") }}
        <InputText
          v-model="steamDb"
          data-testid="import-steam-db-path"
        />
      </label>
      <Button
        type="submit"
        :label="t('rules.importDialog.importButton')"
        data-testid="import-rimsort-submit"
      />
    </form>
  </Dialog>
</template>
