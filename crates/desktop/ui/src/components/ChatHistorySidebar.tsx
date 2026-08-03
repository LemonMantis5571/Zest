import { useEffect, useRef, useState } from "react";
import {
  ChevronRightIcon,
  ChevronsLeftIcon,
  ChevronsRightIcon,
  FolderIcon,
  FolderOpenIcon,
  PlusIcon,
  Trash2Icon,
} from "lucide-react";

import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import { getBackend } from "@/lib/backend";
import type { ProjectChats, ThreadSummary } from "@/lib/types";
import { cn } from "@/lib/utils";

type Props = {
  open: boolean;
  activeThreadId: string;
  activeProjectPath: string;
  sending: boolean;
  onOpenChange: (open: boolean) => void;
  onNewChat: () => void;
  onLoadThread: (id: string) => void;
  onOpenProjectChat: (options: {
    root: string;
    threadId?: string;
    newThread?: boolean;
  }) => Promise<void>;
  onDeleteThread: (id: string, projectPath: string) => Promise<void>;
  onOpenFolder: () => void;
};

function threadTitle(thread: ThreadSummary) {
  const title = thread.title?.trim();
  if (title) return title;
  return "Untitled chat";
}

/** Compact relative age like the reference UI (6m, 8h, 2d). */
function formatAge(epochSecs: number) {
  if (!epochSecs) return "";
  const now = Math.floor(Date.now() / 1000);
  const delta = Math.max(0, now - epochSecs);
  if (delta < 60) return `${delta}s`;
  if (delta < 3600) return `${Math.floor(delta / 60)}m`;
  if (delta < 86400) return `${Math.floor(delta / 3600)}h`;
  if (delta < 86400 * 14) return `${Math.floor(delta / 86400)}d`;
  try {
    return new Intl.DateTimeFormat(undefined, {
      month: "short",
      day: "numeric",
    }).format(new Date(epochSecs * 1000));
  } catch {
    return "";
  }
}

const STORAGE_KEY = "zest.sidebarOpen";
const EXPANDED_KEY = "zest.sidebarProjectsExpanded";

export function readSidebarOpen(): boolean {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw === null) return true;
    return raw === "1" || raw === "true";
  } catch {
    return true;
  }
}

export function writeSidebarOpen(open: boolean) {
  try {
    localStorage.setItem(STORAGE_KEY, open ? "1" : "0");
  } catch {
    /* ignore */
  }
}

function readExpandedMap(): Record<string, boolean> {
  try {
    const raw = localStorage.getItem(EXPANDED_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, boolean>;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function writeExpandedMap(map: Record<string, boolean>) {
  try {
    localStorage.setItem(EXPANDED_KEY, JSON.stringify(map));
  } catch {
    /* ignore */
  }
}

export function ChatHistorySidebar({
  open,
  activeThreadId,
  activeProjectPath,
  sending,
  onOpenChange,
  onNewChat,
  onLoadThread,
  onOpenProjectChat,
  onDeleteThread,
  onOpenFolder,
}: Props) {
  const [projects, setProjects] = useState<ProjectChats[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const [expanded, setExpanded] = useState<Record<string, boolean>>(readExpandedMap);
  const [pendingDelete, setPendingDelete] = useState<{
    thread: ThreadSummary;
    projectPath: string;
  } | null>(null);
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    getBackend()
      .listChatProjects()
      .then((list) => {
        if (!cancelled) setProjects(list);
      })
      .catch((err) => {
        if (!cancelled) setError(String(err));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, activeThreadId, activeProjectPath, tick]);

  const wasSending = useRef(false);
  useEffect(() => {
    if (sending) {
      wasSending.current = true;
      return;
    }
    if (open && wasSending.current) {
      wasSending.current = false;
      setTick((n) => n + 1);
    }
  }, [open, sending]);

  function isExpanded(project: ProjectChats) {
    if (project.path in expanded) return expanded[project.path];
    // Default: open active project and any project that already has chats.
    return project.active || project.threads.length > 0;
  }

  function toggleExpanded(path: string) {
    setExpanded((prev) => {
      const project = projects.find((p) => p.path === path);
      const currently =
        path in prev
          ? prev[path]
          : Boolean(project && (project.active || project.threads.length > 0));
      const next = { ...prev, [path]: !currently };
      writeExpandedMap(next);
      return next;
    });
  }

  async function confirmDelete() {
    if (!pendingDelete) return;
    setDeleting(true);
    try {
      await onDeleteThread(pendingDelete.thread.id, pendingDelete.projectPath);
      setPendingDelete(null);
      setTick((n) => n + 1);
    } catch {
      /* parent toasts */
    } finally {
      setDeleting(false);
    }
  }

  return (
    <aside
      className={cn(
        "relative flex h-full shrink-0 flex-col border-r border-border/60 bg-[var(--sidebar)] text-[var(--sidebar-foreground)] transition-[width] duration-200 ease-out",
        open ? "w-[260px]" : "w-11"
      )}
    >
      <div
        className={cn(
          "flex h-[49px] shrink-0 items-center border-b border-border/60",
          open ? "justify-between gap-1 px-2" : "justify-center px-1"
        )}
      >
        {open ? (
          <>
            <span className="truncate px-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
              Projects
            </span>
            <div className="flex items-center gap-0.5">
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                title="Open project folder"
                disabled={sending}
                onClick={onOpenFolder}
              >
                <FolderOpenIcon />
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                title="Collapse sidebar"
                aria-expanded={open}
                onClick={() => onOpenChange(false)}
              >
                <ChevronsLeftIcon />
              </Button>
            </div>
          </>
        ) : (
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            title="Expand projects"
            aria-expanded={open}
            onClick={() => onOpenChange(true)}
          >
            <ChevronsRightIcon />
          </Button>
        )}
      </div>

      {open ? (
        <div className="min-h-0 flex-1 overflow-y-auto px-1.5 py-2">
          {loading && projects.length === 0 ? (
            <p className="px-2 py-1 text-xs text-muted-foreground">Loading…</p>
          ) : error ? (
            <p className="px-2 py-1 text-xs text-destructive">{error}</p>
          ) : projects.length === 0 ? (
            <p className="px-2 py-1 text-xs text-muted-foreground">
              Open a project folder to start.
            </p>
          ) : (
            <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
              {projects.map((project) => {
                const expandedHere = isExpanded(project);
                // Only worth the visual noise once a project actually has
                // chats under more than one provider.
                const showsProviders =
                  new Set(
                    project.threads
                      .map((t) => t.providerId)
                      .filter((id): id is string => Boolean(id))
                  ).size > 1;
                return (
                  <li key={project.path} className="min-w-0">
                    <div className="group/project flex items-center gap-0.5">
                      <button
                        type="button"
                        title={project.path}
                        onClick={() => toggleExpanded(project.path)}
                        className={cn(
                          "flex min-w-0 flex-1 cursor-pointer items-center gap-1.5 rounded-md px-1.5 py-1.5 text-left outline-none transition-colors",
                          "hover:bg-[var(--sidebar-accent)] focus-visible:ring-2 focus-visible:ring-ring/50",
                          project.active && "text-foreground"
                        )}
                      >
                        <ChevronRightIcon
                          className={cn(
                            "size-3 shrink-0 text-muted-foreground transition-transform",
                            expandedHere && "rotate-90"
                          )}
                        />
                        {expandedHere ? (
                          <FolderOpenIcon className="size-3.5 shrink-0 text-muted-foreground" />
                        ) : (
                          <FolderIcon className="size-3.5 shrink-0 text-muted-foreground" />
                        )}
                        <span className="truncate text-[13px] font-medium">
                          {project.name}
                        </span>
                      </button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="icon-xs"
                        title={`New chat in ${project.name}`}
                        disabled={sending}
                        className="shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover/project:opacity-100 focus-visible:opacity-100"
                        onClick={() => {
                          void onOpenProjectChat({
                            root: project.path,
                            newThread: true,
                          });
                        }}
                      >
                        <PlusIcon />
                      </Button>
                    </div>

                    {expandedHere ? (
                      <ul className="m-0 mt-0.5 mb-1 flex list-none flex-col gap-0.5 p-0 pl-3">
                        {project.threads.length === 0 ? (
                          <li className="px-2 py-1 text-[11px] text-muted-foreground/80">
                            No chats yet
                          </li>
                        ) : (
                          project.threads.map((thread) => {
                            const active =
                              project.active && thread.id === activeThreadId;
                            const title = threadTitle(thread);
                            const age = formatAge(thread.updatedAt);
                            // Threads belong to one provider for their whole
                            // life (wire history is provider-specific), so
                            // switching provider shows a different set. Without
                            // this tag that reads as chats disappearing.
                            const owner = showsProviders
                              ? thread.providerId
                              : undefined;
                            return (
                              <li key={thread.id} className="group/thread relative">
                                <button
                                  type="button"
                                  disabled={sending || active}
                                  onClick={() => {
                                    if (project.active) {
                                      onLoadThread(thread.id);
                                      return;
                                    }
                                    void onOpenProjectChat({
                                      root: project.path,
                                      threadId: thread.id,
                                    });
                                  }}
                                  className={cn(
                                    "flex w-full cursor-pointer items-center gap-2 rounded-md py-1.5 pr-7 pl-2 text-left outline-none transition-colors",
                                    "hover:bg-[var(--sidebar-accent)] hover:text-[var(--sidebar-accent-foreground)]",
                                    "focus-visible:ring-2 focus-visible:ring-ring/50",
                                    active
                                      ? "bg-[var(--sidebar-accent)] text-[var(--sidebar-accent-foreground)]"
                                      : "disabled:pointer-events-none"
                                  )}
                                >
                                  <span className="min-w-0 flex-1 truncate text-[13px]">
                                    {title}
                                  </span>
                                  {owner ? (
                                    <span
                                      title={`This chat belongs to ${owner}. Switch to ${owner} to reopen it.`}
                                      className="shrink-0 rounded-sm bg-white/[0.06] px-1 py-px font-mono text-[9px] tracking-tight text-muted-foreground"
                                    >
                                      {owner}
                                    </span>
                                  ) : null}
                                  {age ? (
                                    <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
                                      {age}
                                    </span>
                                  ) : null}
                                </button>
                                <Button
                                  type="button"
                                  variant="ghost"
                                  size="icon-xs"
                                  title={`Delete “${title}”`}
                                  disabled={sending || deleting}
                                  className={cn(
                                    "absolute top-1 right-0.5 text-muted-foreground transition-opacity",
                                    "hover:bg-destructive/15 hover:text-destructive",
                                    "focus-visible:opacity-100",
                                    // Keep trash visible on the open chat; otherwise hover-only.
                                    active
                                      ? "opacity-100"
                                      : "opacity-0 group-hover/thread:opacity-100"
                                  )}
                                  onClick={(e) => {
                                    e.stopPropagation();
                                    setPendingDelete({
                                      thread,
                                      projectPath: project.path,
                                    });
                                  }}
                                >
                                  <Trash2Icon />
                                </Button>
                              </li>
                            );
                          })
                        )}
                      </ul>
                    ) : null}
                  </li>
                );
              })}
            </ul>
          )}

          {open ? (
            <div className="mt-2 border-t border-border/40 px-1 pt-2">
              <button
                type="button"
                disabled={sending}
                onClick={onNewChat}
                className="flex w-full cursor-pointer items-center gap-2 rounded-md px-2 py-1.5 text-left text-[12px] text-muted-foreground outline-none hover:bg-[var(--sidebar-accent)] hover:text-foreground disabled:pointer-events-none"
              >
                <PlusIcon className="size-3.5" />
                New chat here
              </button>
            </div>
          ) : null}
        </div>
      ) : null}

      <ConfirmDialog
        open={pendingDelete != null}
        title="Delete chat?"
        description={
          pendingDelete
            ? `“${threadTitle(pendingDelete.thread)}” will be permanently removed.`
            : ""
        }
        confirmLabel="Delete"
        cancelLabel="Cancel"
        destructive
        busy={deleting}
        onCancel={() => {
          if (!deleting) setPendingDelete(null);
        }}
        onConfirm={() => {
          void confirmDelete();
        }}
      />
    </aside>
  );
}
