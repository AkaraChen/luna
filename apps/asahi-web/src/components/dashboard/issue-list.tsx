import { IconCircleDashed, IconHash } from "@tabler/icons-react";

import { type Issue } from "@/api/asahi";
import { Badge } from "@/components/ui/badge";
import { cn } from "@/lib/utils";

import { Priority, StatusBadge } from "./issue-badges";

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
