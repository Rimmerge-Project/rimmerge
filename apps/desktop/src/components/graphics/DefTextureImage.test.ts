import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";

import DefTextureImage from "@/components/graphics/DefTextureImage.vue";
import { RimmergeError } from "@/services/ipc";
import type { DefTextureDto } from "@/types/generated/DefTextureDto";
import { ONE_PIXEL_PNG } from "@/utils/defGraphics.test-support";

function render(
  texture: DefTextureDto | undefined,
  options: {
    size?: "thumb" | "viewer";
    mirrored?: boolean;
    isPending?: boolean;
    error?: unknown;
  } = {},
) {
  return mount(DefTextureImage, {
    props: {
      texture,
      isPending: options.isPending ?? false,
      error: options.error ?? null,
      size: options.size ?? "viewer",
      mirrored: options.mirrored ?? false,
      alt: "Wall, South view",
    },
  });
}

const image = (from: "direct" | "pngSibling"): DefTextureDto => ({
  kind: "image",
  dataUrl: ONE_PIXEL_PNG,
  format: "png",
  bytes: 68,
  owner: "a",
  from,
});

describe("DefTextureImage", () => {
  it("renders a direct image with its alt text and no note", () => {
    const wrapper = render(image("direct"));

    const img = wrapper.get('[data-testid="def-texture-image"]');
    expect(img.attributes("src")).toBe(ONE_PIXEL_PNG);
    expect(img.attributes("alt")).toBe("Wall, South view");
    expect(wrapper.find('[data-testid="def-texture-note"]').exists()).toBe(false);
  });

  it("renders a PNG sibling image with the note that the game loads the DDS", () => {
    const wrapper = render(image("pngSibling"));

    expect(wrapper.find('[data-testid="def-texture-image"]').exists()).toBe(true);
    expect(wrapper.get('[data-testid="def-texture-note"]').text()).toBe(
      "Shown from the PNG copy; the game loads the DDS.",
    );
  });

  it("flips a mirrored face and leaves an ordinary one alone", () => {
    expect(
      render(image("direct"), { mirrored: true })
        .get('[data-testid="def-texture-image"]')
        .classes(),
    ).toContain("-scale-x-100");
    expect(
      render(image("direct")).get('[data-testid="def-texture-image"]').classes(),
    ).not.toContain("-scale-x-100");
  });

  it.each<[DefTextureDto, string]>([
    [{ kind: "ddsNotPreviewable", owner: "a" }, "DDS texture. No preview is available."],
    [
      { kind: "undecodableInGame", owner: "a" },
      "The game can't decode this DDS file and shows a placeholder.",
    ],
    [
      { kind: "notViewable", isUncertain: false },
      "Built into the game or an asset bundle, which can't be shown here.",
    ],
    [
      { kind: "notViewable", isUncertain: true },
      "No texture file was found. It may be built into the game, which can't be shown here.",
    ],
    [{ kind: "notFound" }, "No texture file found. The game shows its error texture."],
    [{ kind: "unreadable", reason: "tooLarge" }, "The image file is too large to preview."],
    [{ kind: "unreadable", reason: "unsupportedFormat" }, "The image format can't be previewed."],
    [{ kind: "unreadable", reason: "io" }, "The image file couldn't be read."],
  ])("labels %j with its sentence in the viewer and no image", (texture, sentence) => {
    const wrapper = render(texture);

    expect(wrapper.get('[data-testid="def-texture-message"]').text()).toBe(sentence);
    expect(wrapper.find('[data-testid="def-texture-image"]').exists()).toBe(false);
  });

  it("puts the same sentence in a tooltip, not in the text, for a thumbnail", () => {
    const wrapper = render({ kind: "notFound" }, { size: "thumb" });

    expect(wrapper.get('[data-testid="def-texture-box"]').attributes("title")).toBe(
      "No texture file found. The game shows its error texture.",
    );
    expect(wrapper.find('[data-testid="def-texture-message"]').exists()).toBe(false);
    expect(wrapper.find('[data-testid="def-texture-fallback"]').exists()).toBe(true);
  });

  it("gives a thumbnail image an empty alt, since it is decorative", () => {
    const wrapper = render(image("direct"), { size: "thumb" });

    expect(wrapper.get('[data-testid="def-texture-image"]').attributes("alt")).toBe("");
  });

  it("shows a status skeleton while pending", () => {
    const wrapper = render(undefined, { isPending: true });

    const skeleton = wrapper.get('[data-testid="def-texture-pending"]');
    expect(skeleton.attributes("role")).toBe("status");
    expect(skeleton.attributes("aria-label")).toBe("Loading texture");
  });

  it("describes an error by its code, with the raw message only as a technical detail", () => {
    const error = new RimmergeError({
      code: "texture_unsupported_format",
      message: "RAW ENGLISH",
      detail: null,
    });
    const wrapper = render(undefined, { error });

    expect(wrapper.get('[data-testid="def-texture-error-message"]').text()).toBe(
      "Texture format can't be shown",
    );
    expect(wrapper.text()).toContain("Technical details: RAW ENGLISH");
  });
});
