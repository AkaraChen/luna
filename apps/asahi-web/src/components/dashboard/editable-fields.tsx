import { useEffect, useRef } from "react";
import { ChevronDown } from "lucide-react";

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
    <Dropdown
      onOpenChange={setOpen}
      open={open}
      trigger={
        <button
          className="asahi-press inline-flex h-7 max-w-full items-center gap-1.5 rounded-md px-1.5 text-left text-[12.5px] text-foreground [transition:background-color_180ms_var(--ease-out-strong)] hover:bg-muted/60 disabled:opacity-50"
          disabled={disabled}
          onClick={() => setOpen(!open)}
          type="button"
        >
          <StatusIcon state={state} />
          <span className="truncate">{state}</span>
          <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
        </button>
      }
    >
      {options.map((option) => (
        <button
          className={cn(
            "flex h-8 w-full items-center gap-2 px-3 text-left text-[12.5px] text-foreground hover:bg-muted/60",
            state === option && "bg-muted",
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
    </Dropdown>
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
  const triggerLabel = priority == null ? "No priority" : `P${priority}`;
  return (
    <Dropdown
      onOpenChange={setOpen}
      open={open}
      trigger={
        <button
          className="asahi-press inline-flex h-7 max-w-full items-center gap-1.5 rounded-md px-1.5 text-left text-[12.5px] text-foreground [transition:background-color_180ms_var(--ease-out-strong)] hover:bg-muted/60 disabled:opacity-50"
          disabled={disabled}
          onClick={() => setOpen(!open)}
          type="button"
        >
          <Priority priority={priority} />
          <span className="truncate">{triggerLabel}</span>
          <ChevronDown className="size-3.5 shrink-0 text-muted-foreground" />
        </button>
      }
    >
      {options.map((option) => (
        <button
          className={cn(
            "flex h-8 w-full items-center gap-2 px-3 text-left text-[12.5px] text-foreground hover:bg-muted/60",
            priority === option && "bg-muted",
          )}
          disabled={disabled || priority === option}
          key={option ?? "none"}
          onClick={() => onChange(option)}
          type="button"
        >
          <Priority priority={option} />
          <span>{option == null ? "No priority" : `P${option}`}</span>
        </button>
      ))}
    </Dropdown>
  );
}

/**
 * Shared dropdown shell: animated open/close (origin-aware), system ease-out,
 * click-outside + Escape to dismiss. Hidden via `data-state` so the
 * transition runs both in and out.
 */
export function Dropdown({
  align = "end",
  children,
  contentClassName,
  onOpenChange,
  open,
  side = "bottom",
  trigger,
}: {
  align?: "start" | "end";
  children: React.ReactNode;
  contentClassName?: string;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  side?: "top" | "bottom";
  trigger: React.ReactNode;
}) {
  const rootRef = useRef<HTMLDivElement>(null);

  // Latest-ref so swapping onOpenChange between renders doesn't churn the
  // document listeners.
  const onOpenChangeRef = useRef(onOpenChange);
  useEffect(() => {
    onOpenChangeRef.current = onOpenChange;
  });

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: PointerEvent) => {
      if (!rootRef.current) return;
      if (!rootRef.current.contains(event.target as Node)) onOpenChangeRef.current(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onOpenChangeRef.current(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const sideClass = side === "top" ? "bottom-full mb-1" : "top-full mt-1";
  const alignClass = align === "start" ? "left-0" : "right-0";
  const origin =
    side === "top"
      ? align === "start"
        ? "bottom left"
        : "bottom right"
      : align === "start"
        ? "top left"
        : "top right";

  return (
    <div className="relative min-w-0" ref={rootRef}>
      {trigger}
      <div
        aria-hidden={!open}
        className={cn(
          "asahi-popover absolute z-20 min-w-36 rounded-md border border-border/70 bg-popover py-1 shadow-[0_1px_2px_oklch(0_0_0_/_0.04)]",
          sideClass,
          alignClass,
          open ? "" : "pointer-events-none",
          contentClassName,
        )}
        data-state={open ? "open" : "closed"}
        style={{ transformOrigin: origin }}
      >
        {children}
      </div>
    </div>
  );
}
