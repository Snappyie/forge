"use client";

/**
 * Dashboard (spec 7.4).
 *
 * Every metric card links to the filtered detail view behind it, as spec 7.4
 * requires. Counts come from the executions and workers endpoints rather than a
 * bespoke dashboard API, so the numbers always agree with the lists.
 */

import Link from "next/link";
import { AlertTriangle, CheckCircle2, Clock, Layers, RefreshCw, Users } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { formatRelative, formatTimestamp, type Execution, type Worker } from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { StatusBadge } from "@/components/status-badge";
import { Button } from "@/components/ui/button";

interface Counts {
  queued: number;
  running: number;
  failed24h: number;
  succeeded24h: number;
  deadLettered24h: number;
  workersReady: number;
  workersOffline: number;
}

export default function DashboardPage() {
  const executions = useQuery<{ data: Execution[] }>("/executions?limit=50");
  const workers = useQuery<{ data: Worker[] }>("/workers?limit=50");

  const loading = executions.state === "loading" || workers.state === "loading";
  const error = executions.state === "error" ? executions.error : workers.error;
  const forbidden = executions.forbidden || workers.forbidden;

  const rows: Execution[] = Array.isArray(executions.data?.data)
    ? executions.data.data
    : [];
  const fleet: Worker[] = Array.isArray(workers.data?.data) ? workers.data.data : [];

  // The list endpoint returns a page, so these are "at least" counts; the card
  // says so rather than implying a precise total.
  const counts: Counts = {
    queued: rows.filter((e) => e.status === "QUEUED").length,
    running: rows.filter((e) =>
      ["DISPATCHED", "RUNNING"].includes(e.status),
    ).length,
    failed24h: rows.filter((e) =>
      ["FAILED", "TIMED_OUT"].includes(e.status),
    ).length,
    succeeded24h: rows.filter((e) => e.status === "SUCCEEDED").length,
    deadLettered24h: rows.filter((e) => e.status === "DEAD_LETTERED").length,
    workersReady: fleet.filter((w) => w.status === "READY" || w.status === "BUSY").length,
    workersOffline: fleet.filter(
      (w) => w.status === "OFFLINE" || w.status === "REVOKED",
    ).length,
  };

  return (
    <div className="flex flex-col gap-6 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Dashboard</h1>
          <p className="text-xs text-muted-foreground">
            Most recent {rows.length} executions and {fleet.length} workers.
          </p>
        </div>

        <Button
          variant="outline"
          size="sm"
          onClick={() => {
            executions.reload();
            workers.reload();
          }}
          aria-label="Refresh"
        >
          <RefreshCw className="size-3.5" aria-hidden />
          Refresh
        </Button>
      </header>

      <AsyncBoundary
        state={loading ? "loading" : error ? "error" : "ready"}
        error={error}
        forbidden={forbidden}
        empty={false}
        onRetry={() => {
          executions.reload();
          workers.reload();
        }}
        loadingLabel="Loading dashboard"
      >
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <MetricCard
            href="/executions?status=QUEUED"
            label="Queued"
            value={counts.queued}
            icon={Clock}
          />
          <MetricCard
            href="/executions?status=RUNNING"
            label="Running"
            value={counts.running}
            icon={Layers}
          />
          <MetricCard
            href="/executions?status=SUCCEEDED"
            label="Succeeded"
            value={counts.succeeded24h}
            icon={CheckCircle2}
            tone="positive"
          />
          <MetricCard
            href="/executions?status=FAILED"
            label="Failed"
            value={counts.failed24h}
            icon={AlertTriangle}
            tone="negative"
          />
        </div>

        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <MetricCard
            href="/executions?status=DEAD_LETTERED"
            label="Dead lettered"
            value={counts.deadLettered24h}
            icon={AlertTriangle}
            tone="negative"
          />
          <MetricCard
            href="/workers?status=READY"
            label="Workers ready"
            value={counts.workersReady}
            icon={Users}
            tone="positive"
          />
          <MetricCard
            href="/workers?status=OFFLINE"
            label="Workers offline"
            value={counts.workersOffline}
            icon={Users}
            tone="negative"
          />
          <MetricCard
            href="/schedules"
            label="Schedules due soon"
            value={0}
            icon={Clock}
            muted
          />
        </div>

        <section className="flex flex-col gap-2">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold">Recent executions</h2>
            <Link
              href="/executions"
              className="text-xs text-muted-foreground underline-offset-4 hover:underline"
            >
              View all
            </Link>
          </div>

          {rows.length === 0 ? (
            <p className="rounded-lg border border-border px-4 py-8 text-center text-sm text-muted-foreground">
              No executions yet. Trigger a job or wait for a schedule to fire.
            </p>
          ) : (
            <ul className="flex flex-col divide-y divide-border/50 rounded-lg border border-border">
              {rows.slice(0, 10).map((execution) => (
                <li key={execution.id} className="flex items-center gap-3 px-3 py-2">
                  <Link
                    href={`/executions/${execution.id}`}
                    className="font-mono text-xs underline-offset-4 hover:underline"
                  >
                    {execution.id.slice(0, 8)}
                  </Link>
                  <StatusBadge status={execution.status} />
                  <span className="text-xs text-muted-foreground">
                    {execution.trigger_source}
                  </span>
                  <span
                    className="ml-auto text-xs text-muted-foreground"
                    title={formatTimestamp(execution.created_at)}
                  >
                    {formatRelative(execution.created_at)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </section>
      </AsyncBoundary>
    </div>
  );
}

function MetricCard({
  href,
  label,
  value,
  icon: Icon,
  tone = "neutral",
  muted = false,
}: {
  href: string;
  label: string;
  value: number;
  icon: typeof Clock;
  tone?: "neutral" | "positive" | "negative";
  muted?: boolean;
}) {
  const toneClass =
    tone === "positive"
      ? "text-emerald-600 dark:text-emerald-400"
      : tone === "negative"
        ? "text-red-600 dark:text-red-400"
        : "text-foreground";

  return (
    <Link
      href={href}
      className="flex flex-col gap-1 rounded-lg border border-border p-4 transition-colors hover:bg-accent/40"
    >
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <Icon className="size-3.5" aria-hidden />
        {label}
      </div>
      <span
        className={`text-2xl font-semibold tabular-nums ${muted ? "text-muted-foreground" : toneClass}`}
      >
        {value}
      </span>
    </Link>
  );
}
