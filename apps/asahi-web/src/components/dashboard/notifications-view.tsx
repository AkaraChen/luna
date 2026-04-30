import { Suspense, useState } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { IconArchive, IconBell, IconCircleCheck } from "@tabler/icons-react";

import {
  archiveNotification,
  fetchIssues,
  fetchNotifications,
  markNotificationRead,
  type AsahiNotification,
} from "@/api/asahi";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { Priority, StatusIcon } from "./issue-badges";
import { IssueDetails, DetailsSkeleton } from "./issue-details";
import { EmptyDetails } from "./issue-list";

export function NotificationsView() {
  const queryClient = useQueryClient();
  const [selectedIssueId, setSelectedIssueId] = useState<string | null>(null);

  const { data } = useSuspenseQuery({
    queryKey: ["notifications", "inbox"],
    queryFn: () => fetchNotifications({ limit: 50 }),
  });

  const readMutation = useMutation({
    mutationFn: markNotificationRead,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
  });

  const archiveMutation = useMutation({
    mutationFn: archiveNotification,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
  });

  if (data.notifications.length === 0) {
    return (
      <div className="flex min-h-[calc(100svh-3.5rem)] flex-col items-center justify-center gap-3 px-6 text-center">
        <div className="flex size-10 items-center justify-center rounded-full bg-muted">
          <IconBell className="size-4.5 text-muted-foreground" stroke={1.8} />
        </div>
        <div>
          <p className="text-sm font-medium">No new notifications</p>
          <p className="mt-1 max-w-xs text-xs text-muted-foreground">
            Issue updates and comments will appear here.
          </p>
        </div>
      </div>
    );
  }

  return (
    <section className="grid min-h-[calc(100svh-3.5rem)] xl:grid-cols-[minmax(0,1fr)_360px]">
      <div className="min-w-0 border-r border-border">
        <div className="flex h-12 items-center justify-between px-4">
          <div className="text-xs text-muted-foreground">
            {data.unread_count ? `${data.unread_count} unread` : "All caught up"}
          </div>
        </div>

        <div>
          {data.notifications.map((notification) => (
            <NotificationRow
              archiveDisabled={archiveMutation.isPending}
              key={notification.id}
              notification={notification}
              onArchive={() => archiveMutation.mutate(notification.id)}
              onRead={() => readMutation.mutate(notification.id)}
              onSelectIssue={setSelectedIssueId}
              readDisabled={readMutation.isPending}
              selected={notification.issue != null && notification.issue.id === selectedIssueId}
            />
          ))}
        </div>
      </div>

      <aside className="min-w-0 bg-card">
        {selectedIssueId ? (
          <Suspense fallback={<DetailsSkeleton />}>
            <NotificationIssueDetails issueId={selectedIssueId} />
          </Suspense>
        ) : (
          <EmptyDetails />
        )}
      </aside>
    </section>
  );
}

function NotificationIssueDetails({ issueId }: { issueId: string }) {
  const { data } = useSuspenseQuery({
    queryKey: ["issues", "all"],
    queryFn: () => fetchIssues(),
  });

  const issue = data.issues.find((i) => i.id === issueId);
  if (!issue) return <EmptyDetails />;

  return <IssueDetails issue={issue} />;
}

function NotificationRow({
  archiveDisabled,
  notification,
  onArchive,
  onRead,
  onSelectIssue,
  readDisabled,
  selected,
}: {
  archiveDisabled: boolean;
  notification: AsahiNotification;
  onArchive: () => void;
  onRead: () => void;
  onSelectIssue: (issueId: string) => void;
  readDisabled: boolean;
  selected: boolean;
}) {
  const unread = notification.read_at == null;
  const issue = notification.issue;

  return (
    <div
      className={cn(
        "grid min-h-13 w-full grid-cols-[1rem_minmax(0,1fr)_auto_auto] items-center gap-3 px-4 py-2 text-left hover:bg-[#f7f6f2]",
        unread ? "bg-[#fbfaf7]" : "bg-background",
        selected && "bg-[#f2f1ec]",
        issue && "cursor-pointer",
      )}
      onClick={() => {
        if (issue) {
          onSelectIssue(issue.id);
        }
      }}
      onKeyDown={(event) => {
        if (issue && (event.key === "Enter" || event.key === " ")) {
          event.preventDefault();
          onSelectIssue(issue.id);
        }
      }}
      role={issue ? "button" : undefined}
      tabIndex={issue ? 0 : undefined}
    >
      {issue ? (
        <StatusIcon state={issue.state} />
      ) : (
        <IconBell className="size-4 shrink-0 text-[#6f6d66]" stroke={1.8} />
      )}

      <div className="min-w-0">
        <span
          className={cn(
            "block truncate text-sm text-[#262522]",
            unread && "font-medium text-[#1f1e1b]",
          )}
        >
          {issue?.title ?? notification.title}
        </span>
        <span className="mt-1 flex min-w-0 items-center gap-2 text-xs text-[#8f8b82]">
          {issue ? <span className="shrink-0">{issue.identifier}</span> : null}
          <span className="shrink-0">{formatDate(notification.created_at)}</span>
          <span className="truncate">{notification.title}</span>
        </span>
      </div>

      <Priority priority={issue?.priority ?? null} showEmpty={false} />

      <div className="flex items-center gap-1">
        <Button
          aria-label="Mark as read"
          aria-disabled={readDisabled || !unread}
          className={cn("aria-disabled:opacity-50", !unread && "text-[#b7b2aa]")}
          onClick={(event) => {
            event.stopPropagation();
            if (readDisabled || !unread) return;
            onRead();
          }}
          size="icon-xs"
          type="button"
          variant="ghost"
        >
          <IconCircleCheck className="size-3.5" />
        </Button>
        <Button
          aria-label="Archive notification"
          aria-disabled={archiveDisabled}
          className="aria-disabled:opacity-50"
          onClick={(event) => {
            event.stopPropagation();
            if (archiveDisabled) return;
            onArchive();
          }}
          size="icon-xs"
          type="button"
          variant="ghost"
        >
          <IconArchive className="size-3.5" />
        </Button>
      </div>
    </div>
  );
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}
