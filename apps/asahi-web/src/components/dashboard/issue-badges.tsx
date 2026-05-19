import { CircleCheck, CircleDashed, CircleDot } from "lucide-react";

import { cn } from "@/lib/utils";

export function StatusIcon({ state, className }: { state: string; className?: string }) {
  if (state === "Done") {
    return (
      <CircleCheck
        className={cn("size-3.5 shrink-0 text-status-done", className)}
        strokeWidth={1.8}
      />
    );
  }
  if (state === "In Progress") {
    return (
      <CircleDot
        className={cn("size-3.5 shrink-0 text-status-progress", className)}
        strokeWidth={1.8}
      />
    );
  }
  if (state === "Todo") {
    return (
      <CircleDot
        className={cn("size-3.5 shrink-0 text-muted-foreground", className)}
        strokeWidth={1.5}
      />
    );
  }
  return (
    <CircleDashed
      className={cn("size-3.5 shrink-0 text-muted-foreground", className)}
      strokeWidth={1.5}
    />
  );
}

export function Priority({
  priority,
  showEmpty = true,
}: {
  priority: number | null;
  showEmpty?: boolean;
}) {
  if (priority == null && !showEmpty) return null;

  const bars = priority == null ? 0 : Math.max(0, 4 - priority);
  const label = priority == null ? "No priority" : `P${priority}`;

  return (
    <span
      aria-label={label}
      className="inline-flex items-end gap-[2px]"
      title={label}
    >
      {[1, 2, 3].map((i) => (
        <span
          className={cn(
            "w-[3px] rounded-[1px]",
            i <= bars ? "bg-foreground/80" : "bg-border",
            i === 1 ? "h-[5px]" : i === 2 ? "h-[8px]" : "h-[11px]",
          )}
          key={i}
        />
      ))}
    </span>
  );
}
