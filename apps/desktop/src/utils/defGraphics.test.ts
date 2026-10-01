import { describe, expect, it } from "vitest";

import type { DefTextureDto } from "@/types/generated/DefTextureDto";
import type { SlotSourceDto } from "@/types/generated/SlotSourceDto";
import type { VariantLabelDto } from "@/types/generated/VariantLabelDto";
import {
  faceOf,
  isDirectional,
  isOverrideOfOwner,
  looseOwnerOf,
  presentEmptyGraphic,
  presentTexture,
  slotLabel,
  variantLabel,
} from "@/utils/defGraphics";
import {
  face,
  loose,
  multiVariant,
  ONE_PIXEL_PNG,
  singleVariant,
} from "@/utils/defGraphics.test-support";

describe("slotLabel", () => {
  it("maps every slot source to its own key, carrying the parameters", () => {
    const cases: [SlotSourceDto, string, Record<string, unknown> | undefined][] = [
      [{ kind: "graphic" }, "defGraphics.slot.graphic", undefined],
      [{ kind: "worn" }, "defGraphics.slot.worn", undefined],
      [{ kind: "lifeStage" }, "defGraphics.slot.lifeStage", undefined],
      [
        { kind: "raceKind", defName: "Colonist" },
        "defGraphics.slot.raceKind",
        { name: "Colonist" },
      ],
      [{ kind: "styleTexture" }, "defGraphics.slot.styleTexture", undefined],
      [{ kind: "headType" }, "defGraphics.slot.headType", undefined],
      [{ kind: "bodyType" }, "defGraphics.slot.bodyType", undefined],
      [{ kind: "terrain" }, "defGraphics.slot.terrain", undefined],
      [{ kind: "icon" }, "defGraphics.slot.icon", undefined],
      [{ kind: "probe", field: "a/li" }, "defGraphics.slot.probe", { field: "a/li" }],
    ];

    for (const [source, key, params] of cases) {
      const label = slotLabel(source);
      expect(label.key).toBe(key);
      expect(label.params).toEqual(params);
    }
  });
});

describe("variantLabel", () => {
  it("maps every variant label, turning zero-based indexes into one-based numbers", () => {
    const cases: [VariantLabelDto, string, Record<string, unknown> | undefined][] = [
      [{ kind: "only" }, "defGraphics.variant.only", undefined],
      [{ kind: "bodyType", name: "Thin" }, "defGraphics.variant.bodyType", { name: "Thin" }],
      [{ kind: "lifeStage", index: 0, count: 3 }, "defGraphics.variant.lifeStage", { number: 1 }],
      [{ kind: "female", stage: 1 }, "defGraphics.variant.female", { number: 2 }],
      [{ kind: "alternate", index: 2 }, "defGraphics.variant.alternate", { number: 3 }],
      [
        { kind: "member", index: 0, count: 2, name: "a" },
        "defGraphics.variant.member",
        { name: "a" },
      ],
      [{ kind: "stack", index: 0, count: 3 }, "defGraphics.variant.stackSingle", undefined],
      [{ kind: "stack", index: 2, count: 3 }, "defGraphics.variant.stack", { number: 2 }],
      [
        { kind: "appearance", name: "Planks" },
        "defGraphics.variant.appearance",
        { name: "Planks" },
      ],
      [
        { kind: "wornPath", index: 0, count: 2, bodyType: null },
        "defGraphics.variant.wornPath",
        { number: 1 },
      ],
      [
        { kind: "wornPath", index: 1, count: 2, bodyType: "Hulk" },
        "defGraphics.variant.wornPathOnBodyType",
        { number: 2, name: "Hulk" },
      ],
      [{ kind: "field", name: "uiIconPath" }, "defGraphics.variant.field", { name: "uiIconPath" }],
    ];

    for (const [label, key, params] of cases) {
      const described = variantLabel(label);
      expect(described.key).toBe(key);
      expect(described.params).toEqual(params);
    }
  });
});

describe("presentTexture", () => {
  const png = (from: "direct" | "pngSibling"): DefTextureDto => ({
    kind: "image",
    dataUrl: ONE_PIXEL_PNG,
    format: "png",
    bytes: 68,
    owner: "a",
    from,
  });

  it("shows a direct image with no note and a PNG sibling with its note", () => {
    expect(presentTexture(png("direct"))).toEqual({
      kind: "image",
      dataUrl: ONE_PIXEL_PNG,
      note: null,
    });
    const sibling = presentTexture(png("pngSibling"));
    expect(sibling.kind === "image" && sibling.note?.key).toBe("defGraphics.state.pngSiblingNote");
  });

  it("labels every non-image outcome with its own sentence", () => {
    const cases: [DefTextureDto, string][] = [
      [{ kind: "ddsNotPreviewable", owner: "a" }, "defGraphics.state.ddsNotPreviewable"],
      [{ kind: "undecodableInGame", owner: "a" }, "defGraphics.state.undecodableInGame"],
      [{ kind: "notViewable", isUncertain: false }, "defGraphics.state.notViewable"],
      [{ kind: "notViewable", isUncertain: true }, "defGraphics.state.notViewableUncertain"],
      [{ kind: "notFound" }, "defGraphics.state.notFound"],
      [{ kind: "unreadable", reason: "tooLarge" }, "defGraphics.state.tooLarge"],
      [{ kind: "unreadable", reason: "unsupportedFormat" }, "defGraphics.state.unsupportedFormat"],
      [{ kind: "unreadable", reason: "io" }, "defGraphics.state.unreadable"],
    ];

    for (const [texture, key] of cases) {
      const presentation = presentTexture(texture);
      expect(presentation.kind).toBe("fallback");
      expect(presentation.kind === "fallback" && presentation.message.key).toBe(key);
    }
  });
});

describe("presentEmptyGraphic", () => {
  it("words the two outcomes that have no slot", () => {
    expect(presentEmptyGraphic({ kind: "noGraphic" }).message.key).toBe(
      "defGraphics.state.noGraphic",
    );
    expect(presentEmptyGraphic({ kind: "composedAtRuntime" }).message.key).toBe(
      "defGraphics.state.composedAtRuntime",
    );
  });
});

describe("faces and owners", () => {
  it("picks a direction from a multi graphic and the same face from a single one", () => {
    const multi = multiVariant("t").faces;
    const single = singleVariant("t_only").faces;

    expect(faceOf(multi, "north").textureKey).toBe("t_north");
    expect(faceOf(multi, "west").isMirrored).toBe(true);
    expect(faceOf(single, "east").textureKey).toBe("t_only");
    expect(isDirectional(multi)).toBe(true);
    expect(isDirectional(single)).toBe(false);
  });

  it("names the owner of a loose face only", () => {
    expect(looseOwnerOf(face("a", { availability: loose("m") }))).toBe("m");
    expect(looseOwnerOf(face("a", { availability: { kind: "notFound" } }))).toBeNull();
    expect(looseOwnerOf(face("a", { availability: { kind: "baseGameOrBundle" } }))).toBeNull();
    expect(looseOwnerOf(face("a", { availability: { kind: "unknown" } }))).toBeNull();
  });

  it("flags a provider other than the def's owner, treating a _steam copy as the same mod", () => {
    expect(isOverrideOfOwner("retexture.mod", "base.mod")).toBe(true);
    expect(isOverrideOfOwner("base.mod_steam", "base.mod")).toBe(false);
    expect(isOverrideOfOwner("base.mod", null)).toBe(false);
  });
});
