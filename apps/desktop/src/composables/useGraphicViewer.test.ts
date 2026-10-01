import { describe, expect, it } from "vitest";
import { nextTick, ref } from "vue";

import { useGraphicViewer } from "@/composables/useGraphicViewer";
import type { DefGraphicDto } from "@/types/generated/DefGraphicDto";
import {
  bodyTypeVariants,
  multiVariant,
  resolved,
  singleVariant,
  slot,
} from "@/utils/defGraphics.test-support";

function setup(initial: DefGraphicDto | undefined, defKey = "ThingDef/A") {
  const graphic = ref<DefGraphicDto | undefined>(initial);
  const key = ref(defKey);
  const viewer = useGraphicViewer(graphic, key);
  return { graphic, key, viewer };
}

describe("useGraphicViewer", () => {
  it("starts on the graphic's default view, facing south", () => {
    const { viewer } = setup(
      resolved([slot([singleVariant("a"), singleVariant("b")], { defaultVariant: 1 })]),
    );

    expect(viewer.variantIndex.value).toBe(1);
    expect(viewer.facing.value).toBe("south");
    expect(viewer.face.value?.textureKey).toBe("b");
  });

  it("hides the facing control for a single graphic and shows it for a multi one", () => {
    const single = setup(resolved([slot([singleVariant("a")])]));
    const multi = setup(resolved([slot([multiVariant("m")])]));

    expect(single.viewer.isFacingShown.value).toBe(false);
    expect(multi.viewer.isFacingShown.value).toBe(true);
    expect(multi.viewer.face.value?.textureKey).toBe("m_south");
  });

  it("wraps the facing and variant steppers at both ends", () => {
    const { viewer } = setup(resolved([slot(bodyTypeVariants(3))]));

    viewer.stepVariant(-1);
    expect(viewer.variantIndex.value).toBe(2);
    viewer.stepVariant(1);
    expect(viewer.variantIndex.value).toBe(0);

    viewer.selectFacing("west");
    viewer.stepFacing(1);
    expect(viewer.facing.value).toBe("north");
    viewer.stepFacing(-1);
    expect(viewer.facing.value).toBe("west");
  });

  it("keeps the facing when the variant changes and the variant when the facing changes", () => {
    const { viewer } = setup(resolved([slot([multiVariant("a"), multiVariant("b")])]));

    viewer.selectFacing("east");
    viewer.stepVariant(1);
    expect(viewer.facing.value).toBe("east");
    expect(viewer.face.value?.textureKey).toBe("b_east");

    viewer.selectFacing("north");
    expect(viewer.variantIndex.value).toBe(1);
  });

  it("moves to the chosen slot's own default variant", () => {
    const { viewer } = setup(
      resolved([slot([singleVariant("a")]), slot(bodyTypeVariants(3), { defaultVariant: 2 })]),
    );

    viewer.selectSlot(1);

    expect(viewer.slotIndex.value).toBe(1);
    expect(viewer.variantIndex.value).toBe(2);
  });

  it("ignores a slot or variant index that does not exist", () => {
    const { viewer } = setup(resolved([slot([singleVariant("a")])]));

    viewer.selectSlot(5);
    viewer.selectVariant(5);

    expect(viewer.slotIndex.value).toBe(0);
    expect(viewer.variantIndex.value).toBe(0);
  });

  it("resets to the default view when the def changes", async () => {
    const { viewer, key } = setup(resolved([slot(bodyTypeVariants(3))]));
    viewer.stepVariant(1);
    viewer.selectFacing("north");

    key.value = "ThingDef/B";
    await nextTick();

    expect(viewer.variantIndex.value).toBe(0);
    expect(viewer.facing.value).toBe("south");
  });

  it("keeps the user's selection when a refetch returns an equal graphic in a new object", async () => {
    const make = () => resolved([slot(bodyTypeVariants(3))]);
    const { viewer, graphic } = setup(make());
    viewer.stepVariant(1);
    viewer.selectFacing("east");

    graphic.value = make();
    await nextTick();

    expect(viewer.variantIndex.value).toBe(1);
    expect(viewer.facing.value).toBe("east");
  });

  it("resets when the graphic's shape changes under the same def", async () => {
    const { viewer, graphic } = setup(resolved([slot(bodyTypeVariants(3))]));
    viewer.stepVariant(1);

    graphic.value = resolved([slot(bodyTypeVariants(2))]);
    await nextTick();

    expect(viewer.variantIndex.value).toBe(0);
  });

  it("has nothing selected for a def that shows no texture", () => {
    const { viewer } = setup({ kind: "noGraphic" });

    expect(viewer.face.value).toBeNull();
    expect(viewer.slots.value).toEqual([]);
    expect(viewer.isFacingShown.value).toBe(false);
  });
});
