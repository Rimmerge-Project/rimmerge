<script setup lang="ts">
import Button from "primevue/button";
import { useI18n } from "vue-i18n";

import type { BulkAction } from "@/utils/organization";

/**
 * The organization and tag actions for one list's selection. Shown only while something is
 * selected. It only says which action was asked for; the page owns the dialogs and the edits,
 * and none of them selects, moves, activates or deactivates anything.
 */
const { kind, listName, hasOrganizationActions } = defineProps<{
  /** Which list the selection is in, for the test ids. */
  kind: "active" | "inactive";
  /** The list's own name ("Active"), so the two lists' bars have different accessible names. */
  listName: string;
  /** Whether groups and colours can be edited (organization loaded). Tags never need it. */
  hasOrganizationActions: boolean;
}>();
const emit = defineEmits<{ action: [action: BulkAction] }>();

const { t } = useI18n();
</script>

<template>
  <div
    class="flex shrink-0 flex-wrap items-center gap-1.5"
    role="group"
    :aria-label="t('mods.bulk.region', { list: listName })"
    :data-testid="`mods-bulk-bar-${kind}`"
  >
    <template v-if="hasOrganizationActions">
      <Button
        :label="t('mods.bulk.addToGroup')"
        icon="pi pi-folder-plus"
        severity="secondary"
        outlined
        size="small"
        :data-testid="`bulk-add-to-group-${kind}`"
        @click="emit('action', 'addToGroup')"
      />
      <Button
        :label="t('mods.bulk.removeFromGroup')"
        icon="pi pi-minus"
        severity="secondary"
        outlined
        size="small"
        :data-testid="`bulk-remove-from-group-${kind}`"
        @click="emit('action', 'removeFromGroup')"
      />
      <Button
        :label="t('mods.bulk.moveToGroup')"
        icon="pi pi-arrow-right-arrow-left"
        severity="secondary"
        outlined
        size="small"
        :data-testid="`bulk-move-to-group-${kind}`"
        @click="emit('action', 'moveToGroup')"
      />
      <Button
        :label="t('mods.bulk.color')"
        icon="pi pi-palette"
        severity="secondary"
        outlined
        size="small"
        :data-testid="`bulk-color-${kind}`"
        @click="emit('action', 'color')"
      />
    </template>
    <Button
      :label="t('mods.bulk.tagsButton')"
      icon="pi pi-tags"
      severity="secondary"
      outlined
      size="small"
      :data-testid="`bulk-tags-${kind}`"
      @click="emit('action', 'tags')"
    />
  </div>
</template>
