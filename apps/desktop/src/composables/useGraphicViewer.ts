import { computed, type MaybeRefOrGetter, ref, toValue, watch } from "vue";

import type { DefGraphicDto } from "@/types/generated/DefGraphicDto";
import type { FacingDto } from "@/types/generated/FacingDto";
import type { GraphicFaceDto } from "@/types/generated/GraphicFaceDto";
import type { GraphicSlotDto } from "@/types/generated/GraphicSlotDto";
import type { GraphicVariantDto } from "@/types/generated/GraphicVariantDto";
import { DEFAULT_FACING, FACINGS, faceOf, isDirectional } from "@/utils/defGraphics";

/** The slots of a resolved graphic, or `null` for any other outcome. */
function slotsOf(graphic: DefGraphicDto | undefined): readonly GraphicSlotDto[] | null {
  return graphic?.kind === "resolved" ? graphic.slots : null;
}

/**
 * A string that changes exactly when the viewer must go back to its
 * default view: another def, or a graphic whose shape changed. It reads
 * only the numbers that decide the default, so a refetch returning an
 * equal DTO (a new object, the same content) produces the same string and
 * leaves the user's slot, variant and facing alone. Keyed on a string
 * rather than object identity for the reason `RowEditor.vue` keys its
 * draft reset on `descriptor.key`.
 */
function shapeKey(defKey: string, graphic: DefGraphicDto | undefined): string {
  if (graphic === undefined) {
    return `${defKey}|pending`;
  }
  if (graphic.kind !== "resolved") {
    return `${defKey}|${graphic.kind}`;
  }
  const counts = graphic.slots.map((slot) => slot.variants.length).join(",");
  const { slot, variant } = graphic.defaultView;
  return `${defKey}|resolved|${counts}|${slot}.${variant}`;
}

/**
 * The viewer's selection state: which slot, which variant, which facing.
 * It resets to the graphic's own default view whenever `defKey` changes or
 * the graphic's shape does (see {@link shapeKey}). Changing the facing
 * keeps the variant; changing the variant keeps the facing, which every
 * directional variant has, so there is no fallback to compute. Selecting
 * a slot moves to that slot's own default variant.
 */
export function useGraphicViewer(
  graphic: MaybeRefOrGetter<DefGraphicDto | undefined>,
  defKey: MaybeRefOrGetter<string>,
) {
  const slotIndex = ref(0);
  const variantIndex = ref(0);
  const facing = ref<FacingDto>(DEFAULT_FACING);

  const slots = computed(() => slotsOf(toValue(graphic)) ?? []);
  const slot = computed<GraphicSlotDto | null>(() => slots.value[slotIndex.value] ?? null);
  const variant = computed<GraphicVariantDto | null>(
    () => slot.value?.variants[variantIndex.value] ?? null,
  );
  const isFacingShown = computed(
    () => variant.value !== null && isDirectional(variant.value.faces),
  );
  const face = computed<GraphicFaceDto | null>(() =>
    variant.value === null ? null : faceOf(variant.value.faces, facing.value),
  );

  function reset(): void {
    const current = toValue(graphic);
    if (current?.kind === "resolved") {
      slotIndex.value = current.defaultView.slot;
      variantIndex.value = current.defaultView.variant;
    } else {
      slotIndex.value = 0;
      variantIndex.value = 0;
    }
    facing.value = DEFAULT_FACING;
  }

  watch(() => shapeKey(toValue(defKey), toValue(graphic)), reset, { immediate: true });

  function selectSlot(index: number): void {
    const target = slots.value[index];
    if (target === undefined) {
      return;
    }
    slotIndex.value = index;
    variantIndex.value = target.defaultVariant;
  }

  function selectVariant(index: number): void {
    if (slot.value?.variants[index] !== undefined) {
      variantIndex.value = index;
    }
  }

  /** Moves the variant by `delta`, wrapping at both ends. */
  function stepVariant(delta: number): void {
    const count = slot.value?.variants.length ?? 0;
    if (count === 0) {
      return;
    }
    variantIndex.value = (((variantIndex.value + delta) % count) + count) % count;
  }

  function selectFacing(next: FacingDto): void {
    facing.value = next;
  }

  /** Moves the facing by `delta` places around N, E, S, W, wrapping. */
  function stepFacing(delta: number): void {
    const at = FACINGS.indexOf(facing.value);
    const next = FACINGS[(((at + delta) % FACINGS.length) + FACINGS.length) % FACINGS.length];
    if (next !== undefined) {
      facing.value = next;
    }
  }

  return {
    slots,
    slot,
    variant,
    face,
    slotIndex,
    variantIndex,
    facing,
    isFacingShown,
    selectSlot,
    selectVariant,
    stepVariant,
    selectFacing,
    stepFacing,
  };
}
