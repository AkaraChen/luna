import { IconCircleDashed } from "@tabler/icons-react";

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
            "flex h-10 w-full items-center gap-3 px-4 text-left hover:bg-[#f7f6f2]",
            selectedId === issue.id && "bg-[#f2f1ec]",
          )}
          key={issue.id}
          onClick={() => onSelect(issue.id)}
          type="button"
        >
          <StatusIcon state={issue.state} />
          <span className="flex-1 truncate text-sm text-[#262522]">{issue.title}</span>
          <Priority priority={issue.priority} />
          <span className="shrink-0 text-xs text-[#a09d97]">{issue.identifier}</span>
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
