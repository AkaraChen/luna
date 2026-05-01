import { useEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import {
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  IconChevronRight,
  IconCircleDashed,
  IconFileText,
  IconFolder,
  IconFolderOpen,
  IconPlus,
  IconX,
} from "@tabler/icons-react";

import {
  createWikiNode,
  fetchWikiNodes,
  type CreateWikiNodeInput,
  type Project,
  type WikiNode,
  type WikiNodeKind,
  type WikiNodeListResponse,
} from "@/api/asahi";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

type InlineComposerState = {
  parentId: string | null;
  title: string;
};

type WikiNodesQueryResult = UseQueryResult<WikiNodeListResponse, Error>;

export function ProjectWiki({ project }: { project: Project }) {
  const queryClient = useQueryClient();
  const [expandedFolderIds, setExpandedFolderIds] = useState<Set<string>>(() => new Set());
  const [selectedNode, setSelectedNode] = useState<WikiNode | null>(null);
  const [inlineComposer, setInlineComposer] = useState<InlineComposerState | null>(null);
  const [composer, setComposer] = useState<{ kind: WikiNodeKind; parentId: string | null } | null>(null);

  const rootQuery = useQuery({
    queryKey: wikiNodesQueryKey(project.id, null),
    queryFn: () => fetchWikiNodes(project.id),
  });

  const expandedIds = useMemo(() => Array.from(expandedFolderIds), [expandedFolderIds]);
  const childQueries = useQueries({
    queries: expandedIds.map((folderId) => ({
      queryKey: wikiNodesQueryKey(project.id, folderId),
      queryFn: () => fetchWikiNodes(project.id, { parentId: folderId }),
    })),
  }) as WikiNodesQueryResult[];

  const childrenByParentId = new Map<string | null, WikiNode[]>();
  const loadedNodes: WikiNode[] = [];
  if (rootQuery.data) {
    const nodes = sortWikiNodes(rootQuery.data.nodes);
    childrenByParentId.set(null, nodes);
    loadedNodes.push(...nodes);
  }
  childQueries.forEach((query, index) => {
    const parentId = expandedIds[index];
    if (!parentId || !query.data) return;
    const nodes = sortWikiNodes(query.data.nodes);
    childrenByParentId.set(parentId, nodes);
    loadedNodes.push(...nodes);
  });

  const activeNode = selectedNode
    ? loadedNodes.find((node) => node.id === selectedNode.id) ?? selectedNode
    : null;
  const activeChildren =
    activeNode?.kind === "folder" ? childrenByParentId.get(activeNode.id) : undefined;

  const createMutation = useMutation({
    mutationFn: (input: CreateWikiNodeInput) => createWikiNode(project.id, input),
    onSuccess: (node, input) => {
      const parentId = input.parent_id ?? null;
      queryClient.setQueryData<WikiNodeListResponse>(
        wikiNodesQueryKey(project.id, parentId),
        (current) =>
          current
            ? {
                nodes: sortWikiNodes([
                  ...current.nodes.filter((candidate) => candidate.id !== node.id),
                  node,
                ]),
              }
            : current,
      );

      setExpandedFolderIds((current) => {
        const next = new Set(current);
        if (parentId) next.add(parentId);
        if (node.kind === "folder") next.add(node.id);
        return next;
      });
      setSelectedNode(node);
      setComposer(null);
      void queryClient.invalidateQueries({ queryKey: ["wiki", project.id] });
    },
  });

  const toggleFolder = (folderId: string) => {
    setExpandedFolderIds((current) => {
      const next = new Set(current);
      if (next.has(folderId)) {
        next.delete(folderId);
      } else {
        next.add(folderId);
      }
      return next;
    });
  };

  const handleNodeClick = (node: WikiNode) => {
    setSelectedNode(node);
    if (node.kind === "folder") {
      toggleFolder(node.id);
    }
  };

  const resolveParentId = (): string | null => {
    return activeNode?.kind === "folder" ? activeNode.id : activeNode?.parent_id ?? null;
  };

  const openComposer = (kind: WikiNodeKind) => {
    createMutation.reset();
    setComposer({
      kind,
      parentId: kind === "page" ? null : resolveParentId(),
    });
  };

  const openInlineFolderComposer = () => {
    createMutation.reset();
    const parentId = resolveParentId();
    setInlineComposer({ parentId, title: "" });
    if (parentId) {
      setExpandedFolderIds((current) => {
        const next = new Set(current);
        next.add(parentId);
        return next;
      });
    }
  };

  const submitInlineFolder = (title: string) => {
    const trimmed = title.trim();
    if (!trimmed || createMutation.isPending) return;
    createMutation.mutate({
      actor_kind: "human",
      kind: "folder",
      parent_id: inlineComposer?.parentId ?? undefined,
      title: trimmed,
    });
    setInlineComposer(null);
  };

  const cancelInlineFolder = () => {
    setInlineComposer(null);
  };

  const handleCreatePageInFolder = (folderId: string) => {
    createMutation.reset();
    setComposer({ kind: "page", parentId: folderId });
  };

  const rootNodes = childrenByParentId.get(null) ?? [];

  return (
    <div className="min-h-[32rem] lg:grid lg:grid-cols-[minmax(15rem,18rem)_minmax(0,1fr)]">
      <div className="min-h-0 border-b border-[#eceae5] lg:border-b-0 lg:border-r">
        <div className="flex h-12 items-center justify-between px-4">
          <div className="text-sm font-medium">Wiki</div>
          <div className="flex items-center gap-1">
            <Button
              aria-label="Create folder"
              onClick={openInlineFolderComposer}
              size="icon-xs"
              title="Create folder"
              type="button"
              variant="ghost"
            >
              <IconFolder className="size-3.5" />
            </Button>
            <Button
              aria-label="Create page"
              onClick={() => openComposer("page")}
              size="icon-xs"
              title="Create page"
              type="button"
              variant="ghost"
            >
              <IconPlus className="size-3.5" />
            </Button>
          </div>
        </div>

        <div className="max-h-[28rem] overflow-auto pb-2 lg:max-h-none">
          {rootQuery.isLoading ? (
            <div className="px-4 py-3 text-xs text-[#8f8b82]">Loading wiki...</div>
          ) : rootQuery.isError ? (
            <div className="px-4 py-3 text-xs text-destructive">Could not load wiki.</div>
          ) : (
            <>
              {rootNodes.map((node) => (
                <WikiTreeNode
                  childrenByParentId={childrenByParentId}
                  childQueries={childQueries}
                  depth={0}
                  expandedFolderIds={expandedFolderIds}
                  expandedIds={expandedIds}
                  inlineComposer={inlineComposer}
                  key={node.id}
                  node={node}
                  onCancelInline={cancelInlineFolder}
                  onCreatePage={handleCreatePageInFolder}
                  onNodeClick={handleNodeClick}
                  onSubmitInline={submitInlineFolder}
                  selectedId={activeNode?.id ?? null}
                />
              ))}
              {inlineComposer?.parentId === null ? (
                <InlineFolderRow
                  depth={0}
                  onCancel={cancelInlineFolder}
                  onSubmit={submitInlineFolder}
                />
              ) : null}
              {rootNodes.length === 0 && inlineComposer?.parentId !== null ? (
                <div className="flex h-56 items-center justify-center px-4 text-center">
                  <div>
                    <IconCircleDashed className="mx-auto mb-3 size-8 text-[#b4b0a7]" stroke={1.5} />
                    <div className="text-sm font-medium">No wiki pages</div>
                  </div>
                </div>
              ) : null}
            </>
          )}
        </div>
      </div>

      <WikiNodeViewer node={activeNode} childNodes={activeChildren} />

      {composer ? (
        <WikiComposer
          error={createMutation.error}
          key={`page:${composer.parentId ?? "root"}`}
          onClose={() => setComposer(null)}
          onSubmit={({ content, title }) => {
            createMutation.mutate({
              actor_kind: "human",
              content,
              kind: "page",
              parent_id: composer.parentId ?? undefined,
              title,
            });
          }}

          pending={createMutation.isPending}
        />
      ) : null}
    </div>
  );
}

function WikiTreeNode({
  childrenByParentId,
  childQueries,
  depth,
  expandedFolderIds,
  expandedIds,
  inlineComposer,
  node,
  onNodeClick,
  onCreatePage,
  selectedId,
  onSubmitInline,
  onCancelInline,
}: {
  childrenByParentId: Map<string | null, WikiNode[]>;
  childQueries: WikiNodesQueryResult[];
  depth: number;
  expandedFolderIds: Set<string>;
  expandedIds: string[];
  inlineComposer?: InlineComposerState | null;
  node: WikiNode;
  onNodeClick: (node: WikiNode) => void;
  onCreatePage?: (folderId: string) => void;
  selectedId: string | null;
  onSubmitInline?: (title: string) => void;
  onCancelInline?: () => void;
}) {
  const expanded = expandedFolderIds.has(node.id);
  const isFolder = node.kind === "folder";
  const isSelected = selectedId === node.id;
  const children = childrenByParentId.get(node.id) ?? [];
  const childQueryIndex = expandedIds.indexOf(node.id);
  const loadingChildren = childQueryIndex >= 0 && childQueries[childQueryIndex]?.isLoading;

  return (
    <div>
      <button
        className={cn(
          "group grid min-h-9 w-full grid-cols-[1rem_1rem_minmax(0,1fr)_auto] items-center gap-2 pr-2 text-left hover:bg-[#f7f6f2]",
          isSelected && "bg-[#f2f1ec]",
        )}
        onClick={() => onNodeClick(node)}
        style={{ paddingLeft: `${0.5 + depth * 0.875}rem` }}
        type="button"
      >
        {isFolder ? (
          <IconChevronRight
            className={cn("size-3.5 text-[#8f8b82] transition-transform", expanded && "rotate-90")}
          />
        ) : (
          <span className="size-3.5" />
        )}
        {isFolder ? (
          expanded ? (
            <IconFolderOpen className="size-4 text-[#6f6b62]" stroke={1.8} />
          ) : (
            <IconFolder className="size-4 text-[#7a756b]" stroke={1.8} />
          )
        ) : (
          <IconFileText className="size-4 text-[#6d7180]" stroke={1.8} />
        )}
        <span className="min-w-0 truncate text-sm text-[#33312d]">{node.title}</span>
        {isFolder ? (
          <span
            className="flex size-5 cursor-pointer items-center justify-center rounded text-[#8f8b82] opacity-0 transition-opacity hover:bg-[#e8e6e0] hover:text-[#33312d] group-hover:opacity-100"
            onClick={(event) => {
              event.stopPropagation();
              onCreatePage?.(node.id);
            }}
            role="button"
            aria-label="Create page in folder"
            title="Create page in folder"
          >
            <IconPlus className="size-3.5" />
          </span>
        ) : null}
      </button>

      {isFolder && expanded ? (
        loadingChildren ? (
          <div
            className="py-2 pr-3 text-xs text-[#8f8b82]"
            style={{ paddingLeft: `${2.375 + depth * 0.875}rem` }}
          >
            Loading...
          </div>
        ) : (
          <>
            {children.map((child) => (
              <WikiTreeNode
                childrenByParentId={childrenByParentId}
                childQueries={childQueries}
                depth={depth + 1}
                expandedFolderIds={expandedFolderIds}
                expandedIds={expandedIds}
                inlineComposer={inlineComposer}
                key={child.id}
                node={child}
                onCancelInline={onCancelInline}
                onCreatePage={onCreatePage}
                onNodeClick={onNodeClick}
                onSubmitInline={onSubmitInline}
                selectedId={selectedId}
              />
            ))}
            {inlineComposer?.parentId === node.id ? (
              <InlineFolderRow
                depth={depth + 1}
                onCancel={onCancelInline}
                onSubmit={onSubmitInline}
              />
            ) : null}
            {children.length === 0 && inlineComposer?.parentId !== node.id ? (
              <div
                className="py-2 pr-3 text-xs text-[#a8a59d]"
                style={{ paddingLeft: `${2.375 + depth * 0.875}rem` }}
              >
                Empty
              </div>
            ) : null}
          </>
        )
      ) : null}
    </div>
  );
}

function WikiNodeViewer({
  childNodes,
  node,
}: {
  childNodes: WikiNode[] | undefined;
  node: WikiNode | null;
}) {
  if (!node) {
    return (
      <div className="flex min-h-[22rem] items-center justify-center px-6 text-center">
        <div>
          <IconCircleDashed className="mx-auto mb-3 size-8 text-[#b4b0a7]" stroke={1.5} />
          <div className="text-sm font-medium">No wiki item selected</div>
        </div>
      </div>
    );
  }

  const isFolder = node.kind === "folder";

  return (
    <article className="min-w-0">
      <div className="border-b border-[#eceae5] px-5 py-4">
        <div className="mb-2 flex min-w-0 items-center gap-2 text-xs text-[#77746c]">
          {isFolder ? (
            <IconFolderOpen className="size-3.5 shrink-0" stroke={1.8} />
          ) : (
            <IconFileText className="size-3.5 shrink-0" stroke={1.8} />
          )}
          <span className="truncate">{node.slug}</span>
          <span className="h-1 w-1 shrink-0 rounded-full bg-[#c9c4bb]" />
          <span className="shrink-0">{formatDate(node.updated_at)}</span>
        </div>
        <h3 className="truncate text-lg font-semibold leading-snug text-[#22211f]">
          {node.title}
        </h3>
        {!isFolder ? (
          <div className="mt-2 text-xs text-[#8f8b82]">
            {node.current_version ? `Version ${node.current_version.version}` : "No versions"}
          </div>
        ) : null}
      </div>

      <div className="px-5 py-5">
        {isFolder ? (
          <div className="text-sm text-[#69665f]">
            {childNodes
              ? childNodes.length === 1
                ? "1 item"
                : `${childNodes.length} items`
              : "Folder"}
          </div>
        ) : node.content?.trim() ? (
          <div className="whitespace-pre-wrap text-sm leading-6 text-[#33312d]">{node.content}</div>
        ) : (
          <p className="text-sm italic text-[#a8a59d]">Empty page</p>
        )}
      </div>
    </article>
  );
}

function InlineFolderRow({
  depth,
  onCancel,
  onSubmit,
}: {
  depth: number;
  onCancel: (() => void) | undefined;
  onSubmit: ((title: string) => void) | undefined;
}) {
  const [title, setTitle] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const handleKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      onSubmit?.(title);
    } else if (event.key === "Escape") {
      event.preventDefault();
      onCancel?.();
    }
  };

  const handleBlur = () => {
    const trimmed = title.trim();
    if (trimmed) {
      onSubmit?.(trimmed);
    } else {
      onCancel?.();
    }
  };

  return (
    <div
      className="grid min-h-9 w-full grid-cols-[1rem_1rem_minmax(0,1fr)] items-center gap-2 pr-3"
      style={{ paddingLeft: `${0.5 + depth * 0.875}rem` }}
    >
      <span className="size-3.5" />
      <IconFolder className="size-4 text-[#7a756b]" stroke={1.8} />
      <input
        ref={inputRef}
        className="min-w-0 bg-transparent text-sm text-[#33312d] outline-none placeholder:text-[#a8a59d]"
        onBlur={handleBlur}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="New folder"
        type="text"
        value={title}
      />
    </div>
  );
}

function WikiComposer({
  error,
  onClose,
  onSubmit,
  pending,
}: {
  error: Error | null;
  onClose: () => void;
  onSubmit: (input: { content: string; title: string }) => void;
  pending: boolean;
}) {
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");

  const submit = (event?: FormEvent) => {
    event?.preventDefault();
    const trimmedTitle = title.trim();
    if (!trimmedTitle || pending) return;
    onSubmit({ content, title: trimmedTitle });
  };

  const handleKeyDown = (event: KeyboardEvent) => {
    if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
      event.preventDefault();
      submit();
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/24 px-4 pt-[14vh] backdrop-blur-[1px]">
      <button
        aria-label="Close wiki composer"
        className="absolute inset-0 cursor-default"
        onClick={onClose}
        type="button"
      />
      <form
        className="relative flex min-h-[20rem] w-[min(42rem,calc(100vw-2rem))] flex-col rounded-[1.15rem] bg-card text-card-foreground shadow-[0_18px_55px_rgba(15,23,42,0.2),0_1px_8px_rgba(15,23,42,0.08)] ring-1 ring-black/10"
        onSubmit={submit}
      >
        <div className="flex items-center justify-between px-4 pt-3.5">
          <span className="text-sm font-medium text-foreground">New wiki item</span>
          <button
            aria-label="Close wiki composer"
            className="flex size-7 items-center justify-center rounded-full text-muted-foreground hover:bg-muted hover:text-foreground"
            onClick={onClose}
            type="button"
          >
            <IconX className="size-4" />
          </button>
        </div>

        <div className="flex-1 px-5 pb-3 pt-5">
          <input
            autoFocus
            className="block h-8 w-full bg-transparent font-semibold text-foreground outline-none placeholder:text-[#9da0a6]"
            onChange={(event) => setTitle(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Page title"
            value={title}
          />
          <Textarea
            className="mt-3 min-h-32 rounded-lg bg-transparent px-0 py-0 leading-6 shadow-none focus-visible:ring-0"
            onChange={(event) => setContent(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Add content..."
            value={content}
          />
          {error ? <div className="mt-3 text-xs text-destructive">{error.message}</div> : null}
        </div>

        <div className="mt-auto flex items-center justify-end px-4 pb-4">
          <Button disabled={pending || !title.trim()} type="submit">
            Create page
          </Button>
        </div>
      </form>
    </div>
  );
}

function wikiNodesQueryKey(projectId: string, parentId: string | null) {
  return ["wiki", projectId, "nodes", parentId ?? "root"] as const;
}

function sortWikiNodes(nodes: WikiNode[]) {
  return [...nodes].sort((a, b) => {
    if (a.kind !== b.kind) {
      return a.kind === "folder" ? -1 : 1;
    }
    return a.title.localeCompare(b.title);
  });
}

function formatDate(value: string | null) {
  if (!value) return "No update";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(new Date(value));
}
