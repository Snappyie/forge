"use client";

/**
 * "What's running now?" (UI.md section 21).
 *
 * A dedicated operational view: what is running with its elapsed time, and what
 * is queued behind it. Elapsed is computed from `started_at`, so it is true for
 * a long-running execution rather than a value captured at render.
 */

import Link from "next/link";
import { useEffect, useState } from "react";
import { CircleDot, ListOrdered } from "lucide-react";

import { useList } from "@/lib/useQuery";
import { formatDuration, formatRelative, type Execution } from "@/lib/types";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { StatusBadge } from "@/components/status-badge";

const RUNNING = "DISPATCHED,RUNNING";

export default function RunningPage() {
  const running = useList<Execution>(`/executions?status=${RUNNING}&limit=50`);
  const queued = useList<Execution>(`/executions?status=QUEUED&limit=50`);

  // A ticking clock so elapsed time advances without polling the list again.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  const runningRows = running.rows;

  return (
    <div className="flex flex-col gap-4 p-6">
      <header>
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <CircleDot className="size-4" aria-hidden />
          What's running now?
        </h1>
        <p className="text-xs text-muted-foreground">
          Executions in flight, with live elapsed time, and what is queued behind them.
        </p>
      </header>

      <AsyncBoundary
        state={running.state}
        error={running.error}
        forbidden={running.forbidden}
        empty={running.empty && queued.empty}
        onRetry={() => {
          running.reload();
          queued.reload();
        }}
        loadingLabel="Reading live executions"
        emptyTitle="Nothing is running"
        emptyDescription="No execution is dispatched or running right now."
      >
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          <Card title="Running" icon={CircleDot} count={runningRows.length}>
            {runningRows.length === 0 ? (
              <EmptyState
                title="Nothing running"
                description="Executions appear here the moment they are dispatched."
              />
            ) : (
              <ul className="flex flex-col divide-y divide-border/50">
                {runningRows.map((execution) => (
                  <li key={execution.id}>
                    <Link
                      href={`/executions/${execution.id}`}
                      className="flex items-center gap-2 py-2 text-xs hover:underline"
                    >
                      <span className="font-mono text-muted-foreground">
                        {execution.id.slice(0, 8)}
                      </span>
                      <StatusBadge status={execution.status} />
                      <span className="ml-auto tabular-nums">
                        {elapsed(execution.started_at, now)}
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            )}
          </Card>

          <Card title="Queued" icon={ListOrdered} count={queued.rows.length}>
            {queued.rows.length === 0 ? (
              <EmptyState
                title="Nothing queued"
                description="Work waiting for a worker appears here."
              />
            ) : (
              <ul className="flex flex-col divide-y divide-border/50">
                {queued.rows.map((execution) => (
                  <li key={execution.id}>
                    <Link
                      href={`/executions/${execution.id}`}
                      className="flex items-center gap-2 py-2 text-xs hover:underline"
                    >
                      <span className="font-mono text-muted-foreground">
                        {execution.id.slice(0, 8)}
                      </span>
                      <span className="text-muted-foreground">
                        {execution.priority.toLowerCase()}
                      </span>
                      <span className="ml-auto text-muted-foreground">
                        waiting {formatRelative(execution.created_at)}
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            )}
          </Card>
        </div>
      </AsyncBoundary>
    </div>
  );
}

function Card({
  title,
  icon: Icon,
  count,
  children,
}: {
  title: string;
  icon: typeof CircleDot;
  count: number;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-lg border border-border">
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <Icon className="size-3.5 text-muted-foreground" aria-hidden />
        <h2 className="text-sm font-medium">{title}</h2>
        <span className="ml-auto text-xs tabular-nums text-muted-foreground">
          {count}
        </span>
      </div>
      <div className="p-3">{children}</div>
    </div>
  );
}

function elapsed(startedAt: string | null, now: number): string {
  if (!startedAt) return "not started";
  return formatDuration(Math.max(0, now - Date.parse(startedAt)));
}
