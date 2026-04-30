import { type TextareaHTMLAttributes } from "react";

import { cn } from "@/lib/utils";

export function Textarea({ className, ...props }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea
      className={cn(
        "w-full rounded-md border border-[#dedbd2] bg-white px-3 py-2 text-sm leading-6 text-[#262522] shadow-none outline-none transition-colors placeholder:text-[#9a968d] focus:border-[#b9b5aa] focus:ring-2 focus:ring-[#e7e4dc]",
        className,
      )}
      {...props}
    />
  );
}
