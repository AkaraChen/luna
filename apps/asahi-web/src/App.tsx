import { Suspense, useMemo, useState } from "react";
import { useSuspenseQuery } from "@tanstack/react-query";
import { IconChevronLeft, IconPlus, IconSearch } from "@tabler/icons-react";
import { useLocation } from "wouter";

import { fetchIssues } from "@/api/asahi";
import { AsahiSidebar, type View } from "@/components/dashboard/asahi-sidebar";
import { CreateIssueTrigger } from "@/components/dashboard/create-issue-trigger";
import { statusFilters, type StatusFilter } from "@/components/dashboard/constants";
import { DashboardSkeleton } from "@/components/dashboard/dashboard-skeleton";
import { IssueDetails, DetailsSkeleton } from "@/components/dashboard/issue-details";
import { IssueList } from "@/components/dashboard/issue-list";
import { NotificationsView } from "@/components/dashboard/notifications-view";
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
  const [location, navigate] = useLocation();
  const view: View = location.startsWith("/inbox") ? "notifications" : "issues";
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [search, setSearch] = useState("");
  const selectedId = issueIdFromLocation(location);

  return (
    <SidebarProvider>
      <AsahiSidebar
        view={view}
        onViewChange={(nextView) => {
          navigate(nextView === "notifications" ? "/inbox" : "/issues");
        }}
      />

      <SidebarInset className="border border-border/70 bg-background">
        <header className="flex h-14 items-center justify-between border-b border-border bg-background/95 px-4">
          <div className="flex min-w-0 items-center gap-3">
            {view === "issues" && selectedId ? (
              <button
                className="inline-flex items-center gap-1.5 text-sm font-semibold text-muted-foreground hover:text-foreground"
                onClick={() => navigate("/issues")}
                type="button"
              >
                <IconChevronLeft className="size-4" />
                Issues
              </button>
            ) : (
              <span className="text-sm font-semibold">
                {view === "notifications" ? "Inbox" : "Issues"}
              </span>
            )}
          </div>

          <div className="flex items-center gap-2">
            {view === "issues" && !selectedId ? (
              <div className="relative">
                <IconSearch className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
                <Input
                  className="hidden h-8 w-[min(42vw,280px)] pl-8 sm:block"
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder="Search issues"
                  value={search}
                />
              </div>
            ) : null}
            {view === "issues" && !selectedId || view === "notifications" ? (
              <CreateIssueTrigger>
                <Button size="sm">
                  <IconPlus className="size-4" />
                  New issue
                </Button>
              </CreateIssueTrigger>
            ) : null}
          </div>
        </header>

        {view === "notifications" ? (
          <NotificationsView />
        ) : selectedId ? (
          <IssueDetailPage selectedId={selectedId} />
        ) : (
          <IssuesView
            onSelect={(id) => navigate(`/issues/${encodeURIComponent(id)}`)}
            search={search}
            statusFilter={statusFilter}
            setStatusFilter={setStatusFilter}
          />
        )}
      </SidebarInset>

    </SidebarProvider>
  );
}

function issueIdFromLocation(location: string) {
  const match = /^\/issues\/([^/?#]+)/.exec(location);
  return match ? decodeURIComponent(match[1]) : null;
}

function IssuesView({
  onSelect,
  search,
  setStatusFilter,
  statusFilter,
}: {
  onSelect: (id: string) => void;
  search: string;
  setStatusFilter: (status: StatusFilter) => void;
  statusFilter: StatusFilter;
}) {
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

  return (
    <section className="flex-1 overflow-auto">
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

      <IssueList issues={visibleIssues} onSelect={onSelect} selectedId={null} />
    </section>
  );
}

function IssueDetailPage({ selectedId }: { selectedId: string }) {
  const { data } = useSuspenseQuery({
    queryKey: ["issues", "all"],
    queryFn: () => fetchIssues(),
  });

  const issue = data.issues.find((i) => i.id === selectedId || i.identifier === selectedId);

  if (!issue) {
    return (
      <div className="flex min-h-[calc(100svh-3.5rem)] items-center justify-center">
        <p className="text-sm text-muted-foreground">Issue not found.</p>
      </div>
    );
  }

  return (
    <div className="min-h-0 flex-1 overflow-hidden">
      <Suspense fallback={<DetailsSkeleton />}>
        <IssueDetails issue={issue} />
      </Suspense>
    </div>
  );
}
