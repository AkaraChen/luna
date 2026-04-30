import { useState } from "react";
import { useSuspenseQuery } from "@tanstack/react-query";
import { IconBell, IconCircleDot, IconFolder, IconPlus } from "@tabler/icons-react";

import { fetchProjects } from "@/api/asahi";
import {
  Sidebar,
  SidebarContent,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
} from "@/components/ui/sidebar";
import { cn } from "@/lib/utils";

import { ProjectComposer } from "./project-composer";

export type View = "issues" | "notifications" | "project";

export function AsahiSidebar({
  activeProjectLocator,
  onProjectSelect,
  view,
  onViewChange,
}: {
  activeProjectLocator: string | null;
  onProjectSelect: (projectLocator: string) => void;
  view: View;
  onViewChange: (view: View) => void;
}) {
  const [composerOpen, setComposerOpen] = useState(false);
  const { data } = useSuspenseQuery({
    queryKey: ["projects"],
    queryFn: () => fetchProjects(),
  });

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

        <SidebarGroup>
          <SidebarGroupLabel>Projects</SidebarGroupLabel>
          <SidebarGroupAction
            aria-label="Create project"
            onClick={() => setComposerOpen(true)}
            title="Create project"
            type="button"
          >
            <IconPlus />
          </SidebarGroupAction>
          <SidebarGroupContent>
            <SidebarMenu>
              {data.projects.length ? (
                data.projects.map((project) => (
                  <SidebarItem
                    active={
                      view === "project" &&
                      (activeProjectLocator === project.id || activeProjectLocator === project.slug)
                    }
                    icon={IconFolder}
                    key={project.id}
                    label={project.name}
                    onClick={() => onProjectSelect(project.slug)}
                  />
                ))
              ) : (
                <div className="px-3 py-2 text-xs text-sidebar-foreground/60 group-data-[collapsible=icon]:hidden">
                  No projects
                </div>
              )}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>

      <SidebarRail />
      {composerOpen ? (
        <ProjectComposer
          onClose={() => setComposerOpen(false)}
          onCreated={(project) => onProjectSelect(project.slug)}
        />
      ) : null}
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
