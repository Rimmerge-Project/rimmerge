<script setup lang="ts">
import Checkbox from "primevue/checkbox";
import { computed } from "vue";
import MarkedText from "@/components/mods/MarkedText.vue";
import MembershipBadge from "@/components/mods/MembershipBadge.vue";
import ModRowColor from "@/components/mods/ModRowColor.vue";
import ModRowGroups from "@/components/mods/ModRowGroups.vue";
import ModRowMenuButton from "@/components/mods/ModRowMenuButton.vue";
import ModRowPosition from "@/components/mods/ModRowPosition.vue";
import ModRowScanFacts from "@/components/mods/ModRowScanFacts.vue";
import ModRowStatePill from "@/components/mods/ModRowStatePill.vue";
import SourceBadge from "@/components/mods/SourceBadge.vue";
import type { ModListRowDto } from "@/types/generated/ModListRowDto";
import { marksForLabel } from "@/utils/modRowMarks";

/**
 * The cells of one mod row, shared by the List view's table and the Groups view's: the colour
 * stripe, the checkbox, the position, name, badges, group chips, update time and scan facts, and
 * the ⋮ button. The table owns the row element itself (its role, focus, keys and drag); this only
 * draws what is inside it, and reports the clicks on its own controls.
 */
const {
  mod,
  selectable = false,
  isSelected = false,
  showActiveColumns = true,
  showUpdated = false,
  showPosition = false,
  hasRowMenu = false,
  isDirectActivate = false,
  label,
  labelTitle,
  updatedText = "",
} = defineProps<{
  mod: ModListRowDto;
  /** Adds the checkbox column. */
  selectable?: boolean;
  isSelected?: boolean;
  /** Whether the tags/hard-dependents columns show (off for the Inactive list). */
  showActiveColumns?: boolean;
  /** Adds the last-updated time, as {@link updatedText}. */
  showUpdated?: boolean;
  /** Adds the load-order position (the Active list). */
  showPosition?: boolean;
  /** Whether the ⋮ button shows. */
  hasRowMenu?: boolean;
  /** Whether the ⋮ button runs "Activate at…" itself. */
  isDirectActivate?: boolean;
  /** The mod as the list labels it (its name, or its id in id mode); the search's matches are marked in it. */
  label: string;
  /** The tooltip for the label, when it differs from what shows. */
  labelTitle: string | undefined;
  updatedText?: string;
}>();
const emit = defineEmits<{
  /** The checkbox changed: toggle this mod. */
  toggleSelect: [];
  /** A click on the checkbox itself (a Shift+click extends a range instead of toggling). */
  checkboxClick: [event: MouseEvent];
  /** A group chip or dot: scope the lists to that group. */
  scopeGroup: [groupId: number];
  /** The ⋮ button. */
  openMenu: [event: MouseEvent];
}>();
const labelMarks = computed(() => marksForLabel(mod, label));
defineSlots<{
  /** Extra content right after the mod's name. */
  afterName?(): unknown;
}>();
</script>

<template>
  <ModRowColor
    :color="mod.color"
    :groups="mod.groups"
    part="stripe"
  />
  <div
    v-if="selectable"
    role="gridcell"
    class="contents"
  >
    <Checkbox
      class="shrink-0"
      binary
      :model-value="isSelected"
      :data-testid="`mod-row-select-${mod.modId}`"
      @click.stop="emit('checkboxClick', $event)"
      @dblclick.stop
      @update:model-value="emit('toggleSelect')"
    />
  </div>
  <div
    role="gridcell"
    class="contents"
  >
    <ModRowPosition
      v-if="showPosition && mod.position !== null"
      :mod-id="mod.modId"
      :position="mod.position"
    />
    <ModRowColor
      :color="mod.color"
      :groups="mod.groups"
      part="name"
    />
    <SourceBadge :source="mod.source" />
    <span
      class="text-text min-w-12 flex-1 truncate font-medium"
      :title="labelTitle"
      :data-testid="`mod-row-name-${mod.modId}`"
    ><MarkedText
      :text="label"
      :ranges="labelMarks"
    /></span>
    <slot name="afterName" />
    <MembershipBadge :mark="mod.membership" />
    <ModRowStatePill
      :state="mod.state"
      :show-pending-scan="showActiveColumns"
    />
    <ModRowGroups
      :groups="mod.groups"
      @scope="(groupId: number) => emit('scopeGroup', groupId)"
    />
    <span
      v-if="showUpdated"
      class="text-text-muted w-24 min-w-10 truncate text-right text-xs @lg:w-28 @lg:shrink-0"
      :title="updatedText"
      :data-testid="`mod-row-updated-${mod.modId}`"
    >{{ updatedText }}</span>
    <ModRowScanFacts
      :state="mod.state"
      :show-dependents="showActiveColumns"
    />
  </div>
  <div
    v-if="hasRowMenu"
    role="gridcell"
    class="contents"
  >
    <ModRowMenuButton
      :mod-id="mod.modId"
      :name="label"
      :is-direct="isDirectActivate"
      @open="emit('openMenu', $event)"
    />
  </div>
</template>
