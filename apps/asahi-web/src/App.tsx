import { Suspense, useMemo, useState, type FormEvent, type ReactNode } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import {
  IconArrowUp,
  IconBell,
  IconCircleCheck,
  IconCircleDashed,
  IconCircleDot,
  IconClockHour4,
  IconFilter,
  IconHash,
  IconMessageCircle,
  IconPlus,
  IconRefresh,
  IconSearch,
  IconSend,
  IconSettings,
  IconSparkles,
} from "@tabler/icons-react";

import {
  createComment,
  createIssue,
  fetchComments,
  fetchIssues,
  updateIssueState,
  type Issue,
} from "@/api/asahi";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarRail,
  SidebarSeparator,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

type StatusFilter = "all" | "Todo" | "In Progress" | "Done";

const statusFilters: StatusFilter[] = ["all", "Todo", "In Progress", "Done"];
const statusColumns = ["Todo", "In Progress", "Done"];

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
      <Sidebar collapsible="icon" variant="inset">
        <SidebarHeader>
          <SidebarMenu>
            <SidebarMenuItem>
              <SidebarMenuButton className="gap-2.5" size="lg" tooltip="Asahi">
                <div className="flex size-8 items-center justify-center rounded-xl bg-primary text-primary-foreground">
                  <IconSparkles className="size-4" stroke={1.8} />
                </div>
                <div className="grid min-w-0 flex-1 text-left leading-tight">
                  <span className="truncate text-sm font-semibold">Asahi</span>
                  <span className="truncate text-xs text-muted-foreground">Task workspace</span>
                </div>
              </SidebarMenuButton>
            </SidebarMenuItem>
          </SidebarMenu>
        </SidebarHeader>

        <SidebarContent>
          <SidebarGroup>
            <SidebarGroupContent>
              <SidebarMenu>
                <SidebarItem
                  active={statusFilter === "all"}
                  count={data.issues.length}
                  icon={IconCircleDot}
                  label="Issues"
                  onClick={() => setStatusFilter("all")}
                />
                <SidebarItem
                  active={statusFilter === "In Progress"}
                  count={counts.get("In Progress")}
                  icon={IconClockHour4}
                  label="Active"
                  onClick={() => setStatusFilter("In Progress")}
                />
                <SidebarItem
                  active={statusFilter === "Todo"}
                  count={counts.get("Todo")}
                  icon={IconCircleDashed}
                  label="Backlog"
                  onClick={() => setStatusFilter("Todo")}
                />
                <SidebarItem
                  active={statusFilter === "Done"}
                  count={counts.get("Done")}
                  icon={IconCircleCheck}
                  label="Completed"
                  onClick={() => setStatusFilter("Done")}
                />
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>

          <SidebarSeparator />

          <SidebarGroup>
            <SidebarGroupLabel>Views</SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {statusColumns.map((status) => (
                  <SidebarItem
                    active={statusFilter === status}
                    count={counts.get(status) ?? 0}
                    icon={
                      status === "Done"
                        ? IconCircleCheck
                        : status === "In Progress"
                          ? IconCircleDot
                          : IconCircleDashed
                    }
                    key={status}
                    label={status}
                    onClick={() => setStatusFilter(status as StatusFilter)}
                  />
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        </SidebarContent>
        <SidebarRail />
      </Sidebar>

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

function SidebarItem({
  active,
  count,
  icon: Icon,
  label,
  onClick,
}: {
  active?: boolean;
  count?: number;
  icon: typeof IconCircleDot;
  label: string;
  onClick: () => void;
}) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton isActive={active} onClick={onClick} tooltip={label}>
        <Icon className={cn(active && "text-primary")} stroke={1.8} />
        <span>{label}</span>
      </SidebarMenuButton>
      {typeof count === "number" ? <SidebarMenuBadge>{count}</SidebarMenuBadge> : null}
    </SidebarMenuItem>
  );
}

function IssueList({
  issues,
  onSelect,
  selectedId,
}: {
  issues: Issue[];
  onSelect: (id: string) => void;
  selectedId: string | null;
}) {
  if (issues.length === 0) {
    return (
      <div className="flex h-[420px] items-center justify-center px-6 text-center">
        <div>
          <IconCircleDashed className="mx-auto mb-3 size-8 text-[#b4b0a7]" stroke={1.5} />
          <div className="text-sm font-medium">No issues</div>
          <div className="mt-1 text-sm text-[#77746c]">Try a different status or search.</div>
        </div>
      </div>
    );
  }

  return (
    <div className="divide-y divide-[#eceae5]">
      {issues.map((issue) => (
        <button
          className={cn(
            "grid w-full grid-cols-[96px_minmax(0,1fr)_132px_92px] items-center gap-3 px-4 py-3 text-left hover:bg-[#f7f6f2]",
            selectedId === issue.id && "bg-[#f2f1ec]",
          )}
          key={issue.id}
          onClick={() => onSelect(issue.id)}
          type="button"
        >
          <div className="flex items-center gap-2 text-xs font-medium text-[#706d65]">
            <IconHash className="size-3.5" />
            {issue.identifier}
          </div>
          <div className="min-w-0">
            <div className="truncate text-sm font-medium text-[#262522]">{issue.title}</div>
            <div className="mt-1 flex min-h-5 items-center gap-1.5 overflow-hidden">
              {issue.labels.slice(0, 3).map((label) => (
                <Badge key={label} variant="secondary">
                  {label}
                </Badge>
              ))}
            </div>
          </div>
          <StatusBadge state={issue.state} />
          <Priority priority={issue.priority} />
        </button>
      ))}
    </div>
  );
}

function IssueDetails({ issue }: { issue: Issue }) {
  const queryClient = useQueryClient();
  const [comment, setComment] = useState("");

  const commentsQuery = useSuspenseQuery({
    queryKey: ["comments", issue.id],
    queryFn: () => fetchComments(issue.id),
  });

  const moveMutation = useMutation({
    mutationFn: (state: string) => updateIssueState(issue.id, state),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
    },
  });

  const commentMutation = useMutation({
    mutationFn: (body: string) => createComment(issue.id, body),
    onSuccess: () => {
      setComment("");
      void queryClient.invalidateQueries({ queryKey: ["comments", issue.id] });
    },
  });

  const submitComment = (event: FormEvent) => {
    event.preventDefault();
    const body = comment.trim();
    if (body) {
      commentMutation.mutate(body);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-[#eceae5] px-5 py-4">
        <div className="mb-3 flex items-center justify-between">
          <div className="flex items-center gap-2 text-xs font-medium text-[#77746c]">
            <IconHash className="size-3.5" />
            {issue.identifier}
          </div>
          <StatusBadge state={issue.state} />
        </div>
        <h2 className="text-lg font-semibold leading-snug text-[#22211f]">{issue.title}</h2>
        {issue.description ? (
          <p className="mt-2 text-sm leading-6 text-[#69665f]">{issue.description}</p>
        ) : null}
      </div>

      <div className="grid grid-cols-2 gap-x-4 gap-y-3 border-b border-[#eceae5] px-5 py-4 text-sm">
        <Detail label="Priority">
          <Priority priority={issue.priority} />
        </Detail>
        <Detail label="Updated">{formatDate(issue.updated_at)}</Detail>
        <Detail label="Labels">
          <div className="flex flex-wrap gap-1.5">
            {issue.labels.length ? (
              issue.labels.map((label) => (
                <Badge key={label} variant="secondary">
                  {label}
                </Badge>
              ))
            ) : (
              <span className="text-[#8a877e]">None</span>
            )}
          </div>
        </Detail>
        <Detail label="Blocked by">
          {issue.blocked_by.length ? (
            issue.blocked_by.map((blocker) => blocker.identifier ?? blocker.id).join(", ")
          ) : (
            <span className="text-[#8a877e]">None</span>
          )}
        </Detail>
      </div>

      <div className="border-b border-[#eceae5] px-5 py-3">
        <div className="flex gap-2">
          {statusColumns.map((state) => (
            <Button
              disabled={moveMutation.isPending || issue.state === state}
              key={state}
              onClick={() => moveMutation.mutate(state)}
              size="sm"
              variant={issue.state === state ? "default" : "outline"}
            >
              {state}
            </Button>
          ))}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
        <div className="mb-3 flex items-center gap-2 text-sm font-medium">
          <IconMessageCircle className="size-4 text-[#77746c]" />
          Activity
        </div>
        <div className="space-y-3">
          {commentsQuery.data.comments.map((item) => (
            <div className="rounded-md border border-[#eceae5] bg-[#fbfbfa] p-3" key={item.id}>
              <div className="mb-1 text-xs text-[#85827a]">{formatDate(item.created_at)}</div>
              <div className="text-sm leading-6 text-[#33312d]">{item.body}</div>
            </div>
          ))}
          {commentsQuery.data.comments.length === 0 ? (
            <div className="rounded-md border border-dashed border-[#dedbd2] p-4 text-sm text-[#77746c]">
              No activity yet.
            </div>
          ) : null}
        </div>
      </div>

      <form className="border-t border-[#eceae5] p-4" onSubmit={submitComment}>
        <Textarea
          className="min-h-20 resize-none"
          onChange={(event) => setComment(event.target.value)}
          placeholder="Add a comment"
          value={comment}
        />
        <div className="mt-2 flex justify-end">
          <Button disabled={commentMutation.isPending || !comment.trim()} size="sm" type="submit">
            <IconSend className="size-4" />
            Send
          </Button>
        </div>
      </form>
    </div>
  );
}

function Detail({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="min-w-0">
      <div className="mb-1 text-xs text-[#85827a]">{label}</div>
      <div className="min-h-5 text-[#33312d]">{children}</div>
    </div>
  );
}

function IssueComposer({ onClose }: { onClose: () => void }) {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [labels, setLabels] = useState("backend");
  const [priority, setPriority] = useState("3");

  const mutation = useMutation({
    mutationFn: () =>
      createIssue({
        project_slug: "engineering",
        team_key: "ENG",
        title,
        description: description || undefined,
        labels: labels
          .split(",")
          .map((label) => label.trim())
          .filter(Boolean),
        priority: priority ? Number(priority) : undefined,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      onClose();
    },
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (title.trim()) {
      mutation.mutate();
    }
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/20">
      <div className="absolute right-0 top-0 h-full w-[480px] border-l border-[#d8d5cc] bg-white shadow-2xl">
        <form className="flex h-full flex-col" onSubmit={submit}>
          <div className="flex h-14 items-center justify-between border-b border-[#eceae5] px-5">
            <div className="text-sm font-semibold">New issue</div>
            <Button onClick={onClose} type="button" variant="ghost">
              Close
            </Button>
          </div>

          <div className="space-y-4 p-5">
            <Input
              autoFocus
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Issue title"
              value={title}
            />
            <Textarea
              className="min-h-32 resize-none"
              onChange={(event) => setDescription(event.target.value)}
              placeholder="Description"
              value={description}
            />
            <div className="grid grid-cols-2 gap-3">
              <Input
                onChange={(event) => setLabels(event.target.value)}
                placeholder="Labels"
                value={labels}
              />
              <Input
                onChange={(event) => setPriority(event.target.value)}
                placeholder="Priority"
                type="number"
                value={priority}
              />
            </div>
          </div>

          <div className="mt-auto flex justify-end gap-2 border-t border-[#eceae5] p-4">
            <Button onClick={onClose} type="button" variant="outline">
              Cancel
            </Button>
            <Button disabled={mutation.isPending || !title.trim()} type="submit">
              <IconPlus className="size-4" />
              Create issue
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
}

function EmptyDetails() {
  return (
    <div className="flex h-full items-center justify-center p-8 text-center">
      <div>
        <IconCircleDashed className="mx-auto mb-3 size-8 text-[#b4b0a7]" stroke={1.5} />
        <div className="text-sm font-medium">No issue selected</div>
      </div>
    </div>
  );
}

function StatusBadge({ state }: { state: string }) {
  const Icon =
    state === "Done" ? IconCircleCheck : state === "In Progress" ? IconCircleDot : IconCircleDashed;

  return (
    <span className="inline-flex h-6 max-w-full items-center gap-1.5 rounded-md border border-[#dedbd2] bg-white px-2 text-xs font-medium text-[#55524b]">
      <Icon className="size-3.5 text-[#6f6d66]" />
      <span className="truncate">{state}</span>
    </span>
  );
}

function Priority({ priority }: { priority: number | null }) {
  if (priority == null) {
    return <span className="text-xs text-[#99958b]">No priority</span>;
  }

  return (
    <span className="inline-flex items-center gap-1 text-xs font-medium text-[#65625b]">
      <IconArrowUp className="size-3.5 text-[#7d7a72]" />P{priority}
    </span>
  );
}

function DashboardSkeleton() {
  return (
    <div className="grid min-h-screen bg-background lg:grid-cols-[248px_minmax(0,1fr)]">
      <div className="hidden border-r border-sidebar-border bg-sidebar lg:block" />
      <div>
        <div className="h-14 border-b border-border" />
        <div className="space-y-3 p-4">
          {Array.from({ length: 8 }).map((_, index) => (
            <div className="h-14 animate-pulse rounded-md bg-muted" key={index} />
          ))}
        </div>
      </div>
    </div>
  );
}

function DetailsSkeleton() {
  return (
    <div className="space-y-4 p-5">
      <div className="h-6 w-32 animate-pulse rounded bg-[#eceae5]" />
      <div className="h-8 w-full animate-pulse rounded bg-[#eceae5]" />
      <div className="h-24 w-full animate-pulse rounded bg-[#eceae5]" />
    </div>
  );
}

function formatDate(value: string | null) {
  if (!value) return "Unknown";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}
