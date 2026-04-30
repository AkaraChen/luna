import { useState, type FormEvent, type ReactNode } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { IconChevronDown, IconLink, IconSend, IconTrash, IconX } from "@tabler/icons-react";

import {
  createComment,
  deleteIssue,
  fetchComments,
  fetchIssues,
  updateIssue,
  updateIssueState,
  type Issue,
} from "@/api/asahi";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

import { statusColumns } from "./constants";
import { Priority, StatusIcon } from "./issue-badges";

const priorityOptions = [null, 1, 2, 3, 4] as const;

export function IssueDetails({ issue }: { issue: Issue }) {
  const queryClient = useQueryClient();
  const [comment, setComment] = useState("");
  const [statusOpen, setStatusOpen] = useState(false);
  const [priorityOpen, setPriorityOpen] = useState(false);
  const [blockersOpen, setBlockersOpen] = useState(false);

  const commentsQuery = useSuspenseQuery({
    queryKey: ["comments", issue.id],
    queryFn: () => fetchComments(issue.id),
  });

  const allIssuesQuery = useSuspenseQuery({
    queryKey: ["issues", "all"],
    queryFn: () => fetchIssues(),
  });

  const moveMutation = useMutation({
    mutationFn: (state: string) => updateIssueState(issue.id, state),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
  });

  const updateMutation = useMutation({
    mutationFn: (input: { priority?: number | null; blocked_by?: string[] }) =>
      updateIssue(issue.id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: () => deleteIssue(issue.id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
      void queryClient.removeQueries({ queryKey: ["comments", issue.id] });
    },
  });

  const commentMutation = useMutation({
    mutationFn: (body: string) => createComment(issue.id, body),
    onSuccess: () => {
      setComment("");
      void queryClient.invalidateQueries({ queryKey: ["comments", issue.id] });
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
  });

  const blockerIds = issue.blocked_by
    .map((blocker) => blocker.id)
    .filter((id): id is string => Boolean(id));
  const availableBlockers = allIssuesQuery.data.issues.filter(
    (candidate) => candidate.id !== issue.id,
  );

  const submitComment = (event: FormEvent) => {
    event.preventDefault();
    const body = comment.trim();
    if (body) {
      commentMutation.mutate(body);
    }
  };

  return (
    <div className="flex h-full flex-col">
      <div className="px-5 pb-4 pt-5">
        <div className="mb-3 flex items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-2">
            <span className="text-xs font-medium text-[#77746c]">{issue.identifier}</span>
            <span className="h-1 w-1 rounded-full bg-[#c9c4bb]" />
            <span className="text-xs text-[#8a877e]">{formatDate(issue.updated_at)}</span>
          </div>
          <Button
            aria-label="Delete issue"
            className="text-[#8a877e] hover:bg-destructive/10 hover:text-destructive focus-visible:border-destructive/40 focus-visible:ring-destructive/20"
            disabled={deleteMutation.isPending}
            onClick={() => {
              if (window.confirm(`Delete ${issue.identifier}?`)) {
                deleteMutation.mutate();
              }
            }}
            size="icon-xs"
            type="button"
            variant="ghost"
          >
            <IconTrash className="size-3.5" />
          </Button>
        </div>
        <h2 className="text-lg font-semibold leading-snug text-[#22211f]">{issue.title}</h2>
        {issue.description ? (
          <p className="mt-3 text-sm leading-6 text-[#69665f]">{issue.description}</p>
        ) : null}
      </div>

      <div className="border-y border-[#eceae5] px-5 py-2">
        <PropertyRow label="Status">
          <EditableStatus
            disabled={moveMutation.isPending}
            onChange={(state) => {
              moveMutation.mutate(state);
              setStatusOpen(false);
            }}
            open={statusOpen}
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
        <PropertyRow label="Updated">
          <div className="text-xs text-[#55524b]">{formatDate(issue.updated_at)}</div>
        </PropertyRow>
      </div>

      <div className="min-h-0 flex-1 overflow-auto border-t border-[#eceae5] px-5 py-4">
        <div className="mb-3 text-sm font-medium">Activity</div>
        <div className="space-y-3">
          {commentsQuery.data.comments.map((item) => (
            <div className="rounded-md bg-[#f7f6f2] p-3" key={item.id}>
              <div className="mb-1 text-xs text-[#85827a]">{formatDate(item.created_at)}</div>
              <div className="text-sm leading-6 text-[#33312d]">{item.body}</div>
            </div>
          ))}
          {commentsQuery.data.comments.length === 0 ? (
            <div className="text-sm text-[#77746c]">
              No activity yet.
            </div>
          ) : null}
        </div>
      </div>

      <form className="p-4 pt-0" onSubmit={submitComment}>
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

function EditableStatus({
  disabled,
  onChange,
  open,
  setOpen,
  state,
}: {
  disabled: boolean;
  onChange: (state: string) => void;
  open: boolean;
  setOpen: (open: boolean) => void;
  state: string;
}) {
  return (
    <div className="relative min-w-0">
      <button
        className="inline-flex h-7 max-w-full items-center gap-1.5 rounded-md px-1.5 text-left text-xs font-medium text-[#55524b] hover:bg-[#f7f6f2] disabled:opacity-50"
        disabled={disabled}
        onClick={() => setOpen(!open)}
        type="button"
      >
        <StatusIcon state={state} />
        <span className="truncate">{state}</span>
        <IconChevronDown className="size-3.5 shrink-0 text-[#8a877e]" />
      </button>

      {open ? (
        <div className="absolute left-0 top-full z-20 mt-1 min-w-40 rounded-md border border-[#eceae5] bg-white py-1 shadow-md">
          {statusColumns.map((option) => (
            <button
              className={cn(
                "flex h-8 w-full items-center gap-2 px-3 text-left text-xs text-[#33312d] hover:bg-[#f7f6f2]",
                state === option && "bg-[#f2f1ec]",
              )}
              disabled={disabled || state === option}
              key={option}
              onClick={() => onChange(option)}
              type="button"
            >
              <StatusIcon state={option} />
              {option}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function EditablePriority({
  disabled,
  onChange,
  open,
  priority,
  setOpen,
}: {
  disabled: boolean;
  onChange: (priority: number | null) => void;
  open: boolean;
  priority: number | null;
  setOpen: (open: boolean) => void;
}) {
  return (
    <div className="relative min-w-0">
      <button
        className="inline-flex h-7 max-w-full items-center gap-1.5 rounded-md px-1.5 text-left hover:bg-[#f7f6f2] disabled:opacity-50"
        disabled={disabled}
        onClick={() => setOpen(!open)}
        type="button"
      >
        <Priority priority={priority} />
        <IconChevronDown className="size-3.5 shrink-0 text-[#8a877e]" />
      </button>

      {open ? (
        <div className="absolute left-0 top-full z-20 mt-1 min-w-34 rounded-md border border-[#eceae5] bg-white py-1 shadow-md">
          {priorityOptions.map((option) => (
            <button
              className={cn(
                "flex h-8 w-full items-center px-3 text-left text-xs text-[#33312d] hover:bg-[#f7f6f2]",
                priority === option && "bg-[#f2f1ec]",
              )}
              disabled={disabled || priority === option}
              key={option ?? "none"}
              onClick={() => onChange(option)}
              type="button"
            >
              <Priority priority={option} />
            </button>
          ))}
        </div>
      ) : null}
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
        <div className="absolute left-0 top-full z-20 mt-1 max-h-72 w-72 overflow-auto rounded-md border border-[#eceae5] bg-white py-1 shadow-md">
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
    <div className="grid min-h-9 grid-cols-[6rem_minmax(0,1fr)] items-center gap-3">
      <div className="text-xs text-[#85827a]">{label}</div>
      <div className="min-w-0 text-[#33312d]">{children}</div>
    </div>
  );
}

export function DetailsSkeleton() {
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
