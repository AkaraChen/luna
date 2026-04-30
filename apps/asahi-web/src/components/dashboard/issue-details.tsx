import { useState, type FormEvent, type ReactNode } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { IconSend } from "@tabler/icons-react";

import { createComment, fetchComments, updateIssueState, type Issue } from "@/api/asahi";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";

import { statusColumns } from "./constants";
import { Priority, StatusIcon } from "./issue-badges";

export function IssueDetails({ issue }: { issue: Issue }) {
  const queryClient = useQueryClient();
  const [comment, setComment] = useState("");
  const [statusOpen, setStatusOpen] = useState(false);

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
      <div className="px-5 py-4">
        <div className="mb-3">
          <div className="text-xs font-medium text-[#77746c]">{issue.identifier}</div>
        </div>
        <h2 className="text-lg font-semibold leading-snug text-[#22211f]">{issue.title}</h2>
        {issue.description ? (
          <p className="mt-2 text-sm leading-6 text-[#69665f]">{issue.description}</p>
        ) : null}
      </div>

      <div className="grid grid-cols-2 gap-x-4 gap-y-3 px-5 py-4 text-sm">
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

      <div className="relative px-5 py-3">
        <button
          className="inline-flex h-6 items-center gap-1.5 rounded-md border border-[#dedbd2] bg-white px-2 text-xs font-medium text-[#55524b] hover:bg-[#f7f6f2]"
          onClick={() => setStatusOpen((o) => !o)}
          type="button"
        >
          <StatusIcon state={issue.state} />
          {issue.state}
        </button>
        {statusOpen && (
          <div className="absolute left-5 top-full z-10 mt-1 min-w-36 rounded-md border border-[#eceae5] bg-white py-1 shadow-md">
            {statusColumns.map((state) => (
              <button
                className="flex w-full items-center gap-2 px-3 py-1.5 text-xs text-[#33312d] hover:bg-[#f7f6f2] disabled:opacity-40"
                disabled={moveMutation.isPending || issue.state === state}
                key={state}
                onClick={() => {
                  moveMutation.mutate(state);
                  setStatusOpen(false);
                }}
                type="button"
              >
                <StatusIcon state={state} />
                {state}
              </button>
            ))}
          </div>
        )}
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

function Detail({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="min-w-0">
      <div className="mb-1 text-xs text-[#85827a]">{label}</div>
      <div className="min-h-5 text-[#33312d]">{children}</div>
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
