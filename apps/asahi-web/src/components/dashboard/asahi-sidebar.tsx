import { IconBell, IconCircleDot } from "@tabler/icons-react";

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
            <SidebarMenuButton className="h-10 justify-start px-2" size="lg" tooltip="Asahi">
              <span className="truncate text-[1.0625rem] font-semibold leading-none text-foreground">
                Asahi
              </span>
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
