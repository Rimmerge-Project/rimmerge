import { useQuery } from "@pinia/colada";

import { queryKeys } from "@/queries/keys";
import { getStartupCosts } from "@/services/ipc";

/**
 * The `/startup` page's per-mod cost table.
 */
export function useStartupCostsQuery() {
  return useQuery({
    key: () => queryKeys.startupCosts(),
    query: () => getStartupCosts(),
  });
}
