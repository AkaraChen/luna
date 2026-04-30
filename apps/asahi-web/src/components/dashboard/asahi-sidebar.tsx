import { IconBell, IconCircleDot, IconSparkles } from "@tabler/icons-react";

import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
} from "@/components/ui/sidebar";
import { cn } from "@/lib/utils";

export type View = "issues" | "notifications";

export function AsahiSidebar({
  view,
  onViewChange,
}: {
  view: View;
  onViewChange: (view: View) => void;
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
                active={view === "notifications"}
                icon={IconBell}
                label="Inbox"
                onClick={() => onViewChange("notifications")}
              />
              <SidebarItem
                active={view === "issues"}
                icon={IconCircleDot}
                label="Issues"
                onClick={() => onViewChange("issues")}
              />
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
  icon: Icon,
  label,
  onClick,
}: {
  active?: boolean;
  icon: typeof IconBell;
  label: string;
  onClick: () => void;
}) {
  return (
    <SidebarMenuItem>
      <SidebarMenuButton isActive={active} onClick={onClick} tooltip={label}>
        <Icon className={cn(active && "text-primary")} stroke={1.8} />
        <span>{label}</span>
      </SidebarMenuButton>
    </SidebarMenuItem>
  );
}
