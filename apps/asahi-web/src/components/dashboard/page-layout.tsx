import { type ReactNode } from "react";

import { cn } from "@/lib/utils";

import { PageTopbar } from "./asahi-sidebar";

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
      <PageTopbar right={right} title={title} />
      <div
        className={cn("flex min-h-0 flex-1 flex-col overflow-hidden", bodyClassName)}
        data-slot="dashboard-page-layout-body"
      >
        {children}
      </div>
    </section>
  );
}

export { PageTopbar };
