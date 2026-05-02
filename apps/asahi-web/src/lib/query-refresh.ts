import type { QueryClient, QueryKey } from "@tanstack/react-query";

export const NOTIFICATIONS_REFETCH_INTERVAL_MS = 2_000;

const ASAHI_QUERY_ROOTS: QueryKey[] = [
  ["activities"],
  ["comments"],
  ["issues"],
  ["notifications"],
  ["projects"],
  ["wiki"],
];

export function refreshAsahiQueries(queryClient: QueryClient) {
  for (const queryKey of ASAHI_QUERY_ROOTS) {
    void queryClient.invalidateQueries({ queryKey, refetchType: "all" });
  }
}

export function refreshNotifications(queryClient: QueryClient) {
  void queryClient.invalidateQueries({ queryKey: ["notifications"], refetchType: "all" });
}
