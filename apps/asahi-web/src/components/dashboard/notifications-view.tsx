import { Suspense, useEffect, useMemo, useRef, useState } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Archive, AtSign, Bell, CheckCheck, MessageSquare, Sparkles, UserPlus } from "lucide-react";

import {
  archiveNotification,
  fetchIssues,
  fetchNotifications,
  markNotificationRead,
  markNotificationUnread,
  type AsahiNotification,
} from "@/api/asahi";
import { Button } from "@/components/ui/button";
import {
  NOTIFICATIONS_REFETCH_INTERVAL_MS,
  refreshNotifications,
} from "@/lib/query-refresh";
import { cn } from "@/lib/utils";

import { IssueDetailSkeleton } from "@/components/dashboard/dashboard-skeleton";
import { IssueDetails } from "./issue-details";
import { EmptyDetails } from "./issue-list";

type Tab = "all" | "unread";

const tabs: Array<{ id: Tab; label: string }> = [
  { id: "all", label: "All" },
  { id: "unread", label: "Unread" },
];

export function NotificationsView() {
  const queryClient = useQueryClient();
  const [selectedIssueId, setSelectedIssueId] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("all");

  const { data } = useSuspenseQuery({
    queryKey: ["notifications", "inbox"],
    queryFn: () => fetchNotifications({ limit: 50 }),
    refetchInterval: NOTIFICATIONS_REFETCH_INTERVAL_MS,
    refetchIntervalInBackground: true,
  });

  const readMutation = useMutation({
    mutationFn: markNotificationRead,
    onSettled: () => refreshNotifications(queryClient),
  });

  const unreadMutation = useMutation({
    mutationFn: markNotificationUnread,
    onSettled: () => refreshNotifications(queryClient),
  });

  const archiveMutation = useMutation({
    mutationFn: archiveNotification,
    onSettled: () => refreshNotifications(queryClient),
  });

  const markAllReadMutation = useMutation({
    mutationFn: async () => {
      const unread = data.notifications.filter((n) => n.read_at == null);
      await Promise.all(unread.map((n) => markNotificationRead(n.id)));
    },
    onSettled: () => refreshNotifications(queryClient),
  });

  const filtered = useMemo(() => {
    if (tab === "unread") return data.notifications.filter((n) => n.read_at == null);
    return data.notifications;
  }, [data.notifications, tab]);

  if (data.notifications.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 text-center">
        <div className="flex size-10 items-center justify-center rounded-full bg-muted">
          <Bell className="size-4 text-muted-foreground" strokeWidth={1.8} />
        </div>
        <div>
          <p className="text-[13.5px] font-medium text-foreground">No new notifications</p>
          <p className="mt-1 max-w-xs text-[12px] text-muted-foreground">
            Issue updates and comments will appear here.
          </p>
        </div>
      </div>
    );
  }

  return (
    <section className="grid min-h-0 flex-1 overflow-hidden xl:grid-cols-[minmax(20rem,28rem)_minmax(0,1fr)]">
      <div className="flex min-h-0 min-w-0 flex-col border-r border-border/60">
        <div className="flex shrink-0 items-center justify-between gap-3 px-5 py-3">
          <div className="inline-flex items-center gap-1 rounded-full border border-border/70 bg-muted/40 p-1">
            {tabs.map((t) => {
              const active = tab === t.id;
              return (
                <button
                  className={cn(
                    "asahi-press inline-flex items-center gap-1.5 rounded-full px-3.5 py-1 text-[12.5px] [transition:color_180ms_var(--ease-out-strong),background-color_180ms_var(--ease-out-strong),transform_140ms_var(--ease-out-strong)] hover:text-foreground",
                    active
                      ? "asahi-pill-lift bg-background text-foreground"
                      : "text-muted-foreground",
                  )}
                  key={t.id}
                  onClick={() => setTab(t.id)}
                  type="button"
                >
                  {t.label}
                  {t.id === "unread" && data.unread_count > 0 ? (
                    <span className="rounded-full bg-foreground px-1.5 text-[10px] font-medium text-background tabular-nums">
                      {data.unread_count}
                    </span>
                  ) : null}
                </button>
              );
            })}
          </div>

          <Button
            className="asahi-press h-7 rounded-md border-border/80 bg-background px-2.5 text-[12px] text-muted-foreground hover:bg-muted/40 hover:text-foreground disabled:opacity-40"
            disabled={data.unread_count === 0 || markAllReadMutation.isPending}
            onClick={() => markAllReadMutation.mutate()}
            size="sm"
            type="button"
            variant="outline"
          >
            <CheckCheck className="size-3.5" data-icon="inline-start" />
            Mark all read
          </Button>
        </div>

        <ul className="min-h-0 flex-1 overflow-auto px-2 pb-4">
          {filtered.map((notification, i) => (
            <NotificationRow
              animationIndex={i}
              archiveDisabled={archiveMutation.isPending}
              key={notification.id}
              notification={notification}
              onArchive={() => archiveMutation.mutate(notification.id)}
              onRead={() => readMutation.mutate(notification.id)}
              onUnread={() => unreadMutation.mutate(notification.id)}
              onSelectIssue={setSelectedIssueId}
              selected={notification.issue != null && notification.issue.id === selectedIssueId}
            />
          ))}
          {filtered.length === 0 ? (
            <li className="px-3 py-8 text-center text-[13px] text-muted-foreground">
              No unread activity.
            </li>
          ) : null}
        </ul>
      </div>

      <aside className="hidden min-w-0 xl:block">
        {selectedIssueId ? (
          <Suspense fallback={<IssueDetailSkeleton />}>
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
  animationIndex,
  archiveDisabled,
  notification,
  onArchive,
  onRead,
  onSelectIssue,
  onUnread,
  selected,
}: {
  animationIndex: number;
  archiveDisabled: boolean;
  notification: AsahiNotification;
  onArchive: () => void;
  onRead: () => void;
  onSelectIssue: (issueId: string) => void;
  onUnread: () => void;
  selected: boolean;
}) {
  const unread = notification.read_at == null;
  const issue = notification.issue;
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!menu) return;
    const handleClick = (event: MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) setMenu(null);
    };
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMenu(null);
    };
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKey);
    };
  }, [menu]);

  const KindIcon = pickIcon(notification.type);
  const verbTone = pickVerbTone(notification.type, unread);

  return (
    <li
      className="asahi-rise"
      style={{ animationDelay: `${Math.min(animationIndex * 22, 200)}ms` }}
    >
      <div
        className={cn(
          "group grid w-full grid-cols-[10px_minmax(0,1fr)_auto] items-baseline gap-x-3 border-b border-border/60 px-3 py-2.5 text-left",
          "[transition:background-color_180ms_var(--ease-out-strong)] hover:bg-muted/40",
          selected && "bg-muted",
          issue && "cursor-pointer",
        )}
        onClick={() => {
          if (issue) {
            onSelectIssue(issue.id);
            if (unread) onRead();
          }
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          setMenu({ x: event.clientX, y: event.clientY });
        }}
        onKeyDown={(event) => {
          if (issue && (event.key === "Enter" || event.key === " ")) {
            event.preventDefault();
            onSelectIssue(issue.id);
            if (unread) onRead();
          }
        }}
        role={issue ? "button" : undefined}
        tabIndex={issue ? 0 : undefined}
      >
        <span aria-hidden className="self-start pt-[0.4rem]">
          {unread ? (
            <span aria-label="Unread" className="block size-1.5 rounded-full bg-foreground" />
          ) : null}
        </span>

        <div className="flex min-w-0 flex-col gap-0.5">
          <p
            className={cn(
              "truncate text-[13.5px] leading-snug",
              unread ? "text-foreground" : "text-muted-foreground",
            )}
          >
            <span className={cn(unread && "font-medium")}>{notification.title}</span>
            {issue ? (
              <>
                <span aria-hidden className="mx-1.5 text-border">
                  ·
                </span>
                <span className="font-mono text-[11.5px] uppercase tracking-wide text-muted-foreground">
                  {issue.identifier}
                </span>
                <span aria-hidden className="mx-1.5 text-border">
                  ·
                </span>
                <span className={verbTone}>{issue.title}</span>
              </>
            ) : null}
          </p>
          {notification.body ? (
            <p className="truncate text-[12px] text-muted-foreground">{notification.body}</p>
          ) : null}
        </div>

        <div className="flex shrink-0 items-baseline gap-3">
          <KindIcon className="size-3.5 text-muted-foreground" />
          <time className="text-[11.5px] tabular-nums text-muted-foreground">
            {formatDate(notification.created_at)}
          </time>
          <Button
            aria-label="Archive notification"
            aria-disabled={archiveDisabled}
            className="asahi-press aria-disabled:opacity-50"
            onClick={(event) => {
              event.stopPropagation();
              if (archiveDisabled) return;
              onArchive();
            }}
            size="icon-xs"
            type="button"
            variant="ghost"
          >
            <Archive className="size-3.5" />
          </Button>
        </div>
      </div>

      {menu ? (
        <div
          className="fixed z-50 min-w-40 rounded-md border border-border/70 bg-popover py-1 shadow-[0_1px_2px_oklch(0_0_0_/_0.04)]"
          ref={menuRef}
          style={{ left: menu.x, top: menu.y }}
        >
          {!unread && (
            <button
              className="flex h-8 w-full items-center px-3 text-left text-[12.5px] text-foreground hover:bg-muted/60"
              onClick={() => {
                onUnread();
                setMenu(null);
              }}
              type="button"
            >
              Mark as unread
            </button>
          )}
          <button
            className="flex h-8 w-full items-center px-3 text-left text-[12.5px] text-foreground hover:bg-muted/60"
            onClick={() => {
              onArchive();
              setMenu(null);
            }}
            type="button"
          >
            Archive
          </button>
        </div>
      ) : null}
    </li>
  );
}

function pickIcon(type: string) {
  if (type.includes("mention")) return AtSign;
  if (type.includes("assign")) return UserPlus;
  if (type.includes("comment")) return MessageSquare;
  if (type.includes("agent")) return Sparkles;
  return Bell;
}

function pickVerbTone(type: string, unread: boolean): string {
  if (!unread) return "text-muted-foreground";
  if (type.includes("mention")) return "text-mention";
  if (type.includes("agent")) return "text-foreground";
  if (type.includes("status")) return "text-status-progress";
  return "text-foreground";
}

function formatDate(value: string) {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}
