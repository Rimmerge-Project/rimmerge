<script setup lang="ts">
import { useI18n } from "vue-i18n";

import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useTextureQuery } from "@/queries/merge";
import { describeCommandError } from "@/utils/errors";

/**
 * One owner's texture file, as one tile in {@link TexturePair}'s
 * side-by-side grid. Its own component (rather than a `v-for` row inside
 * `TexturePair`) because {@link useTextureQuery} is a composable — Vue
 * only allows calling one at the top of a component's own `setup`, never
 * once per loop iteration inside a single component.
 */
const {
  modId,
  texturePath,
  isWinner = false,
} = defineProps<{
  modId: string;
  texturePath: string;
  /** Whether this owner is the load-order winner — badged, never decided here. */
  isWinner?: boolean;
  /** Whether a `preferWinner` alternative exists for this owner. */
  canPreferWinner: boolean;
  /** Whether a `shipAsset` alternative exists for this owner. */
  canShipAsset: boolean;
}>();
const emit = defineEmits<{
  /** "Use this one": the `preferWinner` alternative naming this owner. */
  preferWinner: [];
  /** "Ship this file": the `shipAsset` alternative naming this owner. */
  shipAsset: [];
}>();

const { t } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();
const texture = useTextureQuery(
  () => modId,
  () => texturePath,
);
</script>

<template>
  <figure
    class="border-border-subtle bg-surface-1 flex flex-col gap-2 rounded-lg border p-2"
    :class="isWinner ? 'border-accent' : ''"
    :data-testid="`texture-tile-${modId}`"
  >
    <div class="flex items-center justify-between gap-2">
      <span
        class="truncate text-xs font-medium"
        :title="modLabel.titleFor(modId)"
      >{{ modLabel.label(modId) }}</span>
      <span
        v-if="isWinner"
        class="bg-status-auto-soft text-status-auto shrink-0 rounded-full px-1.5 py-0.5 text-[10px] font-medium"
        data-testid="texture-winner-badge"
      >{{ t("inbox.textureTile.winnerBadge") }}</span>
    </div>

    <p
      v-if="texture.isPending.value"
      class="text-text-muted text-xs"
    >
      {{ t("common.loading") }}
    </p>
    <p
      v-else-if="texture.error.value"
      class="text-status-danger text-xs"
      data-testid="texture-unavailable"
    >
      {{ tm(describeCommandError(texture.error.value).title) }}
    </p>
    <template v-else-if="texture.data.value">
      <img
        :src="texture.data.value.dataUrl"
        :alt="`${modLabel.label(modId)}'s ${texturePath}`"
        class="bg-surface-2 h-40 w-full rounded object-contain [image-rendering:pixelated]"
        data-testid="texture-image"
      >
      <p
        class="text-text-faint truncate text-[11px]"
        :title="texture.data.value.path"
      >
        {{
          t(
            "inbox.textureTile.sizeLine",
            { format: texture.data.value.format, count: texture.data.value.bytes },
            texture.data.value.bytes,
          )
        }}
      </p>
    </template>

    <div class="mt-auto flex gap-2">
      <button
        type="button"
        class="border-border-subtle text-text-muted disabled:text-text-faint cursor-pointer rounded border px-2 py-1 text-[11px] disabled:cursor-not-allowed"
        data-testid="texture-use-this-one"
        :disabled="!canPreferWinner"
        @click="emit('preferWinner')"
      >
        {{ t("inbox.textureTile.useThisOne") }}
      </button>
      <button
        type="button"
        class="border-border-subtle text-text-muted disabled:text-text-faint cursor-pointer rounded border px-2 py-1 text-[11px] disabled:cursor-not-allowed"
        data-testid="texture-ship-this-file"
        :disabled="!canShipAsset"
        @click="emit('shipAsset')"
      >
        {{ t("inbox.textureTile.shipThisFile") }}
      </button>
    </div>
  </figure>
</template>
