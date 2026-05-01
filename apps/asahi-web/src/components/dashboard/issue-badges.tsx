import { IconArrowUp, IconCircleCheck, IconCircleDashed, IconCircleDot } from "@tabler/icons-react";

function statusIcon(state: string) {
  return state === "Done"
    ? IconCircleCheck
    : state === "In Progress"
      ? IconCircleDot
      : IconCircleDashed;
}

export function StatusIcon({ state }: { state: string }) {
  const Icon = statusIcon(state);
  return <Icon className="size-4 shrink-0 text-[#6f6d66]" stroke={1.8} />;
}

export function Priority({
  priority,
  showEmpty = true,
}: {
  priority: number | null;
  showEmpty?: boolean;
}) {
  if (priority == null) {
    if (!showEmpty) return null;
    return <span className="text-xs text-[#99958b]">No priority</span>;
  }

  return (
    <span className="inline-flex items-center gap-1 text-xs font-medium text-[#65625b]">
      <IconArrowUp className="size-3.5 text-[#7d7a72]" />P{priority}
    </span>
  );
}
