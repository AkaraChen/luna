import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { X } from "lucide-react";

import { createProject, type Project } from "@/api/asahi";
import { Button } from "@/components/ui/button";
import { refreshAsahiQueries } from "@/lib/query-refresh";

export function ProjectComposer({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (project: Project) => void;
}) {
  const queryClient = useQueryClient();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");

  // Defer the "open" data-state by one frame so the enter transition fires
  // from the unset (hidden) base styles. Same pattern as IssueComposer.
  const [entered, setEntered] = useState(false);
  useEffect(() => {
    const id = requestAnimationFrame(() => setEntered(true));
    return () => cancelAnimationFrame(id);
  }, []);
  const state: "open" | undefined = entered ? "open" : undefined;

  // Latest-ref so the document Escape listener doesn't re-subscribe every render.
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });
  useEffect(() => {
    const handler = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  const mutation = useMutation({
    mutationFn: () =>
      createProject({
        name,
        description: description || undefined,
      }),
    onSuccess: (project) => {
      onCreated(project);
      onClose();
    },
    onSettled: () => refreshAsahiQueries(queryClient),
  });

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    if (name.trim()) {
      mutation.mutate();
    }
  };

  const handleKeyDown = (event: KeyboardEvent) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      submit();
    }
  };

  const node = (
    <div
      className="asahi-modal-backdrop fixed inset-0 z-50 flex items-start justify-center bg-black/24 px-4 pt-[18vh] backdrop-blur-[1px]"
      data-state={state}
    >
      <button
        aria-label="Close composer"
        className="absolute inset-0 cursor-default"
        onClick={onClose}
        type="button"
      />
      <form
        aria-labelledby="project-composer-title"
        className="asahi-modal-panel relative flex min-h-[17rem] w-[min(38rem,calc(100vw-2rem))] flex-col rounded-xl bg-card text-card-foreground ring-1 ring-border/70"
        data-state={state}
        onSubmit={submit}
      >
        <div className="flex items-center justify-between px-4 pt-3.5">
          <span className="text-[13.5px] font-medium text-foreground" id="project-composer-title">
            New project
          </span>
          <button
            aria-label="Close composer"
            className="asahi-press flex size-7 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
            onClick={onClose}
            type="button"
          >
            <X className="size-4" />
          </button>
        </div>

        <div className="flex-1 px-5 pb-3 pt-6">
          <input
            autoFocus
            className="block h-8 w-full bg-transparent text-[15px] font-medium text-foreground outline-none placeholder:text-muted-foreground"
            onChange={(event) => setName(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Project name"
            value={name}
          />
          <textarea
            className="mt-2 block min-h-20 w-full resize-none bg-transparent text-[13.5px] leading-relaxed text-foreground outline-none placeholder:text-muted-foreground"
            onChange={(event) => setDescription(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Add a description"
            value={description}
          />
        </div>

        <div className="mt-auto flex items-center justify-end px-4 pb-4">
          <Button
            aria-disabled={mutation.isPending || !name.trim()}
            className="px-4 aria-disabled:opacity-80"
            type="submit"
          >
            Create project
          </Button>
        </div>
      </form>
    </div>
  );

  return createPortal(node, document.body);
}
