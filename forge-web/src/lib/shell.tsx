"use client";

/**
 * Shell preferences: collapsed sidebar, theme, and the last-visited items.
 *
 * Persisted to `localStorage` because UI.md §1 requires the sidebar to
 * remember its state across visits.
 */

import { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";

export type Theme = "light" | "dark" | "system";

interface ShellState {
  sidebarCollapsed: boolean;
  theme: Theme;
  /** Most recently visited routes, newest first (UI.md §1). */
  recent: { href: string; label: string }[];
}

const DEFAULTS: ShellState = {
  sidebarCollapsed: false,
  theme: "system",
  recent: [],
};

const STORAGE_KEY = "forge.shell";
const MAX_RECENT = 6;

interface ShellContextValue extends ShellState {
  toggleSidebar: () => void;
  setTheme: (theme: Theme) => void;
  /** Records a visit, de-duplicating by route. */
  markVisited: (href: string, label: string) => void;
  resolvedTheme: "light" | "dark";
}

const ShellContext = createContext<ShellContextValue | null>(null);

function read(): ShellState {
  if (typeof window === "undefined") return DEFAULTS;
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<ShellState>;
    return {
      sidebarCollapsed: Boolean(parsed.sidebarCollapsed),
      theme: (parsed.theme as Theme) ?? "system",
      recent: Array.isArray(parsed.recent) ? parsed.recent : [],
    };
  } catch {
    // Corrupt preferences must not break the app.
    return DEFAULTS;
  }
}

export function ShellProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<ShellState>(DEFAULTS);
  const [systemDark, setSystemDark] = useState(false);

  // Preferences are read after mount so server and client render the same shell.
  useEffect(() => {
    setState(read());
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    setSystemDark(query.matches);
    const onChange = (event: MediaQueryListEvent) => setSystemDark(event.matches);
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const persist = useCallback((next: ShellState) => {
    setState(next);
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    } catch {
      // A full or disabled store is not worth failing a page over.
    }
  }, []);

  const resolvedTheme: "light" | "dark" =
    state.theme === "system" ? (systemDark ? "dark" : "light") : state.theme;

  // The document class drives the dark palette the rest of the console assumes.
  useEffect(() => {
    document.documentElement.classList.toggle("dark", resolvedTheme === "dark");
  }, [resolvedTheme]);

  const value = useMemo<ShellContextValue>(
    () => ({
      ...state,
      resolvedTheme,
      toggleSidebar: () => persist({ ...state, sidebarCollapsed: !state.sidebarCollapsed }),
      setTheme: (theme) => persist({ ...state, theme }),
      markVisited: (href, label) => {
        const recent = [
          { href, label },
          ...state.recent.filter((r) => r.href !== href),
        ].slice(0, MAX_RECENT);
        persist({ ...state, recent });
      },
    }),
    [state, persist, resolvedTheme],
  );

  return <ShellContext.Provider value={value}>{children}</ShellContext.Provider>;
}

export function useShell(): ShellContextValue {
  const context = useContext(ShellContext);
  if (!context) throw new Error("useShell must be used inside a ShellProvider");
  return context;
}
