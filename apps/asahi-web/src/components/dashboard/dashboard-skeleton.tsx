export function DashboardSkeleton() {
  return (
    <div className="grid min-h-svh bg-background lg:grid-cols-[248px_minmax(0,1fr)]">
      <div className="hidden border-r border-sidebar-border bg-sidebar lg:block" />
      <div>
        <div className="h-14 border-b border-border" />
        <div className="space-y-3 p-4">
          {Array.from({ length: 8 }).map((_, index) => (
            <div className="h-14 animate-pulse rounded-md bg-muted" key={index} />
          ))}
        </div>
      </div>
    </div>
  );
}
