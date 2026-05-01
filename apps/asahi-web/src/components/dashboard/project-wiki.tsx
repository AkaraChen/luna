import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
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
  IconEdit,
  IconFileText,
  IconFolder,
  IconFolderOpen,
  IconPlus,
} from "@tabler/icons-react";

import {
  createWikiNode,
  fetchWikiNodes,
  updateWikiNode,
  type CreateWikiNodeInput,
  type Project,
  type UpdateWikiNodeInput,
  type WikiNode,
  type WikiNodeKind,
  type WikiNodeListResponse,
} from "@/api/asahi";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

type InlineComposerState = {
  parentId: string | null;
  kind: WikiNodeKind;
};

type WikiNodesQueryResult = UseQueryResult<WikiNodeListResponse, Error>;

export function ProjectWiki({ project }: { project: Project }) {
  const queryClient = useQueryClient();
  const [expandedFolderIds, setExpandedFolderIds] = useState<Set<string>>(() => new Set());
  const [selectedNode, setSelectedNode] = useState<WikiNode | null>(null);
  const [inlineComposer, setInlineComposer] = useState<InlineComposerState | null>(null);

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
    if (node.kind === "folder") {
      toggleFolder(node.id);
    } else {
      setSelectedNode(node);
    }
  };

  const resolveParentId = (): string | null => {
    return activeNode?.kind === "folder" ? activeNode.id : activeNode?.parent_id ?? null;
  };

  const openInlineComposer = (kind: WikiNodeKind) => {
    createMutation.reset();
    const parentId = kind === "page" ? null : resolveParentId();
    setInlineComposer({ parentId, kind });
    if (parentId && kind === "folder") {
      setExpandedFolderIds((current) => {
        const next = new Set(current);
        next.add(parentId);
        return next;
      });
    }
  };

  const submitInline = (title: string) => {
    const trimmed = title.trim();
    if (!trimmed || !inlineComposer || createMutation.isPending) return;
    createMutation.mutate({
      actor_kind: "human",
      kind: inlineComposer.kind,
      parent_id: inlineComposer.parentId ?? undefined,
      title: trimmed,
    });
    setInlineComposer(null);
  };

  const cancelInline = () => {
    setInlineComposer(null);
  };

  const handleCreatePageInFolder = (folderId: string) => {
    createMutation.reset();
    setExpandedFolderIds((current) => {
      const next = new Set(current);
      next.add(folderId);
      return next;
    });
    setInlineComposer({ parentId: folderId, kind: "page" });
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
              onClick={() => openInlineComposer("folder")}
              size="icon-xs"
              title="Create folder"
              type="button"
              variant="ghost"
            >
              <IconFolder className="size-3.5" />
            </Button>
            <Button
              aria-label="Create page"
              onClick={() => openInlineComposer("page")}
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
                  onCancelInline={cancelInline}
                  onCreatePage={handleCreatePageInFolder}
                  onNodeClick={handleNodeClick}
                  onSubmitInline={submitInline}
                  selectedId={activeNode?.id ?? null}
                />
              ))}
              {inlineComposer?.parentId === null ? (
                <InlineRow
                  depth={0}
                  kind={inlineComposer.kind}
                  onCancel={cancelInline}
                  onSubmit={submitInline}
                />
              ) : null}
              {rootNodes.length === 0 && !inlineComposer ? (
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

      <WikiNodeViewer
        node={activeNode}
        childNodes={activeChildren}
        projectId={project.id}
      />
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
              <InlineRow
                depth={depth + 1}
                kind={inlineComposer.kind}
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
  projectId,
}: {
  childNodes: WikiNode[] | undefined;
  node: WikiNode | null;
  projectId: string;
}) {
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [contentDraft, setContentDraft] = useState("");

  const updateMutation = useMutation({
    mutationFn: (input: UpdateWikiNodeInput) => updateWikiNode(projectId, node!.id, input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["wiki", projectId] });
      setEditing(false);
    },
  });

  useEffect(() => {
    if (node) {
      setTitleDraft(node.title);
      setContentDraft(node.content ?? "");
    }
    setEditing(false);
  }, [node?.id]);

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
      <div className="px-5 py-4">
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

        {editing ? (
          <input
            autoFocus
            className="block w-full bg-transparent text-lg font-semibold leading-snug text-[#22211f] outline-none placeholder:text-[#a8a59d]"
            onChange={(e) => setTitleDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                updateMutation.mutate({ title: titleDraft, content: contentDraft || null });
              }
            }}
            value={titleDraft}
          />
        ) : (
          <div className="flex items-center justify-between gap-3">
            <h3 className="truncate text-lg font-semibold leading-snug text-[#22211f]">
              {node.title}
            </h3>
            {!isFolder && (
              <div className="flex shrink-0 items-center gap-2">
                <span className="text-xs text-[#8f8b82]">
                  {node.current_version ? `Version ${node.current_version.version}` : "No versions"}
                </span>
                <Button
                  aria-label="Edit page"
                  className="text-[#8a877e] hover:text-[#33312d]"
                  onClick={() => {
                    setEditing(true);
                    setTitleDraft(node.title);
                    setContentDraft(node.content ?? "");
                  }}
                  size="icon-xs"
                  type="button"
                  variant="ghost"
                >
                  <IconEdit className="size-3.5" />
                </Button>
              </div>
            )}
          </div>
        )}
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
        ) : editing ? (
          <div>
            <Textarea
              className="min-h-48 resize-none rounded-lg bg-transparent px-0 py-0 leading-6 shadow-none focus-visible:ring-0"
              onChange={(e) => setContentDraft(e.target.value)}
              placeholder="Add content..."
              value={contentDraft}
            />
            <div className="mt-3 flex items-center gap-2">
              <Button
                disabled={updateMutation.isPending || !titleDraft.trim()}
                onClick={() =>
                  updateMutation.mutate({ title: titleDraft, content: contentDraft || null })
                }
                size="sm"
                type="button"
              >
                Save
              </Button>
              <Button
                onClick={() => {
                  setEditing(false);
                  setTitleDraft(node.title);
                  setContentDraft(node.content ?? "");
                }}
                size="sm"
                type="button"
                variant="ghost"
              >
                Cancel
              </Button>
              {updateMutation.error ? (
                <span className="text-xs text-destructive">{updateMutation.error.message}</span>
              ) : null}
            </div>
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

function InlineRow({
  depth,
  kind,
  onCancel,
  onSubmit,
}: {
  depth: number;
  kind: WikiNodeKind;
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
      {kind === "folder" ? (
        <IconFolder className="size-4 text-[#7a756b]" stroke={1.8} />
      ) : (
        <IconFileText className="size-4 text-[#6d7180]" stroke={1.8} />
      )}
      <input
        ref={inputRef}
        className="min-w-0 bg-transparent text-sm text-[#33312d] outline-none placeholder:text-[#a8a59d]"
        onBlur={handleBlur}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={kind === "folder" ? "New folder" : "New page"}
        type="text"
        value={title}
      />
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
