import { Suspense, useMemo, useState } from "react";
import { useSuspenseQuery } from "@tanstack/react-query";
import { IconBell, IconPlus, IconSearch } from "@tabler/icons-react";

import { fetchIssues } from "@/api/asahi";
import { AsahiSidebar, type View } from "@/components/dashboard/asahi-sidebar";
import { statusFilters, type StatusFilter } from "@/components/dashboard/constants";
import { DashboardSkeleton } from "@/components/dashboard/dashboard-skeleton";
import { IssueDetails, DetailsSkeleton } from "@/components/dashboard/issue-details";
import { EmptyDetails, IssueList } from "@/components/dashboard/issue-list";
import { IssueComposer } from "@/components/dashboard/issue-composer";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { SidebarInset, SidebarProvider } from "@/components/ui/sidebar";
import { cn } from "@/lib/utils";

export function App() {
  return (
    <Suspense fallback={<DashboardSkeleton />}>
      <Dashboard />
    </Suspense>
  );
}

function Dashboard() {
  const [view, setView] = useState<View>("issues");
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

return (
    <SidebarProvider>
      <AsahiSidebar view={view} onViewChange={setView} />

      <SidebarInset className="min-h-svh overflow-hidden border border-border/70 bg-background">
        <header className="flex h-14 items-center justify-between border-b border-border bg-background/95 px-4">
          <div className="flex min-w-0 items-center gap-3">
            <span className="text-sm font-semibold">
              {view === "notifications" ? "Inbox" : "Issues"}
            </span>
          </div>

          {view === "issues" && (
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
<Button onClick={() => setComposerOpen(true)} size="sm">
                <IconPlus className="size-4" />
                New issue
              </Button>
            </div>
          )}
        </header>

        {view === "notifications" ? (
          <NotificationsView />
        ) : (
          <section className="grid min-h-[calc(100svh-3.5rem)] xl:grid-cols-[minmax(0,1fr)_360px]">
            <div className="min-w-0 border-r border-border">
              <div className="flex h-12 items-center justify-between px-4">
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
        )}
      </SidebarInset>

      {isComposerOpen ? <IssueComposer onClose={() => setComposerOpen(false)} /> : null}
    </SidebarProvider>
  );
}

function NotificationsView() {
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-32 text-center">
      <div className="flex size-10 items-center justify-center rounded-full bg-muted">
        <IconBell className="size-4.5 text-muted-foreground" stroke={1.8} />
      </div>
      <div>
        <p className="text-sm font-medium">No new notifications</p>
        <p className="mt-1 text-xs text-muted-foreground max-w-xs">
          You'll be notified about issue updates, mentions, and activity here.
        </p>
      </div>
    </div>
  );
}
