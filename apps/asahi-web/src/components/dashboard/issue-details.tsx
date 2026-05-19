import { Suspense, useEffect, useState, type ReactNode } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { ChevronDown, Edit3, Link2, Trash2, X } from "lucide-react";
import { useLocation } from "wouter";

import {
  createComment,
  deleteIssue,
  fetchActivities,
  fetchComments,
  fetchIssues,
  updateIssue,
  updateIssueState,
  type Activity,
  type Comment,
  type Issue,
} from "@/api/asahi";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { ActivitySkeleton } from "@/components/dashboard/dashboard-skeleton";
import { IssueCommentForm } from "@/components/dashboard/issue-comment-form";
import {
  ASAHI_LIVE_REFETCH_INTERVAL_MS,
  refreshAsahiQueries,
} from "@/lib/query-refresh";
import { sanitizeRichText } from "@/lib/sanitize";
import { cn } from "@/lib/utils";

import { statusColumns } from "./constants";
import { EditablePriority, EditableStatus } from "./editable-fields";

const priorityOptions = [null, 1, 2, 3, 4] as const;

export function IssueDetails({ issue }: { issue: Issue }) {
  const queryClient = useQueryClient();
  const [, navigate] = useLocation();
  const [comment, setComment] = useState("");
  const [statusOpen, setStatusOpen] = useState(false);
  const [priorityOpen, setPriorityOpen] = useState(false);
  const [blockersOpen, setBlockersOpen] = useState(false);
  const [editingDescription, setEditingDescription] = useState(false);
  const [descriptionDraft, setDescriptionDraft] = useState(issue.description ?? "");
  const [deleteOpen, setDeleteOpen] = useState(false);

  const allIssuesQuery = useSuspenseQuery({
    queryKey: ["issues", "all"],
    queryFn: () => fetchIssues(),
  });

  const moveMutation = useMutation({
    mutationFn: (state: string) => updateIssueState(issue.id, state),
    onSettled: () => refreshAsahiQueries(queryClient),
  });

  const updateMutation = useMutation({
    mutationFn: (input: {
      title?: string;
      description?: string | null;
      priority?: number | null;
      blocked_by?: string[];
    }) => updateIssue(issue.id, input),
    onSettled: () => refreshAsahiQueries(queryClient),
  });

  const deleteMutation = useMutation({
    mutationFn: () => deleteIssue(issue.id),
    onSuccess: () => {
      navigate("/issues");
      queryClient.removeQueries({ queryKey: ["comments", issue.id] });
      queryClient.removeQueries({ queryKey: ["activities", issue.id] });
    },
    onSettled: () => refreshAsahiQueries(queryClient),
  });

  const commentMutation = useMutation({
    mutationFn: (body: string) => createComment(issue.id, body),
    onSuccess: () => setComment(""),
    onSettled: () => refreshAsahiQueries(queryClient),
  });

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "d" && (event.metaKey || event.ctrlKey)) {
        const target = event.target as HTMLElement;
        if (
          target.tagName === "TEXTAREA" ||
          target.tagName === "INPUT" ||
          target.isContentEditable
        ) {
          return;
        }
        event.preventDefault();
        setDeleteOpen(true);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const blockerIds = issue.blocked_by.reduce<string[]>((acc, blocker) => {
    if (blocker.id) acc.push(blocker.id);
    return acc;
  }, []);
  const availableBlockers = allIssuesQuery.data.issues.filter(
    (candidate) => candidate.id !== issue.id,
  );

  return (
    <section className="grid h-full min-h-0 flex-1 lg:grid-cols-[minmax(0,1fr)_18rem]">
      {/* Middle column: content scrolls in its own region; composer always pinned to the column's bottom edge */}
      <div className="flex min-h-0 min-w-0 flex-col">
        <div className="min-h-0 flex-1 overflow-auto px-5 pt-6 pb-6">
          <header className="asahi-rise flex items-start justify-between gap-4">
            <div className="flex min-w-0 flex-col gap-1.5">
              <div className="flex items-center gap-2 text-[11.5px] text-muted-foreground">
                <span className="font-mono uppercase tracking-wide">{issue.identifier}</span>
                <span aria-hidden>·</span>
                <span>{formatDate(issue.updated_at)}</span>
              </div>
              <h1 className="text-[15px] font-medium leading-snug text-foreground">
                {issue.title}
              </h1>
            </div>
            <AlertDialog onOpenChange={setDeleteOpen} open={deleteOpen}>
              <Button
                aria-label="Delete issue"
                className="asahi-press text-muted-foreground hover:bg-destructive/10 hover:text-destructive focus-visible:ring-destructive/30"
                disabled={deleteMutation.isPending}
                onClick={() => setDeleteOpen(true)}
                size="icon-xs"
                type="button"
                variant="ghost"
              >
                <Trash2 className="size-3.5" />
              </Button>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Delete {issue.identifier}?</AlertDialogTitle>
                  <AlertDialogDescription>
                    This action cannot be undone. The issue will be permanently removed.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel onClick={() => setDeleteOpen(false)}>
                    Cancel
                  </AlertDialogCancel>
                  <AlertDialogAction
                    disabled={deleteMutation.isPending}
                    onClick={() => deleteMutation.mutate()}
                    variant="destructive"
                  >
                    Delete
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          </header>

          <div
            className="asahi-rise group/description relative mt-4 max-w-2xl"
            style={{ animationDelay: "40ms" }}
          >
            {editingDescription ? (
              <div>
                <Textarea
                  autoFocus
                  className="min-h-24 resize-none rounded-md border-border/70 bg-muted/40 px-3 py-2 text-[13.5px] leading-relaxed"
                  onChange={(event) => setDescriptionDraft(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape") {
                      setEditingDescription(false);
                      setDescriptionDraft(issue.description ?? "");
                    }
                  }}
                  placeholder="Add a description"
                  value={descriptionDraft}
                />
                <div className="mt-2 flex items-center gap-2">
                  <Button
                    disabled={updateMutation.isPending}
                    onClick={() => {
                      updateMutation.mutate(
                        { description: descriptionDraft || null },
                        { onSuccess: () => setEditingDescription(false) },
                      );
                    }}
                    size="sm"
                    type="button"
                  >
                    Save
                  </Button>
                  <Button
                    onClick={() => {
                      setEditingDescription(false);
                      setDescriptionDraft(issue.description ?? "");
                    }}
                    size="sm"
                    type="button"
                    variant="ghost"
                  >
                    Cancel
                  </Button>
                </div>
              </div>
            ) : (
              <>
                {issue.description ? (
                  <div
                    className="prose prose-sm max-w-2xl text-[13.5px] leading-relaxed text-muted-foreground"
                    dangerouslySetInnerHTML={{ __html: sanitizeRichText(issue.description) }}
                  />
                ) : (
                  <p className="text-[13.5px] italic text-muted-foreground">No description</p>
                )}
                <Button
                  aria-label="Edit description"
                  className="absolute -right-1 -top-1 opacity-0 transition-opacity group-hover/description:opacity-100"
                  onClick={() => setEditingDescription(true)}
                  size="icon-xs"
                  type="button"
                  variant="ghost"
                >
                  <Edit3 className="size-3.5 text-muted-foreground" />
                </Button>
              </>
            )}
          </div>

          <div className="mt-10 flex items-baseline justify-between">
            <h2 className="asahi-eyebrow">Activity</h2>
          </div>

          <Suspense fallback={<ActivitySkeleton />}>
            <IssueActivity issueId={issue.id} />
          </Suspense>
        </div>

        <IssueCommentForm
          isSubmitting={commentMutation.isPending}
          onChange={setComment}
          onSubmit={(body) => commentMutation.mutate(body)}
          value={comment}
        />
      </div>

      {/* Right metadata rail: sticky */}
      <aside className="sticky top-14 hidden h-[calc(100svh-3.5rem)] w-full shrink-0 self-start overflow-y-auto border-l border-border/60 px-5 py-6 lg:block">
        <dl className="flex flex-col gap-4 text-[13px]">
          <MetaRow label="Status">
            <EditableStatus
              disabled={moveMutation.isPending}
              onChange={(state) => {
                moveMutation.mutate(state);
                setStatusOpen(false);
              }}
              open={statusOpen}
              options={statusColumns}
              setOpen={setStatusOpen}
              state={issue.state}
            />
          </MetaRow>
          <MetaRow label="Priority">
            <EditablePriority
              disabled={updateMutation.isPending}
              onChange={(priority) => {
                updateMutation.mutate({ priority });
                setPriorityOpen(false);
              }}
              open={priorityOpen}
              options={[...priorityOptions]}
              priority={issue.priority}
              setOpen={setPriorityOpen}
            />
          </MetaRow>
          <MetaRow label="Blocked by">
            <EditableBlockers
              blockers={issue.blocked_by}
              disabled={updateMutation.isPending}
              issueOptions={availableBlockers}
              onClear={() => updateMutation.mutate({ blocked_by: [] })}
              onToggle={(issueId) => {
                const next = blockerIds.includes(issueId)
                  ? blockerIds.filter((id) => id !== issueId)
                  : [...blockerIds, issueId];
                updateMutation.mutate({ blocked_by: next });
              }}
              open={blockersOpen}
              selectedIds={blockerIds}
              setOpen={setBlockersOpen}
            />
          </MetaRow>
        </dl>
      </aside>
    </section>
  );
}

type TimelineItem = { type: "activity"; data: Activity } | { type: "comment"; data: Comment };

function Timeline({ activities, comments }: { activities: Activity[]; comments: Comment[] }) {
  const items: TimelineItem[] = activities.reduce<TimelineItem[]>((acc, a) => {
    if (a.kind !== "comment_created") acc.push({ type: "activity", data: a });
    return acc;
  }, []);
  for (const c of comments) items.push({ type: "comment", data: c });

  items.sort(
    (a, b) => new Date(a.data.created_at).getTime() - new Date(b.data.created_at).getTime(),
  );

  if (items.length === 0) {
    return <div className="mt-3 text-[13px] text-muted-foreground">No activity yet.</div>;
  }

  return (
    <ol className="mt-3 flex flex-col">
      {items.map((item, i) =>
        item.type === "comment" ? (
          <li
            className="asahi-rise flex flex-col gap-1.5 py-3"
            key={`comment-${item.data.id}`}
            style={{ animationDelay: `${Math.min(i * 40, 200)}ms` }}
          >
            <div className="flex items-center gap-2 text-[12px]">
              <Avatar initials="You" />
              <span className="font-medium text-foreground">You</span>
              <span className="text-muted-foreground">·</span>
              <time className="text-muted-foreground">{formatDate(item.data.created_at)}</time>
            </div>
            <div
              className="prose prose-sm max-w-2xl pl-7 text-[13.5px] leading-relaxed text-foreground"
              dangerouslySetInnerHTML={{ __html: sanitizeRichText(item.data.body) }}
            />
          </li>
        ) : (
          <li
            className="asahi-fade relative my-3 flex items-center justify-center"
            key={`activity-${item.data.id}`}
            style={{ animationDelay: `${Math.min(i * 40, 200)}ms` }}
          >
            <span aria-hidden className="absolute inset-x-0 top-1/2 h-px bg-border/60" />
            <span className="relative bg-background px-3 text-[11.5px] text-muted-foreground">
              {item.data.title}
              <span aria-hidden className="mx-1.5 text-border">
                ·
              </span>
              <time>{formatDate(item.data.created_at)}</time>
            </span>
          </li>
        ),
      )}
    </ol>
  );
}

function Avatar({ initials }: { initials: string }) {
  return (
    <span
      aria-hidden
      className="inline-flex size-5 items-center justify-center rounded-full bg-muted text-[9.5px] font-medium text-foreground"
    >
      {initials.slice(0, 2)}
    </span>
  );
}

function EditableBlockers({
  blockers,
  disabled,
  issueOptions,
  onClear,
  onToggle,
  open,
  selectedIds,
  setOpen,
}: {
  blockers: Issue["blocked_by"];
  disabled: boolean;
  issueOptions: Issue[];
  onClear: () => void;
  onToggle: (issueId: string) => void;
  open: boolean;
  selectedIds: string[];
  setOpen: (open: boolean) => void;
}) {
  return (
    <div className="relative min-w-0">
      <button
        className="asahi-press inline-flex min-h-7 max-w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left [transition:background-color_180ms_var(--ease-out-strong)] hover:bg-muted/60 disabled:opacity-50"
        disabled={disabled}
        onClick={() => setOpen(!open)}
        type="button"
      >
        <Link2 className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate text-[12.5px] text-foreground">
          {blockers.length
            ? blockers.map((blocker) => blocker.identifier ?? blocker.id).join(", ")
            : "None"}
        </span>
        <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
      </button>

      {open ? (
        <div className="absolute right-0 top-full z-20 mt-1 max-h-72 w-72 overflow-auto rounded-md border border-border/70 bg-popover py-1 shadow-[0_1px_2px_oklch(0_0_0_/_0.04)]">
          {issueOptions.length ? (
            issueOptions.map((candidate) => {
              const selected = selectedIds.includes(candidate.id);
              return (
                <button
                  className={cn(
                    "flex min-h-9 w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-muted/60",
                    selected && "bg-muted",
                  )}
                  disabled={disabled}
                  key={candidate.id}
                  onClick={() => onToggle(candidate.id)}
                  type="button"
                >
                  <span
                    className={cn(
                      "flex size-4 shrink-0 items-center justify-center rounded border border-border",
                      selected ? "bg-foreground" : "bg-background",
                    )}
                  >
                    {selected ? <span className="size-1.5 rounded-full bg-background" /> : null}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-mono text-[11.5px] uppercase tracking-wide text-foreground">
                      {candidate.identifier}
                    </span>
                    <span className="block truncate text-[11.5px] text-muted-foreground">
                      {candidate.title}
                    </span>
                  </span>
                </button>
              );
            })
          ) : (
            <div className="px-3 py-2 text-[11.5px] text-muted-foreground">No other issues</div>
          )}

          {blockers.length ? (
            <button
              className="flex h-8 w-full items-center gap-2 border-t border-border/60 px-3 text-left text-[11.5px] text-foreground hover:bg-muted/60"
              disabled={disabled}
              onClick={onClear}
              type="button"
            >
              <X className="size-3.5" />
              Clear blockers
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function MetaRow({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="grid grid-cols-[5.5rem_minmax(0,1fr)] items-center gap-3">
      <dt className="text-[12px] text-muted-foreground">{label}</dt>
      <dd className="flex min-w-0 justify-end text-right text-foreground">{children}</dd>
    </div>
  );
}

function IssueActivity({ issueId }: { issueId: string }) {
  const { data: commentsData } = useSuspenseQuery({
    queryKey: ["comments", issueId],
    queryFn: () => fetchComments(issueId),
    refetchInterval: ASAHI_LIVE_REFETCH_INTERVAL_MS,
  });
  const { data: activitiesData } = useSuspenseQuery({
    queryKey: ["activities", issueId],
    queryFn: () => fetchActivities(issueId),
    refetchInterval: ASAHI_LIVE_REFETCH_INTERVAL_MS,
  });

  return <Timeline activities={activitiesData.activities} comments={commentsData.comments} />;
}

const DATETIME_SHORT = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
  hour: "2-digit",
  minute: "2-digit",
});

function formatDate(value: string | null) {
  if (!value) return "—";
  return DATETIME_SHORT.format(new Date(value));
}
