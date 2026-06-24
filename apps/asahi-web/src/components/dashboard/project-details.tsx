import { useMemo, useState, type ReactNode } from "react";
import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { Calendar, CircleDashed, Clock, FileText, FolderClosed, Plus, Trash2 } from "lucide-react";
import { useLocation } from "wouter";
import { parseAsStringLiteral, useQueryState } from "nuqs";

import {
  deleteProject,
  fetchIssues,
  fetchProjects,
  fetchWikiNodes,
  updateProject,
  updateProjectState,
  type Issue,
  type Project,
  type WikiNode,
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
import { refreshAsahiQueries } from "@/lib/query-refresh";
import { cn } from "@/lib/utils";

import { CreateIssueTrigger } from "./create-issue-trigger";
import { EditablePriority, EditableStatus } from "./editable-fields";
import { EmptyDetails, IssueList } from "./issue-list";
import { IssueDetails } from "./issue-details";
import { ProjectWiki } from "./project-wiki";
import { Priority, StatusIcon } from "./issue-badges";

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
          <CircleDashed className="mx-auto mb-3 size-8 text-muted-foreground" strokeWidth={1.5} />
          <div className="text-[13.5px] font-medium text-foreground">Project not found</div>
          <div className="mt-1 text-[12.5px] text-muted-foreground">
            Choose a project from the sidebar.
          </div>
        </div>
      </div>
    );
  }

  return <ProjectPage onSelectIssue={onSelectIssue} project={project} />;
}

function ProjectPage({
  onSelectIssue,
  project,
}: {
  onSelectIssue: (issueId: string) => void;
  project: Project;
}) {
  const queryClient = useQueryClient();
  const [, navigate] = useLocation();
  const [section, setSection] = useQueryState<ProjectSection>(
    "view",
    parseAsStringLiteral(PROJECT_SECTIONS).withDefault("overview"),
  );
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
    onSuccess: () => navigate("/issues"),
    onSettled: () => refreshAsahiQueries(queryClient),
  });

  const stateMutation = useMutation({
    mutationFn: (state: string) => updateProjectState(project.id, state),
    onSettled: () => refreshAsahiQueries(queryClient),
  });

  const priorityMutation = useMutation({
    mutationFn: (priority: number | null) => updateProject(project.id, { priority }),
    onSettled: () => refreshAsahiQueries(queryClient),
  });

  const selectedIssue = data.issues.find((i) => i.id === selectedIssueId);

  return (
    <section className="flex min-h-0 flex-1 flex-col overflow-hidden">
      <div className="z-10 shrink-0 px-6 py-2">
        <div className="flex min-h-8 items-center justify-between gap-3">
          <div className="inline-flex items-center rounded-full border border-border/70 bg-muted/40 p-1">
            {PROJECT_SECTIONS.map((option) => (
              <button
                className={cn(
                  "asahi-press inline-flex h-7 items-center gap-1.5 rounded-full px-3.5 text-[12.5px] capitalize [transition:color_180ms_var(--ease-out-strong),background-color_180ms_var(--ease-out-strong),transform_140ms_var(--ease-out-strong)] hover:text-foreground",
                  section === option
                    ? "asahi-pill-lift bg-background text-foreground"
                    : "text-muted-foreground",
                )}
                key={option}
                onClick={() => setSection(option)}
                type="button"
              >
                <span>{option}</span>
                {option === "issues" ? (
                  <span className="font-mono text-[10.5px] tabular-nums text-muted-foreground">
                    {data.issues.length}
                  </span>
                ) : null}
              </button>
            ))}
          </div>

          {section !== "wiki" ? (
            <CreateIssueTrigger projectId={project.id}>
              <Button aria-label="Create issue in project" size="icon-xs" variant="ghost">
                <Plus className="size-3.5" />
              </Button>
            </CreateIssueTrigger>
          ) : null}
        </div>
      </div>

      {section === "overview" ? (
        <OverviewPane
          deleteMutation={deleteMutation}
          deleteOpen={deleteOpen}
          issues={data.issues}
          onOpenWiki={() => setSection("wiki")}
          onSelectIssue={onSelectIssue}
          priorityMutation={priorityMutation}
          priorityOpen={priorityOpen}
          project={project}
          setDeleteOpen={setDeleteOpen}
          setPriorityOpen={setPriorityOpen}
          setStatusOpen={setStatusOpen}
          stateMutation={stateMutation}
          statusOpen={statusOpen}
        />
      ) : null}

      {section === "issues" ? (
        <div className="grid min-h-0 flex-1 overflow-hidden px-6 pt-4 xl:grid-cols-[minmax(20rem,26rem)_minmax(0,1fr)]">
          <div className="min-h-0 overflow-auto">
            <IssueList
              issues={data.issues}
              onSelect={(issueId) => {
                setSelectedIssueId(issueId);
                onSelectIssue(issueId);
              }}
              selectedId={selectedIssueId}
            />
          </div>
          <div className="hidden min-h-0 overflow-hidden xl:flex xl:flex-1">
            {selectedIssue ? <IssueDetails issue={selectedIssue} /> : <EmptyDetails />}
          </div>
        </div>
      ) : null}

      {section === "wiki" ? (
        <div className="min-h-0 h-full flex-1 overflow-hidden px-6 pt-4">
          <ProjectWiki project={project} />
        </div>
      ) : null}
    </section>
  );
}

function OverviewPane({
  deleteMutation,
  deleteOpen,
  issues,
  onOpenWiki,
  onSelectIssue,
  priorityMutation,
  priorityOpen,
  project,
  setDeleteOpen,
  setPriorityOpen,
  setStatusOpen,
  stateMutation,
  statusOpen,
}: {
  deleteMutation: { isPending: boolean; mutate: () => void };
  deleteOpen: boolean;
  issues: Issue[];
  onOpenWiki: () => void;
  onSelectIssue: (issueId: string) => void;
  priorityMutation: { isPending: boolean; mutate: (priority: number | null) => void };
  priorityOpen: boolean;
  project: Project;
  setDeleteOpen: (value: boolean) => void;
  setPriorityOpen: (value: boolean) => void;
  setStatusOpen: (value: boolean) => void;
  stateMutation: { isPending: boolean; mutate: (state: string) => void };
  statusOpen: boolean;
}) {
  const issuesByStatus = useMemo(() => {
    const map = new Map<string, Issue[]>();
    for (const s of ["In Progress", "Todo", "Backlog", "Done"]) map.set(s, []);
    for (const i of issues) map.get(i.state)?.push(i);
    return map;
  }, [issues]);

  const open = issues.filter((i) => i.state !== "Done").length;
  const done = issues.filter((i) => i.state === "Done").length;
  const recent = useMemo(
    () =>
      issues.toSorted((a, b) => (b.updated_at ?? "").localeCompare(a.updated_at ?? "")).slice(0, 5),
    [issues],
  );

  return (
    <div className="min-h-0 flex-1 overflow-auto">
      <div className="px-6 py-4">
        <header className="asahi-rise">
          <div className="flex items-center justify-between gap-4">
            <span className="inline-flex items-center gap-2 text-[12px] text-muted-foreground">
              <FolderClosed className="size-3.5" strokeWidth={1.8} />
              {project.slug}
            </span>
            <AlertDialog onOpenChange={setDeleteOpen} open={deleteOpen}>
              <Button
                aria-label="Delete project"
                className="asahi-press text-muted-foreground hover:bg-destructive/10 hover:text-destructive focus-visible:ring-destructive/30"
                disabled={deleteMutation.isPending}
                onClick={() => setDeleteOpen(true)}
                size="icon-xs"
                type="button"
                variant="ghost"
              >
                <Trash2 className="size-3.5" />
              </Button>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>Delete {project.name}?</AlertDialogTitle>
                  <AlertDialogDescription>
                    This action cannot be undone. Issues in this project will be moved out of the
                    project.
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel onClick={() => setDeleteOpen(false)}>Cancel</AlertDialogCancel>
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

          <h1 className="mt-3 text-[15px] font-medium leading-snug text-foreground">
            {project.name}
          </h1>
          {project.description ? (
            <p className="mt-2 max-w-[68ch] whitespace-pre-wrap text-[13.5px] leading-relaxed text-muted-foreground">
              {project.description}
            </p>
          ) : (
            <p className="mt-2 text-[13px] italic text-muted-foreground">No description</p>
          )}

          <dl className="mt-4 flex flex-wrap items-center gap-x-6 gap-y-2 text-[12px]">
            <InlineMeta label="Status">
              <EditableStatus
                disabled={stateMutation.isPending}
                onChange={(next) => {
                  stateMutation.mutate(next);
                  setStatusOpen(false);
                }}
                open={statusOpen}
                options={PROJECT_STATES}
                setOpen={setStatusOpen}
                state={project.state}
              />
            </InlineMeta>
            <InlineMeta label="Priority">
              <EditablePriority
                disabled={priorityMutation.isPending}
                onChange={(next) => {
                  priorityMutation.mutate(next);
                  setPriorityOpen(false);
                }}
                open={priorityOpen}
                options={[...PRIORITY_OPTIONS]}
                priority={project.priority}
                setOpen={setPriorityOpen}
              />
            </InlineMeta>
            <InlineMeta label="Issues">
              <span className="tabular-nums text-foreground">{issues.length}</span>
            </InlineMeta>
            <InlineMeta label="Updated">
              <span className="inline-flex items-center gap-1.5 text-foreground">
                <Clock className="size-3.5 text-muted-foreground" strokeWidth={1.8} />
                {formatDate(project.updated_at)}
              </span>
            </InlineMeta>
            <InlineMeta label="Created">
              <span className="inline-flex items-center gap-1.5 text-foreground">
                <Calendar className="size-3.5 text-muted-foreground" strokeWidth={1.8} />
                {formatDate(project.created_at)}
              </span>
            </InlineMeta>
          </dl>
        </header>

        <div className="my-8 h-px w-full bg-border/60" />

        <div className="grid gap-x-10 gap-y-12 lg:grid-cols-2">
          <IssuesPanel
            byStatus={issuesByStatus}
            done={done}
            mine={0}
            onSelectIssue={onSelectIssue}
            open={open}
            recent={recent}
          />
          <WikiPanel onOpenWiki={onOpenWiki} project={project} />
        </div>
      </div>
    </div>
  );
}

function InlineMeta({ children, label }: { children: ReactNode; label: string }) {
  return (
    <div className="inline-flex items-center gap-1.5">
      <dt className="text-muted-foreground">{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

function IssuesPanel({
  byStatus,
  done,
  mine,
  onSelectIssue,
  open,
  recent,
}: {
  byStatus: Map<string, Issue[]>;
  done: number;
  mine: number;
  onSelectIssue: (issueId: string) => void;
  open: number;
  recent: Issue[];
}) {
  const order = ["In Progress", "Todo", "Backlog", "Done"];
  const max = Math.max(...order.map((s) => byStatus.get(s)?.length ?? 0), 1);

  return (
    <section className="flex min-w-0 flex-col">
      <PanelHeader eyebrow="Issues" summary={`${open} open · ${mine} mine · ${done} done`} />

      <ul className="mt-4 flex flex-col gap-1.5">
        {order.map((status) => {
          const list = byStatus.get(status) ?? [];
          const ratio = list.length / max;
          return (
            <li
              className="grid grid-cols-[20px_minmax(0,1fr)_2.5rem_minmax(96px,160px)] items-center gap-3 px-3 py-1.5 text-[13px]"
              key={status}
            >
              <StatusIcon state={status} />
              <span className="truncate text-foreground">{status}</span>
              <span className="text-right font-mono tabular-nums text-muted-foreground">
                {list.length}
              </span>
              <span
                aria-hidden
                className="relative h-1 w-full overflow-hidden rounded-full bg-muted"
              >
                <span
                  className="absolute inset-y-0 left-0 rounded-full"
                  style={{
                    background: statusBarColor(status),
                    width: `${Math.max(ratio * 100, list.length > 0 ? 6 : 0)}%`,
                  }}
                />
              </span>
            </li>
          );
        })}
      </ul>

      <div className="mt-8">
        <h3 className="asahi-eyebrow">Recently updated</h3>
        {recent.length === 0 ? (
          <p className="mt-3 text-[13px] text-muted-foreground">No issues yet.</p>
        ) : (
          <ul className="mt-2">
            {recent.map((i, idx) => (
              <li className="asahi-rise" key={i.id} style={{ animationDelay: `${idx * 22}ms` }}>
                <button
                  className="asahi-press flex w-full items-baseline gap-3 rounded-md px-3 py-2.5 text-left [transition:background-color_180ms_var(--ease-out-strong)] hover:bg-muted/40"
                  aria-label={`Open issue ${i.identifier}: ${i.title}`}
                  onClick={() => onSelectIssue(i.id)}
                  type="button"
                >
                  <StatusIcon state={i.state} />
                  <span className="min-w-0 flex-1 truncate text-[13.5px] text-foreground">
                    {i.title}
                  </span>
                  <span className="hidden shrink-0 font-mono text-[11.5px] uppercase tracking-wide text-muted-foreground sm:inline">
                    {i.identifier}
                  </span>
                  <Priority priority={i.priority} showEmpty={false} />
                  <span className="shrink-0 text-[11.5px] tabular-nums text-muted-foreground">
                    {formatShortDate(i.updated_at)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}

function WikiPanel({ onOpenWiki, project }: { onOpenWiki: () => void; project: Project }) {
  const { data } = useSuspenseQuery({
    queryKey: ["wiki", project.slug],
    queryFn: () => fetchWikiNodes(project.slug),
  });

  const pages = data.nodes.filter((n) => n.kind === "page");
  const recent = pages
    .toSorted((a, b) => (b.updated_at ?? "").localeCompare(a.updated_at ?? ""))
    .slice(0, 5);

  return (
    <section className="flex min-w-0 flex-col">
      <PanelHeader
        action={
          <button
            className="asahi-press text-[12px] text-muted-foreground [transition:color_180ms_var(--ease-out-strong)] hover:text-foreground"
            onClick={onOpenWiki}
            type="button"
          >
            Open the wiki →
          </button>
        }
        eyebrow="Wiki"
        summary={`${pages.length} ${pages.length === 1 ? "document" : "documents"}`}
      />

      {recent.length === 0 ? (
        <p className="mt-4 text-[13px] text-muted-foreground">
          No documents yet. Open the Wiki tab to create the first one.
        </p>
      ) : (
        <ul className="mt-3">
          {recent.map((doc, i) => (
            <li
              className="asahi-rise rounded-md"
              key={doc.id}
              style={{ animationDelay: `${i * 22}ms` }}
            >
              <button
                className="asahi-press flex w-full items-baseline gap-3 rounded-md px-3 py-2.5 text-left [transition:background-color_180ms_var(--ease-out-strong)] hover:bg-muted/40"
                onClick={onOpenWiki}
                type="button"
              >
                <FileText className="size-3.5 shrink-0 translate-y-0.5 text-muted-foreground" />
                <span className="min-w-0 flex-1 truncate text-[13.5px] text-foreground">
                  {doc.title}
                </span>
                {doc.current_version ? (
                  <span className="shrink-0 font-mono text-[11.5px] uppercase tracking-wide tabular-nums text-muted-foreground">
                    v{doc.current_version.version}
                  </span>
                ) : null}
                <span className="shrink-0 text-[11.5px] tabular-nums text-muted-foreground">
                  {formatShortDate(doc.updated_at)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function PanelHeader({
  action,
  eyebrow,
  summary,
}: {
  action?: ReactNode;
  eyebrow: string;
  summary: string;
}) {
  return (
    <div className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
      <div className="flex items-baseline gap-3">
        <h2 className="asahi-eyebrow">{eyebrow}</h2>
        <span className="text-[12.5px] text-muted-foreground">{summary}</span>
      </div>
      {action}
    </div>
  );
}

function statusBarColor(state: string) {
  if (state === "Done") return "oklch(0.55 0.1 150)";
  if (state === "In Progress") return "oklch(0.62 0.13 85)";
  if (state === "Todo") return "oklch(0.4 0 0)";
  return "oklch(0.7 0 0)";
}

const SHORT_DATE = new Intl.DateTimeFormat(undefined, {
  month: "short",
  day: "numeric",
});

function formatDate(value: string | null) {
  if (!value) return "—";
  return SHORT_DATE.format(new Date(value));
}

function formatShortDate(value: string | null) {
  return formatDate(value);
}

/** Re-export for tree shaking of unused types in larger files */
export type { WikiNode };
