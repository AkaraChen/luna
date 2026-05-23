import { Suspense, useMemo, useState } from "react";
import { keepPreviousData, useQuery, useSuspenseQuery } from "@tanstack/react-query";
import { ChevronLeft, Folder, Plus, Search } from "lucide-react";
import { useLocation } from "wouter";

import { fetchIssues } from "@/api/asahi";
import { AppShell } from "@/components/dashboard/asahi-sidebar";
import { CreateIssueTrigger } from "@/components/dashboard/create-issue-trigger";
import { statusFilters, type StatusFilter } from "@/components/dashboard/constants";
import {
  IssueDetailSkeleton,
  IssueListSkeleton,
  NotificationsViewSkeleton,
  ProjectDetailsSkeleton,
  SidebarSkeleton,
} from "@/components/dashboard/dashboard-skeleton";
import { IssueDetails } from "@/components/dashboard/issue-details";
import { IssueList } from "@/components/dashboard/issue-list";
import { NotificationsView } from "@/components/dashboard/notifications-view";
import { DashboardPageLayout } from "@/components/dashboard/page-layout";
import { ProjectDetails } from "@/components/dashboard/project-details";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

export function App() {
  return <Dashboard />;
}

function Dashboard() {
  const [location, navigate] = useLocation();
  const projectRoute = location.startsWith("/projects");
  const view = projectRoute
    ? ("project" as const)
    : location.startsWith("/inbox")
      ? ("notifications" as const)
      : ("issues" as const);

  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [search, setSearch] = useState("");
  const selectedId = issueIdFromLocation(location);
  const selectedProjectLocator = projectLocatorFromLocation(location);
  const showIssueSearch = view === "issues" && !selectedId;
  const showCreateIssue = (view === "issues" && !selectedId) || view === "notifications";

  const headerRight =
    showIssueSearch || showCreateIssue ? (
      <>
        {showIssueSearch ? (
          <div className="relative hidden sm:block">
            <Search className="pointer-events-none absolute left-3 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
            <Input
              className="h-8 w-[min(42vw,260px)] rounded-md border-border/80 bg-background pl-8 text-[13px]"
              onChange={(event) => setSearch(event.target.value)}
              placeholder="Search"
              value={search}
            />
          </div>
        ) : null}
        {showCreateIssue ? (
          <CreateIssueTrigger>
            <Button
              className="asahi-press h-8 rounded-md border-border/80 bg-background px-3 text-[13px] font-normal text-foreground"
              size="sm"
              variant="outline"
            >
              <Plus className="size-3.5" data-icon="inline-start" />
              New issue
            </Button>
          </CreateIssueTrigger>
        ) : null}
      </>
    ) : null;

  return (
    <Suspense fallback={<SidebarSkeleton />}>
      <AppShell>
        <DashboardPageLayout
          right={headerRight}
          title={
            view === "issues" && selectedId ? (
              <button
                className="inline-flex items-center gap-1 text-[13px] text-muted-foreground hover:text-foreground"
                onClick={() => navigate("/issues")}
                type="button"
              >
                <ChevronLeft className="size-3.5" />
                Issues
              </button>
            ) : (
              <span className="text-[13.5px] font-medium text-foreground">
                {view === "notifications" ? "Inbox" : view === "project" ? "Project" : "Issues"}
              </span>
            )
          }
        >
          {view === "notifications" ? (
            <Suspense fallback={<NotificationsViewSkeleton />}>
              <NotificationsView
                onSelectIssue={(id) => navigate(`/issues/${encodeURIComponent(id)}`)}
              />
            </Suspense>
          ) : view === "project" ? (
            selectedProjectLocator ? (
              <Suspense fallback={<ProjectDetailsSkeleton />}>
                <ProjectDetails
                  locator={selectedProjectLocator}
                  onSelectIssue={(id) => navigate(`/issues/${encodeURIComponent(id)}`)}
                />
              </Suspense>
            ) : (
              <NoProjectSelected />
            )
          ) : selectedId ? (
            <Suspense fallback={<IssueDetailSkeleton />}>
              <IssueDetailPage selectedId={selectedId} />
            </Suspense>
          ) : (
            <IssuesView
              onSelect={(id) => navigate(`/issues/${encodeURIComponent(id)}`)}
              search={search}
              setStatusFilter={setStatusFilter}
              statusFilter={statusFilter}
            />
          )}
        </DashboardPageLayout>
      </AppShell>
    </Suspense>
  );
}

function issueIdFromLocation(location: string) {
  const match = /^\/issues\/([^/?#]+)/.exec(location);
  return match ? decodeURIComponent(match[1]) : null;
}

function projectLocatorFromLocation(location: string) {
  const match = /^\/projects\/([^/?#]+)/.exec(location);
  return match ? decodeURIComponent(match[1]) : null;
}

function NoProjectSelected() {
  return (
    <div className="flex min-h-0 flex-1 items-center justify-center px-6 text-center">
      <div>
        <div className="mx-auto mb-3 flex size-9 items-center justify-center rounded-full bg-muted">
          <Folder className="size-4 text-muted-foreground" strokeWidth={1.8} />
        </div>
        <div className="text-[13.5px] font-medium">Select a project</div>
      </div>
    </div>
  );
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

  // Non-suspending: we want the toolbar (status pills, counts) to remain
  // mounted while the filter changes; the list region shows its own skeleton
  // only on the very first load. Subsequent filter changes keep the previous
  // data rendered via keepPreviousData, so the skeleton doesn't flash.
  const { data, isPending } = useQuery({
    queryKey: ["issues", states?.join(",") ?? "all"],
    queryFn: () => fetchIssues({ states }),
    placeholderData: keepPreviousData,
  });

  const issues = data?.issues ?? [];

  const visibleIssues = useMemo(() => {
    const query = search.trim().toLowerCase();
    if (!query) return issues;
    return issues.filter((issue) => {
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
  }, [issues, search]);

  const totals = useMemo(() => {
    const open = issues.filter((i) => i.state !== "Done").length;
    return { open, shown: visibleIssues.length };
  }, [issues, visibleIssues]);

  return (
    <section className="min-h-0 flex-1 overflow-auto">
      <div className="mx-auto flex max-w-5xl flex-wrap items-baseline justify-between gap-x-6 gap-y-2 px-6 pt-3 pb-2">
        <div className="inline-flex items-center gap-1 rounded-full border border-border/70 bg-muted/40 p-1">
          {statusFilters.map((status) => (
            <button
              className={cn(
                "asahi-press h-7 rounded-full px-3 text-[12.5px] [transition:color_180ms_var(--ease-out-strong),background-color_180ms_var(--ease-out-strong),transform_140ms_var(--ease-out-strong)] hover:text-foreground",
                statusFilter === status
                  ? "asahi-pill-lift bg-background text-foreground"
                  : "text-muted-foreground",
              )}
              key={status}
              onClick={() => setStatusFilter(status)}
              type="button"
            >
              {status === "all" ? "All" : status}
            </button>
          ))}
        </div>
        {data ? (
          <p className="text-[12px] text-muted-foreground">
            <span className="text-foreground tabular-nums">{totals.open}</span> open
            <span className="mx-2 text-border">·</span>
            <span className="tabular-nums">{totals.shown}</span> shown
          </p>
        ) : null}
      </div>

      <div className="mx-auto max-w-5xl px-6 pb-12">
        {isPending && !data ? (
          <IssueListSkeleton />
        ) : (
          <IssueList issues={visibleIssues} onSelect={onSelect} selectedId={null} />
        )}
      </div>
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
      <div className="flex min-h-0 flex-1 items-center justify-center">
        <p className="text-[13px] text-muted-foreground">Issue not found.</p>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 overflow-hidden">
      <IssueDetails issue={issue} />
    </div>
  );
}
