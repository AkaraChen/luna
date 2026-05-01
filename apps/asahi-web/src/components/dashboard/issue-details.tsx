import { Suspense, useEffect, useState, type FormEvent, type ReactNode } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { IconChevronDown, IconEdit, IconLink, IconSend, IconTrash, IconX } from "@tabler/icons-react";
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
import { RichTextEditor } from "@/components/ui/rich-text-editor";
import { Textarea } from "@/components/ui/textarea";
import { ActivitySkeleton } from "@/components/dashboard/dashboard-skeleton";
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
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
      void queryClient.invalidateQueries({ queryKey: ["activities", issue.id] });
    },
  });

  const updateMutation = useMutation({
    mutationFn: (input: { title?: string; description?: string | null; priority?: number | null; blocked_by?: string[] }) =>
      updateIssue(issue.id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
      void queryClient.invalidateQueries({ queryKey: ["activities", issue.id] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => deleteIssue(issue.id),
    onSuccess: () => {
      navigate("/issues");
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
      queryClient.removeQueries({ queryKey: ["comments", issue.id] });
    },
  });

  const commentMutation = useMutation({
    mutationFn: (body: string) => createComment(issue.id, body),
    onSuccess: () => {
      setComment("");
      void queryClient.invalidateQueries({ queryKey: ["comments", issue.id] });
      void queryClient.invalidateQueries({ queryKey: ["activities", issue.id] });
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
  });

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "d" && (event.metaKey || event.ctrlKey)) {
        const target = event.target as HTMLElement;
        if (target.tagName === "TEXTAREA" || target.tagName === "INPUT" || target.isContentEditable) {
          return;
        }
        event.preventDefault();
        setDeleteOpen(true);
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  const blockerIds = issue.blocked_by
    .map((blocker) => blocker.id)
    .filter((id): id is string => Boolean(id));
  const availableBlockers = allIssuesQuery.data.issues.filter(
    (candidate) => candidate.id !== issue.id,
  );

  const submitComment = (event: FormEvent) => {
    event.preventDefault();
    const body = comment.replace(/<[^>]*>/g, "").trim();
    if (body) {
      commentMutation.mutate(body);
    }
  };

  return (
    <section className="grid min-h-0 flex-1 overflow-auto lg:grid-cols-[minmax(0,1fr)_18.5rem]">
      <div className="min-w-0 flex flex-col">
        <div className="px-5 pb-4 pt-5">
          <div className="mb-3 flex items-center justify-between gap-3">
            <div className="flex min-w-0 items-center gap-2">
              <span className="text-xs font-medium text-[#77746c]">{issue.identifier}</span>
              <span className="h-1 w-1 rounded-full bg-[#c9c4bb]" />
              <span className="text-xs text-[#8a877e]">{formatDate(issue.updated_at)}</span>
            </div>
            <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
              <Button
                aria-label="Delete issue"
                className="text-[#8a877e] hover:bg-destructive/10 hover:text-destructive focus-visible:border-destructive/40 focus-visible:ring-destructive/20"
                disabled={deleteMutation.isPending}
                onClick={() => setDeleteOpen(true)}
                size="icon-xs"
                type="button"
                variant="ghost"
              >
                <IconTrash className="size-3.5" />
              </Button>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Delete {issue.identifier}?</AlertDialogTitle>
                  <AlertDialogDescription>
                    This action cannot be undone. This will permanently delete the issue.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel onClick={() => setDeleteOpen(false)}>Cancel</AlertDialogCancel>
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
          </div>
          <h2 className="text-lg font-semibold leading-snug text-[#22211f]">{issue.title}</h2>
          {editingDescription ? (
            <div className="mt-3">
              <Textarea
                autoFocus
                className="min-h-24 resize-none text-sm"
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
                      {
                        onSuccess: () => {
                          setEditingDescription(false);
                        },
                      },
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
            <div className="group/description relative mt-3">
              {issue.description ? (
                <p className="text-sm leading-6 text-[#69665f]">{issue.description}</p>
              ) : (
                <p className="text-sm italic text-[#a8a59d]">No description</p>
              )}
              <Button
                aria-label="Edit description"
                className="absolute -right-1 -top-1 opacity-0 transition-opacity group-hover/description:opacity-100"
                onClick={() => setEditingDescription(true)}
                size="icon-xs"
                type="button"
                variant="ghost"
              >
                <IconEdit className="size-3.5 text-[#8a877e]" />
              </Button>
            </div>
          )}
        </div>

        <div className="min-h-0 flex-1 overflow-auto border-t border-[#eceae5]">
          <Suspense fallback={<ActivitySkeleton />}>
            <IssueActivity issueId={issue.id} />
          </Suspense>
        </div>

        <form className="p-4 pt-0" onSubmit={submitComment}>
          <RichTextEditor
            content={comment}
            onChange={(html) => setComment(html)}
          />
          <div className="mt-2 flex justify-end">
            <Button
              disabled={commentMutation.isPending || !comment.replace(/<[^>]*>/g, "").trim()}
              size="sm"
              type="submit"
            >
              <IconSend className="size-4" />
              Send
            </Button>
          </div>
        </form>
      </div>

      <aside className="border-t border-[#eceae5] bg-background px-5 py-3 lg:sticky lg:top-0 lg:min-h-full lg:border-l lg:border-t-0">
        <div className="grid gap-1">
          <PropertyRow label="Status">
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
          </PropertyRow>
          <PropertyRow label="Priority">
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
          </PropertyRow>
          <PropertyRow label="Blocked by">
            <EditableBlockers
              blockers={issue.blocked_by}
              disabled={updateMutation.isPending}
              issueOptions={availableBlockers}
              onToggle={(issueId) => {
                const next = blockerIds.includes(issueId)
                  ? blockerIds.filter((id) => id !== issueId)
                  : [...blockerIds, issueId];
                updateMutation.mutate({ blocked_by: next });
              }}
              onClear={() => updateMutation.mutate({ blocked_by: [] })}
              open={blockersOpen}
              selectedIds={blockerIds}
              setOpen={setBlockersOpen}
            />
          </PropertyRow>
        </div>
      </aside>
    </section>
  );
}

type TimelineItem =
  | { type: "activity"; data: Activity }
  | { type: "comment"; data: Comment };

function Timeline({
  activities,
  comments,
}: {
  activities: Activity[];
  comments: Comment[];
}) {
  const items: TimelineItem[] = [
    ...activities
      .filter((a) => a.kind !== "comment_created")
      .map((a): TimelineItem => ({ type: "activity", data: a })),
    ...comments.map((c): TimelineItem => ({ type: "comment", data: c })),
  ];

  items.sort(
    (a, b) =>
      new Date(a.data.created_at).getTime() -
      new Date(b.data.created_at).getTime(),
  );

  if (items.length === 0) {
    return <div className="text-sm text-[#77746c]">No activity yet.</div>;
  }

  return (
    <div className="space-y-3">
      {items.map((item) =>
        item.type === "comment" ? (
          <div className="rounded-md bg-[#f7f6f2] p-3" key={`comment-${item.data.id}`}>
            <div className="mb-1 text-xs text-[#85827a]">
              {formatDate(item.data.created_at)}
            </div>
            <div className="text-sm leading-6 text-[#33312d]">{item.data.body}</div>
          </div>
        ) : (
          <div className="flex items-center gap-2 py-1" key={`activity-${item.data.id}`}>
            <span className="size-1.5 shrink-0 rounded-full bg-[#c9c4bb]" />
            <div className="min-w-0 flex-1 text-sm text-[#55524b]">{item.data.title}</div>
            <div className="shrink-0 text-xs text-[#a8a59d]">
              {formatDate(item.data.created_at)}
            </div>
          </div>
        ),
      )}
    </div>
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
        className="inline-flex min-h-7 max-w-full items-center gap-1.5 rounded-md px-1.5 py-1 text-left hover:bg-[#f7f6f2] disabled:opacity-50"
        disabled={disabled}
        onClick={() => setOpen(!open)}
        type="button"
      >
        <IconLink className="size-3.5 shrink-0 text-[#7d7a72]" />
        <span className="truncate text-xs text-[#55524b]">
          {blockers.length
            ? blockers.map((blocker) => blocker.identifier ?? blocker.id).join(", ")
            : "None"}
        </span>
        <IconChevronDown className="size-3.5 shrink-0 text-[#8a877e]" />
      </button>

      {open ? (
        <div className="absolute right-0 top-full z-20 mt-1 max-h-72 w-72 overflow-auto rounded-md border border-[#eceae5] bg-white py-1 shadow-md">
          {issueOptions.length ? (
            issueOptions.map((candidate) => {
              const selected = selectedIds.includes(candidate.id);
              return (
                <button
                  className={cn(
                    "flex min-h-9 w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-[#f7f6f2]",
                    selected && "bg-[#f2f1ec]",
                  )}
                  disabled={disabled}
                  key={candidate.id}
                  onClick={() => onToggle(candidate.id)}
                  type="button"
                >
                  <span
                    className={cn(
                      "flex size-4 shrink-0 items-center justify-center rounded border border-[#c8c3b8] text-[10px] text-white",
                      selected ? "bg-[#25231f]" : "bg-white",
                    )}
                  >
                    {selected ? <span className="size-1.5 rounded-full bg-white" /> : null}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-xs font-medium text-[#33312d]">
                      {candidate.identifier}
                    </span>
                    <span className="block truncate text-xs text-[#77746c]">{candidate.title}</span>
                  </span>
                </button>
              );
            })
          ) : (
            <div className="px-3 py-2 text-xs text-[#77746c]">No other issues</div>
          )}

          {blockers.length ? (
            <button
              className="flex h-8 w-full items-center gap-2 border-t border-[#eceae5] px-3 text-left text-xs text-[#55524b] hover:bg-[#f7f6f2]"
              disabled={disabled}
              onClick={onClear}
              type="button"
            >
              <IconX className="size-3.5" />
              Clear blockers
            </button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}

function PropertyRow({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="grid min-h-9 grid-cols-[5.5rem_minmax(0,1fr)] items-center gap-3">
      <div className="text-xs text-[#85827a]">{label}</div>
      <div className="flex min-w-0 justify-end text-right text-[#33312d]">{children}</div>
    </div>
  );
}

function IssueActivity({ issueId }: { issueId: string }) {
  const { data: commentsData } = useSuspenseQuery({
    queryKey: ["comments", issueId],
    queryFn: () => fetchComments(issueId),
  });

  const { data: activitiesData } = useSuspenseQuery({
    queryKey: ["activities", issueId],
    queryFn: () => fetchActivities(issueId),
  });

  return (
    <div className="px-5 py-4">
      <div className="mb-3 text-sm font-medium">Activity</div>
      <div className="space-y-3">
        <Timeline
          activities={activitiesData.activities}
          comments={commentsData.comments}
        />
      </div>
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
