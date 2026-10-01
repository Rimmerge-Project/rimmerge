import { useQuery } from "@pinia/colada";

import { queryKeys } from "@/queries/keys";
import { getDashboard } from "@/services/ipc";

/** The dashboard's summary counts for the currently selected order. */
export function useDashboardQuery() {
  return useQuery({
    key: queryKeys.dashboard,
    query: getDashboard,
  });
}
