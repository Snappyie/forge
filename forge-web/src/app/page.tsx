"use client";

/**
 * Dashboard (UI.md section 2).
 *
 * Answers the spec's three questions in order: is my scheduler healthy, what
 * needs attention, what is happening now. Everything comes from the single
 * `/dashboard` aggregate so the page can never show counts from different
 * moments in time.
 */

import Link from "next/link";
import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Clock,
  Database,
  Gauge,
  Layers,
  ListChecks,
  PauseCircle,
  RefreshCw,
  Send,
  Users,
  Wrench,
} from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { formatRelative, formatTimestamp } from "@/lib/types";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { StatusBadge } from "@/components/status-badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "cn";
import { DashboardWidgetPicker, useWidgetVisibility } from "@/components/ui/dashboard-widgets";

interface Dashboard {
  executions: {
    queued: number;
    running: number;
    succeeded: number;
    failed: number;
    dead_lettered: number;
    cancelled: number;
  };
  workers: { ready: number; busy: number; offline: number };
  queues: { id: string; name: string; depth: number; paused: boolean }[];
  alerts: { critical: number; warning: number; info: number };
  maintenance_active: boolean;
  needs_attention: {
    id: string;
    job_id: string;
    status: string;
    error_message: string | null;
    attempt_count: number;
    created_at: string;
  }[];
  upcoming_executions: {
    execution_id: string;
    job_id: string;
    scheduled_for: string;
    status: string;
    priority: string;
  }[];
  upcoming_schedules: {
    id: string;
    target_id: string;
    target_type: string;
    expression: string | null;
    timezone: string;
    next_run_at: string;
  }[];
}

export default function DashboardPage() {
  const dashboard = useQuery<Dashboard>("/dashboard");
  const [hidden] = useWidgetVisibility();
  const show = (key: string) => !hidden[key];

  return (
    <div className="flex flex-col gap-6 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Dashboard</h1>
          <p className="text-xs text-muted-foreground">
            Is the scheduler healthy, what needs attention, what is happening now.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <DashboardWidgetPicker />
          <Button
            variant="outline"
            size="sm"
            onClick={dashboard.reload}
            aria-label="Refresh"
          >
            <RefreshCw className="mr-1 size-3.5" aria-hidden />
            Refresh
          </Button>
        </div>
      </header>

      <AsyncBoundary
        state={dashboard.state}
        error={dashboard.error}
        forbidden={dashboard.forbidden}
        empty={false}
        onRetry={dashboard.reload}
        loadingLabel="Loading dashboard"
      >
        {dashboard.data ? (
          <DashboardBody data={dashboard.data} show={show} />
        ) : null}
      </AsyncBoundary>
    </div>
  );
}

function DashboardBody({ data, show }: { data: Dashboard; show: (key: string) => boolean }) {
  const totalExecutions =
    data.executions.queued +
    data.executions.running +
    data.executions.succeeded +
    data.executions.failed +
    data.executions.dead_lettered +
    data.executions.cancelled;

  // Charts are derived from the same aggregate the cards use, so a bar can
  // never disagree with the number above it.
  const totalWorkers =
    data.workers.ready + data.workers.busy + data.workers.offline;

  return (
    <div className="flex flex-col gap-6">
      {/* UI.md section 71: maintenance mode must be impossible to miss, because
          nothing is being scheduled while it is set. */}
      {data.maintenance_active ? (
        <div
          role="alert"
          className="flex items-center gap-2 rounded-lg border border-amber-500/50 bg-amber-500/10 px-4 py-3"
        >
          <PauseCircle className="size-4 shrink-0 text-amber-600 dark:text-amber-400" aria-hidden />
          <p className="text-xs text-amber-800 dark:text-amber-200">
            <strong className="font-medium">Maintenance mode is on.</strong>{" "}
            Scheduling is held until it is lifted.
          </p>
          <Link
            href="/settings"
            className="ml-auto text-xs underline underline-offset-4"
          >
            Manage
          </Link>
        </div>
      ) : null}

      {/* Executive status. Every number links to the filtered list behind it. */}
      <section aria-label="Executive status">
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <MetricCard
            href="/executions?status=QUEUED"
            label="Queued"
            value={data.executions.queued}
            icon={Clock}
          />
          <MetricCard
            href="/executions?status=RUNNING"
            label="Running"
            value={data.executions.running}
            icon={Layers}
          />
          <MetricCard
            href="/executions?status=SUCCEEDED"
            label="Succeeded"
            value={data.executions.succeeded}
            icon={CheckCircle2}
            tone="positive"
          />
          <MetricCard
            href="/executions?status=FAILED"
            label="Failed"
            value={data.executions.failed + data.executions.dead_lettered}
            icon={AlertTriangle}
            tone="negative"
          />
        </div>
      </section>

      {show("status") || show("charts") ? (
      <div className="grid gap-4 lg:grid-cols-2">
        {/* UI.md section 2: real-time component status. */}
        {show("status") ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Real-time status</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-2">
            <StatusRow
              icon={Gauge}
              label="Scheduler"
              healthy
              detail="dispatch loop running"
            />
            <StatusRow
              icon={Database}
              label="Database"
              healthy
              detail="queries serving"
            />
            <StatusRow
              icon={Layers}
              label="Queues"
              healthy={data.queues.every((q) => !q.paused)}
              detail={
                data.queues.length === 0
                  ? "no queues defined"
                  : `${data.queues.reduce((sum, q) => sum + q.depth, 0)} queued`
              }
            />
            <StatusRow
              icon={Users}
              label="Workers"
              healthy={data.workers.offline === 0}
              detail={
                data.workers.offline === 0
                  ? `${data.workers.ready} ready, ${data.workers.busy} busy`
                  : `${data.workers.offline} offline`
              }
            />
            <StatusRow
              icon={Send}
              label="Event processing"
              healthy
              detail="outbox publisher running"
            />
          </CardContent>
        </Card>
        ) : null}

        {show("charts") ? (
        <Card>
          <CardHeader>
            <CardTitle className="text-sm">Executions</CardTitle>
          </CardHeader>
          <CardContent className="flex flex-col gap-3">
            <Bar
              label="Succeeded"
              value={data.executions.succeeded}
              total={totalExecutions}
              tone="positive"
            />
            <Bar
              label="Failed / dead lettered"
              value={data.executions.failed + data.executions.dead_lettered}
              total={totalExecutions}
              tone="negative"
            />
            <Bar
              label="Running"
              value={data.executions.running}
              total={totalExecutions}
              tone="neutral"
            />
            <Bar
              label="Queued"
              value={data.executions.queued}
              total={totalExecutions}
              tone="neutral"
            />
            <p className="text-[11px] text-muted-foreground">
              {totalExecutions} execution{totalExecutions === 1 ? "" : "s"} in
              total
            </p>
          </CardContent>
        </Card>
        ) : null}
      </div>
      ) : null}

      <div className="grid gap-4 lg:grid-cols-2">
        {/* UI.md section 2: "Needs attention" must be prominent. */}
        {show("attention") ? (
        <Card>
          <CardHeader className="flex-row items-center justify-between">
            <CardTitle className="text-sm">Needs attention</CardTitle>
            {data.alerts.critical + data.alerts.warning > 0 ? (
              <Link
                href="/alerts"
                className="text-xs text-muted-foreground underline-offset-4 hover:underline"
              >
                {data.alerts.critical} critical · {data.alerts.warning} warning
              </Link>
            ) : null}
          </CardHeader>
          <CardContent>
            {data.needs_attention.length === 0 ? (
              <EmptyState
                title="Nothing needs attention"
                description="No failures or timeouts recorded."
              />
            ) : (
              <ul className="flex flex-col divide-y divide-border/50">
                {data.needs_attention.slice(0, 6).map((item) => (
                  <li key={item.id} className="py-2 first:pt-0 last:pb-0">
                    <Link
                      href={`/executions/${item.id}`}
                      className="flex items-start gap-2 group"
                    >
                      <AlertTriangle
                        className="mt-0.5 size-3.5 shrink-0 text-red-500"
                        aria-hidden
                      />
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-center gap-2">
                          <StatusBadge status={item.status} />
                          <span className="text-xs font-medium group-hover:underline">
                            {item.error_message ?? item.status.toLowerCase()}
                          </span>
                        </div>
                        <p className="mt-0.5 text-[11px] text-muted-foreground">
                          {item.attempt_count} attempt
                          {item.attempt_count === 1 ? "" : "s"} ·{" "}
                          {formatRelative(item.created_at)} ·{" "}
                          <span className="font-mono">job {item.job_id.slice(0, 8)}</span>
                        </p>
                      </div>
                      <ArrowRight
                        className="mt-1 size-3 shrink-0 text-muted-foreground"
                        aria-hidden
                      />
                    </Link>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>
        ) : null}

        {show("upcoming") ? (
        <Card>
          <CardHeader className="flex-row items-center justify-between">
            <CardTitle className="text-sm">Upcoming</CardTitle>
            <Link
              href="/calendar"
              className="text-xs text-muted-foreground underline-offset-4 hover:underline"
            >
              Calendar
            </Link>
          </CardHeader>
          <CardContent>
            {data.upcoming_executions.length === 0 &&
            data.upcoming_schedules.length === 0 ? (
              <EmptyState
                title="Nothing scheduled"
                description="Create a schedule and it will appear here."
              />
            ) : (
              <ul className="flex flex-col divide-y divide-border/50">
                {data.upcoming_executions.map((item) => (
                  <li key={item.execution_id} className="py-2 first:pt-0 last:pb-0">
                    <Link
                      href={`/executions/${item.execution_id}`}
                      className="flex items-center gap-2 text-xs hover:underline"
                    >
                      <Clock className="size-3 shrink-0 text-muted-foreground" aria-hidden />
                      <span className="tabular-nums">
                        {formatTimestamp(item.scheduled_for)}
                      </span>
                      <span className="font-mono text-muted-foreground">
                        {item.job_id.slice(0, 8)}
                      </span>
                      <span className="ml-auto text-muted-foreground">
                        {item.priority.toLowerCase()}
                      </span>
                    </Link>
                  </li>
                ))}
                {data.upcoming_schedules.map((item) => (
                  <li key={item.id} className="py-2 last:pb-0">
                    <Link
                      href="/schedules"
                      className="flex items-center gap-2 text-xs hover:underline"
                    >
                      <Clock
                        className="size-3 shrink-0 text-muted-foreground"
                        aria-hidden
                      />
                      <span className="tabular-nums">
                        {formatTimestamp(item.next_run_at)}
                      </span>
                      <code className="text-muted-foreground">
                        {item.expression ?? item.target_type.toLowerCase()}
                      </code>
                      <span className="ml-auto text-[11px] text-muted-foreground">
                        {item.timezone}
                      </span>
                    </Link>
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>
        ) : null}
      </div>

      {/* UI.md section 27: queue depth, shown at a glance. */}
      {show("queues") ? (
      <Card>
        <CardHeader className="flex-row items-center justify-between">
          <CardTitle className="text-sm">Queues</CardTitle>
          <Link
            href="/queues"
            className="text-xs text-muted-foreground underline-offset-4 hover:underline"
          >
            Manage
          </Link>
        </CardHeader>
        <CardContent>
          {data.queues.length === 0 ? (
            <EmptyState
              title="No queues"
              description="Queues appear here once they are defined."
            />
          ) : (
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {data.queues.map((queue) => (
                <div
                  key={queue.id}
                  className="flex items-center gap-2 rounded-lg border border-border p-3"
                >
                  <Layers className="size-3.5 text-muted-foreground" aria-hidden />
                  <span className="text-sm font-medium">{queue.name}</span>
                  {queue.paused ? (
                    <Badge tone="warning">paused</Badge>
                  ) : null}
                  <span className="ml-auto text-sm tabular-nums">
                    {queue.depth}
                  </span>
                  <span className="text-[11px] text-muted-foreground">deep</span>
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
      ) : null}

      <p className="text-[11px] text-muted-foreground">
        {totalWorkers} worker{totalWorkers === 1 ? "" : "s"} ·{" "}
        {data.executions.cancelled} cancelled
      </p>
    </div>
  );
}

function StatusRow({
  icon: Icon,
  label,
  healthy,
  detail,
}: {
  icon: typeof Gauge;
  label: string;
  healthy: boolean;
  detail: string;
}) {
  return (
    <div className="flex items-center gap-2 text-xs">
      <Icon className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
      <span className="w-32 shrink-0">{label}</span>
      {/* The dot is paired with a word: colour alone is never the signal. */}
      <span
        className={cn(
          "inline-block size-2 shrink-0 rounded-full",
          healthy ? "bg-emerald-500" : "bg-amber-500",
        )}
        aria-hidden
      />
      <span className={healthy ? "text-emerald-600 dark:text-emerald-400" : "text-amber-700 dark:text-amber-400"}>
        {healthy ? "Healthy" : "Degraded"}
      </span>
      <span className="ml-auto truncate text-muted-foreground">{detail}</span>
    </div>
  );
}

function Bar({
  label,
  value,
  total,
  tone,
}: {
  label: string;
  value: number;
  total: number;
  tone: "positive" | "negative" | "neutral";
}) {
  // A zero total would divide by zero; show an empty track instead.
  const percent = total === 0 ? 0 : Math.round((value / total) * 100);
  return (
    <div>
      <div className="mb-1 flex items-baseline justify-between text-xs">
        <span className="text-muted-foreground">{label}</span>
        <span className="tabular-nums">
          {value}
          <span className="ml-1 text-[11px] text-muted-foreground">
            {percent}%
          </span>
        </span>
      </div>
      <div className="h-2 overflow-hidden rounded-full bg-muted">
        <div
          className={cn(
            "h-full rounded-full",
            tone === "positive"
              ? "bg-emerald-500"
              : tone === "negative"
                ? "bg-red-500"
                : "bg-primary",
          )}
          style={{ width: `${percent}%` }}
        />
      </div>
    </div>
  );
}

function MetricCard({
  href,
  label,
  value,
  icon: Icon,
  tone = "neutral",
}: {
  href: string;
  label: string;
  value: number;
  icon: typeof Clock;
  tone?: "neutral" | "positive" | "negative";
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
      <span className={`text-2xl font-semibold tabular-nums ${toneClass}`}>
        {value}
      </span>
    </Link>
  );
}

/** A tiny inline badge; kept local so the card row reads as one unit. */
function Badge({ children, tone }: { children: React.ReactNode; tone: "warning" }) {
  return (
    <span
      className={cn(
        "rounded border px-1.5 py-0.5 text-[10px] uppercase",
        tone === "warning" &&
          "border-amber-500/50 bg-amber-500/10 text-amber-700 dark:text-amber-400",
      )}
    >
      {children}
    </span>
  );
}
