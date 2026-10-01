<script setup lang="ts">
import TextureTile from "@/components/inbox/TextureTile.vue";
import type { AlternativeDto } from "@/types/generated/AlternativeDto";
import type { FindingDto } from "@/types/generated/FindingDto";

/**
 * The texture-override change summary: every owner's texture file, side
 * by side, through `read_texture`. "Use this one"/"Ship this file" route
 * through the exact same `pickAlternative` index `AlternativeList`'s own
 * `1`..`9` keyboard shortcuts use — found here, not decided here, so the
 * keyboard and the buttons can never disagree about which alternative a
 * choice maps to (see `crate::ledger::suggest::texture_override`: one
 * `preferWinner` alternative per owner, then one `shipAsset` per owner, in
 * that order). The winner badge follows `finding.winner`, which the
 * backend computes for the selected order: `finding.owners` is in scan
 * order, so its last entry is not necessarily the winner.
 */
const { finding, alternatives } = defineProps<{
  finding: Extract<FindingDto, { kind: "textureOverride" }>;
  alternatives: AlternativeDto[];
}>();
const emit = defineEmits<{ pickAlternative: [oneBasedIndex: number] }>();

/** 1-based index of the `preferWinner` alternative naming `modId`, or `null` when the ledger didn't offer one. */
function preferWinnerIndex(modId: string): number | null {
  const index = alternatives.findIndex(
    (alternative) =>
      alternative.action.kind === "preferWinner" && alternative.action.winner === modId,
  );
  return index === -1 ? null : index + 1;
}

/** 1-based index of the `shipAsset` alternative naming `modId` as the source, or `null`. */
function shipAssetIndex(modId: string): number | null {
  const index = alternatives.findIndex(
    (alternative) => alternative.action.kind === "shipAsset" && alternative.action.from === modId,
  );
  return index === -1 ? null : index + 1;
}

function handlePreferWinner(modId: string): void {
  const index = preferWinnerIndex(modId);
  if (index !== null) {
    emit("pickAlternative", index);
  }
}

function handleShipAsset(modId: string): void {
  const index = shipAssetIndex(modId);
  if (index !== null) {
    emit("pickAlternative", index);
  }
}
</script>

<template>
  <div
    class="flex flex-wrap gap-3"
    data-testid="texture-pair"
  >
    <TextureTile
      v-for="owner in finding.owners"
      :key="owner"
      :mod-id="owner"
      :texture-path="finding.texturePath"
      :is-winner="owner === finding.winner"
      :can-prefer-winner="preferWinnerIndex(owner) !== null"
      :can-ship-asset="shipAssetIndex(owner) !== null"
      @prefer-winner="handlePreferWinner(owner)"
      @ship-asset="handleShipAsset(owner)"
    />
  </div>
</template>
