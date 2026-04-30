import { useState, type FormEvent, type ReactNode } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { IconFlag, IconMapPin, IconTag, IconX } from "@tabler/icons-react";

import { createIssue } from "@/api/asahi";
import { Button } from "@/components/ui/button";

export function IssueComposer({ onClose }: { onClose: () => void }) {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [labels, setLabels] = useState("");
  const [priority, setPriority] = useState("");

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
          <div className="flex min-w-0 items-center gap-2 text-sm font-medium text-muted-foreground">
            <span className="inline-flex h-6 items-center gap-1.5 rounded-full border border-border bg-background px-2 text-xs text-muted-foreground shadow-sm">
              <IconMapPin className="size-3.5 text-[#5bb974]" stroke={2.2} />
              ENG
            </span>
            <span className="text-muted-foreground/70">›</span>
            <span className="truncate text-foreground">New issue</span>
          </div>

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
            className="block h-8 w-full bg-transparent text-[1.25rem] font-semibold leading-8 tracking-normal text-foreground outline-none placeholder:text-[#9da0a6]"
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Issue title"
            value={title}
          />
          <textarea
            className="mt-2 block min-h-16 w-full resize-none bg-transparent text-[1rem] leading-6 text-foreground outline-none placeholder:text-[#a9abb1]"
            onChange={(event) => setDescription(event.target.value)}
            placeholder="Add description..."
            value={description}
          />
        </div>

        <div className="flex flex-wrap items-center gap-2 px-4 pb-4">
          <ComposerPill icon={IconFlag}>
            <input
              aria-label="Priority"
              className="w-14 bg-transparent outline-none placeholder:text-muted-foreground"
              max={4}
              min={0}
              onChange={(event) => setPriority(event.target.value)}
              placeholder="Priority"
              type="number"
              value={priority}
            />
          </ComposerPill>
          <ComposerPill icon={IconTag}>
            <input
              aria-label="Labels"
              className="w-24 bg-transparent outline-none placeholder:text-muted-foreground"
              onChange={(event) => setLabels(event.target.value)}
              placeholder="Labels"
              value={labels}
            />
          </ComposerPill>
        </div>

        <div className="mt-auto flex items-center justify-end px-4 pb-4">
          <Button
            aria-disabled={mutation.isPending || !title.trim()}
            className="bg-[#7478d7] px-4 text-white hover:bg-[#666bd2] aria-disabled:opacity-80"
            type="submit"
          >
            Create issue
          </Button>
        </div>
      </form>
    </div>
  );
}

function ComposerPill({ children, icon: Icon }: { children: ReactNode; icon: typeof IconFlag }) {
  return (
    <span className="inline-flex h-7 items-center gap-1.5 rounded-full border border-border bg-background px-2.5 text-xs font-medium text-muted-foreground shadow-sm">
      <Icon className="size-4" stroke={1.8} />
      {children}
    </span>
  );
}
