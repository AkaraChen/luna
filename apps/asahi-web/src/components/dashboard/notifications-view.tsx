import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import {
  IconArchive,
  IconBell,
  IconCircleCheck,
  IconMessage,
  IconPencil,
  IconPlus,
} from "@tabler/icons-react";

import {
  archiveNotification,
  fetchNotifications,
  markNotificationRead,
  type AsahiNotification,
} from "@/api/asahi";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { Priority, StatusBadge } from "./issue-badges";

export function NotificationsView() {
  const queryClient = useQueryClient();
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
    <section className="min-h-[calc(100svh-3.5rem)] bg-background">
      <div className="flex h-12 items-center justify-between border-b border-border px-4">
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
            readDisabled={readMutation.isPending}
          />
        ))}
      </div>
    </section>
  );
}

function NotificationRow({
  archiveDisabled,
  notification,
  onArchive,
  onRead,
  readDisabled,
}: {
  archiveDisabled: boolean;
  notification: AsahiNotification;
  onArchive: () => void;
  onRead: () => void;
  readDisabled: boolean;
}) {
  const unread = notification.read_at == null;
  const Icon = notificationIcon(notification.type);

  return (
    <article
      className={cn(
        "grid min-h-16 grid-cols-[1.5rem_minmax(0,1fr)_auto] items-start gap-3 border-b border-border px-4 py-3",
        unread ? "bg-[#fbfaf7]" : "bg-background",
      )}
    >
      <div className="pt-1">
        <span
          className={cn(
            "block size-2 rounded-full",
            unread ? "bg-[#276ef1]" : "bg-transparent",
          )}
        />
      </div>

      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <Icon className="size-4 shrink-0 text-[#76736b]" stroke={1.8} />
          <span className="truncate text-sm font-medium text-[#25231f]">{notification.title}</span>
          <span className="shrink-0 text-xs text-[#9a968d]">
            {formatDate(notification.created_at)}
          </span>
        </div>

        {notification.body ? (
          <p className="mt-1 line-clamp-2 text-sm leading-5 text-[#625f58]">{notification.body}</p>
        ) : null}

        {notification.issue ? (
          <div className="mt-2 flex min-w-0 flex-wrap items-center gap-2">
            <span className="text-xs font-medium text-[#5f5b53]">
              {notification.issue.identifier}
            </span>
            <span className="min-w-0 truncate text-xs text-[#7e7a72]">
              {notification.issue.title}
            </span>
            <StatusBadge state={notification.issue.state} />
            <Priority priority={notification.issue.priority} />
          </div>
        ) : null}
      </div>

      <div className="flex items-center gap-1">
        {unread ? (
          <Button
            aria-label="Mark as read"
            disabled={readDisabled}
            onClick={onRead}
            size="icon-xs"
            type="button"
            variant="ghost"
          >
            <IconCircleCheck className="size-3.5" />
          </Button>
        ) : null}
        <Button
          aria-label="Archive notification"
          disabled={archiveDisabled}
          onClick={onArchive}
          size="icon-xs"
          type="button"
          variant="ghost"
        >
          <IconArchive className="size-3.5" />
        </Button>
      </div>
    </article>
  );
}

function notificationIcon(type: string) {
  if (type === "comment_created") return IconMessage;
  if (type === "issue_created") return IconPlus;
  if (type === "issue_updated") return IconPencil;
  return IconBell;
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}
