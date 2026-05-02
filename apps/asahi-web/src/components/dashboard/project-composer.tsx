import { useState, type FormEvent, type KeyboardEvent } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { IconX } from "@tabler/icons-react";

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

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/24 px-4 pt-[18vh] backdrop-blur-[1px]">
      <button
        aria-label="Close composer"
        className="absolute inset-0 cursor-default"
        onClick={onClose}
        type="button"
      />
      <form
        className="relative flex min-h-[17rem] w-[min(38rem,calc(100vw-2rem))] flex-col rounded-[1.15rem] bg-card text-card-foreground shadow-[0_18px_55px_rgba(15,23,42,0.2),0_1px_8px_rgba(15,23,42,0.08)] ring-1 ring-black/10"
        onSubmit={submit}
      >
        <div className="flex items-center justify-between px-4 pt-3.5">
          <span className="text-sm font-medium text-foreground">New project</span>
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
            onChange={(event) => setName(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Project name"
            value={name}
          />
          <textarea
            className="mt-2 block min-h-20 w-full resize-none bg-transparent text-foreground outline-none placeholder:text-[#a9abb1]"
            onChange={(event) => setDescription(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Add description..."
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
}
