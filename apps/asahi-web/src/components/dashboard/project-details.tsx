import { useState, type ReactNode } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import {
  IconCalendar,
  IconCircleDashed,
  IconClock,
  IconFolder,
  IconPlus,
  IconTrash,
} from "@tabler/icons-react";
import { useLocation } from "wouter";

import {
  deleteProject,
  fetchIssues,
  fetchProjects,
  updateProject,
  updateProjectState,
  type Project,
} from "@/api/asahi";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { CreateIssueTrigger } from "./create-issue-trigger";
import { EditablePriority, EditableStatus } from "./editable-fields";
import { EmptyDetails, IssueList } from "./issue-list";
import { IssueDetails } from "./issue-details";
import { ProjectWiki } from "./project-wiki";

const PROJECT_STATES = ["Backlog", "Todo", "In Progress", "Done"];
const PRIORITY_OPTIONS = [null, 1, 2, 3] as const;
const PROJECT_SECTIONS = ["overview", "issues", "wiki"] as const;

type ProjectSection = (typeof PROJECT_SECTIONS)[number];

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
      <div className="flex min-h-0 flex-1 items-center justify-center px-6 text-center">
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
  onSelectIssue: _onSelectIssue,
  project,
}: {
  onSelectIssue: (issueId: string) => void;
  project: Project;
}) {
  const queryClient = useQueryClient();
  const [, navigate] = useLocation();
  const [section, setSection] = useState<ProjectSection>("overview");
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [statusOpen, setStatusOpen] = useState(false);
  const [priorityOpen, setPriorityOpen] = useState(false);
  const { data } = useSuspenseQuery({
    queryKey: ["issues", "project", project.id],
    queryFn: () => fetchIssues({ projectId: project.id }),
  });
  const [selectedIssueId, setSelectedIssueId] = useState<string | null>(data.issues[0]?.id ?? null);

  const deleteMutation = useMutation({
    mutationFn: () => deleteProject(project.id),
    onSuccess: () => {
      navigate("/issues");
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
    },
  });

  const stateMutation = useMutation({
    mutationFn: (state: string) => updateProjectState(project.id, state),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
    },
  });

  const priorityMutation = useMutation({
    mutationFn: (priority: number | null) => updateProject(project.id, { priority }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
    },
  });

  const issueCountLabel = formatCount(data.issues.length, "issue");
  const selectedIssue = data.issues.find((i) => i.id === selectedIssueId);

  return (
    <section className="flex min-h-0 flex-1 flex-col overflow-hidden">
      {/* Segment control */}
      <div className="z-10 shrink-0 border-b border-[#eceae5] bg-background/95 px-5 py-2 backdrop-blur supports-[backdrop-filter]:bg-background/80">
        <div className="flex min-h-8 items-center justify-between gap-3">
          <div className="inline-flex rounded-full border border-border bg-muted/60 p-0.5">
            {PROJECT_SECTIONS.map((option) => (
              <button
                className={cn(
                  "inline-flex h-7 items-center gap-1.5 rounded-full px-3 text-xs font-medium capitalize text-muted-foreground",
                  section === option && "bg-background text-foreground shadow-sm",
                )}
                key={option}
                onClick={() => setSection(option)}
                type="button"
              >
                <span>{option}</span>
                {option === "issues" ? (
                  <span
                    className={cn(
                      "rounded-full px-1.5 text-[0.625rem] leading-4 text-[#8f8b82]",
                      section === option && "bg-muted text-[#55524b]",
                    )}
                  >
                    {data.issues.length}
                  </span>
                ) : null}
              </button>
            ))}
          </div>

          {section !== "wiki" ? (
            <CreateIssueTrigger projectId={project.id}>
              <Button aria-label="Create issue in project" size="icon-xs" variant="ghost">
                <IconPlus className="size-3.5" />
              </Button>
            </CreateIssueTrigger>
          ) : null}
        </div>
      </div>

      {/* Overview */}
      {section === "overview" && (
        <div className="grid min-h-0 flex-1 overflow-auto lg:grid-cols-[minmax(0,1fr)_18.5rem] lg:overflow-hidden">
          <div className="min-h-0 min-w-0 lg:overflow-auto">
            <div className="border-b border-[#eceae5] px-5 pb-6 pt-5">
              <div className="mb-4 flex items-start justify-between gap-3">
                <div className="flex min-w-0 items-center gap-2 text-xs text-[#77746c]">
                  <IconFolder className="size-3.5 shrink-0" stroke={1.8} />
                  <span className="truncate">{project.slug}</span>
                  <span className="h-1 w-1 shrink-0 rounded-full bg-[#c9c4bb]" />
                  <span className="shrink-0">{project.state}</span>
                </div>
                <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
                  <Button
                    aria-label="Delete project"
                    className="text-[#8a877e] hover:bg-destructive/10 hover:text-destructive focus-visible:border-destructive/40 focus-visible:ring-destructive/20"
                    disabled={deleteMutation.isPending}
                    onClick={() => setDeleteOpen(true)}
                    size="icon-xs"
                    type="button"
                    variant="ghost"
                  >
                    <IconTrash className="size-3.5" />
                  </Button>
                  <AlertDialogContent>
                    <AlertDialogHeader>
                      <AlertDialogTitle>Delete {project.name}?</AlertDialogTitle>
                      <AlertDialogDescription>
                        This action cannot be undone. Issues in this project will be moved out of
                        the project.
                      </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                      <AlertDialogCancel onClick={() => setDeleteOpen(false)}>
                        Cancel
                      </AlertDialogCancel>
                      <AlertDialogAction
                        disabled={deleteMutation.isPending}
                        onClick={() => deleteMutation.mutate()}
                        variant="destructive"
                      >
                        Delete
                      </AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
              </div>

              <h2 className="max-w-3xl text-[1.45rem] font-semibold leading-tight text-[#22211f]">
                {project.name}
              </h2>

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
          </div>

          <aside className="border-t border-[#eceae5] bg-background px-5 py-3 lg:min-h-0 lg:overflow-auto lg:border-l lg:border-t-0">
            <div className="grid gap-1 sm:grid-cols-2 lg:grid-cols-1">
              <PropertyRow label="Status">
                <EditableStatus
                  disabled={stateMutation.isPending}
                  onChange={(state) => {
                    stateMutation.mutate(state);
                    setStatusOpen(false);
                  }}
                  open={statusOpen}
                  options={PROJECT_STATES}
                  setOpen={setStatusOpen}
                  state={project.state}
                />
              </PropertyRow>
              <PropertyRow label="Priority">
                <EditablePriority
                  disabled={priorityMutation.isPending}
                  onChange={(priority) => {
                    priorityMutation.mutate(priority);
                    setPriorityOpen(false);
                  }}
                  open={priorityOpen}
                  options={[...PRIORITY_OPTIONS]}
                  priority={project.priority}
                  setOpen={setPriorityOpen}
                />
              </PropertyRow>
              <PropertyRow label="Issues">
                <span className="text-xs font-medium text-[#55524b]">{issueCountLabel}</span>
              </PropertyRow>
              <PropertyRow label="Updated">
                <span className="inline-flex items-center gap-1.5 text-xs font-medium text-[#55524b]">
                  <IconClock className="size-3.5 text-[#8a877e]" stroke={1.8} />
                  {formatDate(project.updated_at)}
                </span>
              </PropertyRow>
              <PropertyRow label="Created">
                <span className="inline-flex items-center gap-1.5 text-xs font-medium text-[#55524b]">
                  <IconCalendar className="size-3.5 text-[#8a877e]" stroke={1.8} />
                  {formatDate(project.created_at)}
                </span>
              </PropertyRow>
            </div>
          </aside>
        </div>
      )}

      {/* Issues: list + detail preview */}
      {section === "issues" && (
        <div className="grid min-h-0 flex-1 overflow-auto lg:grid-cols-[minmax(15rem,20rem)_minmax(0,1fr)] lg:overflow-hidden">
          <div className="min-h-0 border-r border-[#eceae5] lg:overflow-auto">
            <IssueList
              issues={data.issues}
              onSelect={(id) => {
                setSelectedIssueId(id);
              }}
              selectedId={selectedIssueId}
            />
          </div>
          <div className="min-h-0 lg:overflow-hidden">
            {selectedIssue ? <IssueDetails issue={selectedIssue} /> : <EmptyDetails />}
          </div>
        </div>
      )}

      {/* Wiki: file tree + preview */}
      {section === "wiki" && <ProjectWiki project={project} />}
    </section>
  );
}

function PropertyRow({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="grid min-h-9 grid-cols-[5.5rem_minmax(0,1fr)] items-center gap-3">
      <div className="text-xs text-[#85827a]">{label}</div>
      <div className="flex min-w-0 justify-end text-right text-[#33312d]">{children}</div>
    </div>
  );
}

function formatCount(count: number, singular: string) {
  return count === 1 ? `1 ${singular}` : `${count} ${singular}s`;
}

function formatDate(value: string | null) {
  if (!value) return "No update";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(new Date(value));
}
