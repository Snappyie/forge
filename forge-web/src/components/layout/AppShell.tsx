"use client";

/**
 * The authenticated application shell (spec 7.3).
 *
 * Sidebar, header with tenant selector, global search, and a user menu. Pages
 * that should render bare (sign-in, register) are detected by path so the same
 * root layout can host both.
 */

import Link from "next/link";
import { usePathname, useRouter } from "next/navigation";
import { useEffect, useState } from "react";
import {
  Activity,
  CalendarClock,
    ClipboardList,
  FileClock,
  Gauge,
    LayoutList,
  LogOut,
  Search,
  Settings,
  Shield,
  Users,
  Workflow,
  X,
} from "lucide-react";

import { useAuth } from "@/lib/auth";
import { LoadingState } from "@/components/states";
import { cn } from "cn";

const NAV = [
  { href: "/", label: "Dashboard", icon: Gauge },
  { href: "/jobs", label: "Jobs", icon: ClipboardList },
  { href: "/workflows", label: "Workflows", icon: Workflow },
  { href: "/executions", label: "Executions", icon: Activity },
  { href: "/schedules", label: "Schedules", icon: CalendarClock },
  { href: "/queues", label: "Queues", icon: LayoutList },
  { href: "/workers", label: "Workers", icon: Users },
  { href: "/audit", label: "Audit", icon: FileClock },
  { href: "/admin", label: "Administration", icon: Shield },
];

/** Routes that render without the shell. */
const BARE_ROUTES = ["/login", "/register"];

export function AppShell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  const router = useRouter();
  const { session, loading, signOut } = useAuth();
  const bare = BARE_ROUTES.some((route) => pathname === route || pathname.startsWith(`${route}/`));

  // Send an unauthenticated visitor to sign-in rather than rendering an empty
  // shell that would fail every request.
  useEffect(() => {
    if (!loading && !session && !bare) {
      router.replace("/login");
    }
  }, [loading, session, bare, router, pathname]);

  // Sign-out lands on the sign-in page.
  useEffect(() => {
    if (!loading && !session && !bare) return;
  }, [loading, session, bare]);

  if (bare) return <>{children}</>;

  if (loading) {
    return (
      <div className="flex min-h-screen items-center justify-center">
        <LoadingState label="Restoring your session" />
      </div>
    );
  }

  if (!session) return <>{children}</>;

  return (
    <div className="flex h-screen overflow-hidden">
      <Sidebar pathname={pathname} />
      <div className="flex min-w-0 flex-1 flex-col">
        <Header tenantId={session.tenantId} role={session.role} onSignOut={signOut} />
        <main className="flex-1 overflow-y-auto">{children}</main>
      </div>
    </div>
  );
}

function Sidebar({ pathname }: { pathname: string }) {
  return (
    <nav
      aria-label="Main"
      className="hidden w-56 shrink-0 flex-col gap-1 border-r border-border p-3 md:flex"
    >
      <Link
        href="/"
        className="mb-4 flex items-center gap-2 px-2 text-sm font-semibold"
      >
        <span className="grid size-7 place-items-center rounded-md bg-primary text-xs font-bold text-primary-foreground">
          F
        </span>
        Forge
      </Link>

      {NAV.map((item) => {
        const active =
          item.href === "/" ? pathname === "/" : pathname.startsWith(item.href);
        const Icon = item.icon;
        return (
          <Link
            key={item.href}
            href={item.href}
            aria-current={active ? "page" : undefined}
            className={cn(
              "flex items-center gap-2 rounded-md px-2 py-1.5 text-sm transition-colors",
              active
                ? "bg-accent text-accent-foreground"
                : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
            )}
          >
            <Icon className="size-4 shrink-0" aria-hidden />
            <span className="truncate">{item.label}</span>
          </Link>
        );
      })}
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
  const [searchOpen, setSearchOpen] = useState(false);

  return (
    <header className="flex h-12 shrink-0 items-center gap-3 border-b border-border px-4">
      <button
        type="button"
        onClick={() => setSearchOpen((open) => !open)}
        aria-label="Search"
        aria-expanded={searchOpen}
        className="flex flex-1 items-center gap-2 rounded-md border border-border px-3 py-1 text-left text-sm text-muted-foreground hover:bg-accent/50"
      >
        <Search className="size-4" aria-hidden />
        <span className="hidden sm:inline">Search jobs, executions, workflows…</span>
        <kbd className="ml-auto hidden rounded border border-border px-1 text-[10px] sm:inline">
          /
        </kbd>
      </button>

      {tenantId ? (
        <span
          className="hidden items-center gap-1 rounded-md border border-border px-2 py-1 text-xs text-muted-foreground lg:flex"
          title="Active tenant"
        >
          tenant
          <code className="max-w-[10ch] truncate">{tenantId.slice(0, 8)}</code>
        </span>
      ) : null}

      <Link
        href="/settings"
        aria-label="Settings"
        className="rounded-md p-1.5 text-muted-foreground hover:bg-accent/60 hover:text-foreground"
      >
        <Settings className="size-4" aria-hidden />
      </Link>

      <div className="flex items-center gap-2 border-l border-border pl-3">
        <span
          className="hidden text-xs text-muted-foreground sm:inline"
          title="Signed-in role"
        >
          {role ?? "—"}
        </span>
        <button
          type="button"
          onClick={onSignOut}
          aria-label="Sign out"
          className="rounded-md p-1.5 text-muted-foreground hover:bg-accent/60 hover:text-foreground"
        >
          <LogOut className="size-4" aria-hidden />
        </button>
      </div>

      {searchOpen ? (
        <div className="absolute inset-x-0 top-12 z-40 border-b border-border bg-popover p-4 shadow-lg">
          <div className="mx-auto flex max-w-2xl items-center gap-2">
            <Search className="size-4 text-muted-foreground" aria-hidden />
            <input
              autoFocus
              type="search"
              placeholder="Search by id, name, or key"
              className="flex-1 bg-transparent text-sm outline-none"
            />
            <button
              type="button"
              onClick={() => setSearchOpen(false)}
              aria-label="Close search"
              className="rounded p-1 text-muted-foreground hover:bg-accent"
            >
              <X className="size-4" aria-hidden />
            </button>
          </div>
        </div>
      ) : null}
    </header>
  );
}

