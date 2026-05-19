import { Skeleton } from "@/components/ui/skeleton";

export function SidebarSkeleton() {
  return (
    <div className="min-h-svh bg-background">
      <div className="flex min-h-svh md:pl-64">
        <aside className="fixed inset-y-0 left-0 z-30 hidden w-64 flex-col gap-2 bg-muted/50 px-3 py-4 md:flex">
          <Skeleton className="h-7 w-20" />
          <div className="mt-2 flex flex-col gap-1">
            <Skeleton className="h-7 w-full" />
            <Skeleton className="h-7 w-full" />
          </div>
          <Skeleton className="mt-6 h-4 w-16" />
          <div className="mt-1 flex flex-col gap-1">
            <Skeleton className="h-7 w-full" />
            <Skeleton className="h-7 w-full" />
            <Skeleton className="h-7 w-full" />
          </div>
        </aside>
        <main className="flex min-w-0 flex-1 flex-col bg-background" />
      </div>
    </div>
  );
}

/** List-only skeleton, no toolbar; for use when the toolbar is already mounted. */
export function IssueListSkeleton() {
  return (
    <ul className="divide-y divide-border/60">
      {Array.from({ length: 6 }).map((_, i) => (
        <li className="flex items-baseline gap-3 px-2 py-3" key={i}>
          <Skeleton className="size-4 rounded-full" />
          <Skeleton className="h-4 flex-1" />
          <Skeleton className="h-3 w-14" />
          <Skeleton className="h-3 w-10" />
        </li>
      ))}
    </ul>
  );
}

export function IssuesViewSkeleton() {
  return (
    <div className="min-h-0 flex-1 overflow-hidden">
      <div className="mx-auto flex max-w-5xl items-center justify-between px-6 pt-3 pb-2">
        <Skeleton className="h-9 w-72 rounded-full" />
        <Skeleton className="h-4 w-32" />
      </div>
      <ul className="mx-auto max-w-5xl divide-y divide-border/60 px-6">
        {Array.from({ length: 6 }).map((_, i) => (
          <li className="flex items-baseline gap-3 px-2 py-3" key={i}>
            <Skeleton className="size-4 rounded-full" />
            <Skeleton className="h-4 flex-1" />
            <Skeleton className="h-3 w-14" />
            <Skeleton className="h-3 w-10" />
          </li>
        ))}
      </ul>
    </div>
  );
}

export function IssueDetailSkeleton() {
  return (
    <div className="grid h-full min-h-0 flex-1 grid-cols-1 lg:grid-cols-[minmax(0,1fr)_18rem]">
      <div className="flex min-h-0 flex-col p-6">
        <div className="flex items-center gap-2">
          <Skeleton className="h-3 w-16" />
          <Skeleton className="h-3 w-20" />
        </div>
        <Skeleton className="mt-3 h-5 w-2/3" />
        <Skeleton className="mt-4 h-3 w-full max-w-xl" />
        <Skeleton className="mt-2 h-3 w-5/6 max-w-xl" />
        <Skeleton className="mt-2 h-3 w-3/4 max-w-xl" />
        <Skeleton className="mt-8 h-3 w-16" />
        <div className="mt-3 flex flex-col gap-4">
          {Array.from({ length: 3 }).map((_, i) => (
            <div className="flex flex-col gap-2" key={i}>
              <div className="flex items-center gap-2">
                <Skeleton className="size-5 rounded-full" />
                <Skeleton className="h-3 w-24" />
              </div>
              <Skeleton className="ml-7 h-3 w-full max-w-md" />
              <Skeleton className="ml-7 h-3 w-2/3 max-w-md" />
            </div>
          ))}
        </div>
      </div>
      <aside className="hidden flex-col gap-4 border-l border-border/60 p-6 lg:flex">
        <Skeleton className="h-3 w-12" />
        <Skeleton className="h-5 w-28" />
        <Skeleton className="h-3 w-12" />
        <Skeleton className="h-5 w-24" />
        <Skeleton className="h-3 w-16" />
        <Skeleton className="h-5 w-32" />
      </aside>
    </div>
  );
}

export function ProjectDetailsSkeleton() {
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center justify-between px-6 pt-3 pb-2">
        <Skeleton className="h-9 w-64 rounded-full" />
      </div>
      <div className="grid gap-x-10 gap-y-8 p-6 lg:grid-cols-2">
        <div className="flex flex-col gap-2">
          <Skeleton className="h-3 w-16" />
          <Skeleton className="h-4 w-40" />
          <Skeleton className="mt-4 h-3 w-full max-w-lg" />
          <Skeleton className="mt-2 h-3 w-3/4 max-w-lg" />
        </div>
        <div className="flex flex-col gap-2">
          <Skeleton className="h-3 w-16" />
          <Skeleton className="mt-2 h-4 w-full" />
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-4 w-full" />
        </div>
      </div>
    </div>
  );
}

export function NotificationsViewSkeleton() {
  return (
    <div className="mx-auto max-w-2xl p-6">
      <div className="flex items-center justify-between pb-3">
        <Skeleton className="h-9 w-44 rounded-full" />
        <Skeleton className="h-7 w-24 rounded-md" />
      </div>
      <ul className="divide-y divide-border/60">
        {Array.from({ length: 6 }).map((_, i) => (
          <li className="flex items-baseline gap-3 py-3" key={i}>
            <Skeleton className="size-1.5 rounded-full" />
            <div className="flex flex-1 flex-col gap-1.5">
              <Skeleton className="h-3 w-4/5" />
              <Skeleton className="h-3 w-2/3" />
            </div>
            <Skeleton className="h-3 w-10" />
          </li>
        ))}
      </ul>
    </div>
  );
}

export function ActivitySkeleton() {
  return (
    <div className="px-5 py-4">
      <Skeleton className="h-3 w-16" />
      <div className="mt-3 flex flex-col gap-4">
        {Array.from({ length: 3 }).map((_, i) => (
          <div className="flex flex-col gap-2" key={i}>
            <div className="flex items-center gap-2">
              <Skeleton className="size-5 rounded-full" />
              <Skeleton className="h-3 w-24" />
            </div>
            <Skeleton className="ml-7 h-3 w-full max-w-md" />
          </div>
        ))}
      </div>
    </div>
  );
}
