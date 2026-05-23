import {
  createContext,
  use,
  useEffect,
  useState,
  type ReactNode,
} from "react";
import { useSuspenseQuery } from "@tanstack/react-query";
import { Bell, CircleDot, FolderClosed, PanelLeft, Plus, Search, Settings } from "lucide-react";
import { Link, useLocation } from "wouter";

import { fetchProjects, type Project } from "@/api/asahi";
import { cn } from "@/lib/utils";

import { ProjectComposer } from "./project-composer";
import { SettingsDialog } from "./settings-dialog";

const SIDEBAR_STORAGE_KEY = "asahi:sidebar-collapsed";

type SidebarContextValue = {
  collapsed: boolean;
  setCollapsed: (next: boolean) => void;
};

const SidebarContext = createContext<SidebarContextValue | null>(null);

function useSidebarContext(): SidebarContextValue {
  const ctx = use(SidebarContext);
  if (!ctx) {
    throw new Error("Sidebar context missing — wrap with AppShell");
  }
  return ctx;
}

export type View = "issues" | "notifications" | "project";

type NavItem = {
  href: string;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  match: (location: string) => boolean;
};

const primary: NavItem[] = [
  {
    href: "/inbox",
    label: "Inbox",
    icon: Bell,
    match: (l) => l.startsWith("/inbox"),
  },
  {
    href: "/issues",
    label: "Issues",
    icon: CircleDot,
    match: (l) => l === "/" || l === "/issues" || l.startsWith("/issues"),
  },
];

export function AppShell({ children }: { children: ReactNode }) {
  const [collapsed, setCollapsedState] = useState<boolean>(() => {
    if (typeof window === "undefined") return false;
    return window.localStorage.getItem(SIDEBAR_STORAGE_KEY) === "1";
  });

  const setCollapsed = (next: boolean) => {
    setCollapsedState(next);
    try {
      window.localStorage.setItem(SIDEBAR_STORAGE_KEY, next ? "1" : "0");
    } catch {
      // ignore (private mode, quota etc.)
    }
  };

  // Cmd/Ctrl + \ to toggle, matching Linear / Cursor convention.
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "\\") {
        event.preventDefault();
        setCollapsed(!collapsed);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [collapsed]);

  return (
    <SidebarContext.Provider value={{ collapsed, setCollapsed }}>
      <div className="min-h-svh bg-background text-foreground">
        <div
          className={cn(
            "flex min-h-svh",
            collapsed ? "md:pl-0" : "md:pl-64",
            "[transition:padding-left_220ms_var(--ease-out-strong)]",
          )}
        >
          <AsahiSidebar />
          <main className="relative flex min-w-0 flex-1 flex-col bg-background">{children}</main>
        </div>
      </div>
    </SidebarContext.Provider>
  );
}


export function AsahiSidebar() {
  const [location] = useLocation();
  const { collapsed, setCollapsed } = useSidebarContext();
  const { data } = useSuspenseQuery({
    queryKey: ["projects"],
    queryFn: () => fetchProjects(),
  });
  const [composerOpen, setComposerOpen] = useState(false);

  return (
    <aside
      aria-hidden={collapsed}
      className={cn(
        "asahi-sidebar fixed inset-y-0 left-0 z-30 hidden w-64 flex-col bg-muted/50 md:flex",
        "[transition:transform_220ms_var(--ease-out-strong),opacity_180ms_var(--ease-out-strong)]",
        collapsed
          ? "pointer-events-none -translate-x-full opacity-0"
          : "translate-x-0 opacity-100",
      )}
    >
      <div className="flex h-12 items-center gap-2 px-4 text-muted-foreground">
        <button
          aria-label={collapsed ? "Show sidebar" : "Hide sidebar"}
          className="asahi-press rounded-md p-1 hover:bg-background/60 hover:text-foreground"
          onClick={() => setCollapsed(!collapsed)}
          title={`Toggle sidebar (⌘\\)`}
          type="button"
        >
          <PanelLeft className="size-4" />
        </button>
        <button
          aria-label="Search"
          className="asahi-press rounded-md p-1 hover:bg-background/60 hover:text-foreground"
          type="button"
        >
          <Search className="size-4" />
        </button>
      </div>

      <div className="px-5 pb-5 pt-1">
        <span className="text-[15px] font-medium tracking-tight text-foreground">Asahi</span>
      </div>

      <nav className="flex flex-col gap-0.5 px-3">
      {primary.map((item) => {
          const Icon = item.icon;
          const active = item.match(location);
          return (
            <Link
              className={cn(
                "asahi-press relative flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13.5px] [transition:background-color_180ms_var(--ease-out-strong),color_180ms_var(--ease-out-strong)]",
                active
                  ? "bg-input font-medium text-foreground"
                  : "text-muted-foreground hover:bg-background/60 hover:text-foreground",
              )}
              aria-current={active ? "page" : undefined}
              href={item.href}
              key={item.href}
            >
              <Icon className="size-4" />
              {item.label}
            </Link>
          );
        })}
      </nav>

      <div className="mt-7 flex items-center justify-between px-5 text-[11px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
        <span>Projects</span>
        <button
          aria-label="New project"
          className="asahi-press rounded-md p-1 hover:bg-background/60 hover:text-foreground"
          onClick={() => setComposerOpen(true)}
          type="button"
        >
          <Plus className="size-3.5" />
        </button>
      </div>
      <nav className="mt-1 flex flex-col gap-0.5 px-3">
        {data.projects.map((project) => (
          <ProjectLink key={project.id} location={location} project={project} />
        ))}
      </nav>

      <div className="mt-auto px-3 pb-4">
        <SettingsButton />
      </div>

      {composerOpen ? (
        <ProjectComposer onClose={() => setComposerOpen(false)} onCreated={() => setComposerOpen(false)} />
      ) : null}
    </aside>
  );
}

function ProjectLink({ location, project }: { location: string; project: Project }) {
  const href = `/projects/${encodeURIComponent(project.slug)}`;
  const active = location === href || location.startsWith(href + "/");
  return (
    <Link
      className={cn(
        "asahi-press relative flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13.5px] [transition:background-color_180ms_var(--ease-out-strong),color_180ms_var(--ease-out-strong)]",
        active
          ? "bg-input font-medium text-foreground"
          : "text-muted-foreground hover:bg-background/60 hover:text-foreground",
      )}
      aria-current={active ? "page" : undefined}
      href={href}
    >
      <FolderClosed className="size-4" />
      <span className="truncate">{project.name}</span>
    </Link>
  );
}

function SettingsButton() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button
        aria-label="Open settings"
        className="asahi-press flex w-full items-center gap-2.5 rounded-md px-2.5 py-1.5 text-[13.5px] text-muted-foreground [transition:background-color_180ms_var(--ease-out-strong),color_180ms_var(--ease-out-strong)] hover:bg-background/60 hover:text-foreground"
        onClick={() => setOpen(true)}
        type="button"
      >
        <Settings className="size-4" />
        Settings
      </button>
      <SettingsDialog onClose={() => setOpen(false)} open={open} />
    </>
  );
}

export function PageTopbar({
  back,
  right,
  title,
}: {
  back?: { href: string; label: string };
  right?: ReactNode;
  title: ReactNode;
}) {
  const { collapsed, setCollapsed } = useSidebarContext();
  return (
    <div className="flex h-14 shrink-0 items-center justify-between gap-4 px-6">
      <div className="flex min-w-0 items-center gap-3 text-[13.5px] font-medium">
        {collapsed ? (
          <button
            aria-label="Show sidebar"
            className="asahi-press hidden size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground [transition:background-color_180ms_var(--ease-out-strong),color_180ms_var(--ease-out-strong)] hover:bg-muted hover:text-foreground md:inline-flex"
            onClick={() => setCollapsed(false)}
            title={`Show sidebar (⌘\\)`}
            type="button"
          >
            <PanelLeft className="size-4" />
          </button>
        ) : null}
        {back ? (
          <Link
            className="flex items-center gap-1 text-[13px] text-muted-foreground hover:text-foreground"
            href={back.href}
          >
            <span aria-hidden>‹</span>
            {back.label}
          </Link>
        ) : (
          <span className="truncate text-foreground">{title}</span>
        )}
      </div>
      <div className="flex items-center gap-3">{right}</div>
    </div>
  );
}
