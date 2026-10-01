import type { DefGraphicDto } from "@/types/generated/DefGraphicDto";
import type { FaceAvailabilityDto } from "@/types/generated/FaceAvailabilityDto";
import type { FacingDto } from "@/types/generated/FacingDto";
import type { GraphicFaceDto } from "@/types/generated/GraphicFaceDto";
import type { GraphicSlotDto } from "@/types/generated/GraphicSlotDto";
import type { GraphicVariantDto } from "@/types/generated/GraphicVariantDto";
import type { SlotSourceDto } from "@/types/generated/SlotSourceDto";
import type { VariantLabelDto } from "@/types/generated/VariantLabelDto";

/** A 1x1 PNG, the same one the mock-IPC tier serves. */
export const ONE_PIXEL_PNG =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGNgAAIAAAUAAen63NgAAAAASUVORK5CYII=";

export function loose(owner: string): FaceAvailabilityDto {
  return { kind: "loose", owner };
}

export function face(
  textureKey: string,
  options: {
    availability?: FaceAvailabilityDto;
    facing?: FacingDto | null;
    isMirrored?: boolean;
  } = {},
): GraphicFaceDto {
  return {
    textureKey,
    facing: options.facing ?? null,
    isMirrored: options.isMirrored ?? false,
    availability: options.availability ?? loose("owner.mod"),
  };
}

/** A variant with one texture for every direction. */
export function singleVariant(
  textureKey: string,
  label: VariantLabelDto = { kind: "only" },
  availability: FaceAvailabilityDto = loose("owner.mod"),
): GraphicVariantDto {
  return {
    label,
    faces: { kind: "single", face: face(textureKey, { availability }) },
    isLocated: availability.kind === "loose",
  };
}

/** A four-direction variant whose keys are `<prefix>_<direction>`; west is east, mirrored. */
export function multiVariant(
  prefix: string,
  availability: FaceAvailabilityDto = loose("owner.mod"),
): GraphicVariantDto {
  return {
    label: { kind: "only" },
    faces: {
      kind: "multi",
      north: face(`${prefix}_north`, { availability, facing: "north" }),
      east: face(`${prefix}_east`, { availability, facing: "east" }),
      south: face(`${prefix}_south`, { availability, facing: "south" }),
      west: face(`${prefix}_east`, { availability, facing: "west", isMirrored: true }),
    },
    isLocated: availability.kind === "loose",
  };
}

export function slot(
  variants: GraphicVariantDto[],
  options: { source?: SlotSourceDto; defaultVariant?: number; truncated?: number } = {},
): GraphicSlotDto {
  return {
    source: options.source ?? { kind: "graphic" },
    graphicClass: null,
    isLayoutInferred: false,
    variants,
    defaultVariant: options.defaultVariant ?? 0,
    truncated: options.truncated ?? 0,
    isLocated: variants.some((variant) => variant.isLocated),
  };
}

export function resolved(slots: GraphicSlotDto[], defaultSlot = 0): DefGraphicDto {
  return {
    kind: "resolved",
    slots,
    defaultView: { slot: defaultSlot, variant: slots[defaultSlot]?.defaultVariant ?? 0 },
    truncatedSlots: 0,
  };
}

/** Body-type variants `Body0` .. `Body<count-1>`, each a single texture `things/body<n>`. */
export function bodyTypeVariants(count: number): GraphicVariantDto[] {
  return Array.from({ length: count }, (_, index) =>
    singleVariant(`things/body${index}`, { kind: "bodyType", name: `Body${index}` }),
  );
}
