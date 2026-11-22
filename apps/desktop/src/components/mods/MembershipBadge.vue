<script setup lang="ts">
import { computed } from "vue";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import type { MembershipMarkDto } from "@/types/generated/MembershipMarkDto";
import { membershipLabel } from "@/utils/fileBaseline";

/**
 * How a pending change differs from your saved active mod list: a pill with an icon and a word,
 * never colour alone, and visibly unlike the neutral source badge. Renders nothing for a row
 * that matches the saved list. The tooltip says what applying does to this mod. In a narrow list
 * the word gives way to the icon (the screen-reader text and the tooltip carry the meaning, as for
 * the pending-scan pill), which keeps a freshly changed row from crowding out its date and menu.
 */
const { mark } = defineProps<{ mark: MembershipMarkDto }>();

const tm = useTranslateMessage();

const label = computed(() => membershipLabel(mark));
</script>

<template>
  <span
    v-if="label !== null"
    class="inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-xs font-medium"
    :class="
      mark === 'added'
        ? 'bg-status-auto-soft text-status-auto'
        : 'bg-status-danger-soft text-status-danger'
    "
    :title="tm(label.title)"
    :data-mark="mark"
    data-testid="membership-badge"
  >
    <i
      :class="['pi text-xs', mark === 'added' ? 'pi-plus' : 'pi-minus']"
      aria-hidden="true"
    />
    <span class="hidden @lg:inline">{{ tm(label.text) }}</span>
    <span class="sr-only">{{ tm(label.title) }}</span>
  </span>
</template>
