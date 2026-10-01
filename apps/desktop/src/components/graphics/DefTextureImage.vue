<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { useTranslateMessage } from "@/composables/useTranslateMessage";
import { descriptor } from "@/i18n/messageDescriptor";
import type { DefTextureDto } from "@/types/generated/DefTextureDto";
import { assertNever } from "@/utils/assertNever";
import { presentTexture } from "@/utils/defGraphics";
import { describeCommandError } from "@/utils/errors";

/**
 * The one renderer of a def texture read (`DefTextureDto`) together with
 * its pending and failed states. A `viewer` is a 160 px box with the full
 * sentence beneath it; a `thumb` fills its parent (40 px in the coverage
 * queue) and puts the same sentence in a `title` tooltip, since there is
 * no room to print it. Every non-image outcome is labelled by
 * `presentTexture`'s exhaustive switch, never a blank box. A mirrored
 * face is flipped with CSS: the bytes are the other direction's file.
 * Images are data URLs rendered with `<img>`; nothing is injected as HTML.
 */
const {
  texture,
  isPending,
  error = null,
  size,
  mirrored = false,
  alt,
} = defineProps<{
  texture: DefTextureDto | undefined;
  isPending: boolean;
  error?: unknown;
  size: "thumb" | "viewer";
  mirrored?: boolean;
  /** The image's accessible description; ignored for a decorative thumbnail. */
  alt: string;
}>();

const { t } = useI18n();
const tm = useTranslateMessage();

type Shown =
  | { kind: "pending" }
  | { kind: "error"; message: string; detail: string | null }
  | { kind: "image"; url: string; note: string | null }
  | { kind: "fallback"; icon: string; message: string };

const shown = computed<Shown>(() => {
  if (error !== null && error !== undefined) {
    const described = describeCommandError(error);
    return { kind: "error", message: tm(described.title), detail: described.technicalDetail };
  }
  if (isPending || texture === undefined) {
    return { kind: "pending" };
  }
  const presentation = presentTexture(texture);
  switch (presentation.kind) {
    case "fallback":
      return { kind: "fallback", icon: presentation.icon, message: tm(presentation.message) };
    case "image":
      return {
        kind: "image",
        url: presentation.dataUrl,
        note: presentation.note === null ? null : tm(presentation.note),
      };
    default:
      return assertNever(presentation);
  }
});

const tooltip = computed<string | undefined>(() => {
  const current = shown.value;
  switch (current.kind) {
    case "error":
    case "fallback":
      return current.message;
    case "image":
      return current.note ?? undefined;
    case "pending":
      return undefined;
    default:
      return assertNever(current);
  }
});

const boxClasses = computed(() =>
  size === "viewer" ? "h-40 w-40 rounded" : "h-full w-full rounded-sm",
);
const glyphClasses = computed(() => (size === "viewer" ? "text-3xl!" : "text-base"));
const loadingLabel = computed(() => tm(descriptor("defGraphics.state.loading")));
</script>

<template>
  <div
    class="flex flex-col gap-1"
    :class="size === 'viewer' ? 'w-40' : 'h-full w-full'"
  >
    <div
      class="bg-surface-2 text-text-faint relative flex shrink-0 items-center justify-center overflow-hidden"
      :class="boxClasses"
      :title="size === 'thumb' ? tooltip : undefined"
      data-testid="def-texture-box"
    >
      <div
        v-if="shown.kind === 'pending'"
        class="h-full w-full animate-pulse"
        role="status"
        :aria-label="loadingLabel"
        data-testid="def-texture-pending"
      />
      <img
        v-else-if="shown.kind === 'image'"
        :src="shown.url"
        :alt="size === 'viewer' ? alt : ''"
        class="h-full w-full object-contain"
        :class="{ '-scale-x-100': mirrored }"
        data-testid="def-texture-image"
      >
      <span
        v-else-if="shown.kind === 'fallback'"
        class="pi"
        :class="[shown.icon, glyphClasses]"
        aria-hidden="true"
        data-testid="def-texture-fallback"
      />
      <span
        v-else
        class="pi pi-exclamation-circle text-status-danger"
        :class="glyphClasses"
        aria-hidden="true"
        data-testid="def-texture-error"
      />
    </div>
    <template v-if="size === 'viewer'">
      <p
        v-if="shown.kind === 'fallback'"
        class="text-text-muted text-xs"
        data-testid="def-texture-message"
      >
        {{ shown.message }}
      </p>
      <p
        v-else-if="shown.kind === 'image' && shown.note"
        class="text-text-muted text-xs"
        data-testid="def-texture-note"
      >
        {{ shown.note }}
      </p>
      <template v-else-if="shown.kind === 'error'">
        <p
          class="text-status-danger text-xs"
          data-testid="def-texture-error-message"
        >
          {{ shown.message }}
        </p>
        <p
          v-if="shown.detail"
          class="text-text-faint text-[11px] break-words"
        >
          {{ t("common.technicalDetail", { detail: shown.detail }) }}
        </p>
      </template>
    </template>
  </div>
</template>
