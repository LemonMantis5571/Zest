import { useEffect, useMemo, useRef, useState } from "react";
import {
  Clock3Icon,
  ChevronRightIcon,
  ChevronsLeftIcon,
  ChevronsRightIcon,
  FolderIcon,
  FolderOpenIcon,
  GitForkIcon,
  PinIcon,
  PlusIcon,
  SearchIcon,
  SquarePenIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";

import { BrandMark } from "@/components/BrandMark";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { ProviderIcon } from "@/components/ProviderIcon";
import { Button } from "@/components/ui/button";
import { getBackend } from "@/lib/backend";
import type { ProjectChats, ThreadSummary } from "@/lib/types";
import { cn } from "@/lib/utils";

type Props = {
  open: boolean;
  activeThreadId: string;
  activeProjectPath: string;
  activeProviderId: string;
  sending: boolean;
  onOpenChange: (open: boolean) => void;
  onNewChat: () => void;
  onOpenProjectChat: (options: {
    root: string;
    threadId?: string;
    newThread?: boolean;
    providerId?: string;
    copyThread?: boolean;
  }) => Promise<void>;
  onForkThread: () => Promise<void>;
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

function matchesQuery(project: ProjectChats, thread: ThreadSummary, query: string) {
  if (!query) return true;
  return (
    project.name.toLowerCase().includes(query) ||
    threadTitle(thread).toLowerCase().includes(query)
  );
}

function navItemClass(active = false) {
  return cn(
    "flex h-8 w-full cursor-pointer items-center gap-2 rounded-md px-2 text-left text-[13px] outline-none transition-colors",
    "hover:bg-[var(--sidebar-accent)] hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50",
    active && "bg-[var(--sidebar-accent)] text-foreground"
  );
}

export function ChatHistorySidebar({
  open,
  activeThreadId,
  activeProjectPath,
  activeProviderId,
  sending,
  onOpenChange,
  onNewChat,
  onOpenProjectChat,
  onForkThread,
  onDeleteThread,
  onOpenFolder,
}: Props) {
  const [projects, setProjects] = useState<ProjectChats[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const [expanded, setExpanded] = useState<Record<string, boolean>>(readExpandedMap);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const searchInputRef = useRef<HTMLInputElement>(null);
  const [pendingDelete, setPendingDelete] = useState<{
    thread: ThreadSummary;
    projectPath: string;
  } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [pinning, setPinning] = useState<string | null>(null);

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
      .catch(() => {
        if (!cancelled) setError("Could not load chat history. Try again.");
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, activeThreadId, activeProjectPath, activeProviderId, tick]);

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

  useEffect(() => {
    if (open && searchOpen) searchInputRef.current?.focus();
  }, [open, searchOpen]);

  const query = searchQuery.trim().toLowerCase();
  const recentThreads = useMemo(
    () =>
      projects
        .flatMap((project) =>
          project.threads
            .filter((thread) => matchesQuery(project, thread, query))
            .map((thread) => ({ project, thread }))
        )
        .sort((a, b) => b.thread.updatedAt - a.thread.updatedAt)
        .slice(0, 8),
    [projects, query]
  );

  const visibleProjects = useMemo(
    () =>
      projects
        .map((project) => ({
          ...project,
          threads: project.threads.filter((thread) =>
            matchesQuery(project, thread, query)
          ),
        }))
        .filter((project) => !query || project.threads.length > 0),
    [projects, query]
  );

  function isExpanded(project: ProjectChats) {
    if (query) return true;
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

  async function togglePinned(projectPath: string, thread: ThreadSummary) {
    if (pinning === thread.id) return;
    setPinning(thread.id);
    try {
      await getBackend().setThreadPinned(
        thread.id,
        projectPath,
        !thread.pinned
      );
      setError(null);
      setTick((n) => n + 1);
    } catch {
      setError("Could not update the pinned chat. Try again.");
    } finally {
      setPinning(null);
    }
  }

  async function openThread(project: ProjectChats, thread: ThreadSummary) {
    // Route every thread open through the project-aware backend. This preserves
    // provider ownership and lets legacy or unavailable chats show recovery
    // actions instead of silently switching providers.
    await onOpenProjectChat({
      root: project.path,
      threadId: thread.id,
    });
  }

  function renderThreadItem(
    project: ProjectChats,
    thread: ThreadSummary,
    key: string
  ) {
    const active = project.active && thread.id === activeThreadId;
    const title = threadTitle(thread);
    const age = formatAge(thread.updatedAt);
    // Shown on every row now that it is a mark rather than a word. The name
    // used to be hidden unless a project mixed providers, because `anthropic`
    // spelled out next to every chat was noise — a glyph is not, and knowing
    // who owns a chat before you open it is worth a few pixels.
    const owner = thread.providerId;

    return (
      <li key={key} className="group/thread relative">
        <button
          type="button"
          disabled={active}
          onClick={() => {
            void openThread(project, thread).catch(() => {
              /* Parent handlers surface the actionable error. */
            });
          }}
          className={cn(
            "flex w-full cursor-pointer items-center gap-2 rounded-md py-1.5 pr-24 pl-2 text-left outline-none transition-colors",
            "hover:bg-[var(--sidebar-accent)] hover:text-[var(--sidebar-accent-foreground)]",
            "focus-visible:ring-2 focus-visible:ring-ring/50",
            active
              ? "bg-[var(--sidebar-accent)] text-[var(--sidebar-accent-foreground)]"
              : "disabled:pointer-events-none"
          )}
        >
          <span className="min-w-0 flex-1 truncate text-[13px]">{title}</span>
          {owner ? (
            <span
              title={`This chat belongs to ${owner}. Zest will keep the original provider or let you open a copy.`}
              className="flex shrink-0 items-center text-muted-foreground"
            >
              {/* The name still reaches a screen reader through the title
                  above, so the glyph itself stays decorative. */}
              <ProviderIcon providerId={owner} className="size-3" />
            </span>
          ) : null}
          {age ? (
            <span className="shrink-0 text-[10px] tabular-nums text-muted-foreground">
              {age}
            </span>
          ) : null}
        </button>
        {active ? (
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            title="Fork conversation"
            aria-label="Fork conversation"
            disabled={sending || deleting}
            className={cn(
              "absolute top-1 right-7 text-muted-foreground transition-opacity",
              "hover:bg-muted hover:text-foreground",
              "focus-visible:opacity-100",
              "opacity-100"
            )}
            onClick={(event) => {
              event.stopPropagation();
              void onForkThread();
            }}
          >
            <GitForkIcon aria-hidden="true" />
          </Button>
        ) : null}
        <Button
          type="button"
          variant="ghost"
          size="icon-xs"
          title={thread.pinned ? "Unpin chat" : "Pin chat"}
          aria-label={thread.pinned ? "Unpin chat" : "Pin chat"}
          aria-pressed={thread.pinned}
          disabled={sending || deleting || pinning === thread.id}
          className={cn(
            "absolute top-1 right-14 text-muted-foreground transition-opacity",
            "hover:bg-muted hover:text-foreground",
            thread.pinned
              ? "fill-current text-primary opacity-100"
              : "opacity-0 group-hover/thread:opacity-100 focus-visible:opacity-100"
          )}
          onClick={(event) => {
            event.stopPropagation();
            void togglePinned(project.path, thread);
          }}
        >
          <PinIcon aria-hidden="true" />
        </Button>
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
            active ? "opacity-100" : "opacity-0 group-hover/thread:opacity-100"
          )}
          onClick={(event) => {
            event.stopPropagation();
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
            <div className="flex min-w-0 items-center gap-2 px-1.5">
              <BrandMark size={20} />
              <span className="truncate text-sm font-semibold tracking-[-0.2px]">
                Zest
              </span>
            </div>
            <div className="flex items-center gap-0.5">
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                title="Open project folder"
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
            title="Expand projects (Ctrl+B)"
            aria-expanded={open}
            onClick={() => onOpenChange(true)}
          >
            <ChevronsRightIcon />
          </Button>
        )}
      </div>

      {!open ? (
        <div className="flex flex-col items-center gap-1 px-1 py-2">
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            title="New chat (Ctrl+N)"
            onClick={onNewChat}
          >
            <PlusIcon />
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            title="Open project folder"
            onClick={onOpenFolder}
          >
            <FolderOpenIcon />
          </Button>
        </div>
      ) : null}

      {open ? (
        <div className="min-h-0 flex-1 overflow-y-auto px-1.5 py-2">
          <nav aria-label="Primary" className="flex flex-col gap-0.5">
            <button
              type="button"
              onClick={onNewChat}
              className={cn(
                navItemClass(),
                "disabled:pointer-events-none disabled:opacity-50"
              )}
            >
              <SquarePenIcon className="size-4 shrink-0 text-muted-foreground" />
              <span>New chat</span>
            </button>
            <button
              type="button"
              aria-expanded={searchOpen}
              onClick={() => {
                if (searchOpen) setSearchQuery("");
                setSearchOpen((value) => !value);
              }}
              className={navItemClass(searchOpen)}
            >
              <SearchIcon className="size-4 shrink-0 text-muted-foreground" />
              <span>Search chats</span>
            </button>
          </nav>

          {searchOpen ? (
            <div
              role="search"
              className="mt-1 flex h-8 items-center gap-1.5 rounded-md border border-border/70 bg-background/50 px-2"
            >
              <SearchIcon className="size-3.5 shrink-0 text-muted-foreground" />
              <input
                ref={searchInputRef}
                value={searchQuery}
                aria-label="Search chats"
                placeholder="Search chats"
                className="min-w-0 flex-1 bg-transparent text-xs text-foreground outline-none placeholder:text-muted-foreground"
                onChange={(event) => setSearchQuery(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    setSearchQuery("");
                    setSearchOpen(false);
                  }
                }}
              />
              {searchQuery ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  title="Clear search"
                  aria-label="Clear search"
                  onClick={() => setSearchQuery("")}
                >
                  <XIcon />
                </Button>
              ) : null}
            </div>
          ) : null}

          <div className="my-3 border-t border-border/40" />

          {loading && projects.length === 0 ? (
            <p className="px-2 py-1 text-xs text-muted-foreground">Loading…</p>
          ) : error ? (
            <p className="px-2 py-1 text-xs text-destructive">{error}</p>
          ) : projects.length === 0 ? (
            <p className="px-2 py-1 text-xs text-muted-foreground">
              Open a project folder to start.
            </p>
          ) : (
            <>
              <section aria-labelledby="projects-heading">
                <div
                  id="projects-heading"
                  className="flex items-center justify-between px-2 pb-1"
                >
                  <span className="flex items-center gap-1.5 text-[11px] font-medium uppercase tracking-wide text-muted-foreground">
                    <FolderIcon className="size-3.5" />
                    Projects
                  </span>
                  <Button
                    type="button"
                    variant="ghost"
                    size="icon-xs"
                    title="Open project folder"
                    aria-label="Open project folder"
                    onClick={onOpenFolder}
                  >
                    <PlusIcon />
                  </Button>
                </div>
                {visibleProjects.length === 0 ? (
                  <p className="px-2 py-1 text-xs text-muted-foreground">
                    No chats match “{searchQuery.trim()}”.
                  </p>
                ) : null}
                <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
                  {visibleProjects.map((project) => {
                    const expandedHere = isExpanded(project);
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
                              project.threads.map((thread) =>
                                renderThreadItem(
                                  project,
                                  thread,
                                  `project:${project.path}:${thread.id}`
                                )
                              )
                            )}
                          </ul>
                        ) : null}
                      </li>
                    );
                  })}
                </ul>
              </section>

              {recentThreads.length > 0 ? (
                <section aria-labelledby="recent-chats-heading" className="mt-4">
                  <div
                    id="recent-chats-heading"
                    className="flex items-center gap-1.5 px-2 pb-1 text-[11px] font-medium uppercase tracking-wide text-muted-foreground"
                  >
                    <Clock3Icon className="size-3.5" />
                    Recent
                  </div>
                  <ul className="m-0 flex list-none flex-col gap-0.5 p-0">
                    {recentThreads.map(({ project, thread }) =>
                      renderThreadItem(
                        project,
                        thread,
                        `recent:${project.path}:${thread.id}`
                      )
                    )}
                  </ul>
                </section>
              ) : null}
            </>
          )}

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
