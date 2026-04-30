import { type ButtonHTMLAttributes } from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-md text-sm font-medium transition-colors disabled:pointer-events-none disabled:opacity-50",
  {
    defaultVariants: {
      size: "default",
      variant: "default",
    },
    variants: {
      size: {
        default: "h-9 px-3",
        icon: "size-8",
        sm: "h-8 px-2.5 text-xs",
      },
      variant: {
        default: "bg-[#20201d] text-white hover:bg-[#34332f]",
        ghost: "text-[#55524b] hover:bg-[#eeece6]",
        outline: "border border-[#dedbd2] bg-white text-[#55524b] hover:bg-[#f7f6f2]",
      },
    },
  },
);

export function Button({
  className,
  size,
  variant,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & VariantProps<typeof buttonVariants>) {
  return (
    <button className={cn(buttonVariants({ size, variant }), className)} type="button" {...props} />
  );
}
