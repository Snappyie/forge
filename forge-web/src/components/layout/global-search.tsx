"use client";

/**
 * Global search (UI.md section 3).
 *
 * Queries `/search`, which returns grouped counts, and supports the documented
 * syntax: `status:failed`, `after:2026-09-01`, `duration:>10m`. Recent queries
 * are kept locally so a repeat search costs nothing.
 */

import Link from "next/link";
import { useEffect, useRef, useState } from "react";
import { Search } from "lucide-react";

import { formatRelative } from "@/lib/types";
import { cn } from "cn";

interface Group {
  kind: string;
  label: string;
  href: string;
  count: number;
  hits: Record<string, unknown>[];
}

interface SearchResult {
  query: string;
  groups: Group[];
  total: number;
}

const RECENT_KEY = "forge.search.recent";
const MAX_RECENT = 8;

export function SearchOverlay({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<SearchResult | null>(null);
  const [recent, setRecent] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const abortRef = useRef<AbortController | null>(null);

  useEffect(() => {
    inputRef.current?.focus();
    try {
      setRecent(JSON.parse(window.localStorage.getItem(RECENT_KEY) ?? "[]"));
    } catch {
      setRecent([]);
    }
  }, []);

  // Debounce so each keystroke does not become a request.
  useEffect(() => {
    const term = query.trim();
    if (term.length < 2) {
      setResult(null);
      return;
    }

    const timer = setTimeout(async () => {
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;
      setLoading(true);

      try {
        const { api } = await import("@/lib/api");
        const data = await api.get<SearchResult>(
          `/search?q=${encodeURIComponent(term)}`,
          controller.signal,
        );
        if (!controller.signal.aborted) setResult(data);
      } catch (error) {
        // An aborted request is expected while typing; anything else simply
        // leaves the previous result in place rather than showing an error the
        // user cannot act on mid-keystroke.
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setResult(null);
        }
      } finally {
        if (!controller.signal.aborted) setLoading(false);
      }
    }, 250);

    return () => clearTimeout(timer);
  }, [query]);

  function remember(term: string) {
    const next = [term, ...recent.filter((r) => r !== term)].slice(0, MAX_RECENT);
    setRecent(next);
    try {
      window.localStorage.setItem(RECENT_KEY, JSON.stringify(next));
    } catch {
      // A disabled store is not worth failing search over.
    }
  }

  const term = query.trim();

  return (
    <div
      className="absolute inset-x-0 top-12 z-40 border-b border-border bg-popover p-4 shadow-lg"
      role="dialog"
      aria-label="Search"
    >
      <div className="mx-auto flex max-w-2xl flex-col gap-3">
        <div className="flex items-center gap-2">
          <Search className="size-4 text-muted-foreground" aria-hidden />
          <input
            ref={inputRef}
            type="search"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && term.length >= 2) remember(term);
            }}
            placeholder="Search jobs, executions, workers, alerts"
            aria-label="Search query"
            className="flex-1 bg-transparent text-sm outline-none"
          />
          {loading ? (
            <span className="text-[11px] text-muted-foreground">searching…</span>
          ) : null}
          <button
            type="button"
            onClick={onClose}
            className="rounded px-2 py-1 text-xs text-muted-foreground hover:bg-accent"
          >
            Close
          </button>
        </div>

        {/* The syntax is discoverable rather than hidden in documentation. */}
        <p className="text-[11px] text-muted-foreground">
          Try <code className="font-mono">status:FAILED</code>,{" "}
          <code className="font-mono">after:2026-09-01</code>, or free text.
        </p>

        {term.length < 2 ? (
          recent.length > 0 ? (
            <div>
              <p className="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
                Recent
              </p>
              <ul className="flex flex-col">
                {recent.map((item) => (
                  <li key={item}>
                    <button
                      type="button"
                      onClick={() => setQuery(item)}
                      className="w-full rounded px-2 py-1 text-left text-xs hover:bg-accent"
                    >
                      {item}
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          ) : null
        ) : result ? (
          result.groups.length === 0 ? (
            <p className="py-4 text-center text-xs text-muted-foreground">
              Nothing matched “{term}”.
            </p>
          ) : (
            <div className="max-h-96 overflow-y-auto">
              {result.groups.map((group) => (
                <section key={group.kind} className="mb-3 last:mb-0">
                  <div className="flex items-baseline justify-between px-2">
                    <h3 className="text-[11px] uppercase tracking-wide text-muted-foreground">
                      {group.label}
                    </h3>
                    <Link
                      href={group.href}
                      onClick={onClose}
                      className="text-[11px] text-muted-foreground underline-offset-4 hover:underline"
                    >
                      {group.count}
                    </Link>
                  </div>
                  <ul>
                    {group.hits.map((hit, index) => (
                      <li key={String(hit.id ?? index)}>
                        <Link
                          href={hitUrl(group.kind, hit)}
                          onClick={() => {
                            remember(term);
                            onClose();
                          }}
                          className="flex items-center gap-2 rounded px-2 py-1 text-xs hover:bg-accent"
                        >
                          <span className="min-w-0 truncate">
                            {String(hit.name ?? hit.title ?? hit.hostname ?? hit.id)}
                          </span>
                          {hit.status ? (
                            <span
                              className={cn(
                                "ml-auto shrink-0 text-[10px] text-muted-foreground",
                              )}
                            >
                              {String(hit.status).toLowerCase()}
                            </span>
                          ) : null}
                          {hit.created_at ? (
                            <span className="shrink-0 text-[10px] text-muted-foreground">
                              {formatRelative(String(hit.created_at))}
                            </span>
                          ) : null}
                        </Link>
                      </li>
                    ))}
                  </ul>
                </section>
              ))}
            </div>
          )
        ) : null}
      </div>
    </div>
  );
}

/** Deep links per result kind, so a hit goes straight to the resource. */
function hitUrl(kind: string, hit: Record<string, unknown>): string {
  switch (kind) {
    case "jobs":
      return `/jobs/${String(hit.id)}`;
    case "executions":
      return `/executions/${String(hit.id)}`;
    case "workers":
      return `/workers/${String(hit.id)}`;
    case "alerts":
      // An alert has no page of its own; the inbox is the closest destination.
      return "/alerts";
    default:
      return "/";
  }
}
