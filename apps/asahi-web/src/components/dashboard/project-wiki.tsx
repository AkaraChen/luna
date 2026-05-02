import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";
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
  IconFolderPlus,
  IconPlus,
  IconTrash,
} from "@tabler/icons-react";

import {
  createWikiNode,
  deleteWikiNode,
  fetchWikiNodes,
  updateWikiNode,
  type CreateWikiNodeInput,
  type Project,
  type UpdateWikiNodeInput,
  type WikiNode,
  type WikiNodeKind,
  type WikiNodeListResponse,
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
import { RichTextEditor } from "@/components/ui/rich-text-editor";
import { cn } from "@/lib/utils";

type InlineComposerState = {
  parentId: string | null;
  kind: WikiNodeKind;
};

type WikiContextMenuState = {
  node: WikiNode;
  x: number;
  y: number;
};

type WikiNodesQueryResult = UseQueryResult<WikiNodeListResponse, Error>;

export function ProjectWiki({ project }: { project: Project }) {
  const queryClient = useQueryClient();
  const [expandedFolderIds, setExpandedFolderIds] = useState<Set<string>>(() => new Set());
  const [selectedNode, setSelectedNode] = useState<WikiNode | null>(null);
  const [inlineComposer, setInlineComposer] = useState<InlineComposerState | null>(null);
  const [contextMenu, setContextMenu] = useState<WikiContextMenuState | null>(null);
  const [renamingNodeId, setRenamingNodeId] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<WikiNode | null>(null);

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
    ? (loadedNodes.find((node) => node.id === selectedNode.id) ?? selectedNode)
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

  const renameMutation = useMutation({
    mutationFn: ({ node, title }: { node: WikiNode; title: string }) =>
      updateWikiNode(project.id, node.id, {
        actor_kind: "human",
        summary: "Rename wiki node",
        title,
      }),
    onSuccess: (updatedNode) => {
      queryClient.setQueryData<WikiNodeListResponse>(
        wikiNodesQueryKey(project.id, updatedNode.parent_id ?? null),
        (current) =>
          current
            ? {
                nodes: sortWikiNodes(
                  current.nodes.map((node) => (node.id === updatedNode.id ? updatedNode : node)),
                ),
              }
            : current,
      );
      setSelectedNode((current) => (current?.id === updatedNode.id ? updatedNode : current));
      setRenamingNodeId(null);
      void queryClient.invalidateQueries({ queryKey: ["wiki", project.id] });
    },
  });

  const deleteMutation = useMutation({
    mutationFn: (node: WikiNode) => deleteWikiNode(project.id, node.id, { actorKind: "human" }),
    onSuccess: (_deletedNode, node) => {
      const deletedIds = collectLoadedWikiNodeIds(node.id, childrenByParentId);
      queryClient.setQueryData<WikiNodeListResponse>(
        wikiNodesQueryKey(project.id, node.parent_id ?? null),
        (current) =>
          current
            ? {
                nodes: current.nodes.filter((candidate) => candidate.id !== node.id),
              }
            : current,
      );
      queryClient.removeQueries({ queryKey: wikiNodesQueryKey(project.id, node.id) });
      setExpandedFolderIds((current) => {
        const next = new Set(current);
        next.delete(node.id);
        return next;
      });
      setInlineComposer((current) =>
        current?.parentId && deletedIds.has(current.parentId) ? null : current,
      );
      setRenamingNodeId((current) => (current && deletedIds.has(current) ? null : current));
      setSelectedNode((current) => {
        if (!current) return current;
        return deletedIds.has(current.id) || current.parent_id === node.id ? null : current;
      });
      setDeleteTarget(null);
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
    return activeNode?.kind === "folder" ? activeNode.id : (activeNode?.parent_id ?? null);
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

  const handleCreateNodeInFolder = (folderId: string, kind: WikiNodeKind) => {
    createMutation.reset();
    setExpandedFolderIds((current) => {
      const next = new Set(current);
      next.add(folderId);
      return next;
    });
    setInlineComposer({ parentId: folderId, kind });
  };

  const openNodeContextMenu = (event: ReactMouseEvent, node: WikiNode) => {
    event.preventDefault();
    setContextMenu({ node, x: event.clientX, y: event.clientY });
  };

  const startRename = (node: WikiNode) => {
    renameMutation.reset();
    setInlineComposer(null);
    setRenamingNodeId(node.id);
  };

  const submitRename = (node: WikiNode, title: string) => {
    const trimmed = title.trim();
    if (!trimmed || renameMutation.isPending) return;
    if (trimmed === node.title) {
      setRenamingNodeId(null);
      return;
    }
    renameMutation.mutate({ node, title: trimmed });
  };

  const openDeleteDialog = (node: WikiNode) => {
    deleteMutation.reset();
    setDeleteTarget(node);
  };

  const rootNodes = childrenByParentId.get(null) ?? [];
  const renamingPendingId = renameMutation.isPending
    ? (renameMutation.variables?.node.id ?? null)
    : null;
  const operationError =
    createMutation.error ?? renameMutation.error ?? deleteMutation.error ?? null;

  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-auto lg:grid lg:grid-cols-[minmax(15rem,18rem)_minmax(0,1fr)] lg:overflow-hidden">
      <div className="min-h-0 border-b border-[#eceae5] lg:flex lg:flex-col lg:border-b-0 lg:border-r">
        <div className="flex h-12 shrink-0 items-center justify-between px-4">
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

        <div className="max-h-[28rem] overflow-auto pb-2 lg:min-h-0 lg:flex-1 lg:max-h-none">
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
                  onCancelRename={() => setRenamingNodeId(null)}
                  onCreateNode={handleCreateNodeInFolder}
                  onNodeClick={handleNodeClick}
                  onOpenContextMenu={openNodeContextMenu}
                  onSubmitInline={submitInline}
                  onSubmitRename={submitRename}
                  renamePendingId={renamingPendingId}
                  renamingNodeId={renamingNodeId}
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
              {operationError ? (
                <div className="px-4 py-2 text-xs text-destructive">{operationError.message}</div>
              ) : null}
            </>
          )}
        </div>
      </div>

      <WikiNodeViewer node={activeNode} childNodes={activeChildren} projectId={project.id} />
      {contextMenu ? (
        <WikiTreeContextMenu
          menu={contextMenu}
          onClose={() => setContextMenu(null)}
          onDelete={openDeleteDialog}
          onRename={startRename}
        />
      ) : null}
      <AlertDialog
        open={deleteTarget != null}
        onOpenChange={(open) => {
          if (!open) setDeleteTarget(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete {deleteTarget?.title}?</AlertDialogTitle>
            <AlertDialogDescription>
              {deleteTarget?.kind === "folder"
                ? "This will delete the folder and its nested wiki items."
                : "This will delete the wiki page."}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => setDeleteTarget(null)}>Cancel</AlertDialogCancel>
            <AlertDialogAction
              disabled={deleteMutation.isPending || !deleteTarget}
              onClick={() => {
                if (deleteTarget) deleteMutation.mutate(deleteTarget);
              }}
              variant="destructive"
            >
              Delete
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
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
  onCancelRename,
  onNodeClick,
  onCreateNode,
  onOpenContextMenu,
  selectedId,
  onSubmitInline,
  onSubmitRename,
  onCancelInline,
  renamePendingId,
  renamingNodeId,
}: {
  childrenByParentId: Map<string | null, WikiNode[]>;
  childQueries: WikiNodesQueryResult[];
  depth: number;
  expandedFolderIds: Set<string>;
  expandedIds: string[];
  inlineComposer?: InlineComposerState | null;
  node: WikiNode;
  onCancelRename?: () => void;
  onNodeClick: (node: WikiNode) => void;
  onCreateNode?: (folderId: string, kind: WikiNodeKind) => void;
  onOpenContextMenu: (event: ReactMouseEvent, node: WikiNode) => void;
  selectedId: string | null;
  onSubmitInline?: (title: string) => void;
  onSubmitRename?: (node: WikiNode, title: string) => void;
  onCancelInline?: () => void;
  renamePendingId: string | null;
  renamingNodeId: string | null;
}) {
  const expanded = expandedFolderIds.has(node.id);
  const isFolder = node.kind === "folder";
  const isSelected = selectedId === node.id;
  const isRenaming = renamingNodeId === node.id;
  const children = childrenByParentId.get(node.id) ?? [];
  const childQueryIndex = expandedIds.indexOf(node.id);
  const loadingChildren = childQueryIndex >= 0 && childQueries[childQueryIndex]?.isLoading;

  return (
    <div>
      {isRenaming ? (
        <RenameRow
          depth={depth}
          expanded={expanded}
          kind={node.kind}
          onCancel={onCancelRename}
          onSubmit={(title) => onSubmitRename?.(node, title)}
          pending={renamePendingId === node.id}
          title={node.title}
        />
      ) : (
        <div
          className={cn(
            "group grid min-h-9 w-full grid-cols-[minmax(0,1fr)_auto_auto] items-center gap-1 pr-2 hover:bg-[#f7f6f2]",
            isSelected && "bg-[#f2f1ec]",
          )}
          onContextMenu={(event) => onOpenContextMenu(event, node)}
          style={{ paddingLeft: `${0.5 + depth * 0.875}rem` }}
        >
          <button
            className="grid min-h-9 min-w-0 grid-cols-[1rem_1rem_minmax(0,1fr)] items-center gap-2 text-left"
            onClick={() => onNodeClick(node)}
            type="button"
          >
            {isFolder ? (
              <IconChevronRight
                className={cn(
                  "size-3.5 text-[#8f8b82] transition-transform",
                  expanded && "rotate-90",
                )}
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
          </button>
          {isFolder ? (
            <button
              aria-label={`Create subfolder in ${node.title}`}
              className="flex size-6 items-center justify-center rounded text-[#8f8b82] opacity-0 transition-opacity hover:bg-[#e8e6e0] hover:text-[#33312d] group-hover:opacity-100 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#c9c4bb]"
              onClick={(event) => {
                event.stopPropagation();
                onCreateNode?.(node.id, "folder");
              }}
              title="Create subfolder"
              type="button"
            >
              <IconFolderPlus className="size-3.5" />
            </button>
          ) : null}
          {isFolder ? (
            <button
              aria-label={`Create page in ${node.title}`}
              className="flex size-6 items-center justify-center rounded text-[#8f8b82] opacity-0 transition-opacity hover:bg-[#e8e6e0] hover:text-[#33312d] group-hover:opacity-100 focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#c9c4bb]"
              onClick={(event) => {
                event.stopPropagation();
                onCreateNode?.(node.id, "page");
              }}
              title="Create page in folder"
              type="button"
            >
              <IconPlus className="size-3.5" />
            </button>
          ) : null}
        </div>
      )}

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
                onCancelRename={onCancelRename}
                onCreateNode={onCreateNode}
                onNodeClick={onNodeClick}
                onOpenContextMenu={onOpenContextMenu}
                onSubmitInline={onSubmitInline}
                onSubmitRename={onSubmitRename}
                renamePendingId={renamePendingId}
                renamingNodeId={renamingNodeId}
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

function WikiTreeContextMenu({
  menu,
  onClose,
  onDelete,
  onRename,
}: {
  menu: WikiContextMenuState;
  onClose: () => void;
  onDelete: (node: WikiNode) => void;
  onRename: (node: WikiNode) => void;
}) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClick = (event: globalThis.MouseEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) {
        onClose();
      }
    };
    const handleKey = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", handleClick);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handleClick);
      document.removeEventListener("keydown", handleKey);
    };
  }, [onClose]);

  const left = typeof window === "undefined" ? menu.x : Math.min(menu.x, window.innerWidth - 152);
  const top = typeof window === "undefined" ? menu.y : Math.min(menu.y, window.innerHeight - 76);

  return (
    <div
      ref={menuRef}
      className="fixed z-50 min-w-36 rounded-md border border-[#eceae5] bg-white py-1 shadow-md"
      role="menu"
      style={{ left, top }}
    >
      <button
        className="flex h-8 w-full items-center gap-2 px-3 text-left text-xs text-[#33312d] hover:bg-[#f7f6f2]"
        onClick={() => {
          onRename(menu.node);
          onClose();
        }}
        role="menuitem"
        type="button"
      >
        <IconEdit className="size-3.5" />
        Rename
      </button>
      <button
        className="flex h-8 w-full items-center gap-2 px-3 text-left text-xs text-destructive hover:bg-destructive/10"
        onClick={() => {
          onDelete(menu.node);
          onClose();
        }}
        role="menuitem"
        type="button"
      >
        <IconTrash className="size-3.5" />
        Delete
      </button>
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
    <article className="min-w-0 lg:min-h-0 lg:overflow-auto">
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

      <div className="px-5 pb-5 pt-0">
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
            <RichTextEditor content={contentDraft} onChange={(html) => setContentDraft(html)} />
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
        ) : node.content?.trim() && node.content.trim() !== "<p></p>" ? (
          <div
            className="prose prose-sm max-w-none"
            dangerouslySetInnerHTML={{ __html: node.content }}
          />
        ) : (
          <p className="text-sm italic text-[#a8a59d]">Empty page</p>
        )}
      </div>
    </article>
  );
}

function RenameRow({
  depth,
  expanded,
  kind,
  onCancel,
  onSubmit,
  pending,
  title,
}: {
  depth: number;
  expanded: boolean;
  kind: WikiNodeKind;
  onCancel: (() => void) | undefined;
  onSubmit: ((title: string) => void) | undefined;
  pending: boolean;
  title: string;
}) {
  const [draft, setDraft] = useState(title);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const submit = () => {
    if (pending) return;
    const trimmed = draft.trim();
    if (trimmed) {
      onSubmit?.(trimmed);
    } else {
      onCancel?.();
    }
  };

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      submit();
    } else if (event.key === "Escape") {
      event.preventDefault();
      onCancel?.();
    }
  };

  return (
    <div
      className="grid min-h-9 w-full grid-cols-[1rem_1rem_minmax(0,1fr)_auto] items-center gap-2 pr-2"
      style={{ paddingLeft: `${0.5 + depth * 0.875}rem` }}
    >
      {kind === "folder" ? (
        <IconChevronRight
          className={cn("size-3.5 text-[#8f8b82] transition-transform", expanded && "rotate-90")}
        />
      ) : (
        <span className="size-3.5" />
      )}
      {kind === "folder" ? (
        expanded ? (
          <IconFolderOpen className="size-4 text-[#6f6b62]" stroke={1.8} />
        ) : (
          <IconFolder className="size-4 text-[#7a756b]" stroke={1.8} />
        )
      ) : (
        <IconFileText className="size-4 text-[#6d7180]" stroke={1.8} />
      )}
      <input
        ref={inputRef}
        className="min-w-0 bg-transparent text-sm text-[#33312d] outline-none placeholder:text-[#a8a59d] disabled:opacity-60"
        disabled={pending}
        onBlur={submit}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={handleKeyDown}
        type="text"
        value={draft}
      />
      <span className="size-5" />
    </div>
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

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
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

function collectLoadedWikiNodeIds(
  rootId: string,
  childrenByParentId: Map<string | null, WikiNode[]>,
) {
  const ids = new Set([rootId]);
  const queue = [rootId];

  while (queue.length > 0) {
    const parentId = queue.shift();
    if (!parentId) continue;
    const children = childrenByParentId.get(parentId) ?? [];
    children.forEach((child) => {
      ids.add(child.id);
      queue.push(child.id);
    });
  }

  return ids;
}

function formatDate(value: string | null) {
  if (!value) return "No update";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
  }).format(new Date(value));
}
