"use client";

/**
 * Log viewer (UI.md section 16).
 *
 * The parity matrix records log search, level filtering, and line numbers as
 * missing, and those are the three things that make a log viewer usable rather
 * than decorative. This adds them without inventing a stream the server does
 * not send: `stdout` and `stderr` are the only two streams the API returns
 * (spec 08), so the filter offers exactly those and nothing else.
 *
 * Search is a plain case-insensitive substring match. That is deliberate: a
 * regex box on top of a substring filter would mostly let an operator shoot
 * themselves in the foot, and the console does not need to guess at their
 * intent. The match count is always shown, so a filter that silently hides
 * everything is impossible to miss.
 */

import { useMemo, useState } from "react";
import { Copy, Download, Search, X } from "lucide-react";

import { copyText, downloadText } from "@/lib/clipboard";
import { formatTimestamp } from "@/lib/types";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/states";

export interface LogLine {
  stream: string;
  content: string;
  at: string;
}

type StreamFilter = "all" | "stdout" | "stderr";

export function LogViewer({
  lines,
  loading,
  executionId,
  onDownloaded,
}: {
  lines: LogLine[];
  loading: boolean;
  executionId: string;
  onDownloaded?: () => void;
}) {
  const [query, setQuery] = useState("");
  const [stream, setStream] = useState<StreamFilter>("all");

  // Line numbers refer to the whole log, not the filtered view, so they stay
  // meaningful when a filter is active and someone is talking about line 412.
  // Pairing the index with each line up front keeps that number correct without
  // relying on object identity surviving a re-render.
  const numbered = useMemo(
    () => lines.map((line, i) => ({ line, n: i + 1 })),
    [lines],
  );

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return numbered.filter(({ line }) => {
      if (stream !== "all" && line.stream !== stream) return false;
      if (needle && !line.content.toLowerCase().includes(needle)) return false;
      return true;
    });
  }, [numbered, query, stream]);

  const matched = filtered.length;
  const filtering = query.trim().length > 0 || stream !== "all";

  function exportAll() {
    const body = lines
      .map((line) => `[${line.at}] ${line.stream.toUpperCase()} ${line.content}`)
      .join("\n");
    downloadText(`execution-${executionId}.log`, body || "No log lines recorded.");
    onDownloaded?.();
  }

  if (loading) {
    return (
      <p className="py-6 text-center text-xs text-muted-foreground">
        Loading logs…
      </p>
    );
  }

  if (lines.length === 0) {
    return (
      <EmptyState
        title="No log lines"
        description="This execution has not written any output yet. A worker sends output as it runs, so an execution that never dispatched will stay empty."
      />
    );
  }

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex min-w-48 flex-1 items-center gap-1.5 rounded-md border border-border px-2">
          <Search className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Filter lines…"
            aria-label="Filter log lines"
            className="min-w-0 flex-1 bg-transparent py-1.5 text-xs outline-none placeholder:text-muted-foreground"
          />
          {query ? (
            <button
              type="button"
              onClick={() => setQuery("")}
              aria-label="Clear filter"
              className="shrink-0 rounded p-0.5 text-muted-foreground hover:text-foreground"
            >
              <X className="size-3" aria-hidden />
            </button>
          ) : null}
        </div>

        <div className="flex overflow-hidden rounded-md border border-border">
          {(["all", "stdout", "stderr"] as StreamFilter[]).map((option) => (
            <button
              key={option}
              type="button"
              onClick={() => setStream(option)}
              aria-pressed={stream === option}
              className={cn(
                "px-2 py-1.5 text-[11px] font-medium transition-colors",
                stream === option
                  ? "bg-muted text-foreground"
                  : "text-muted-foreground hover:bg-accent",
              )}
            >
              {option}
            </button>
          ))}
        </div>

        {/* The match count is what makes an over-narrow filter obvious. */}
        <span className="text-[11px] tabular-nums text-muted-foreground">
          {filtering
            ? `${matched} of ${lines.length}`
            : `${lines.length} line${lines.length === 1 ? "" : "s"}`}
        </span>

        <div className="ml-auto flex items-center gap-1">
          <Button
            variant="ghost"
            size="sm"
            onClick={() =>
              copyText(
                filtered
                  .map(
                    ({ line }) =>
                      `[${line.at}] ${line.stream.toUpperCase()} ${line.content}`,
                  )
                  .join("\n"),
              )
            }
            disabled={matched === 0}
          >
            <Copy className="mr-1 size-3" aria-hidden />
            Copy
          </Button>
          <Button variant="ghost" size="sm" onClick={exportAll}>
            <Download className="mr-1 size-3" aria-hidden />
            Download
          </Button>
        </div>
      </div>

      {matched === 0 ? (
        <EmptyState
          title="No lines match"
          description={
            query.trim()
              ? `Nothing in this execution contains "${query.trim()}". Clear the filter to see all ${lines.length} lines.`
              : "This execution has no lines on this stream."
          }
        />
      ) : (
        <div className="max-h-96 overflow-auto rounded-md bg-muted/40">
          <pre className="font-mono text-[11px] leading-relaxed">
            {filtered.map(({ line, n }) => (
              <div
                key={`${n}-${line.at}`}
                className="flex gap-3 px-2 hover:bg-accent/40"
              >
                <span className="w-8 shrink-0 select-none text-right text-muted-foreground">
                  {n}
                </span>
                <span className="shrink-0 text-muted-foreground">
                  {formatTimestamp(line.at)}
                </span>
                <span
                  className={cn(
                    "shrink-0 uppercase",
                    line.stream === "stderr"
                      ? "text-red-600 dark:text-red-400"
                      : "text-muted-foreground",
                  )}
                >
                  {line.stream}
                </span>
                <span
                  className={cn(
                    "min-w-0 flex-1 break-words whitespace-pre-wrap",
                    // The matched span is highlighted so the eye lands on the
                    // hit without having to re-read the line.
                    query.trim() ? "bg-yellow-200/40 dark:bg-yellow-500/20" : "",
                  )}
                >
                  {line.content}
                </span>
              </div>
            ))}
          </pre>
        </div>
      )}
    </div>
  );
}