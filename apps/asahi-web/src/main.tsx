import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import "./style.css";
import { App } from "./App";
import { TooltipProvider } from "@/components/ui/tooltip";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      // Treat data as fresh for 5s so tab switches within that window don't
      // refetch at all. After that, refetch in the background using cached
      // data as the rendered source (no Suspense fall-through, no skeleton
      // flash). Live mutations still call refreshAsahiQueries() to force a
      // fresh fetch synchronously.
      staleTime: 5_000,
      refetchOnMount: true,
      refetchOnReconnect: "always",
      refetchOnWindowFocus: "always",
      throwOnError: true,
    },
  },
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <App />
      </TooltipProvider>
    </QueryClientProvider>
  </StrictMode>,
);
