<script setup lang="ts">
import { computed } from "vue";

import DefTextureImage from "@/components/graphics/DefTextureImage.vue";
import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { useDefGraphicQuery, useDefTextureQuery } from "@/queries/defGraphics";
import type { DefRef } from "@/types/brands";
import { DEFAULT_FACING, faceOf, presentEmptyGraphic } from "@/utils/defGraphics";
import { describeCommandError } from "@/utils/errors";

/**
 * The queries behind a {@link DefThumbnail}: the def's graphic, then the
 * default view's default-facing texture. A separate component so the
 * thumbnail can mount it only while on screen: once it unmounts, its two
 * queries lose their last observer and `useDefTextureQuery`'s 30 s `gcTime`
 * drops the image, exactly as the mod preview does.
 */
const { defRef } = defineProps<{ defRef: DefRef }>();

const tm = useTranslateMessage();
const graphic = useDefGraphicQuery(() => defRef);

const defaultFace = computed(() => {
  const resolved = graphic.data.value;
  if (resolved?.kind !== "resolved") {
    return null;
  }
  const { slot, variant } = resolved.defaultView;
  const faces = resolved.slots[slot]?.variants[variant]?.faces;
  return faces === undefined ? null : faceOf(faces, DEFAULT_FACING);
});

const texture = useDefTextureQuery(
  () => defRef,
  () => defaultFace.value?.textureKey ?? null,
);

const emptyGraphic = computed(() => {
  const resolved = graphic.data.value;
  return resolved === undefined || resolved.kind === "resolved"
    ? null
    : presentEmptyGraphic(resolved);
});

const errorTitle = computed(() => {
  const failure = graphic.error.value;
  return failure === null || failure === undefined ? null : tm(describeCommandError(failure).title);
});
</script>

<template>
  <div
    v-if="emptyGraphic"
    class="bg-surface-2 text-text-faint flex h-full w-full items-center justify-center rounded-sm"
    :title="tm(emptyGraphic.message)"
    data-testid="def-thumbnail-empty"
  >
    <span
      class="pi text-base"
      :class="emptyGraphic.icon"
      aria-hidden="true"
    />
  </div>
  <div
    v-else-if="errorTitle"
    class="bg-surface-2 text-status-danger flex h-full w-full items-center justify-center rounded-sm"
    :title="errorTitle"
    data-testid="def-thumbnail-error"
  >
    <span
      class="pi pi-exclamation-circle text-base"
      aria-hidden="true"
    />
  </div>
  <DefTextureImage
    v-else
    size="thumb"
    alt=""
    :texture="texture.data.value"
    :is-pending="graphic.isPending.value || texture.isPending.value"
    :error="texture.error.value"
    :mirrored="defaultFace?.isMirrored ?? false"
  />
</template>
