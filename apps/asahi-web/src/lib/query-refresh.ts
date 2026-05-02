import type { QueryClient, QueryKey } from "@tanstack/react-query";

export const ASAHI_LIVE_REFETCH_INTERVAL_MS = 2_000;
export const NOTIFICATIONS_REFETCH_INTERVAL_MS = ASAHI_LIVE_REFETCH_INTERVAL_MS;

const ASAHI_QUERY_ROOTS: QueryKey[] = [
  ["activities"],
  ["comments"],
  ["issues"],
  ["notifications"],
  ["projects"],
  ["wiki"],
];

export function refreshAsahiQueries(queryClient: QueryClient) {
  return Promise.all(
    ASAHI_QUERY_ROOTS.map((queryKey) =>
      queryClient.invalidateQueries({ queryKey, refetchType: "all" }),
    ),
  ).then(() => undefined);
}

export function refreshNotifications(queryClient: QueryClient) {
  return queryClient.invalidateQueries({ queryKey: ["notifications"], refetchType: "all" });
}
