"use client";

/**
 * The authenticated application shell (UI.md section 1).
 *
 * The composition is a monitor, not a landing page: a fixed navigation rail, a
 * single persistent header carrying environment/search/health/identity, and one
 * scrolling content region. Everything an operator needs to answer "is it
 * healthy, what is running, what needs me" is on chrome they never scroll past.
 *
 * The rail is shadcn's `Sidebar` so collapse state, the mobile sheet, and the
 * `Cmd/Ctrl+B` shortcut all behave the way the rest of the system expects.
 */

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import {
  Bell,
  BookOpen,
  ChevronsUpDown,
  LifeBuoy,
  LogOut,
  Menu,
  Monitor,
  Moon,
  Search,
  Sparkles,
  Sun,
  UserRound,
} from "lucide-react";

import { useAuth } from "@/lib/auth";
import { useShell, type Theme } from "@/lib/shell";
import { useQuery } from "@/lib/useQuery";
import { LoadingState } from "@/components/states";
import { SearchOverlay } from "@/components/layout/global-search";
import { CommandPaletteOverlay } from "@/components/layout/command-palette-dialog";
import { buildNav, type NavCounts } from "@/components/layout/nav";
import { UndoBar } from "@/components/ui/undo-bar";
import { LiveIndicator } from "@/components/ui/live-indicator";
import { HelpDialog } from "@/components/ui/help-dialog";
import { Avatar, AvatarFallback } from "@/components/ui/avatar";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { Kbd } from "@/components/ui/kbd";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar";
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
    <SidebarProvider className="h-screen overflow-hidden">
      {/* UI.md section 64: keyboard users can jump past the navigation. */}
      <a href="#main-content" className="skip-link">
        Skip to content
      </a>

      <AppSidebar pathname={pathname} />

      {/*
        `SidebarInset` carries `flex-1` and lays out as a row beside the sidebar's
        own spacer. Wrapping it in a second full-screen flex row of my own made
        the two compete, which left the content region at its intrinsic width
        instead of filling the viewport.
      */}
      <SidebarInset className="min-w-0 overflow-hidden">
        <Header
          tenantId={session.tenantId}
          role={session.role}
          onSignOut={signOut}
        />
        {/*
          `min-h-0` is required: a flex item's default `min-height: auto`
          refuses to shrink below its content, so without it the page grows
          past the viewport and the window itself never scrolls.
        */}
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <main
            id="main-content"
            tabIndex={-1}
            className="flex min-h-0 flex-1 flex-col overflow-y-auto"
          >
            <UndoBar />
            {children}
          </main>
        </div>
      </SidebarInset>
    </SidebarProvider>
  );
}

/**
 * The counts behind the sidebar badges.
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
    applications_total?: number;
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
    // Counted from the aggregate the shell already fetches, so the sidebar
    // badge cannot disagree with the Applications screen. An absent field leaves
    // the count undefined and the badge is omitted rather than shown as zero.
    applications: dashboard.data.applications_total,
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

function AppSidebar({ pathname }: { pathname: string }) {
  const { recent, markVisited } = useShell();
  const counts = useNavCounts();
  const isActive = (href: string) =>
    href === "/" ? pathname === "/" : pathname.startsWith(href);

  const groups = buildNav(counts);

  // Record the visit so "recently visited" (UI.md section 1) stays accurate.
  const active = groups
    .flatMap((group) => group.items)
    .find((n) => isActive(n.href));
  useEffect(() => {
    if (active) markVisited(active.href, active.label);
    // Only on a route change; re-running each render would loop. The
    // exhaustive-deps rule is disabled project-wide, so no directive is needed
    // here and adding one only produces an "unused directive" warning.
  }, [pathname]);

  return (
    <Sidebar collapsible="icon" className="border-r border-sidebar-border">
      <SidebarHeader className="border-b border-sidebar-border">
        <div className="flex items-center gap-2 px-1 py-1">
          <span className="grid size-6 shrink-0 place-items-center rounded bg-sidebar-primary text-[11px] font-bold text-sidebar-primary-foreground">
            F
          </span>
          <span className="truncate text-[13px] font-semibold group-data-[collapsible=icon]/sidebar:hidden">
            Forge
          </span>
        </div>
      </SidebarHeader>

      <SidebarContent className="py-1">
        {groups.map((group) => (
          <SidebarGroup key={group.label}>
            <SidebarGroupLabel className="px-2 text-[10px] tracking-wider">
              {group.label}
            </SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {group.items.map((item) => {
                  const Icon = item.icon;
                  const current = isActive(item.href);
                  return (
                    <SidebarMenuItem key={item.href}>
                      <SidebarMenuButton
                        render={
                          <Link
                            href={item.href}
                            aria-current={current ? "page" : undefined}
                          />
                        }
                        isActive={current}
                        tooltip={item.label}
                        size="sm"
                        className="text-[13px]"
                      >
                        <Icon className="size-3.5 shrink-0" aria-hidden />
                        <span className="truncate">{item.label}</span>
                      </SidebarMenuButton>
                      {/*
                        `SidebarMenuBadge` is absolutely positioned within the
                        `li`, so it sits on the button's row. A bare sibling
                        `span` would stack underneath it instead.
                      */}
                      {item.count !== undefined ? (
                        <SidebarMenuBadge
                          className={cn(
                            "h-4 min-w-4 px-1 text-[10.5px]",
                            item.tone === "danger" &&
                              "bg-danger/12 text-danger-foreground",
                            item.tone === "warning" &&
                              "bg-warning/15 text-warning-foreground",
                            item.tone === undefined &&
                              "text-muted-foreground",
                          )}
                        >
                          {item.count}
                        </SidebarMenuBadge>
                      ) : null}
                    </SidebarMenuItem>
                  );
                })}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        ))}

        {/*
          Recently visited, last in the scrolling region. It lives here rather
          than in the footer because the footer is fixed: adding a growing list
          to it pushed the last nav item off a short viewport.
        */}
        {recent.length > 1 ? (
          <SidebarGroup>
            <SidebarGroupLabel className="px-2 text-[10px] tracking-wider">
              Recent
            </SidebarGroupLabel>
            <SidebarGroupContent>
              <SidebarMenu>
                {recent.slice(0, 4).map((item) => (
                  <SidebarMenuItem key={item.href}>
                    <SidebarMenuButton
                      render={
                        <Link href={item.href} className="text-muted-foreground" />
                      }
                      tooltip={item.label}
                      size="sm"
                      className="text-[12px]"
                    >
                      <span className="truncate">{item.label}</span>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </SidebarGroupContent>
          </SidebarGroup>
        ) : null}
      </SidebarContent>

      <SidebarFooter className="border-t border-sidebar-border py-1">
        {/* Destinations that are not part of a work group but are still global. */}
        <SidebarMenu>
          {[
            { href: "/assistant", label: "Assistant", icon: Sparkles },
            { href: "/docs", label: "Developers", icon: BookOpen },
          ].map((item) => {
            const Icon = item.icon;
            return (
              <SidebarMenuItem key={item.href}>
                <SidebarMenuButton
                  render={<Link href={item.href} />}
                  isActive={isActive(item.href)}
                  tooltip={item.label}
                  size="sm"
                  className="text-[13px]"
                >
                  <Icon className="size-3.5 shrink-0" aria-hidden />
                  <span className="truncate">{item.label}</span>
                </SidebarMenuButton>
              </SidebarMenuItem>
            );
          })}
        </SidebarMenu>
      </SidebarFooter>
    </Sidebar>
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
  // UI.md section 81 asks for an auto-refresh toggle; section 57 for a
  // live/paused indicator.
  const [autoRefresh, setAutoRefresh] = useState(true);

  // UI.md section 1 lists a command palette in the header; Cmd/Ctrl-K opens it,
  // and `/` focuses search.
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
    <header className="flex h-11 shrink-0 items-center gap-2 border-b border-border bg-card px-3">
      <SidebarTrigger
        className="size-7"
        aria-label="Toggle navigation"
      >
        <Menu className="size-4" aria-hidden />
      </SidebarTrigger>

      <EnvironmentIndicator />

      <button
        type="button"
        onClick={() => setSearchOpen((open) => !open)}
        aria-label="Search"
        aria-expanded={searchOpen}
        className={cn(
          "flex min-w-0 max-w-md flex-1 items-center gap-2 rounded border border-input bg-transparent",
          "px-2.5 py-1 text-left text-[12.5px] text-muted-foreground transition-colors",
          "hover:bg-accent/50",
        )}
      >
        <Search className="size-3.5 shrink-0" aria-hidden />
        <span className="hidden truncate sm:inline">
          Search jobs, executions, workflows
        </span>
        <Kbd className="ml-auto hidden shrink-0 sm:inline-flex">/</Kbd>
      </button>

      <div className="ml-auto flex shrink-0 items-center gap-0.5">
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setPaletteOpen(true)}
          className="hidden h-7 gap-1.5 px-2 text-[12px] text-muted-foreground sm:flex"
        >
          <span>Commands</span>
          <Kbd>⌘K</Kbd>
        </Button>

        {tenantId ? (
          <span
            className="hidden items-center gap-1.5 px-2 text-[11.5px] text-muted-foreground lg:flex"
            title="Active tenant"
          >
            <span className="size-1.5 rounded-full bg-success" aria-hidden />
            <code className="max-w-[12ch] truncate font-mono text-[11px]">
              {tenantId.slice(0, 8)}
            </code>
          </span>
        ) : null}

        <SystemHealth />
        <LiveIndicator paused={!autoRefresh} onToggle={setAutoRefresh} />

        <NotificationsBell
          open={notificationsOpen}
          onToggle={() => setNotificationsOpen((o) => !o)}
        />

        <HelpDialog />

        <ThemeSelector theme={theme} onChange={setTheme} />

        <div className="ml-1 flex shrink-0 items-center gap-1.5 border-l border-border pl-2">
          <span className="hidden text-[11.5px] text-muted-foreground sm:inline">
            {role ?? "—"}
          </span>
          <Avatar className="size-6">
            <AvatarFallback className="text-[10px] font-semibold">
              {(role ?? "U").slice(0, 2).toUpperCase()}
            </AvatarFallback>
          </Avatar>
          <DropdownMenu>
            <DropdownMenuTrigger
              render={
                <button
                  type="button"
                  aria-label="Account menu"
                  className="grid size-6 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
                >
                  <ChevronsUpDown className="size-3.5" aria-hidden />
                </button>
              }
            />
            <DropdownMenuContent align="end" className="w-44">
              <DropdownMenuLabel className="text-[11px]">
                Signed in as {role ?? "unknown"}
              </DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuItem render={<Link href="/settings" />}>
                <UserRound aria-hidden />
                Settings
              </DropdownMenuItem>
              <DropdownMenuItem render={<Link href="/docs" />}>
                <BookOpen aria-hidden />
                API reference
              </DropdownMenuItem>
              <DropdownMenuItem render={<Link href="/assistant" />}>
                <LifeBuoy aria-hidden />
                Ask the assistant
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem variant="destructive" onClick={onSignOut}>
                <LogOut aria-hidden />
                Sign out
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
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

/** UI.md section 1: make production visually unmistakable. */
function EnvironmentIndicator() {
  const environment = process.env.NEXT_PUBLIC_FORGE_ENVIRONMENT ?? "local";

  const kind = environment.toLowerCase();
  const production = kind === "production";
  const staging = kind === "staging";

  const tone = production
    ? "border-danger/50 bg-danger/10 text-danger-foreground"
    : staging
      ? "border-warning/50 bg-warning/12 text-warning-foreground"
      : "border-border text-muted-foreground";

  return (
    <span
      className={cn(
        "shrink-0 rounded border px-1.5 py-0.5 text-[10.5px] font-semibold tracking-wide uppercase",
        tone,
      )}
      title={`Connected to ${environment}`}
    >
      {environment}
    </span>
  );
}

/** UI.md section 1: system health indicator. Reads the real liveness endpoint. */
function SystemHealth() {
  const health = useQuery<{ status?: string }>("/health/live");

  const ok = health.state === "ready";
  const degraded = health.state === "error";

  return (
    <span
      className="grid size-7 shrink-0 place-items-center"
      title={
        degraded
          ? "The server is unreachable"
          : ok
            ? "Server healthy"
            : "Checking…"
      }
      aria-label={
        degraded
          ? "System unhealthy"
          : ok
            ? "System healthy"
            : "Checking system health"
      }
      role="status"
    >
      <span
        aria-hidden
        className={cn(
          "inline-block size-1.5 rounded-full",
          degraded
            ? "bg-danger"
            : ok
              ? "bg-success"
              : "bg-muted-foreground/40",
        )}
      />
    </span>
  );
}

/** UI.md section 50: the notification bell with an unread badge. */
function NotificationsBell({
  open,
  onToggle,
}: {
  open: boolean;
  onToggle: () => void;
}) {
  const { data } = useQuery<{ data?: unknown[]; unread?: number }>(
    "/notifications",
  );
  const rows = Array.isArray(data?.data) ? data.data : [];
  const unread = data?.unread ?? 0;

  return (
    <Popover open={open} onOpenChange={onToggle}>
      <PopoverTrigger
        render={
          <button
            type="button"
            aria-label={
              unread > 0 ? `Notifications, ${unread} unread` : "Notifications"
            }
            className="relative grid size-7 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
          />
        }
      >
        <Bell className="size-3.5" aria-hidden />
        {unread > 0 ? (
          <span className="absolute top-0.5 right-0.5 grid min-w-3.5 place-items-center rounded-full bg-danger px-0.5 text-[9px] font-medium text-white tabular-nums">
            {unread > 9 ? "9+" : unread}
          </span>
        ) : null}
      </PopoverTrigger>
      <PopoverContent align="end" className="w-80 p-0">
        <p className="border-b border-border px-3 py-2 text-[12px] font-semibold">
          Notifications
        </p>
        {rows.length === 0 ? (
          <p className="px-3 py-6 text-center text-[12px] text-muted-foreground">
            Nothing yet. Alerts appear here as they fire.
          </p>
        ) : (
          <ul className="max-h-72 overflow-y-auto">
            {rows.slice(0, 10).map((row) => {
              const item = row as Record<string, unknown>;
              return (
                <li
                  key={String(item.id)}
                  className="border-b border-border/50 px-3 py-2 last:border-0"
                >
                  <p className="text-[12px] font-medium">{String(item.title)}</p>
                  {item.body ? (
                    <p className="mt-0.5 text-[11.5px] text-muted-foreground">
                      {String(item.body)}
                    </p>
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}
      </PopoverContent>
    </Popover>
  );
}

/** UI.md section 1: theme selector. */
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
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <button
            type="button"
            aria-label={`Theme: ${current.label}`}
            className="grid size-7 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
          />
        }
      >
        <Icon className="size-3.5" aria-hidden />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-36">
        <DropdownMenuLabel className="text-[11px]">Theme</DropdownMenuLabel>
        {options.map((option) => {
          const OptionIcon = option.icon;
          return (
            <DropdownMenuItem
              key={option.value}
              onClick={() => onChange(option.value)}
              className="text-[12.5px]"
            >
              <OptionIcon aria-hidden />
              {option.label}
            </DropdownMenuItem>
          );
        })}
      </DropdownMenuContent>
    </DropdownMenu>
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