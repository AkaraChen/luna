import { type HTMLAttributes } from "react";

import { cn } from "@/lib/utils";

export function Badge({
  className,
  variant = "default",
  ...props
}: HTMLAttributes<HTMLSpanElement> & {
  variant?: "default" | "secondary";
}) {
  return (
    <span
      className={cn(
        "inline-flex h-5 max-w-full items-center rounded px-1.5 text-[11px] font-medium",
        variant === "secondary" ? "bg-[#efeee8] text-[#69665f]" : "bg-[#20201d] text-white",
        className,
      )}
      {...props}
    />
  );
}
