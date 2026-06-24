import { CircleDashed, Link2 } from "lucide-react";

import { type Issue } from "@/api/asahi";
import { cn } from "@/lib/utils";

import { Priority, StatusIcon } from "./issue-badges";

export function IssueList({
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
          <CircleDashed className="mx-auto mb-3 size-8 text-muted-foreground" strokeWidth={1.5} />
          <div className="text-[13.5px] font-medium text-foreground">No issues</div>
          <div className="mt-1 text-[12.5px] text-muted-foreground">
            Try a different status or search.
          </div>
        </div>
      </div>
    );
  }

  return (
    <ul>
      {issues.map((issue, i) => (
        <li
          className="asahi-rise"
          key={issue.id}
          style={{ animationDelay: `${Math.min(i * 22, 220)}ms` }}
        >
          <button
            className={cn(
              "group flex w-full items-baseline gap-3 rounded-md px-3 py-2.5 text-left",
              "[transition:background-color_180ms_var(--ease-out-strong)] hover:bg-muted/40",
              selectedId === issue.id && "bg-muted",
            )}
            aria-label={`Open issue ${issue.identifier}: ${issue.title}`}
            onClick={() => onSelect(issue.id)}
            type="button"
          >
            <StatusIcon className="translate-y-0.5" state={issue.state} />
            <div className="flex min-w-0 flex-1 items-baseline gap-3">
              <span className="truncate text-[13.5px] text-foreground">{issue.title}</span>
              {issue.labels.length ? (
                <span className="hidden shrink-0 items-center gap-1.5 md:inline-flex">
                  {issue.labels.slice(0, 2).map((label) => (
                    <span
                      className="inline-flex items-center gap-1 text-[11px] text-muted-foreground"
                      key={label}
                    >
                      <span aria-hidden className="size-1.5 rounded-full bg-muted-foreground/70" />
                      {label}
                    </span>
                  ))}
                </span>
              ) : null}
              <span className="hidden shrink-0 font-mono text-[11.5px] uppercase tracking-wide text-muted-foreground sm:inline">
                {issue.identifier}
              </span>
              {issue.blocked_by.length ? (
                <span className="hidden shrink-0 items-center gap-1 text-[11.5px] text-muted-foreground lg:inline-flex">
                  <Link2 className="size-3" />
                  {issue.blocked_by.map((b) => b.identifier ?? b.id).join(", ")}
                </span>
              ) : null}
            </div>
            <div className="flex shrink-0 items-baseline gap-3">
              <Priority priority={issue.priority} showEmpty={false} />
              <span className="text-[11.5px] tabular-nums text-muted-foreground">
                {formatDate(issue.updated_at)}
              </span>
            </div>
          </button>
        </li>
      ))}
    </ul>
  );
}

const SHORT_DATE = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
});

function formatDate(value: string | null) {
  if (!value) return "—";
  return SHORT_DATE.format(new Date(value));
}

export function EmptyDetails() {
  return (
    <div className="flex h-full items-center justify-center p-8 text-center">
      <div>
        <CircleDashed className="mx-auto mb-3 size-8 text-muted-foreground" strokeWidth={1.5} />
        <div className="text-[13.5px] font-medium text-foreground">No issue selected</div>
      </div>
    </div>
  );
}
