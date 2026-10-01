import { useQuery } from "@pinia/colada";

import { queryKeys } from "@/queries/keys";
import { getDefCacheCarrier } from "@/services/ipc";

/**
 * Whether an active mod carries a known def-cache plugin — the apply
 * dialog's own note.
 */
export function useDefCacheCarrierQuery() {
  return useQuery({
    key: () => queryKeys.defCacheCarrier(),
    query: () => getDefCacheCarrier(),
  });
}
