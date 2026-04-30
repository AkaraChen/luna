import { IconCircleDashed, IconLink } from "@tabler/icons-react";

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
          <IconCircleDashed className="mx-auto mb-3 size-8 text-[#b4b0a7]" stroke={1.5} />
          <div className="text-sm font-medium">No issues</div>
          <div className="mt-1 text-sm text-[#77746c]">Try a different status or search.</div>
        </div>
      </div>
    );
  }

  return (
    <div>
      {issues.map((issue) => (
        <button
          className={cn(
            "grid min-h-13 w-full grid-cols-[1rem_minmax(0,1fr)_auto] items-center gap-3 px-4 py-2 text-left hover:bg-[#f7f6f2]",
            selectedId === issue.id && "bg-[#f2f1ec]",
          )}
          key={issue.id}
          onClick={() => onSelect(issue.id)}
          type="button"
        >
          <StatusIcon state={issue.state} />
          <span className="min-w-0">
            <span className="block truncate text-sm text-[#262522]">{issue.title}</span>
            <span className="mt-1 flex min-w-0 items-center gap-2 text-xs text-[#8f8b82]">
              <span className="shrink-0">{issue.identifier}</span>
              <span className="shrink-0">{formatDate(issue.updated_at)}</span>
              {issue.blocked_by.length ? (
                <span className="inline-flex min-w-0 items-center gap-1">
                  <IconLink className="size-3 shrink-0" />
                  <span className="truncate">
                    {issue.blocked_by.map((blocker) => blocker.identifier ?? blocker.id).join(", ")}
                  </span>
                </span>
              ) : null}
            </span>
          </span>
          <Priority priority={issue.priority} />
        </button>
      ))}
    </div>
  );
}

function formatDate(value: string | null) {
  if (!value) return "No update";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(new Date(value));
}

export function EmptyDetails() {
  return (
    <div className="flex h-full items-center justify-center p-8 text-center">
      <div>
        <IconCircleDashed className="mx-auto mb-3 size-8 text-[#b4b0a7]" stroke={1.5} />
        <div className="text-sm font-medium">No issue selected</div>
      </div>
    </div>
  );
}
