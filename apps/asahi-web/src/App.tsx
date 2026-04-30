import { Suspense, useMemo, useState } from "react";
import { useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import {
  IconBell,
  IconFilter,
  IconPlus,
  IconRefresh,
  IconSearch,
  IconSettings,
} from "@tabler/icons-react";

import { fetchIssues } from "@/api/asahi";
import { AsahiSidebar } from "@/components/dashboard/asahi-sidebar";
import { statusColumns, statusFilters, type StatusFilter } from "@/components/dashboard/constants";
import { DashboardSkeleton } from "@/components/dashboard/dashboard-skeleton";
import { IssueDetails, DetailsSkeleton } from "@/components/dashboard/issue-details";
import { EmptyDetails, IssueList } from "@/components/dashboard/issue-list";
import { IssueComposer } from "@/components/dashboard/issue-composer";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { cn } from "@/lib/utils";

export function App() {
  return (
    <Suspense fallback={<DashboardSkeleton />}>
      <Dashboard />
    </Suspense>
  );
}

function Dashboard() {
  const queryClient = useQueryClient();
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [isComposerOpen, setComposerOpen] = useState(false);

  const states = statusFilter === "all" ? undefined : [statusFilter];
  const { data } = useSuspenseQuery({
    queryKey: ["issues", states?.join(",") ?? "all"],
    queryFn: () => fetchIssues({ states }),
  });

  const visibleIssues = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return data.issues;

    return data.issues.filter((issue) => {
      const haystack = [
        issue.identifier,
        issue.title,
        issue.state,
        issue.description ?? "",
        ...issue.labels,
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(query);
    });
  }, [data.issues, search]);

  const selectedIssue =
    visibleIssues.find((issue) => issue.id === selectedId) ?? visibleIssues[0] ?? null;

  const counts = useMemo(() => {
    const result = new Map<string, number>();
    for (const status of statusColumns) {
      result.set(status, 0);
    }
    for (const issue of data.issues) {
      result.set(issue.state, (result.get(issue.state) ?? 0) + 1);
    }
    return result;
  }, [data.issues]);

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ["issues"] });
  };

  return (
    <SidebarProvider>
      <AsahiSidebar
        counts={counts}
        onStatusFilterChange={setStatusFilter}
        statusFilter={statusFilter}
        totalIssues={data.issues.length}
      />

      <SidebarInset className="min-h-svh overflow-hidden border border-border/70 bg-background">
        <header className="flex h-14 items-center justify-between border-b border-border bg-background/95 px-4">
          <div className="flex min-w-0 items-center gap-3">
            <SidebarTrigger />
            <div className="min-w-0">
              <div className="text-sm font-semibold">Dashboard</div>
              <div className="text-xs text-muted-foreground">
                {visibleIssues.length} visible issues
              </div>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <div className="relative">
              <IconSearch className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                className="hidden h-8 w-[min(42vw,280px)] pl-8 sm:block"
                onChange={(event) => setSearch(event.target.value)}
                placeholder="Search issues"
                value={search}
              />
            </div>
            <Button aria-label="Refresh issues" onClick={refresh} size="icon-sm" variant="ghost">
              <IconRefresh className="size-4" />
            </Button>
            <Button aria-label="Notifications" size="icon-sm" variant="ghost">
              <IconBell className="size-4" />
            </Button>
            <Button aria-label="Settings" size="icon-sm" variant="ghost">
              <IconSettings className="size-4" />
            </Button>
            <Button onClick={() => setComposerOpen(true)} size="sm">
              <IconPlus className="size-4" />
              New issue
            </Button>
          </div>
        </header>

        <section className="grid min-h-[calc(100svh-4.5rem)] xl:grid-cols-[minmax(0,1fr)_360px]">
          <div className="min-w-0 border-r border-border">
            <div className="flex h-12 items-center justify-between border-b border-border px-4">
              <div className="flex items-center gap-1 rounded-full border border-border bg-muted/60 p-0.5">
                {statusFilters.map((status) => (
                  <button
                    className={cn(
                      "h-7 rounded-full px-3 text-xs font-medium text-muted-foreground",
                      statusFilter === status && "bg-background text-foreground shadow-sm",
                    )}
                    key={status}
                    onClick={() => setStatusFilter(status)}
                    type="button"
                  >
                    {status === "all" ? "All" : status}
                  </button>
                ))}
              </div>
              <Button size="sm" variant="outline">
                <IconFilter className="size-4" />
                Filter
              </Button>
            </div>

            <IssueList
              issues={visibleIssues}
              onSelect={setSelectedId}
              selectedId={selectedIssue?.id ?? null}
            />
          </div>

          <aside className="min-w-0 bg-card">
            {selectedIssue ? (
              <Suspense fallback={<DetailsSkeleton />}>
                <IssueDetails issue={selectedIssue} />
              </Suspense>
            ) : (
              <EmptyDetails />
            )}
          </aside>
        </section>
      </SidebarInset>

      {isComposerOpen ? <IssueComposer onClose={() => setComposerOpen(false)} /> : null}
    </SidebarProvider>
  );
}
