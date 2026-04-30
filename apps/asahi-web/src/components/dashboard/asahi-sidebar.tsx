import {
  IconCircleCheck,
  IconCircleDashed,
  IconCircleDot,
  IconClockHour4,
  IconSparkles,
} from "@tabler/icons-react";

import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  SidebarSeparator,
} from "@/components/ui/sidebar";
import { cn } from "@/lib/utils";

import { statusColumns, type StatusFilter } from "./constants";

export function AsahiSidebar({
  counts,
  onStatusFilterChange,
  statusFilter,
  totalIssues,
}: {
  counts: Map<string, number>;
  onStatusFilterChange: (status: StatusFilter) => void;
  statusFilter: StatusFilter;
  totalIssues: number;
}) {
  return (
    <Sidebar collapsible="icon" variant="inset">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton className="gap-2.5" size="lg" tooltip="Asahi">
              <div className="flex size-8 items-center justify-center rounded-xl bg-primary text-primary-foreground">
                <IconSparkles className="size-4" stroke={1.8} />
              </div>
              <div className="grid min-w-0 flex-1 text-left leading-tight">
                <span className="truncate text-sm font-semibold">Asahi</span>
                <span className="truncate text-xs text-muted-foreground">Task workspace</span>
              </div>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupContent>
            <SidebarMenu>
              <SidebarItem
                active={statusFilter === "all"}
                count={totalIssues}
                icon={IconCircleDot}
                label="Issues"
                onClick={() => onStatusFilterChange("all")}
              />
              <SidebarItem
                active={statusFilter === "In Progress"}
                count={counts.get("In Progress")}
                icon={IconClockHour4}
                label="Active"
                onClick={() => onStatusFilterChange("In Progress")}
              />
              <SidebarItem
                active={statusFilter === "Todo"}
                count={counts.get("Todo")}
                icon={IconCircleDashed}
                label="Backlog"
                onClick={() => onStatusFilterChange("Todo")}
              />
              <SidebarItem
                active={statusFilter === "Done"}
                count={counts.get("Done")}
                icon={IconCircleCheck}
                label="Completed"
                onClick={() => onStatusFilterChange("Done")}
              />
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>

        <SidebarSeparator />

        <SidebarGroup>
          <SidebarGroupLabel>Views</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {statusColumns.map((status) => (
                <SidebarItem
                  active={statusFilter === status}
                  count={counts.get(status) ?? 0}
                  icon={
                    status === "Done"
                      ? IconCircleCheck
                      : status === "In Progress"
                        ? IconCircleDot
                        : IconCircleDashed
                  }
                  key={status}
                  label={status}
                  onClick={() => onStatusFilterChange(status as StatusFilter)}
                />
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarRail />
    </Sidebar>
  );
}

function SidebarItem({
  active,
  count,
  icon: Icon,
  label,
  onClick,
}: {
  active?: boolean;
  count?: number;
  icon: typeof IconCircleDot;
  label: string;
  onClick: () => void;
}) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton isActive={active} onClick={onClick} tooltip={label}>
        <Icon className={cn(active && "text-primary")} stroke={1.8} />
        <span>{label}</span>
      </SidebarMenuButton>
      {typeof count === "number" ? <SidebarMenuBadge>{count}</SidebarMenuBadge> : null}
    </SidebarMenuItem>
  );
}
