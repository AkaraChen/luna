import { useState, type FormEvent, type ReactNode } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  IconArrowsDiagonal,
  IconBox,
  IconCircleDashed,
  IconDots,
  IconFlag,
  IconMapPin,
  IconPaperclip,
  IconTag,
  IconUserCircle,
  IconX,
} from "@tabler/icons-react";

import { createIssue } from "@/api/asahi";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export function IssueComposer({ onClose }: { onClose: () => void }) {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [labels, setLabels] = useState("");
  const [priority, setPriority] = useState("");
  const [createMore, setCreateMore] = useState(false);

  const mutation = useMutation({
    mutationFn: () =>
      createIssue({
        project_slug: "engineering",
        team_key: "ENG",
        title,
        description: description || undefined,
        labels: labels
          .split(",")
          .map((label) => label.trim())
          .filter(Boolean),
        priority: priority ? Number(priority) : undefined,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["issues"] });
      if (createMore) {
        setTitle("");
        setDescription("");
        return;
      }
      onClose();
    },
  });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (title.trim()) {
      mutation.mutate();
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/25 px-4 pt-[14vh] backdrop-blur-[1px]">
      <button
        aria-label="Close composer"
        className="absolute inset-0 cursor-default"
        onClick={onClose}
        type="button"
      />
      <form
        className="relative flex min-h-[26.25rem] w-[min(68rem,calc(100vw-2rem))] flex-col rounded-[1.35rem] bg-card text-card-foreground shadow-[0_22px_70px_rgba(15,23,42,0.22),0_2px_10px_rgba(15,23,42,0.08)] ring-1 ring-black/10"
        onSubmit={submit}
      >
        <div className="flex items-center justify-between px-5 pt-4">
          <div className="flex min-w-0 items-center gap-2 text-sm font-medium text-muted-foreground">
            <span className="inline-flex h-7 items-center gap-1.5 rounded-full border border-border bg-background px-2.5 text-xs text-muted-foreground shadow-sm">
              <IconMapPin className="size-3.5 text-[#5bb974]" stroke={2.2} />
              ALL
            </span>
            <span className="text-muted-foreground/70">›</span>
            <span className="truncate text-foreground">New issue</span>
          </div>

          <div className="flex items-center gap-1">
            <button
              aria-label="Expand composer"
              className="flex size-8 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
              type="button"
            >
              <IconArrowsDiagonal className="size-4" />
            </button>
            <button
              aria-label="Close composer"
              className="flex size-8 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
              onClick={onClose}
              type="button"
            >
              <IconX className="size-4" />
            </button>
          </div>
        </div>

        <div className="flex-1 px-7 pb-6 pt-8">
          <input
            autoFocus
            className="block h-9 w-full bg-transparent text-[1.55rem] font-semibold leading-9 tracking-normal text-foreground outline-none placeholder:text-[#9da0a6]"
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Issue title"
            value={title}
          />
          <textarea
            className="mt-3 block min-h-28 w-full resize-none bg-transparent text-[1.35rem] leading-8 text-foreground outline-none placeholder:text-[#a9abb1]"
            onChange={(event) => setDescription(event.target.value)}
            placeholder="Add description..."
            value={description}
          />
        </div>

        <div className="flex flex-wrap items-center gap-2 px-5 pb-8">
          <ComposerPill icon={IconCircleDashed} label="Backlog" />
          <ComposerPill icon={IconFlag}>
            <input
              aria-label="Priority"
              className="w-16 bg-transparent outline-none placeholder:text-muted-foreground"
              max={4}
              min={0}
              onChange={(event) => setPriority(event.target.value)}
              placeholder="Priority"
              type="number"
              value={priority}
            />
          </ComposerPill>
          <ComposerPill icon={IconUserCircle} label="Assignee" />
          <ComposerPill icon={IconBox} label="Project" />
          <ComposerPill icon={IconTag}>
            <input
              aria-label="Labels"
              className="w-20 bg-transparent outline-none placeholder:text-muted-foreground"
              onChange={(event) => setLabels(event.target.value)}
              placeholder="Labels"
              value={labels}
            />
          </ComposerPill>
          <button
            aria-label="More issue options"
            className="inline-flex size-8 items-center justify-center rounded-full border border-border bg-background text-muted-foreground shadow-sm hover:bg-muted hover:text-foreground"
            type="button"
          >
            <IconDots className="size-4" />
          </button>
        </div>

        <div className="mt-auto flex items-center justify-between px-5 pb-5">
          <button
            aria-label="Attach file"
            className="flex size-9 items-center justify-center rounded-full border border-border bg-background text-muted-foreground shadow-sm hover:bg-muted hover:text-foreground"
            type="button"
          >
            <IconPaperclip className="size-4" />
          </button>

          <div className="flex items-center gap-4">
            <button
              className="flex items-center gap-2 text-sm text-muted-foreground hover:text-foreground"
              onClick={() => setCreateMore((value) => !value)}
              type="button"
            >
              <span
                className={cn(
                  "relative inline-flex h-5 w-9 items-center rounded-full transition-colors",
                  createMore ? "bg-primary" : "bg-muted-foreground/35",
                )}
              >
                <span
                  className={cn(
                    "absolute size-4 rounded-full bg-white shadow-sm transition-transform",
                    createMore ? "translate-x-4.5" : "translate-x-0.5",
                  )}
                />
              </span>
              Create more
            </button>
            <Button
              aria-disabled={mutation.isPending || !title.trim()}
              className="bg-[#7478d7] px-4 text-white hover:bg-[#666bd2] aria-disabled:opacity-80"
              type="submit"
            >
              Create issue
            </Button>
          </div>
        </div>
      </form>
    </div>
  );
}

function ComposerPill({
  children,
  icon: Icon,
  label,
}: {
  children?: ReactNode;
  icon: typeof IconCircleDashed;
  label?: string;
}) {
  return (
    <span className="inline-flex h-8 items-center gap-1.5 rounded-full border border-border bg-background px-2.5 text-sm font-medium text-muted-foreground shadow-sm">
      <Icon className="size-4" stroke={1.8} />
      {children ?? label}
    </span>
  );
}
