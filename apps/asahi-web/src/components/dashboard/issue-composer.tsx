import { useState, type FormEvent, type KeyboardEvent } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  IconBox,
  IconCheck,
  IconChevronDown,
  IconLink,
  IconX,
} from "@tabler/icons-react";

import { createIssue, fetchIssues, fetchProjects } from "@/api/asahi";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { RichTextEditor } from "@/components/ui/rich-text-editor";
import { cn } from "@/lib/utils";

import { EditablePriority, EditableStatus } from "./editable-fields";

const STATUS_OPTIONS = ["Backlog", "Todo", "In Progress", "Done"];
const PRIORITY_OPTIONS = [null, 1, 2, 3, 4] as const;

export function IssueComposer({
  onClose,
  projectId,
}: {
  onClose: () => void;
  projectId?: string;
}) {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [selectedProjectId, setSelectedProjectId] = useState(projectId ?? "");
  const [projectOpen, setProjectOpen] = useState(false);

  const [state, setState] = useState("Todo");
  const [statusOpen, setStatusOpen] = useState(false);

  const [priority, setPriority] = useState<number | null>(null);
  const [priorityOpen, setPriorityOpen] = useState(false);

  const [blockedByIds, setBlockedByIds] = useState<string[]>([]);
  const [blockersOpen, setBlockersOpen] = useState(false);
  const [createMore, setCreateMore] = useState(false);

  const projectsQuery = useQuery({
    queryKey: ["projects"],
    queryFn: () => fetchProjects(),
  });

  const allIssuesQuery = useQuery({
    queryKey: ["issues", "all"],
    queryFn: () => fetchIssues(),
  });

  const availableBlockers =
    allIssuesQuery.data?.issues.filter(
      (candidate) => !blockedByIds.includes(candidate.id),
    ) ?? [];

  const mutation = useMutation({
    mutationFn: () =>
      createIssue({
        project_id: selectedProjectId || undefined,
        title,
        description: description || undefined,
        state,
        priority: priority ?? undefined,
        blocked_by: blockedByIds.length ? blockedByIds : undefined,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      void queryClient.invalidateQueries({ queryKey: ["projects"] });
      if (createMore) {
        setTitle("");
        setDescription("");
        setBlockedByIds([]);
      } else {
        onClose();
      }
    },
  });

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    if (title.trim()) {
      mutation.mutate();
    }
  };

  const handleKeyDown = (event: KeyboardEvent) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      submit();
    }
  };

  const toggleBlocker = (issueId: string) => {
    setBlockedByIds((prev) =>
      prev.includes(issueId)
        ? prev.filter((id) => id !== issueId)
        : [...prev, issueId],
    );
  };

  const selectedBlockerLabels = blockedByIds
    .map(
      (id) =>
        allIssuesQuery.data?.issues.find((i) => i.id === id)?.identifier ?? id,
    )
    .join(", ");

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/24 px-4 pt-[18vh] backdrop-blur-[1px]">
      <button
        aria-label="Close composer"
        className="absolute inset-0 cursor-default"
        onClick={onClose}
        type="button"
      />
      <form
        className="relative flex min-h-[18rem] w-[min(42rem,calc(100vw-2rem))] flex-col rounded-[1.15rem] bg-card text-card-foreground shadow-[0_18px_55px_rgba(15,23,42,0.2),0_1px_8px_rgba(15,23,42,0.08)] ring-1 ring-black/10"
        onSubmit={submit}
      >
        <div className="flex items-center justify-between px-4 pt-3.5">
          <span className="text-sm font-medium text-foreground">New issue</span>
          <button
            aria-label="Close composer"
            className="flex size-7 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
            onClick={onClose}
            type="button"
          >
            <IconX className="size-4" />
          </button>
        </div>

        <div className="flex-1 px-5 pb-3 pt-6">
          <input
            autoFocus
            className="block h-8 w-full bg-transparent font-semibold text-foreground outline-none placeholder:text-[#9da0a6]"
            onChange={(event) => setTitle(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Issue title"
            value={title}
          />
          <div className="mt-2">
            <RichTextEditor
              content={description}
              onChange={(html) => setDescription(html)}
            />
          </div>
        </div>

        <div className="mt-auto flex items-center justify-between px-4 pb-4">
          <div className="flex items-center gap-1.5">
            <Popover open={projectOpen} onOpenChange={setProjectOpen}>
              <PopoverTrigger asChild>
                <button
                  className="inline-flex h-7 items-center gap-1.5 rounded-full bg-muted/70 px-2.5 text-xs font-medium text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                  disabled={projectsQuery.isLoading}
                  type="button"
                >
                  <IconBox className="size-3.5" />
                  <span>
                    {selectedProjectId
                      ? projectsQuery.data?.projects.find(
                          (p) => p.id === selectedProjectId,
                        )?.name ?? "No project"
                      : "No project"}
                  </span>
                  <IconChevronDown className="size-3 text-muted-foreground/70" />
                </button>
              </PopoverTrigger>
              <PopoverContent
                align="start"
                className="w-56 rounded-xl border-border/60 bg-popover p-1.5 shadow-lg"
                side="top"
                sideOffset={6}
              >
                <div className="text-xs font-medium text-muted-foreground px-2 pt-1 pb-1">
                  Project
                </div>
                <button
                  className="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-sm text-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
                  onClick={() => {
                    setSelectedProjectId("");
                    setProjectOpen(false);
                  }}
                  type="button"
                >
                  <span className="flex size-4 items-center justify-center">
                    {!selectedProjectId && <IconCheck className="size-3.5" />}
                  </span>
                  No project
                </button>
                {(projectsQuery.data?.projects ?? []).map((project) => (
                  <button
                    key={project.id}
                    className="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-sm text-foreground transition-colors hover:bg-accent hover:text-accent-foreground"
                    onClick={() => {
                      setSelectedProjectId(project.id);
                      setProjectOpen(false);
                    }}
                    type="button"
                  >
                    <span className="flex size-4 items-center justify-center">
                      {selectedProjectId === project.id && (
                        <IconCheck className="size-3.5" />
                      )}
                    </span>
                    {project.name}
                  </button>
                ))}
              </PopoverContent>
            </Popover>

            <EditableStatus
              disabled={false}
              onChange={(s) => {
                setState(s);
                setStatusOpen(false);
              }}
              open={statusOpen}
              options={STATUS_OPTIONS}
              setOpen={setStatusOpen}
              state={state}
            />

            <EditablePriority
              disabled={false}
              onChange={(p) => {
                setPriority(p);
                setPriorityOpen(false);
              }}
              open={priorityOpen}
              options={[...PRIORITY_OPTIONS]}
              priority={priority}
              setOpen={setPriorityOpen}
            />

            <div className="relative min-w-0">
              <button
                className="inline-flex h-7 max-w-full items-center gap-1.5 rounded-md px-1.5 text-left hover:bg-[#f7f6f2] disabled:opacity-50"
                disabled={allIssuesQuery.isLoading}
                onClick={() => setBlockersOpen(!blockersOpen)}
                type="button"
              >
                <IconLink className="size-3.5 shrink-0 text-[#7d7a72]" />
                <span className="truncate text-xs text-[#55524b]">
                  {blockedByIds.length
                    ? selectedBlockerLabels || `${blockedByIds.length} blocked`
                    : "Blocked by"}
                </span>
                <IconChevronDown className="size-3.5 shrink-0 text-[#8a877e]" />
              </button>

              {blockersOpen ? (
                <div className="absolute bottom-full left-0 z-20 mb-1 max-h-72 w-72 overflow-auto rounded-md border border-[#eceae5] bg-white py-1 shadow-md">
                  {availableBlockers.length || blockedByIds.length ? (
                    (allIssuesQuery.data?.issues ?? []).map((candidate) => {
                      const selected = blockedByIds.includes(candidate.id);
                      return (
                        <button
                          className={cn(
                            "flex min-h-9 w-full items-center gap-2 px-3 py-1.5 text-left hover:bg-[#f7f6f2]",
                            selected && "bg-[#f2f1ec]",
                          )}
                          key={candidate.id}
                          onClick={() => toggleBlocker(candidate.id)}
                          type="button"
                        >
                          <span
                            className={cn(
                              "flex size-4 shrink-0 items-center justify-center rounded border border-[#c8c3b8] text-[10px] text-white",
                              selected ? "bg-[#25231f]" : "bg-white",
                            )}
                          >
                            {selected ? (
                              <span className="size-1.5 rounded-full bg-white" />
                            ) : null}
                          </span>
                          <span className="min-w-0 flex-1">
                            <span className="block truncate text-xs font-medium text-[#33312d]">
                              {candidate.identifier}
                            </span>
                            <span className="block truncate text-xs text-[#77746c]">
                              {candidate.title}
                            </span>
                          </span>
                        </button>
                      );
                    })
                  ) : (
                    <div className="px-3 py-2 text-xs text-[#77746c]">
                      No issues
                    </div>
                  )}

                  {blockedByIds.length ? (
                    <button
                      className="flex h-8 w-full items-center gap-2 border-t border-[#eceae5] px-3 text-left text-xs text-[#55524b] hover:bg-[#f7f6f2]"
                      onClick={() => setBlockedByIds([])}
                      type="button"
                    >
                      <IconX className="size-3.5" />
                      Clear blockers
                    </button>
                  ) : null}
                </div>
              ) : null}
            </div>
          </div>

          <label className="inline-flex cursor-pointer items-center gap-2 text-xs text-[#77746c]">
            <input
              checked={createMore}
              className="size-3.5 rounded border-[#c9c4bb] text-foreground accent-foreground"
              onChange={(e) => setCreateMore(e.target.checked)}
              type="checkbox"
            />
            Create more
          </label>

          <Button
            aria-disabled={mutation.isPending || !title.trim()}
            className="px-4 aria-disabled:opacity-80"
            type="submit"
          >
            Create issue
          </Button>
        </div>
      </form>
    </div>
  );
}
