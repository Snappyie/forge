"use client";

/**
 * Dashboard (UI.md section 2).
 *
 * The composition is a monitor, not a card wall. It answers, in reading order:
 * what is the state of the system, what needs a human, and what happens next.
 *
 * Everything comes from the single `/dashboard` aggregate so the page can never
 * show counts captured at different moments.
 */

import Link from "next/link";
import { useEffect, useState } from "react";
import {
  AlertTriangle,
  Clock,
  Layers,
  RefreshCw,
  Users,
} from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { formatRelative, formatTimestamp } from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { Button } from "@/components/ui/button";
import { GettingStarted } from "@/components/ui/getting-started";
import {
  DataTable,
  DataTableCell,
  DataTableHead,
  NumCell,
  PageHeader,
  RowLink,
  SplitBar,
  LegendDot,
  Stat,
  Toolbar,
} from "@/components/page";
import { StatusCell } from "@/components/status-badge";

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
  // Stamped when data lands, not when the component renders: `new Date()` read
  // during render is "now" on every render, which makes the freshness line say
  // "0s ago" no matter how stale the data actually is.
  const [fetchedAt, setFetchedAt] = useState<string | null>(null);

  useEffect(() => {
    if (dashboard.state === "ready") {
      setFetchedAt(new Date().toISOString());
    }
  }, [dashboard.state, dashboard.data]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Dashboard"
        description="Is the scheduler healthy, what needs attention, what is happening now."
        actions={
          <Button
            variant="outline"
            size="sm"
            onClick={dashboard.reload}
            disabled={dashboard.state === "loading"}
          >
            <RefreshCw
              className={
                dashboard.state === "loading" ? "animate-spin" : undefined
              }
              aria-hidden
            />
            Refresh
          </Button>
        }
      />

      <div className="min-h-0 flex-1 overflow-y-auto">
        <AsyncBoundary
          state={dashboard.state}
          error={dashboard.error}
          forbidden={dashboard.forbidden}
          empty={false}
          onRetry={dashboard.reload}
          loadingLabel="Loading dashboard"
        >
          {dashboard.data ? (
            <DashboardBody data={dashboard.data} fetchedAt={fetchedAt} />
          ) : null}
        </AsyncBoundary>
      </div>
    </div>
  );
}

function DashboardBody({
  data,
  fetchedAt,
}: {
  data: Dashboard;
  fetchedAt: string | null;
}) {
  const { executions, workers, queues, alerts } = data;

  const failedTotal = executions.failed + executions.dead_lettered;
  const totalWorkers = workers.ready + workers.busy + workers.offline;
  const totalExecutions =
    executions.queued +
    executions.running +
    executions.succeeded +
    failedTotal +
    executions.cancelled;

  const openAlerts = alerts.critical + alerts.warning;

  return (
    <div className="flex flex-col gap-4 p-4">
      {/*
        UI.md section 71: maintenance mode is impossible to miss, because nothing
        is being scheduled while it is set. It replaces the metric strip rather
        than sitting above it — a banner plus normal-looking numbers reads as
        "everything is fine, with a note".
      */}
      {data.maintenance_active ? (
        <div
          role="alert"
          className="flex items-center gap-2 rounded-md border border-warning/50 bg-warning/10 px-3 py-2"
        >
          <AlertTriangle
            className="size-4 shrink-0 text-warning-foreground"
            aria-hidden
          />
          <p className="text-[12.5px] text-warning-foreground">
            <strong className="font-semibold">Maintenance mode is on.</strong>{" "}
            Scheduling is held until it is lifted.
          </p>
          <Button
            variant="outline"
            size="sm"
            className="ml-auto h-7 text-[12.5px]"
            render={<Link href="/settings" />}
          >
            Manage
          </Button>
        </div>
      ) : null}

      {/*
        A tenant that has never run anything gets the ordered checklist rather
        than a wall of zeroes. `GettingStarted` decides for itself whether to
        render, so there is no hand-maintained condition here that could strand
        a working tenant on a setup screen.
      */}
      <GettingStarted />

      {/*
        The metric strip. Every figure links to the filtered list behind it, so a
        number is a way in rather than something to be read and then re-typed
        into a search box.
      */}
      <section
        aria-label="Execution and capacity summary"
        className="grid grid-cols-2 gap-2 sm:grid-cols-3 lg:grid-cols-6"
      >
        <Stat
          href="/executions?status=RUNNING"
          label="Running"
          value={executions.running}
          tone={executions.running > 0 ? "info" : "neutral"}
        />
        <Stat
          href="/executions?status=QUEUED"
          label="Queued"
          value={executions.queued}
          tone={executions.queued > 0 ? "warning" : "neutral"}
        />
        <Stat
          href="/executions?status=SUCCEEDED"
          label="Succeeded"
          value={executions.succeeded}
          tone="success"
        />
        <Stat
          href="/executions?status=FAILED"
          label="Failed"
          value={failedTotal}
          tone={failedTotal > 0 ? "danger" : "neutral"}
          hint={
            executions.dead_lettered > 0
              ? `${executions.dead_lettered} dead lettered`
              : undefined
          }
        />
        <Stat
          href="/workers"
          label="Workers"
          value={totalWorkers}
          hint={
            workers.offline > 0
              ? `${workers.offline} offline`
              : `${workers.ready} ready`
          }
          tone={workers.offline > 0 ? "warning" : "neutral"}
        />
        <Stat
          href="/alerts"
          label="Open alerts"
          value={openAlerts}
          tone={openAlerts > 0 ? "danger" : "neutral"}
          hint={
            alerts.critical > 0 ? `${alerts.critical} critical` : "none urgent"
          }
        />
      </section>

      {/*
        The execution split. A single bar plus a legend states the proportion in
        one glance, where a row of separate stat cards makes the operator add the
        numbers up themselves.
      */}
      <section
        aria-label="Execution outcome split"
        className="rounded-md border border-border bg-card px-3 py-2.5"
      >
        <div className="mb-2 flex flex-wrap items-baseline justify-between gap-2">
          <h2 className="text-[13px] font-semibold">Executions</h2>
          <p className="text-[12px] text-muted-foreground tabular-nums">
            {totalExecutions} total
            {executions.cancelled > 0
              ? ` · ${executions.cancelled} cancelled`
              : ""}
          </p>
        </div>
        <SplitBar
          segments={[
            { label: "Succeeded", value: executions.succeeded, tone: "success" },
            { label: "Failed", value: failedTotal, tone: "danger" },
            { label: "Queued", value: executions.queued, tone: "warning" },
            { label: "Running", value: executions.running, tone: "info" },
            {
              label: "Cancelled",
              value: executions.cancelled,
              tone: "neutral",
            },
          ]}
        />
        <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1">
          <LegendDot tone="success">Succeeded {executions.succeeded}</LegendDot>
          <LegendDot tone="danger">
            Failed {failedTotal}
          </LegendDot>
          <LegendDot tone="warning">Queued {executions.queued}</LegendDot>
          <LegendDot tone="info">Running {executions.running}</LegendDot>
          {executions.cancelled > 0 ? (
            <LegendDot tone="neutral">
              Cancelled {executions.cancelled}
            </LegendDot>
          ) : null}
        </div>
      </section>

      {/*
        Needs attention and upcoming, side by side. The left column is what needs
        a human; the right is what will need one soon.
      */}
      <div className="grid gap-4 lg:grid-cols-2">
        <section
          aria-label="Needs attention"
          className="rounded-md border border-border bg-card"
        >
          <div className="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
            <h2 className="text-[13px] font-semibold">Needs attention</h2>
            {openAlerts > 0 ? (
              <Link
                href="/alerts"
                className="text-[12px] text-muted-foreground underline-offset-2 hover:underline"
              >
                {alerts.critical} critical · {alerts.warning} warning
              </Link>
            ) : null}
          </div>

          {data.needs_attention.length === 0 ? (
            <p className="px-3 py-6 text-center text-[12.5px] text-muted-foreground">
              Nothing needs attention. No failures or timeouts recorded.
            </p>
          ) : (
            <ul className="max-h-72 overflow-y-auto">
              {data.needs_attention.slice(0, 8).map((item) => (
                <li
                  key={item.id}
                  className="border-b border-border/60 last:border-0"
                >
                  <Link
                    href={`/executions/${item.id}`}
                    className="flex items-start gap-2 px-3 py-2 transition-colors hover:bg-accent/50"
                  >
                    <span className="mt-1.5 size-1.5 shrink-0 rounded-full bg-danger" aria-hidden />
                    <span className="min-w-0 flex-1">
                      <span className="flex items-center gap-2">
                        <StatusCell status={item.status} />
                      </span>
                      <span
                        className="mt-0.5 block truncate text-[12px]"
                        title={item.error_message ?? undefined}
                      >
                        {item.error_message ??
                          item.status.toLowerCase().replace(/_/g, " ")}
                      </span>
                      <span className="mt-0.5 block text-[11px] text-muted-foreground">
                        {item.attempt_count} attempt
                        {item.attempt_count === 1 ? "" : "s"} ·{" "}
                        {formatRelative(item.created_at)} · job{" "}
                        <code className="font-mono">
                          {item.job_id.slice(0, 8)}
                        </code>
                      </span>
                    </span>
                  </Link>
                </li>
              ))}
            </ul>
          )}
        </section>

        <section
          aria-label="Upcoming executions"
          className="rounded-md border border-border bg-card"
        >
          <div className="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
            <h2 className="text-[13px] font-semibold">Upcoming</h2>
            <Link
              href="/calendar"
              className="text-[12px] text-muted-foreground underline-offset-2 hover:underline"
            >
              Calendar
            </Link>
          </div>

          {data.upcoming_executions.length === 0 &&
          data.upcoming_schedules.length === 0 ? (
            <div className="px-3 py-6 text-center">
              <p className="text-[12.5px] text-muted-foreground">
                Nothing scheduled. Create a schedule and it will appear here.
              </p>
              <Button
                size="sm"
                variant="outline"
                className="mt-2 h-7 text-[12.5px]"
                render={<Link href="/schedules" />}
              >
                Go to schedules
              </Button>
            </div>
          ) : (
            <DataTable>
              <thead>
                <tr>
                  <DataTableHead>When</DataTableHead>
                  <DataTableHead>Target</DataTableHead>
                  <DataTableHead className="w-24">Timezone</DataTableHead>
                </tr>
              </thead>
              <tbody>
                {data.upcoming_executions.slice(0, 5).map((item) => (
                  <tr key={item.execution_id} className="h-8 hover:bg-muted/40">
                    <DataTableCell className="tabular-nums">
                      {formatTimestamp(item.scheduled_for)}
                    </DataTableCell>
                    <DataTableCell>
                      <RowLink href={`/executions/${item.execution_id}`}>
                        <code className="font-mono text-[11.5px]">
                          {item.job_id.slice(0, 8)}
                        </code>
                      </RowLink>
                    </DataTableCell>
                    <DataTableCell className="text-[11.5px] text-muted-foreground">
                      {item.priority.toLowerCase()}
                    </DataTableCell>
                  </tr>
                ))}
                {data.upcoming_schedules.slice(0, 8).map((item) => (
                  <tr key={item.id} className="h-8 hover:bg-muted/40">
                    <DataTableCell className="tabular-nums">
                      {formatTimestamp(item.next_run_at)}
                    </DataTableCell>
                    <DataTableCell>
                      <code className="font-mono text-[11.5px]">
                        {item.expression ?? item.target_type.toLowerCase()}
                      </code>
                    </DataTableCell>
                    <DataTableCell className="text-[11.5px] text-muted-foreground">
                      {item.timezone}
                    </DataTableCell>
                  </tr>
                ))}
              </tbody>
            </DataTable>
          )}
        </section>
      </div>

      {/* UI.md section 27: queue depth, shown at a glance. */}
      <section
        aria-label="Queues"
        className="rounded-md border border-border bg-card"
      >
        <div className="flex items-center justify-between gap-2 border-b border-border px-3 py-2">
          <h2 className="text-[13px] font-semibold">Queues</h2>
          <Link
            href="/queues"
            className="text-[12px] text-muted-foreground underline-offset-2 hover:underline"
          >
            Manage
          </Link>
        </div>

        {queues.length === 0 ? (
          <div className="px-3 py-6 text-center">
            <p className="text-[12.5px] text-muted-foreground">
              No queues defined yet.
            </p>
            <Button
              size="sm"
              variant="outline"
              className="mt-2 h-7 text-[12.5px]"
              render={<Link href="/queues" />}
            >
              Create a queue
            </Button>
          </div>
        ) : (
          <DataTable>
            <thead>
              <tr>
                <DataTableHead>Queue</DataTableHead>
                <DataTableHead className="w-24">State</DataTableHead>
                <DataTableHead className="w-24" align="right">
                  Depth
                </DataTableHead>
              </tr>
            </thead>
            <tbody>
              {queues.map((queue) => (
                <tr key={queue.id} className="h-8 hover:bg-muted/40">
                  <DataTableCell className="font-medium">
                    <span className="flex items-center gap-2">
                      <Layers
                        className="size-3.5 shrink-0 text-muted-foreground"
                        aria-hidden
                      />
                      {queue.name}
                    </span>
                  </DataTableCell>
                  <DataTableCell>
                    {queue.paused ? (
                      <span className="text-[12.5px] text-warning-foreground">
                        Paused
                      </span>
                    ) : (
                      <span className="text-[12.5px] text-muted-foreground">
                        Accepting
                      </span>
                    )}
                  </DataTableCell>
                  <NumCell className="text-[12.5px]">{queue.depth}</NumCell>
                </tr>
              ))}
            </tbody>
          </DataTable>
        )}
      </section>

      <p className="flex items-center gap-1.5 text-[11.5px] text-muted-foreground">
        <Users className="size-3" aria-hidden />
        {totalWorkers} worker{totalWorkers === 1 ? "" : "s"}
        {fetchedAt ? (
          <>
            {" · "}
            <Clock className="size-3" aria-hidden />
            updated {formatRelative(fetchedAt)}
          </>
        ) : null}
      </p>
    </div>
  );
}