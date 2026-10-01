<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import type { ScanWarningDto } from "@/types/generated/ScanWarningDto";

/**
 * Renders one scan note per row: the mod it's about, linked to that mod's
 * detail page when the scan could attribute it to one, and its message
 * verbatim (it usually embeds a file path, so it's rendered monospace).
 * Shared by the dashboard's "Scan notes" card (every note) and the mod
 * detail page's own section (that mod's notes only) so the two stay in
 * sync — see `ScanWarningDto`'s own doc comment for why nothing here
 * parses `message`.
 */
const { notes } = defineProps<{ notes: ScanWarningDto[] }>();
const { t } = useI18n();
const modLabel = useModLabel();
</script>

<template>
  <p
    v-if="notes.length === 0"
    class="text-text-muted text-sm"
    data-testid="scan-note-list-empty"
  >
    {{ t("mods.scanNotes.empty") }}
  </p>
  <ul
    v-else
    class="flex flex-col gap-1"
    data-testid="scan-note-list"
  >
    <li
      v-for="(note, index) in notes"
      :key="`${note.modId ?? 'unattributed'}-${index}`"
      class="border-border-subtle bg-surface-1 flex flex-col gap-0.5 rounded border px-2 py-1"
      data-testid="scan-note"
    >
      <RouterLink
        v-if="note.modId"
        :to="`/mods/${encodeURIComponent(note.modId)}`"
        class="text-accent w-fit text-sm font-medium underline"
        :title="modLabel.titleFor(note.modId)"
      >
        {{ modLabel.label(note.modId) }}
      </RouterLink>
      <span class="text-text-muted font-mono text-xs break-all">{{ note.message }}</span>
    </li>
  </ul>
</template>
