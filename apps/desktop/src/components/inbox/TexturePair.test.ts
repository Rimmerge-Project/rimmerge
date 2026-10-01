import { PiniaColada } from "@pinia/colada";
import { clearMocks } from "@tauri-apps/api/mocks";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import TexturePair from "@/components/inbox/TexturePair.vue";
import { installMockIpc } from "@/services/ipc.mock";
import type { AlternativeDto } from "@/types/generated/AlternativeDto";
import type { FindingDto } from "@/types/generated/FindingDto";

const FINDING: Extract<FindingDto, { kind: "textureOverride" }> = {
  kind: "textureOverride",
  texturePath: "Things/Wall.png",
  owners: ["a.mod", "b.mod"],
  // Not the last owner: the scan-order list and the selected-order winner disagree.
  winner: "a.mod",
};

// One `preferWinner` then one `shipAsset` per owner, in that order —
// mirrors `rim_resolve::ledger::suggest::texture_override`'s own shape.
const ALTERNATIVES: AlternativeDto[] = [
  {
    action: { kind: "preferWinner", key: { defType: "_", defName: "_" }, winner: "a.mod" },
    rationale: "Force one mod's texture to win.",
    rationaleCode: { kind: "forceTextureWinner" },
  },
  {
    action: { kind: "preferWinner", key: { defType: "_", defName: "_" }, winner: "b.mod" },
    rationale: "Force one mod's texture to win.",
    rationaleCode: { kind: "forceTextureWinner" },
  },
  {
    action: { kind: "shipAsset", texturePath: "Things/Wall.png", from: "a.mod" },
    rationale: "Copy this mod's texture file into the merge mod.",
    rationaleCode: { kind: "copyTextureIntoMergeMod" },
  },
  {
    action: { kind: "shipAsset", texturePath: "Things/Wall.png", from: "b.mod" },
    rationale: "Copy this mod's texture file into the merge mod.",
    rationaleCode: { kind: "copyTextureIntoMergeMod" },
  },
];

beforeEach(() =>
  installMockIpc({
    list_mod_names: {},
    read_texture: (payload: unknown) => ({
      dataUrl: `data:image/png;base64,${(payload as { modId: string }).modId}`,
      format: "png",
      bytes: 1,
      path: `Textures/${(payload as { modId: string }).modId}.png`,
    }),
  }),
);
afterEach(() => clearMocks());

function mountPair(
  alternatives: AlternativeDto[] = ALTERNATIVES,
  finding: typeof FINDING = FINDING,
) {
  return mount(TexturePair, {
    props: { finding, alternatives },
    global: { plugins: [createPinia(), PiniaColada] },
  });
}

describe("TexturePair", () => {
  it("shows one tile per owner, each with its own image, and badges the finding's winner", async () => {
    const wrapper = mountPair();
    await flushPromises();

    const images = wrapper.findAll('[data-testid="texture-image"]');
    expect(images).toHaveLength(2);

    const winnerTile = wrapper.get('[data-testid="texture-tile-a.mod"]');
    expect(winnerTile.find('[data-testid="texture-winner-badge"]').exists()).toBe(true);
    const loserTile = wrapper.get('[data-testid="texture-tile-b.mod"]');
    expect(loserTile.find('[data-testid="texture-winner-badge"]').exists()).toBe(false);
  });

  it("moves the badge with the winner while the owner list stays as it was", async () => {
    const wrapper = mountPair(ALTERNATIVES, { ...FINDING, winner: "b.mod" });
    await flushPromises();

    expect(
      wrapper
        .get('[data-testid="texture-tile-b.mod"]')
        .find('[data-testid="texture-winner-badge"]')
        .exists(),
    ).toBe(true);
    expect(
      wrapper
        .get('[data-testid="texture-tile-a.mod"]')
        .find('[data-testid="texture-winner-badge"]')
        .exists(),
    ).toBe(false);
  });

  it("emits pickAlternative with the preferWinner alternative's own index when 'Use this one' is clicked", async () => {
    const wrapper = mountPair();
    await flushPromises();

    await wrapper
      .get('[data-testid="texture-tile-a.mod"] [data-testid="texture-use-this-one"]')
      .trigger("click");

    expect(wrapper.emitted("pickAlternative")).toEqual([[1]]);
  });

  it("emits pickAlternative with the shipAsset alternative's own index when 'Ship this file' is clicked", async () => {
    const wrapper = mountPair();
    await flushPromises();

    await wrapper
      .get('[data-testid="texture-tile-b.mod"] [data-testid="texture-ship-this-file"]')
      .trigger("click");

    expect(wrapper.emitted("pickAlternative")).toEqual([[4]]);
  });

  it("disables both buttons when the ledger offered no matching alternative", async () => {
    const wrapper = mountPair([]);
    await flushPromises();

    const tile = wrapper.get('[data-testid="texture-tile-a.mod"]');
    expect(
      (tile.get('[data-testid="texture-use-this-one"]').element as HTMLButtonElement).disabled,
    ).toBe(true);
    expect(
      (tile.get('[data-testid="texture-ship-this-file"]').element as HTMLButtonElement).disabled,
    ).toBe(true);
  });
});
