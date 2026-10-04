"use client";

/**
 * The authenticated application shell (UI.md §1).
 *
 * Navigation, the persistent header, and the environment indicator. The sidebar
 * collapses and remembers its state, and the header carries the controls
 * UI.md §1 lists: search, command palette, notifications, system health, help,
 * profile, and theme.
 */

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import {
  Bell,
  BookOpen,
  Monitor,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  Search,
  Sparkles,
  Sun,
} from "lucide-react";

import { useAuth } from "@/lib/auth";
import { useShell, type Theme } from "@/lib/shell";
import { useQuery } from "@/lib/useQuery";
import { LoadingState } from "@/components/states";
import { SearchOverlay } from "@/components/layout/global-search";
import { buildNav, buildPaletteNav, type NavCounts } from "@/components/layout/nav";
import { UndoBar } from "@/components/ui/undo-bar";
import { LiveIndicator } from "@/components/ui/live-indicator";
import { HelpDialog } from "@/components/ui/help-dialog";
import { cn } from "cn";

/** Routes that render without the shell. */
const BARE_ROUTES = ["/login", "/register"];

export function AppShell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const router = useRouter();
  const { session, loading, signOut } = useAuth();
  const bare = BARE_ROUTES.some(
    (r) => pathname === r || pathname.startsWith(`${r}/`),
  );

  // Send an unauthenticated visitor to sign-in rather than rendering a shell
  // whose every request would fail.
  useEffect(() => {
    if (!loading && !session && !bare) {
      router.replace("/login");
    }
  }, [loading, session, bare, router, pathname]);

  if (bare) return <>{children}</>;
  if (loading) {
    return (
      <div className="grid min-h-screen place-items-center">
        <LoadingState label="Restoring your session" />
      </div>
    );
  }
  if (!session) return <>{children}</>;

  return (
    <div className="flex h-screen overflow-hidden">
      {/* UI.md section 64: keyboard users can jump past the navigation. */}
      <a href="#main-content" className="skip-link">
        Skip to content
      </a>
      <Sidebar pathname={pathname} />
      <div className="flex min-w-0 flex-1 flex-col">
        <Header
          tenantId={session.tenantId}
          role={session.role}
          onSignOut={signOut}
        />
        {/* `min-h-0` is required: a flex item's default `min-height: auto`
            refuses to shrink below its content, so without it the page grows
            past the viewport and the window itself never scrolls. */}
        <main
          id="main-content"
          tabIndex={-1}
          className="min-h-0 flex-1 overflow-y-auto"
        >
          <div className="p-4 pb-0">
            <UndoBar />
          </div>
          {children}
        </main>
      </div>
    </div>
  );
}

/**
 * The counts behind the Monitor group's badges.
 *
 * Read from the dashboard aggregate, which the shell already has a request for
 * on most pages, so a badge can never disagree with the dashboard. A failed
 * request leaves the count undefined and the badge is simply omitted: an unknown
 * count is not zero, and showing "0" would be a confident wrong answer.
 */
function useNavCounts(): NavCounts {
  const dashboard = useQuery<{
    executions?: { running?: number };
    alerts?: { critical?: number; warning?: number };
    needs_attention?: unknown[];
    queues?: { id: string }[];
    workers?: { ready?: number; busy?: number; offline?: number };
    jobs_total?: number;
    workflows_total?: number;
  }>("/dashboard");

  if (dashboard.state !== "ready" || !dashboard.data) return {};

  const running = dashboard.data.executions?.running;
  const alerts = dashboard.data.alerts;
  const critical = alerts?.critical ?? 0;
  const warning = alerts?.warning ?? 0;

  const w = dashboard.data.workers;
  const totalWorkers =
    w ? (w.ready ?? 0) + (w.busy ?? 0) + (w.offline ?? 0) : undefined;

  return {
    running,
    // Only badge Alerts when something needs a human, otherwise every tenant
    // that is simply healthy shows a permanent red "0".
    alerts: critical + warning > 0 ? critical + warning : undefined,
    incidents: dashboard.data.needs_attention?.length
      ? dashboard.data.needs_attention.length
      : undefined,
    jobs: dashboard.data.jobs_total,
    workflows: dashboard.data.workflows_total,
    queues: dashboard.data.queues?.length,
    workers: totalWorkers,
  };
}

function Sidebar({ pathname }: { pathname: string }) {
  const { sidebarCollapsed, toggleSidebar, recent, markVisited } = useShell();
  const counts = useNavCounts();

  const isActive = (href: string) =>
    href === "/" ? pathname === "/" : pathname.startsWith(href);

  const groups = buildNav(counts);

  // Record the visit so "recently visited" (UI.md §1) stays accurate.
  const active = groups
    .flatMap((group) => group.items)
    .find((n) => isActive(n.href));
  useEffect(() => {
    if (active) markVisited(active.href, active.label);
    // Only on a route change; re-running each render would loop.
     
  }, [pathname]);

  return (
    <nav
      aria-label="Main"
      className={cn(
        // The navigation is taller than a short viewport, so it scrolls
        // independently; `min-h-0` stops it being stretched by the shell
        // instead of shrinking to fit.
        "hidden min-h-0 shrink-0 flex-col gap-1 overflow-y-auto border-r border-border p-3 transition-[width] md:flex",
        sidebarCollapsed ? "w-14" : "w-56",
      )}
    >
      <div className="mb-4 flex items-center justify-between gap-1">
        <Link href="/" className="flex items-center gap-2 text-sm font-semibold">
          <span className="grid size-7 shrink-0 place-items-center rounded-md bg-primary text-xs font-bold text-primary-foreground">
            F
          </span>
          {!sidebarCollapsed ? <span>Forge</span> : null}
        </Link>
        <button
          type="button"
          onClick={toggleSidebar}
          aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          aria-expanded={!sidebarCollapsed}
          className="rounded p-1 text-muted-foreground hover:bg-accent"
        >
          {sidebarCollapsed ? (
            <PanelLeftOpen className="size-4" aria-hidden />
          ) : (
            <PanelLeftClose className="size-4" aria-hidden />
          )}
        </button>
      </div>

      {groups.map((group) => (
        <div key={group.label} className="mb-1">
          {/* The group labels are hidden when collapsed, but they are kept in
              the accessibility tree so the grouping is still announced. */}
          <p
            className={cn(
              "px-2 pb-1 text-[10px] font-semibold uppercase tracking-wider text-muted-foreground",
              sidebarCollapsed && "sr-only",
            )}
          >
            {group.label}
          </p>
          {group.items.map((item) => {
            const Icon = item.icon;
            const current = isActive(item.href);
            return (
              <Link
                key={item.href}
                href={item.href}
                aria-current={current ? "page" : undefined}
                title={sidebarCollapsed ? item.label : undefined}
                className={cn(
                  "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors",
                  current
                    ? "bg-accent text-accent-foreground"
                    : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
                )}
              >
                <Icon className="size-4 shrink-0" aria-hidden />
                {!sidebarCollapsed ? <span className="truncate">{item.label}</span> : null}
                {!sidebarCollapsed && item.count !== undefined ? (
                  <span
                    className={cn(
                      "ml-auto rounded-full bg-secondary px-1.5 py-0.5 text-[10px] font-medium tabular-nums text-muted-foreground",
                      item.tone === "bad" && "bg-red-500/15 text-red-600 dark:text-red-400",
                      item.tone === "wait" &&
                        "bg-amber-500/15 text-amber-700 dark:text-amber-400",
                    )}
                  >
                    {item.count}
                  </span>
                ) : null}
              </Link>
            );
          })}
        </div>
      ))}

      {/* Destinations that do not belong to a work group but are still global:
          the assistant and the developer reference. */}
      <div className="mt-auto border-t border-border pt-2">
        <Link
          href="/assistant"
          title={sidebarCollapsed ? "Assistant" : undefined}
          className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground"
        >
          <Sparkles className="size-4 shrink-0" aria-hidden />
          {!sidebarCollapsed ? <span className="truncate">Assistant</span> : null}
        </Link>
        <Link
          href="/docs"
          title={sidebarCollapsed ? "Developers" : undefined}
          className="flex items-center gap-2 rounded-md px-2 py-1.5 text-sm text-muted-foreground transition-colors hover:bg-accent/60 hover:text-foreground"
        >
          <BookOpen className="size-4 shrink-0" aria-hidden />
          {!sidebarCollapsed ? <span className="truncate">Developers</span> : null}
        </Link>
      </div>

      {/* UI.md §1: recently visited items. */}
      {!sidebarCollapsed && recent.length > 1 ? (
        <div className="border-t border-border pt-2">
          <p className="px-2 pb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
            Recent
          </p>
          {recent.slice(0, 4).map((item) => (
            <Link
              key={item.href}
              href={item.href}
              className="block truncate rounded px-2 py-1 text-xs text-muted-foreground hover:bg-accent/60 hover:text-foreground"
            >
              {item.label}
            </Link>
          ))}
        </div>
      ) : null}
    </nav>
  );
}

function Header({
  tenantId,
  role,
  onSignOut,
}: {
  tenantId?: string;
  role?: string;
  onSignOut: () => void;
}) {
  const { theme, setTheme } = useShell();
  const [searchOpen, setSearchOpen] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [notificationsOpen, setNotificationsOpen] = useState(false);
  // §81 asks for an auto-refresh toggle; §57 for a live/paused indicator.
  const [autoRefresh, setAutoRefresh] = useState(true);

  // UI.md §1 lists a command palette in the header; Cmd/Ctrl-K opens it, and `/`
  // focuses search.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setPaletteOpen((open) => !open);
        return;
      }
      if (event.key === "/" && !isTypingTarget(event.target)) {
        event.preventDefault();
        setSearchOpen(true);
        return;
      }
      if (event.key === "Escape") {
        setSearchOpen(false);
        setPaletteOpen(false);
        setNotificationsOpen(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <header className="flex h-12 shrink-0 items-center gap-2 border-b border-border px-4">
      <EnvironmentIndicator />

      <button
        type="button"
        onClick={() => setSearchOpen((open) => !open)}
        aria-label="Search"
        aria-expanded={searchOpen}
        className="flex min-w-0 flex-1 items-center gap-2 rounded-md border border-border px-3 py-1 text-left text-sm text-muted-foreground hover:bg-accent/50"
      >
        <Search className="size-4 shrink-0" aria-hidden />
        <span className="hidden truncate sm:inline">
          Search jobs, executions, workflows…
        </span>
        <kbd className="ml-auto hidden shrink-0 rounded border border-border px-1 text-[10px] sm:inline">
          /
        </kbd>
      </button>

      <button
        type="button"
        onClick={() => setPaletteOpen(true)}
        aria-label="Open command palette"
        className="hidden shrink-0 items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-accent/50 sm:flex"
      >
        <span>Commands</span>
        <kbd className="rounded border border-border px-1 text-[10px]">⌘K</kbd>
      </button>

      {tenantId ? (
        <span
          className="hidden shrink-0 items-center gap-1.5 rounded-md border border-border px-2.5 py-1 text-xs text-muted-foreground lg:flex"
          title="Active tenant"
        >
          <span className="size-1.5 rounded-full bg-emerald-500" />
          <code className="max-w-[16ch] truncate font-medium">{tenantId.slice(0, 8)}</code>
        </span>
      ) : null}

      <SystemHealth />
      <LiveIndicator paused={!autoRefresh} onToggle={setAutoRefresh} />

      <NotificationsBell
        open={notificationsOpen}
        onToggle={() => setNotificationsOpen((o) => !o)}
      />

      <ThemeSelector theme={theme} onChange={setTheme} />

      <HelpDialog />

      <div className="flex shrink-0 items-center gap-2 border-l border-border pl-3">
        <span
          className="hidden text-xs text-muted-foreground sm:inline"
          title="Signed-in role"
        >
          {role ?? "—"}
        </span>
        <span className="grid size-7 shrink-0 place-items-center rounded-full bg-zinc-200 text-xs font-semibold text-zinc-700 dark:bg-zinc-700 dark:text-zinc-200">
          {(role ?? "U").slice(0, 2).toUpperCase()}
        </span>
        <button
          type="button"
          onClick={onSignOut}
          aria-label="Sign out"
          className="rounded-md px-2 py-1 text-xs text-muted-foreground hover:bg-accent/60 hover:text-foreground"
        >
          Sign out
        </button>
      </div>

      {searchOpen ? (
        <SearchOverlay onClose={() => setSearchOpen(false)} />
      ) : null}
      {paletteOpen ? (
        <CommandPaletteOverlay onClose={() => setPaletteOpen(false)} />
      ) : null}
    </header>
  );
}

/** UI.md §1: make production visually unmistakable. */
function EnvironmentIndicator() {
  const environment = process.env.NEXT_PUBLIC_FORGE_ENVIRONMENT ?? "local";

  const kind = environment.toLowerCase();
  const production = kind === "production";
  const staging = kind === "staging";

  const tone = production
    ? "border-red-500/50 bg-red-500/10 text-red-600 dark:text-red-400"
    : staging
      ? "border-amber-500/50 bg-amber-500/10 text-amber-700 dark:text-amber-400"
      : "border-border text-muted-foreground";

  return (
    <span
      className={cn(
        "shrink-0 rounded-md border px-2 py-1 text-[11px] font-semibold uppercase tracking-wide",
        tone,
      )}
      title={`Connected to ${environment}`}
    >
      {production ? "● " : ""}
      {environment}
    </span>
  );
}

/** UI.md §1: system health indicator. Reads the real liveness endpoint. */
function SystemHealth() {
  const health = useQuery<{ status?: string }>("/health/live");

  const ok = health.state === "ready";
  const degraded = health.state === "error";

  return (
    <span
      className="shrink-0"
      title={
        degraded
          ? "The server is unreachable"
          : ok
            ? "Server healthy"
            : "Checking…"
      }
      aria-label={
        degraded ? "System unhealthy" : ok ? "System healthy" : "Checking system health"
      }
      role="status"
    >
      <span
        className={cn(
          "inline-block size-2 rounded-full",
          degraded ? "bg-red-500" : ok ? "bg-emerald-500" : "bg-muted-foreground/40",
        )}
      />
    </span>
  );
}

/** UI.md §50: the notification bell with an unread badge. */
function NotificationsBell({
  open,
  onToggle,
}: {
  open: boolean;
  onToggle: () => void;
}) {
  const { data } = useQuery<{ data: unknown[]; unread: number }>("/notifications");
  const rows = Array.isArray(data?.data) ? data.data : [];
  const unread = data?.unread ?? 0;

  return (
    <div className="relative shrink-0">
      <button
        type="button"
        onClick={onToggle}
        aria-label={unread > 0 ? `Notifications, ${unread} unread` : "Notifications"}
        aria-expanded={open}
        className="relative rounded-md p-1.5 text-muted-foreground hover:bg-accent/60 hover:text-foreground"
      >
        <Bell className="size-4" aria-hidden />
        {unread > 0 ? (
          <span className="absolute -right-0.5 -top-0.5 grid size-4 place-items-center rounded-full bg-red-500 text-[10px] font-medium text-white">
            {unread > 9 ? "9+" : unread}
          </span>
        ) : null}
      </button>

      {open ? (
        <div className="absolute right-0 top-9 z-50 w-80 rounded-md border border-border bg-popover p-2 shadow-lg">
          <p className="px-2 py-1 text-xs font-semibold">Notifications</p>
          {rows.length === 0 ? (
            <p className="px-2 py-3 text-xs text-muted-foreground">
              Nothing yet. Alerts appear here as they fire.
            </p>
          ) : (
            <ul className="max-h-72 overflow-y-auto">
              {rows.slice(0, 10).map((row) => {
                const item = row as Record<string, unknown>;
                return (
                  <li
                    key={String(item.id)}
                    className="border-t border-border/50 py-2 first:border-0"
                  >
                    <p className="text-xs font-medium">{String(item.title)}</p>
                    {item.body ? (
                      <p className="text-[11px] text-muted-foreground">
                        {String(item.body)}
                      </p>
                    ) : null}
                  </li>
                );
              })}
            </ul>
          )}
        </div>
      ) : null}
    </div>
  );
}

/** UI.md §1: theme selector. */
function ThemeSelector({
  theme,
  onChange,
}: {
  theme: Theme;
  onChange: (theme: Theme) => void;
}) {
  const options: { value: Theme; label: string; icon: typeof Sun }[] = [
    { value: "light", label: "Light", icon: Sun },
    { value: "dark", label: "Dark", icon: Moon },
    { value: "system", label: "System", icon: Monitor },
  ];

  const current = options.find((o) => o.value === theme) ?? options[2];
  const Icon = current.icon;

  return (
    <div className="group relative shrink-0">
      <button
        type="button"
        aria-label={`Theme: ${current.label}`}
        className="rounded-md p-1.5 text-muted-foreground hover:bg-accent/60 hover:text-foreground"
      >
        <Icon className="size-4" aria-hidden />
      </button>
      <div className="invisible absolute right-0 top-8 z-50 w-28 rounded-md border border-border bg-popover p-1 opacity-0 shadow-lg transition group-focus-within:visible group-focus-within:opacity-100 group-hover:visible group-hover:opacity-100">
        {options.map((option) => {
          const OptionIcon = option.icon;
          return (
            <button
              key={option.value}
              type="button"
              onClick={() => onChange(option.value)}
              aria-pressed={theme === option.value}
              className="flex w-full items-center gap-2 rounded px-2 py-1 text-xs hover:bg-accent"
            >
              <OptionIcon className="size-3.5" aria-hidden />
              {option.label}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.isContentEditable
  );
}

/** UI.md section 4: command palette. Navigation plus the action commands the
 * spec lists, and the context-aware job commands when a job page is open. */
function CommandPaletteOverlay({ onClose }: { onClose: () => void }) {
  const pathname = usePathname();
  const jobMatch = pathname.match(/^\/jobs\/([0-9a-f-]{36})$/);
  const jobId = jobMatch?.[1];

  // Context-aware commands: only offered while viewing a job, because acting on
  // "this job" is meaningless anywhere else.
  const actions = [
    { label: "Create job", href: "/jobs/builder" },
    { label: "Create workflow", href: "/workflows" },
    { label: "Run job", href: "/jobs" },
    { label: "Search jobs", href: "/jobs" },
    { label: "Open failed executions", href: "/executions?status=FAILED" },
    { label: "View workers", href: "/workers" },
    { label: "View alerts", href: "/alerts" },
    { label: "Open system health", href: "/settings" },
  ];

  const jobActions = jobId
    ? [
        { label: "Run now", href: `/jobs/${jobId}` },
        { label: "View executions", href: `/jobs/${jobId}` },
        { label: "Edit schedule", href: `/jobs/${jobId}` },
        { label: "Clone", href: `/jobs/${jobId}` },
      ]
    : [];

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-32"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      onClick={onClose}
    >
      <div
        className="w-full max-w-md rounded-lg border border-border bg-popover p-2 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <p className="px-2 py-1 text-[11px] uppercase tracking-wide text-muted-foreground">
          Actions
        </p>
        <ul className="max-h-80 overflow-y-auto">
          {[...jobActions, ...actions].map((command, index) => (
            <li key={`${command.label}-${index}`}>
              <Link
                href={command.href}
                onClick={onClose}
                className="flex items-center gap-2 rounded px-2 py-2 text-sm hover:bg-accent"
              >
                {command.label}
              </Link>
            </li>
          ))}
        </ul>

        <p className="mt-2 px-2 py-1 text-[11px] uppercase tracking-wide text-muted-foreground">
          Go to
        </p>
        <ul className="max-h-48 overflow-y-auto">
          {buildPaletteNav().map((command) => {
            const Icon = command.icon;
            return (
              <li key={command.href}>
                <Link
                  href={command.href}
                  onClick={onClose}
                  className="flex items-center gap-2 rounded px-2 py-2 text-sm hover:bg-accent"
                >
                  <Icon className="size-4" aria-hidden />
                  Go to {command.label}
                </Link>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}
