import { useQuery } from "@pinia/colada";
import { type MaybeRefOrGetter, toValue } from "vue";

import { queryKeys } from "@/queries/keys";
import { explainPlacement, listOrder } from "@/services/ipc";
import type { OrderSourceDto } from "@/types/generated/OrderSourceDto";

/** Lists every mod in `source`'s order, one row per mod. */
export function useOrderQuery(source: MaybeRefOrGetter<OrderSourceDto>) {
  return useQuery({
    key: () => queryKeys.order(toValue(source)),
    query: () => listOrder(toValue(source)),
  });
}

/**
 * The full why-panel explanation for one mod's placement in the suggested
 * order. Disabled while `modId` is `null` (no row selected / no route
 * param yet).
 */
export function usePlacementQuery(modId: MaybeRefOrGetter<string | null>) {
  return useQuery({
    key: () => queryKeys.placement(toValue(modId) ?? ""),
    query: () => explainPlacement(toValue(modId) ?? ""),
    enabled: () => toValue(modId) !== null,
  });
}
