import { useQuery } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { queryKeys } from "@/queries/keys";
import { readDefTexture, resolveDefGraphic } from "@/services/ipc";
import type { DefRef } from "@/types/brands";

/**
 * What one def shows under the selected order. Disabled while `defRef` is
 * `null`. The selected order isn't part of the key: switching it already
 * invalidates every query on success (`useOrderSource`). `staleTime:
 * Infinity`: the answer only changes with the order or the project.
 */
export function useDefGraphicQuery(defRef: MaybeRefOrGetter<DefRef | null>) {
  return useQuery({
    key: () => queryKeys.defs.graphic(toValue(defRef) ?? ""),
    query: () => resolveDefGraphic(toValue(defRef) ?? ("" as DefRef)),
    enabled: () => toValue(defRef) !== null,
    staleTime: Number.POSITIVE_INFINITY,
  });
}

/**
 * One texture of a def's resolved graphic. Disabled while either argument
 * is `null`. The key must come from that def's own {@link useDefGraphicQuery}
 * answer: the backend refuses any other.
 *
 * `staleTime: Infinity` (an image never changes within a loaded project;
 * `session://changed` still invalidates it) and `gcTime: 30_000`, the
 * reasoning of `useModPreviewQuery`: arrowing through hundreds of rows
 * must not keep hundreds of data URLs alive, so an image whose row left
 * view is dropped thirty seconds later.
 */
export function useDefTextureQuery(
  defRef: MaybeRefOrGetter<DefRef | null>,
  textureKey: MaybeRefOrGetter<string | null>,
) {
  return useQuery({
    key: () => queryKeys.defs.texture(toValue(defRef) ?? "", toValue(textureKey) ?? ""),
    query: () => readDefTexture(toValue(defRef) ?? ("" as DefRef), toValue(textureKey) ?? ""),
    enabled: () => toValue(defRef) !== null && toValue(textureKey) !== null,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: 30_000,
  });
}
