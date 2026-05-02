import { type ReactNode } from "react";

import { cn } from "@/lib/utils";

export function DashboardPageLayout({
  bodyClassName,
  children,
  className,
  right,
  title,
}: {
  bodyClassName?: string;
  children: ReactNode;
  className?: string;
  right?: ReactNode;
  title: ReactNode;
}) {
  return (
    <section
      className={cn("flex min-h-0 flex-1 flex-col overflow-hidden", className)}
      data-slot="dashboard-page-layout"
    >
      <header
        className="sticky top-0 z-20 flex h-14 shrink-0 items-center justify-between gap-3 border-b border-border bg-background/95 px-4 backdrop-blur supports-[backdrop-filter]:bg-background/80"
        data-slot="dashboard-page-layout-header"
      >
        <div className="flex min-w-0 flex-1 items-center gap-3">{title}</div>
        {right ? <div className="flex shrink-0 items-center gap-2">{right}</div> : null}
      </header>

      <div
        className={cn("flex min-h-0 flex-1 flex-col overflow-auto", bodyClassName)}
        data-slot="dashboard-page-layout-body"
      >
        {children}
      </div>
    </section>
  );
}
