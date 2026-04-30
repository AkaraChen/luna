import { type InputHTMLAttributes } from "react";

import { cn } from "@/lib/utils";

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={cn(
        "h-9 rounded-md border border-[#dedbd2] bg-white px-3 text-sm text-[#262522] shadow-none outline-none transition-colors placeholder:text-[#9a968d] focus:border-[#b9b5aa] focus:ring-2 focus:ring-[#e7e4dc]",
        className,
      )}
      {...props}
    />
  );
}
