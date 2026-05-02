import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuSkeleton,
} from "@/components/ui/sidebar";

export function SidebarSkeleton() {
  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <div className="flex h-10 items-center px-2">
              <div className="h-5 w-20 animate-pulse rounded bg-sidebar-accent" />
            </div>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarMenuSkeleton showIcon />
              <SidebarMenuSkeleton showIcon />
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
        <SidebarGroup>
          <SidebarGroupLabel>Projects</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarMenuSkeleton showIcon />
              <SidebarMenuSkeleton showIcon />
              <SidebarMenuSkeleton showIcon />
              <SidebarMenuSkeleton showIcon />
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
    </Sidebar>
  );
}

export function IssuesViewSkeleton() {
  return (
    <section className="min-h-0 flex-1 overflow-auto">
      <div className="flex h-12 items-center justify-between px-4">
        <div className="flex items-center gap-1 rounded-full border border-border bg-muted/60 p-0.5">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-7 w-16 animate-pulse rounded-full bg-muted" />
          ))}
        </div>
      </div>
      <div className="space-y-3 p-4">
        {Array.from({ length: 8 }).map((_, index) => (
          <div className="h-14 animate-pulse rounded-md bg-muted" key={index} />
        ))}
      </div>
    </section>
  );
}

export function IssueDetailSkeleton() {
  return (
    <section className="grid h-full min-h-0 flex-1 overflow-auto lg:grid-cols-[minmax(0,1fr)_18.5rem] lg:overflow-hidden">
      <div className="flex min-h-0 flex-col space-y-4 p-5">
        <div className="h-4 w-32 animate-pulse rounded bg-[#eceae5]" />
        <div className="h-8 w-3/4 animate-pulse rounded bg-[#eceae5]" />
        <div className="h-20 w-full animate-pulse rounded bg-[#eceae5]" />
        <div className="min-h-0 flex-1 space-y-3 pt-4">
          <div className="h-5 w-16 animate-pulse rounded bg-[#eceae5]" />
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="h-16 animate-pulse rounded-md bg-[#f2f1ec]" />
          ))}
        </div>
      </div>
      <aside className="border-t border-[#eceae5] bg-background px-5 py-3 lg:min-h-0 lg:overflow-auto lg:border-l lg:border-t-0">
        {Array.from({ length: 3 }).map((_, i) => (
          <div key={i} className="grid min-h-9 grid-cols-[5.5rem_minmax(0,1fr)] items-center gap-3">
            <div className="h-4 w-12 animate-pulse rounded bg-[#eceae5]" />
            <div className="ml-auto h-7 w-32 animate-pulse rounded bg-[#eceae5]" />
          </div>
        ))}
      </aside>
    </section>
  );
}

export function ProjectDetailsSkeleton() {
  return (
    <section className="grid min-h-0 flex-1 overflow-auto lg:grid-cols-[minmax(0,1fr)_18.5rem] lg:overflow-hidden">
      <div className="min-h-0 min-w-0 lg:overflow-auto">
        <div className="border-b border-[#eceae5] px-5 pb-6 pt-5">
          <div className="mb-4 flex items-center gap-2">
            <div className="h-4 w-4 animate-pulse rounded bg-[#eceae5]" />
            <div className="h-4 w-24 animate-pulse rounded bg-[#eceae5]" />
            <div className="h-1 w-1 rounded-full bg-[#eceae5]" />
            <div className="h-4 w-16 animate-pulse rounded bg-[#eceae5]" />
          </div>
          <div className="h-8 w-3/4 animate-pulse rounded bg-[#eceae5]" />
          <div className="mt-3 h-16 w-full max-w-3xl animate-pulse rounded bg-[#eceae5]" />
        </div>
        <div className="border-b border-[#eceae5] px-5 py-2">
          <div className="flex min-h-8 items-center justify-between gap-3">
            <div className="h-8 w-36 animate-pulse rounded-full bg-[#eceae5]" />
            <div className="h-6 w-6 animate-pulse rounded-full bg-[#eceae5]" />
          </div>
        </div>
        <div className="flex h-12 items-center justify-between px-5">
          <div className="h-5 w-24 animate-pulse rounded bg-[#eceae5]" />
          <div className="h-4 w-20 animate-pulse rounded bg-[#eceae5]" />
        </div>
        <div className="space-y-3 px-5">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-14 animate-pulse rounded-md bg-muted" />
          ))}
        </div>
      </div>
      <aside className="border-t border-[#eceae5] bg-background px-5 py-3 lg:min-h-0 lg:overflow-auto lg:border-l lg:border-t-0">
        <div className="grid gap-1 sm:grid-cols-2 lg:grid-cols-1">
          {Array.from({ length: 5 }).map((_, i) => (
            <div
              key={i}
              className="grid min-h-9 grid-cols-[5.5rem_minmax(0,1fr)] items-center gap-3"
            >
              <div className="h-4 w-12 animate-pulse rounded bg-[#eceae5]" />
              <div className="ml-auto h-6 w-24 animate-pulse rounded bg-[#eceae5]" />
            </div>
          ))}
        </div>
      </aside>
    </section>
  );
}

export function NotificationsViewSkeleton() {
  return (
    <section className="grid min-h-0 flex-1 overflow-auto xl:grid-cols-[minmax(15rem,20rem)_minmax(0,1fr)] xl:overflow-hidden">
      <div className="flex min-h-0 min-w-0 flex-col border-r border-border">
        <div className="flex h-12 shrink-0 items-center justify-between px-4">
          <div className="h-4 w-20 animate-pulse rounded bg-muted" />
        </div>
        <div className="space-y-3 p-4 xl:min-h-0 xl:flex-1 xl:overflow-auto">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="h-14 animate-pulse rounded-md bg-muted" />
          ))}
        </div>
      </div>
      <aside className="min-w-0 bg-card xl:min-h-0 xl:overflow-hidden">
        <div className="flex h-full items-center justify-center">
          <div className="h-8 w-32 animate-pulse rounded bg-muted" />
        </div>
      </aside>
    </section>
  );
}

export function ActivitySkeleton() {
  return (
    <div className="space-y-3 px-5 py-4">
      <div className="mb-3 h-5 w-16 animate-pulse rounded bg-[#eceae5]" />
      {Array.from({ length: 4 }).map((_, i) => (
        <div key={i} className="h-16 animate-pulse rounded-md bg-[#f2f1ec]" />
      ))}
    </div>
  );
}
