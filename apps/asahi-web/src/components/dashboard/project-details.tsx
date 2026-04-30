import { type ReactNode } from "react";
import { useSuspenseQuery } from "@tanstack/react-query";
import { IconCircleDashed, IconFolder } from "@tabler/icons-react";

import { fetchIssues, fetchProjects, type Project } from "@/api/asahi";

import { Priority, StatusBadge } from "./issue-badges";
import { IssueList } from "./issue-list";

export function ProjectDetails({
  locator,
  onSelectIssue,
}: {
  locator: string;
  onSelectIssue: (issueId: string) => void;
}) {
  const { data } = useSuspenseQuery({
    queryKey: ["projects"],
    queryFn: () => fetchProjects(),
  });

  const project = data.projects.find(
    (candidate) => candidate.id === locator || candidate.slug === locator,
  );

  if (!project) {
    return (
      <div className="flex min-h-[calc(100svh-3.5rem)] items-center justify-center px-6 text-center">
        <div>
          <IconCircleDashed className="mx-auto mb-3 size-8 text-[#b4b0a7]" stroke={1.5} />
          <div className="text-sm font-medium">Project not found</div>
          <div className="mt-1 text-sm text-[#77746c]">Choose a project from the sidebar.</div>
        </div>
      </div>
    );
  }

  return <ProjectPage project={project} onSelectIssue={onSelectIssue} />;
}

function ProjectPage({
  onSelectIssue,
  project,
}: {
  onSelectIssue: (issueId: string) => void;
  project: Project;
}) {
  const { data } = useSuspenseQuery({
    queryKey: ["issues", "project", project.id],
    queryFn: () => fetchIssues({ projectId: project.id }),
  });

  return (
    <section className="min-h-0 flex-1 overflow-auto">
      <div className="px-5 pb-5 pt-5">
        <div className="mb-3 flex items-center gap-2 text-xs text-[#77746c]">
          <IconFolder className="size-3.5" stroke={1.8} />
          <span>{project.slug}</span>
          <span className="h-1 w-1 rounded-full bg-[#c9c4bb]" />
          <span>{formatDate(project.updated_at)}</span>
        </div>

        <h2 className="text-lg font-semibold leading-snug text-[#22211f]">{project.name}</h2>

        <div className="mt-3 max-w-3xl">
          {project.description ? (
            <p className="whitespace-pre-wrap text-sm leading-6 text-[#69665f]">
              {project.description}
            </p>
          ) : (
            <p className="text-sm italic text-[#a8a59d]">No description</p>
          )}
        </div>
      </div>

      <div className="border-y border-[#eceae5] px-5 py-2">
        <PropertyRow label="Status">
          <StatusBadge state={project.state} />
        </PropertyRow>
        <PropertyRow label="Priority">
          <Priority priority={project.priority} />
        </PropertyRow>
      </div>

      <div>
        <div className="flex h-12 items-center justify-between px-5">
          <div className="text-sm font-medium">Issues</div>
          <div className="text-xs text-[#8f8b82]">
            {data.issues.length === 1 ? "1 issue" : `${data.issues.length} issues`}
          </div>
        </div>

        {data.issues.length ? (
          <IssueList issues={data.issues} onSelect={onSelectIssue} selectedId={null} />
        ) : (
          <div className="flex h-[260px] items-center justify-center px-6 text-center">
            <div>
              <IconCircleDashed className="mx-auto mb-3 size-8 text-[#b4b0a7]" stroke={1.5} />
              <div className="text-sm font-medium">No issues in this project</div>
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

function PropertyRow({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="grid min-h-9 grid-cols-[6rem_minmax(0,1fr)] items-center gap-3">
      <div className="text-xs text-[#85827a]">{label}</div>
      <div className="flex min-w-0 justify-end text-right text-[#33312d]">{children}</div>
    </div>
  );
}

function formatDate(value: string | null) {
  if (!value) return "No update";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(new Date(value));
}
