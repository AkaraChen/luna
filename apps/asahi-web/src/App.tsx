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
  IconLayoutSidebarLeftCollapse,
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
    <div className="min-h-screen bg-[#f8faf9] text-[#1f1f1d]">
      <div className="grid min-h-screen lg:grid-cols-[248px_minmax(0,1fr)]">
        <aside className="hidden border-r border-[#dfe7e4] bg-[#f1f5f4] lg:block">
          <div className="flex h-14 items-center gap-2 border-b border-[#dfe7e4] px-4">
            <div className="flex size-7 items-center justify-center rounded-md bg-[#20201d] text-white">
              <IconSparkles className="size-4" stroke={1.8} />
            </div>
            <div className="min-w-0">
              <div className="truncate text-sm font-semibold">Asahi</div>
              <div className="truncate text-xs text-[#6f6d66]">Task workspace</div>
            </div>
          </div>

          <nav className="space-y-1 px-2 py-3">
            <SidebarItem active icon={IconCircleDot} label="Issues" count={data.issues.length} />
            <SidebarItem icon={IconClockHour4} label="Active" count={counts.get("In Progress")} />
            <SidebarItem icon={IconCircleDashed} label="Backlog" count={counts.get("Todo")} />
            <SidebarItem icon={IconCircleCheck} label="Completed" count={counts.get("Done")} />
          </nav>

          <div className="mx-4 mt-3 border-t border-[#dfe7e4] pt-4">
            <div className="mb-2 px-1 text-[11px] font-medium uppercase tracking-[0.08em] text-[#85827a]">
              Views
            </div>
            <div className="space-y-1">
              {statusColumns.map((status) => (
                <button
                  key={status}
                  className="flex h-8 w-full items-center justify-between rounded-md px-2 text-left text-sm text-[#57544d] hover:bg-[#e8efed]"
                  onClick={() => setStatusFilter(status as StatusFilter)}
                  type="button"
                >
                  <span>{status}</span>
                  <span className="text-xs text-[#8a877e]">{counts.get(status) ?? 0}</span>
                </button>
              ))}
            </div>
          </div>
        </aside>

        <main className="min-w-0">
          <header className="flex h-14 items-center justify-between border-b border-[#dfe7e4] bg-[#f8faf9]/95 px-4">
            <div className="flex min-w-0 items-center gap-3">
              <Button aria-label="Toggle sidebar" size="icon" variant="ghost">
                <IconLayoutSidebarLeftCollapse className="size-4" />
              </Button>
              <div className="min-w-0">
                <div className="text-sm font-semibold">Dashboard</div>
                <div className="text-xs text-[#77746c]">{visibleIssues.length} visible issues</div>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <div className="relative">
                <IconSearch className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-[#85827a]" />
                <Input
                  className="hidden h-8 w-[min(42vw,280px)] pl-8 sm:block"
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder="Search issues"
                  value={search}
                />
              </div>
              <Button aria-label="Refresh issues" onClick={refresh} size="icon" variant="ghost">
                <IconRefresh className="size-4" />
              </Button>
              <Button aria-label="Notifications" size="icon" variant="ghost">
                <IconBell className="size-4" />
              </Button>
              <Button aria-label="Settings" size="icon" variant="ghost">
                <IconSettings className="size-4" />
              </Button>
              <Button onClick={() => setComposerOpen(true)} size="sm">
                <IconPlus className="size-4" />
                New issue
              </Button>
            </div>
          </header>

          <section className="grid min-h-[calc(100vh-3.5rem)] xl:grid-cols-[minmax(0,1fr)_360px]">
            <div className="min-w-0 border-r border-[#dfe7e4]">
              <div className="flex h-12 items-center justify-between border-b border-[#eceae5] px-4">
                <div className="flex items-center gap-1 rounded-md border border-[#dedbd2] bg-white p-0.5">
                  {statusFilters.map((status) => (
                    <button
                      className={cn(
                        "h-7 rounded-[5px] px-3 text-xs font-medium text-[#69665f]",
                        statusFilter === status && "bg-[#20201d] text-white",
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

            <aside className="min-w-0 bg-white">
              {selectedIssue ? (
                <Suspense fallback={<DetailsSkeleton />}>
                  <IssueDetails issue={selectedIssue} />
                </Suspense>
              ) : (
                <EmptyDetails />
              )}
            </aside>
          </section>
        </main>
      </div>

      {isComposerOpen ? <IssueComposer onClose={() => setComposerOpen(false)} /> : null}
    </div>
  );
}

function SidebarItem({
  active,
  count,
  icon: Icon,
  label,
}: {
  active?: boolean;
  count?: number;
  icon: typeof IconCircleDot;
  label: string;
}) {
  return (
    <button
      className={cn(
        "flex h-8 w-full items-center gap-2 rounded-md px-2 text-sm text-[#55524b]",
        active ? "bg-white text-[#20201d] shadow-[0_1px_0_rgba(0,0,0,0.04)]" : "hover:bg-[#e8efed]",
      )}
      type="button"
    >
      <Icon className="size-4 text-[#7f7b72]" stroke={1.8} />
      <span className="min-w-0 flex-1 truncate text-left">{label}</span>
      {typeof count === "number" ? <span className="text-xs text-[#8a877e]">{count}</span> : null}
    </button>
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
    <div className="grid min-h-screen bg-[#f8faf9] lg:grid-cols-[248px_minmax(0,1fr)]">
      <div className="hidden border-r border-[#dfe7e4] bg-[#f1f5f4] lg:block" />
      <div>
        <div className="h-14 border-b border-[#dfe7e4]" />
        <div className="space-y-3 p-4">
          {Array.from({ length: 8 }).map((_, index) => (
            <div className="h-14 animate-pulse rounded-md bg-[#eceae5]" key={index} />
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
