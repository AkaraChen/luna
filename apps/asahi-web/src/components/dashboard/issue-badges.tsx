import { IconArrowUp, IconCircleCheck, IconCircleDashed, IconCircleDot } from "@tabler/icons-react";

export function StatusBadge({ state }: { state: string }) {
  const Icon =
    state === "Done" ? IconCircleCheck : state === "In Progress" ? IconCircleDot : IconCircleDashed;

  return (
    <span className="inline-flex h-6 max-w-full items-center gap-1.5 rounded-md border border-[#dedbd2] bg-white px-2 text-xs font-medium text-[#55524b]">
      <Icon className="size-3.5 text-[#6f6d66]" />
      <span className="truncate">{state}</span>
    </span>
  );
}

export function Priority({ priority }: { priority: number | null }) {
  if (priority == null) {
    return <span className="text-xs text-[#99958b]">No priority</span>;
  }

  return (
    <span className="inline-flex items-center gap-1 text-xs font-medium text-[#65625b]">
      <IconArrowUp className="size-3.5 text-[#7d7a72]" />P{priority}
    </span>
  );
}
