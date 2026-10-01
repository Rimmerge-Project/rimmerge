<script setup lang="ts">
import Select from "primevue/select";
import { computed, nextTick, useTemplateRef } from "vue";
import { useI18n } from "vue-i18n";

import DefTextureImage from "@/components/graphics/DefTextureImage.vue";
import { useGraphicViewer } from "@/composables/useGraphicViewer";
import { useModLabel } from "@/composables/useModLabel";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { descriptor } from "@/i18n/messageDescriptor";
import { useDefGraphicQuery, useDefTextureQuery } from "@/queries/defGraphics";
import type { DefRef } from "@/types/brands";
import {
  FACINGS,
  facingName,
  facingShortName,
  isOverrideOfOwner,
  looseOwnerOf,
  MAX_STEPPER_VARIANTS,
  presentEmptyGraphic,
  slotLabel,
  variantLabel,
} from "@/utils/defGraphics";

/**
 * What one def shows: a 160 px image of the selected texture with the
 * controls to look at the others (facing, variant, source). Read-only, and
 * shows nothing the backend did not resolve: the facing and variant
 * choices are the DTO's own faces. The provider of the shown file is named
 * (and flagged when it replaces the def owner's own texture), because the
 * file the game loads is the last-loaded mod's, not necessarily the
 * def's.
 */
const { defRef, defOwner = null } = defineProps<{
  defRef: DefRef;
  /** The mod that owns the def, to flag a retexture from another mod; `null` when unknown. */
  defOwner?: string | null;
}>();

const { t } = useI18n();
const tm = useTranslateMessage();
const modLabel = useModLabel();

const graphic = useDefGraphicQuery(() => defRef);
const viewer = useGraphicViewer(
  () => graphic.data.value,
  () => defRef,
);
const texture = useDefTextureQuery(
  () => defRef,
  () => viewer.face.value?.textureKey ?? null,
);

const facingGroup = useTemplateRef<HTMLElement>("facingGroup");

const emptyGraphic = computed(() => {
  const resolved = graphic.data.value;
  return resolved === undefined || resolved.kind === "resolved"
    ? null
    : presentEmptyGraphic(resolved);
});

const slotOptions = computed(() =>
  viewer.slots.value.map((slot, index) => ({ label: tm(slotLabel(slot.source)), value: index })),
);

const variants = computed(() => viewer.slot.value?.variants ?? []);
const variantOptions = computed(() =>
  variants.value.map((variant, index) => ({
    label: tm(variantLabel(variant.label)),
    value: index,
  })),
);
const isVariantDropdown = computed(() => variants.value.length > MAX_STEPPER_VARIANTS);
const isVariantStepper = computed(() => variants.value.length > 1 && !isVariantDropdown.value);
const variantText = computed(() => {
  const current = viewer.variant.value;
  if (current === null) {
    return "";
  }
  const label = tm(variantLabel(current.label));
  return variants.value.length > 1
    ? t("defGraphics.viewer.variantPosition", {
        label,
        index: viewer.variantIndex.value + 1,
        count: variants.value.length,
      })
    : label;
});
const isVariantLabelShown = computed(
  () =>
    viewer.variant.value !== null &&
    (variants.value.length > 1 || viewer.variant.value.label.kind !== "only"),
);

const providerId = computed(() => {
  const face = viewer.face.value;
  return face === null ? null : looseOwnerOf(face);
});
const isOverride = computed(
  () => providerId.value !== null && isOverrideOfOwner(providerId.value, defOwner),
);

const altText = computed(() => {
  const face = viewer.face.value;
  if (face === null || face.facing === null) {
    return tm(descriptor("defGraphics.altNoFacing", { def: defRef }));
  }
  return tm(descriptor("defGraphics.alt", { def: defRef, facing: tm(facingName(face.facing)) }));
});

function onFacingKeydown(event: KeyboardEvent): void {
  const delta =
    event.key === "ArrowRight" || event.key === "ArrowDown"
      ? 1
      : event.key === "ArrowLeft" || event.key === "ArrowUp"
        ? -1
        : 0;
  if (delta === 0) {
    return;
  }
  event.preventDefault();
  viewer.stepFacing(delta);
  void nextTick(() => {
    facingGroup.value
      ?.querySelector<HTMLElement>(`[data-facing="${viewer.facing.value}"]`)
      ?.focus();
  });
}
</script>

<template>
  <section
    class="flex w-40 shrink-0 flex-col gap-2"
    :aria-label="t('defGraphics.viewer.regionLabel')"
    data-testid="def-graphic-viewer"
  >
    <DefTextureImage
      v-if="graphic.error.value"
      size="viewer"
      :alt="altText"
      :texture="undefined"
      :is-pending="false"
      :error="graphic.error.value"
    />
    <div
      v-else-if="emptyGraphic"
      class="flex flex-col gap-1"
    >
      <div class="bg-surface-2 text-text-faint flex h-40 w-40 items-center justify-center rounded">
        <span
          class="pi text-3xl!"
          :class="emptyGraphic.icon"
          aria-hidden="true"
        />
      </div>
      <p
        class="text-text-muted text-xs"
        data-testid="def-graphic-empty"
      >
        {{ tm(emptyGraphic.message) }}
      </p>
    </div>
    <DefTextureImage
      v-else-if="graphic.isPending.value || viewer.face.value === null"
      size="viewer"
      :alt="altText"
      :texture="undefined"
      :is-pending="true"
    />
    <template v-else>
      <DefTextureImage
        size="viewer"
        :alt="altText"
        :texture="texture.data.value"
        :is-pending="texture.isPending.value"
        :error="texture.error.value"
        :mirrored="viewer.face.value.isMirrored"
      />

      <p
        v-if="providerId"
        class="text-text-muted text-xs"
        data-testid="def-graphic-provider"
      >
        <span :title="modLabel.titleFor(providerId)">{{
          t("defGraphics.viewer.fromMod", { mod: modLabel.label(providerId) })
        }}</span>
      </p>
      <p
        v-if="isOverride && defOwner"
        class="text-status-input text-xs"
        data-testid="def-graphic-override"
      >
        {{ t("defGraphics.viewer.overridesOwner", { owner: modLabel.label(defOwner) }) }}
      </p>

      <div
        v-if="viewer.isFacingShown.value"
        ref="facingGroup"
        role="radiogroup"
        class="flex gap-1"
        :aria-label="t('defGraphics.viewer.facingGroupLabel')"
        data-testid="def-graphic-facing"
        @keydown="onFacingKeydown"
      >
        <button
          v-for="facing in FACINGS"
          :key="facing"
          type="button"
          role="radio"
          class="border-border-subtle hover:bg-surface-2 aria-checked:bg-accent-soft aria-checked:border-accent focus-visible:outline-accent h-7 min-w-0 flex-1 cursor-pointer rounded border text-xs font-medium focus-visible:outline-2"
          :aria-checked="viewer.facing.value === facing"
          :aria-label="tm(facingName(facing))"
          :tabindex="viewer.facing.value === facing ? 0 : -1"
          :data-facing="facing"
          :data-testid="`def-graphic-facing-${facing}`"
          @click="viewer.selectFacing(facing)"
        >
          {{ tm(facingShortName(facing)) }}
        </button>
      </div>
      <p
        v-if="viewer.isFacingShown.value"
        class="text-text-faint min-h-4 text-[11px]"
        data-testid="def-graphic-mirrored"
      >
        {{ viewer.face.value.isMirrored ? t("defGraphics.viewer.mirrored") : "" }}
      </p>

      <Select
        v-if="slotOptions.length > 1"
        :model-value="viewer.slotIndex.value"
        :options="slotOptions"
        option-label="label"
        option-value="value"
        size="small"
        :aria-label="t('defGraphics.viewer.slotSelectLabel')"
        data-testid="def-graphic-slot"
        @update:model-value="(index: number) => viewer.selectSlot(index)"
      />

      <Select
        v-if="isVariantDropdown"
        :model-value="viewer.variantIndex.value"
        :options="variantOptions"
        option-label="label"
        option-value="value"
        size="small"
        :aria-label="t('defGraphics.viewer.variantSelectLabel')"
        data-testid="def-graphic-variant-select"
        @update:model-value="(index: number) => viewer.selectVariant(index)"
      />
      <div
        v-else-if="isVariantStepper"
        class="flex items-center gap-1"
      >
        <button
          type="button"
          class="border-border-subtle hover:bg-surface-2 focus-visible:outline-accent h-7 w-7 shrink-0 cursor-pointer rounded border focus-visible:outline-2"
          :aria-label="t('defGraphics.viewer.previousVariant')"
          data-testid="def-graphic-variant-prev"
          @click="viewer.stepVariant(-1)"
        >
          <span
            class="pi pi-chevron-left text-xs"
            aria-hidden="true"
          />
        </button>
        <span
          class="text-text-muted min-w-0 flex-1 text-center text-xs"
          aria-live="polite"
          data-testid="def-graphic-variant-label"
        >{{ variantText }}</span>
        <button
          type="button"
          class="border-border-subtle hover:bg-surface-2 focus-visible:outline-accent h-7 w-7 shrink-0 cursor-pointer rounded border focus-visible:outline-2"
          :aria-label="t('defGraphics.viewer.nextVariant')"
          data-testid="def-graphic-variant-next"
          @click="viewer.stepVariant(1)"
        >
          <span
            class="pi pi-chevron-right text-xs"
            aria-hidden="true"
          />
        </button>
      </div>
      <p
        v-else-if="isVariantLabelShown"
        class="text-text-muted text-xs"
        data-testid="def-graphic-variant-label"
      >
        {{ variantText }}
      </p>

      <p
        v-if="viewer.slot.value?.isLayoutInferred"
        class="text-text-faint text-[11px]"
        data-testid="def-graphic-inferred"
      >
        {{ t("defGraphics.viewer.layoutInferred") }}
      </p>
      <p
        v-if="viewer.slot.value && viewer.slot.value.truncated > 0"
        class="text-text-faint text-[11px]"
        data-testid="def-graphic-truncated"
      >
        {{
          t(
            "defGraphics.viewer.truncatedVariants",
            { count: viewer.slot.value.truncated },
            viewer.slot.value.truncated,
          )
        }}
      </p>
      <p
        v-if="graphic.data.value?.kind === 'resolved' && graphic.data.value.truncatedSlots > 0"
        class="text-text-faint text-[11px]"
        data-testid="def-graphic-truncated-slots"
      >
        {{
          t(
            "defGraphics.viewer.truncatedSlots",
            { count: graphic.data.value.truncatedSlots },
            graphic.data.value.truncatedSlots,
          )
        }}
      </p>
    </template>
  </section>
</template>
