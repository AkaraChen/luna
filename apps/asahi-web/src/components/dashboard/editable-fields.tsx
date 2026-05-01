import { IconChevronDown } from "@tabler/icons-react";

import { cn } from "@/lib/utils";

import { Priority, StatusIcon } from "./issue-badges";

export function EditableStatus({
  disabled,
  onChange,
  open,
  options,
  setOpen,
  state,
}: {
  disabled: boolean;
  onChange: (state: string) => void;
  open: boolean;
  options: string[];
  setOpen: (open: boolean) => void;
  state: string;
}) {
  return (
    <div className="relative min-w-0">
      <button
        className="inline-flex h-7 max-w-full items-center gap-1.5 rounded-md px-1.5 text-left text-xs font-medium text-[#55524b] hover:bg-[#f7f6f2] disabled:opacity-50"
        disabled={disabled}
        onClick={() => setOpen(!open)}
        type="button"
      >
        <StatusIcon state={state} />
        <span className="truncate">{state}</span>
        <IconChevronDown className="size-3.5 shrink-0 text-[#8a877e]" />
      </button>

      {open ? (
        <div className="absolute right-0 top-full z-20 mt-1 min-w-40 rounded-md border border-[#eceae5] bg-white py-1 shadow-md">
          {options.map((option) => (
            <button
              className={cn(
                "flex h-8 w-full items-center gap-2 px-3 text-left text-xs text-[#33312d] hover:bg-[#f7f6f2]",
                state === option && "bg-[#f2f1ec]",
              )}
              disabled={disabled || state === option}
              key={option}
              onClick={() => onChange(option)}
              type="button"
            >
              <StatusIcon state={option} />
              {option}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

export function EditablePriority({
  disabled,
  onChange,
  open,
  options,
  priority,
  setOpen,
}: {
  disabled: boolean;
  onChange: (priority: number | null) => void;
  open: boolean;
  options: (number | null)[];
  priority: number | null;
  setOpen: (open: boolean) => void;
}) {
  return (
    <div className="relative min-w-0">
      <button
        className="inline-flex h-7 max-w-full items-center gap-1.5 rounded-md px-1.5 text-left hover:bg-[#f7f6f2] disabled:opacity-50"
        disabled={disabled}
        onClick={() => setOpen(!open)}
        type="button"
      >
        <Priority priority={priority} />
        <IconChevronDown className="size-3.5 shrink-0 text-[#8a877e]" />
      </button>

      {open ? (
        <div className="absolute right-0 top-full z-20 mt-1 min-w-34 rounded-md border border-[#eceae5] bg-white py-1 shadow-md">
          {options.map((option) => (
            <button
              className={cn(
                "flex h-8 w-full items-center px-3 text-left text-xs text-[#33312d] hover:bg-[#f7f6f2]",
                priority === option && "bg-[#f2f1ec]",
              )}
              disabled={disabled || priority === option}
              key={option ?? "none"}
              onClick={() => onChange(option)}
              type="button"
            >
              <Priority priority={option} />
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
