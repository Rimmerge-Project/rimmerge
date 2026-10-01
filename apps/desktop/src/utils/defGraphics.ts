// Pure helpers behind the def-graphic viewer and thumbnails: every DTO union
// is mapped through an exhaustive switch to a `MessageDescriptor` (a
// literal i18n key), so a new backend variant is a compile error here
// rather than a silently unlabelled control.

import type { MessageDescriptor } from "@/i18n/messageDescriptor";
import { descriptor } from "@/i18n/messageDescriptor";
import type { DefGraphicDto } from "@/types/generated/DefGraphicDto";
import type { DefTextureDto } from "@/types/generated/DefTextureDto";
import type { FacingDto } from "@/types/generated/FacingDto";
import type { GraphicFaceDto } from "@/types/generated/GraphicFaceDto";
import type { GraphicFacesDto } from "@/types/generated/GraphicFacesDto";
import type { SlotSourceDto } from "@/types/generated/SlotSourceDto";
import type { VariantLabelDto } from "@/types/generated/VariantLabelDto";
import { assertNever } from "@/utils/assertNever";
import { baseModId } from "@/utils/modId";

/** The four directions of a directional graphic, in the engine's slot order. */
export const FACINGS: readonly FacingDto[] = ["north", "east", "south", "west"];

/** The direction shown first, and the fallback when a variant has no such face. */
export const DEFAULT_FACING: FacingDto = "south";

/** Beyond this many variants the variant control becomes a dropdown. */
export const MAX_STEPPER_VARIANTS = 8;

/** The full name of a direction (a button's accessible name). */
export function facingName(facing: FacingDto): MessageDescriptor {
  switch (facing) {
    case "north":
      return descriptor("defGraphics.facing.north");
    case "east":
      return descriptor("defGraphics.facing.east");
    case "south":
      return descriptor("defGraphics.facing.south");
    case "west":
      return descriptor("defGraphics.facing.west");
    default:
      return assertNever(facing);
  }
}

/** The one-letter label of a direction (a button's visible text). */
export function facingShortName(facing: FacingDto): MessageDescriptor {
  switch (facing) {
    case "north":
      return descriptor("defGraphics.facingShort.north");
    case "east":
      return descriptor("defGraphics.facingShort.east");
    case "south":
      return descriptor("defGraphics.facingShort.south");
    case "west":
      return descriptor("defGraphics.facingShort.west");
    default:
      return assertNever(facing);
  }
}

/** What a slot is, for the slot dropdown. */
export function slotLabel(source: SlotSourceDto): MessageDescriptor {
  switch (source.kind) {
    case "graphic":
      return descriptor("defGraphics.slot.graphic");
    case "worn":
      return descriptor("defGraphics.slot.worn");
    case "lifeStage":
      return descriptor("defGraphics.slot.lifeStage");
    case "raceKind":
      return descriptor("defGraphics.slot.raceKind", { name: source.defName });
    case "styleTexture":
      return descriptor("defGraphics.slot.styleTexture");
    case "headType":
      return descriptor("defGraphics.slot.headType");
    case "bodyType":
      return descriptor("defGraphics.slot.bodyType");
    case "terrain":
      return descriptor("defGraphics.slot.terrain");
    case "icon":
      return descriptor("defGraphics.slot.icon");
    case "probe":
      return descriptor("defGraphics.slot.probe", { field: source.field });
    default:
      return assertNever(source);
  }
}

/** What distinguishes a variant from its siblings (zero-based indexes become one-based numbers). */
export function variantLabel(label: VariantLabelDto): MessageDescriptor {
  switch (label.kind) {
    case "only":
      return descriptor("defGraphics.variant.only");
    case "bodyType":
      return descriptor("defGraphics.variant.bodyType", { name: label.name });
    case "lifeStage":
      return descriptor("defGraphics.variant.lifeStage", { number: label.index + 1 });
    case "female":
      return descriptor("defGraphics.variant.female", { number: label.stage + 1 });
    case "alternate":
      return descriptor("defGraphics.variant.alternate", { number: label.index + 1 });
    case "member":
      return descriptor("defGraphics.variant.member", { name: label.name });
    case "stack":
      return label.index === 0
        ? descriptor("defGraphics.variant.stackSingle")
        : descriptor("defGraphics.variant.stack", { number: label.index });
    case "appearance":
      return descriptor("defGraphics.variant.appearance", { name: label.name });
    case "wornPath":
      return label.bodyType === null
        ? descriptor("defGraphics.variant.wornPath", { number: label.index + 1 })
        : descriptor("defGraphics.variant.wornPathOnBodyType", {
            number: label.index + 1,
            name: label.bodyType,
          });
    case "field":
      return descriptor("defGraphics.variant.field", { name: label.name });
    default:
      return assertNever(label);
  }
}

/** The face of `faces` for `facing`; a single graphic has the same face for every direction. */
export function faceOf(faces: GraphicFacesDto, facing: FacingDto): GraphicFaceDto {
  switch (faces.kind) {
    case "single":
      return faces.face;
    case "multi":
      return faces[facing];
    default:
      return assertNever(faces);
  }
}

/** Whether `faces` needs a facing control. */
export function isDirectional(faces: GraphicFacesDto): boolean {
  switch (faces.kind) {
    case "single":
      return false;
    case "multi":
      return true;
    default:
      return assertNever(faces);
  }
}

/** The mod whose loose file serves `face`, or `null` when nothing does. */
export function looseOwnerOf(face: GraphicFaceDto): string | null {
  const { availability } = face;
  switch (availability.kind) {
    case "loose":
      return availability.owner;
    case "baseGameOrBundle":
    case "notFound":
    case "unknown":
      return null;
    default:
      return assertNever(availability);
  }
}

/**
 * Whether a file served from `owner` replaces what `defOwner`, the def's
 * own mod, ships: the retexture case worth naming. A `_steam` copy counts
 * as the same mod.
 */
export function isOverrideOfOwner(owner: string, defOwner: string | null): boolean {
  return defOwner !== null && baseModId(owner) !== baseModId(defOwner);
}

/** What shows in place of an image, and why. */
export type TextureFallback = {
  /** A PrimeIcons class for the thumbnail glyph. */
  readonly icon: string;
  readonly message: MessageDescriptor;
};

/** How one read texture is presented: as an image (with an optional note) or as a labelled fallback. */
export type TexturePresentation =
  | {
      readonly kind: "image";
      /** The `data:` URL to render, taken from the texture itself. */
      readonly dataUrl: string;
      readonly note: MessageDescriptor | null;
    }
  | ({ readonly kind: "fallback" } & TextureFallback);

/** The presentation of every `DefTextureDto` variant. */
export function presentTexture(texture: DefTextureDto): TexturePresentation {
  switch (texture.kind) {
    case "image":
      switch (texture.from) {
        case "direct":
          return { kind: "image", dataUrl: texture.dataUrl, note: null };
        case "pngSibling":
          return {
            kind: "image",
            dataUrl: texture.dataUrl,
            note: descriptor("defGraphics.state.pngSiblingNote"),
          };
        default:
          return assertNever(texture.from);
      }
    case "ddsNotPreviewable":
      return {
        kind: "fallback",
        icon: "pi-image",
        message: descriptor("defGraphics.state.ddsNotPreviewable"),
      };
    case "undecodableInGame":
      return {
        kind: "fallback",
        icon: "pi-exclamation-triangle",
        message: descriptor("defGraphics.state.undecodableInGame"),
      };
    case "notViewable":
      return {
        kind: "fallback",
        icon: "pi-box",
        message: texture.isUncertain
          ? descriptor("defGraphics.state.notViewableUncertain")
          : descriptor("defGraphics.state.notViewable"),
      };
    case "notFound":
      return {
        kind: "fallback",
        icon: "pi-ban",
        message: descriptor("defGraphics.state.notFound"),
      };
    case "unreadable":
      return { kind: "fallback", icon: "pi-exclamation-circle", message: unreadable(texture) };
    default:
      return assertNever(texture);
  }
}

function unreadable(texture: Extract<DefTextureDto, { kind: "unreadable" }>): MessageDescriptor {
  switch (texture.reason) {
    case "tooLarge":
      return descriptor("defGraphics.state.tooLarge");
    case "unsupportedFormat":
      return descriptor("defGraphics.state.unsupportedFormat");
    case "io":
      return descriptor("defGraphics.state.unreadable");
    default:
      return assertNever(texture.reason);
  }
}

/** What a def with no slots to show says, for the two non-resolved outcomes. */
export function presentEmptyGraphic(
  graphic: Exclude<DefGraphicDto, { kind: "resolved" }>,
): TextureFallback {
  switch (graphic.kind) {
    case "noGraphic":
      return { icon: "pi-minus-circle", message: descriptor("defGraphics.state.noGraphic") };
    case "composedAtRuntime":
      return { icon: "pi-users", message: descriptor("defGraphics.state.composedAtRuntime") };
    default:
      return assertNever(graphic);
  }
}
