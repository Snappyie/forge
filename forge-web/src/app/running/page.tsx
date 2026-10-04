"use client";

/**
 * "What's running now?" (UI.md section 21).
 *
 * A dedicated operational view: what is running with live elapsed time, and what
 * is queued behind it.
 *
 * Elapsed is computed from `started_at` against a ticking clock, so a
 * long-running execution keeps counting rather than freezing at whatever value
 * was true when the page loaded.
 */

import { useEffect, useMemo, useState } from "react";
import { RefreshCw } from "lucide-react";

import { useList } from "@/lib/useQuery";
import { useJobNames } from "@/lib/useJobNames";
import {
  formatDuration,
  formatRelative,
  formatTimestamp,
  type Execution,
} from "@/lib/types";
import { Button } from "@/components/ui/button";
import { TableBody, TableHeader, TableRow } from "@/components/ui/table";
import {
  DataTable,
  DataTableCell,
  DataTableHead,
  NumCell,
  PageHeader,
  Panel,
  RowLink,
  TableSkeleton,
} from "@/components/page";
import { PriorityBadge, StatusCell } from "@/components/status-badge";
import { EmptyState, ErrorState, ForbiddenState } from "@/components/states";

/*
 * One request per status, merged here.
 *
 * `GET /executions` accepts a comma-separated `status`, and the server splits
 * on commas — but a deployment running an older build rejects the whole value
 * with "`DISPATCHED,RUNNING` is not a valid status". Sending the two statuses
 * separately and concatenating the rows works against both, and the page shows
 * what is running either way instead of silently reporting nothing.
 */
const RUNNING_STATUSES = ["DISPATCHED", "RUNNING"] as const;

export default function RunningPage() {
  const dispatched = useList<Execution>(
    `/executions?status=${RUNNING_STATUSES[0]}&limit=100`,
  );
  const inFlight = useList<Execution>(
    `/executions?status=${RUNNING_STATUSES[1]}&limit=100`,
  );
  const queued = useList<Execution>(`/executions?status=QUEUED&limit=100`);
  const { nameFor } = useJobNames();

  // A ticking clock so elapsed time advances without re-polling the list.
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);

  // Merge without duplicating: the two queries are disjoint today, but a status
  // appearing in both would otherwise show the operator the same run twice.
  const runningRows = useMemo(() => {
    const seen = new Set<string>();
    return [...dispatched.rows, ...inFlight.rows].filter((row) => {
      if (seen.has(row.id)) return false;
      seen.add(row.id);
      return true;
    });
  }, [dispatched.rows, inFlight.rows]);

  const loading =
    dispatched.state === "loading" ||
    inFlight.state === "loading" ||
    queued.state === "loading";
  const errored = dispatched.state === "error" || queued.state === "error";
  const nothing =
    dispatched.state === "ready" &&
    inFlight.state === "ready" &&
    queued.state === "ready" &&
    runningRows.length === 0 &&
    queued.rows.length === 0;

  function reloadAll() {
    dispatched.reload();
    inFlight.reload();
    queued.reload();
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Running now"
        description="Executions in flight with live elapsed time, and what is queued behind them."
        actions={
          <Button
            variant="outline"
            size="sm"
            onClick={reloadAll}
            disabled={loading}
          >
            <RefreshCw className={loading ? "animate-spin" : undefined} aria-hidden />
            Refresh
          </Button>
        }
      />

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
        {errored ? (
          dispatched.forbidden || inFlight.forbidden || queued.forbidden ? (
            <ForbiddenState />
          ) : (
            <ErrorState
              error={dispatched.error ?? inFlight.error ?? queued.error}
              onRetry={reloadAll}
            />
          )
        ) : loading ? (
          <Panel title="Running">
            <DataTable>
              <TableHeader>
                <TableRow>
                  <DataTableHead>Execution</DataTableHead>
                  <DataTableHead className="w-28">Status</DataTableHead>
                  <DataTableHead className="w-32" align="right">
                    Elapsed
                  </DataTableHead>
                </TableRow>
              </TableHeader>
              <TableSkeleton rows={5} columns={3} />
            </DataTable>
          </Panel>
        ) : nothing ? (
          <EmptyState
            title="Nothing is running"
            description="No execution is dispatched or running, and nothing is waiting. Executions appear here the moment a worker claims work."
          />
        ) : (
          <>
            <Panel
              title="Running"
              description="In flight right now."
              bodyClassName="p-0"
              actions={
                <span className="text-[12px] text-muted-foreground tabular-nums">
                  {runningRows.length}
                </span>
              }
            >
              {runningRows.length === 0 ? (
                <p className="px-3 py-6 text-center text-[12.5px] text-muted-foreground">
                  Nothing running. Executions appear here the moment they are
                  dispatched.
                </p>
              ) : (
                <DataTable>
                  <TableHeader>
                    <TableRow>
                      <DataTableHead>Job</DataTableHead>
                      <DataTableHead className="w-28">Status</DataTableHead>
                      <DataTableHead className="w-24">Priority</DataTableHead>
                      <DataTableHead className="w-24">Worker</DataTableHead>
                      <DataTableHead className="w-28" align="right">
                        Started
                      </DataTableHead>
                      <DataTableHead className="w-28" align="right">
                        Elapsed
                      </DataTableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {runningRows.map((execution) => {
                      const jobName = nameFor(execution.job_id);
                      return (
                        <TableRow key={execution.id} className="h-8">
                          <DataTableCell className="font-medium">
                            {jobName ? (
                              <RowLink href={`/jobs/${execution.job_id}`}>
                                {jobName}
                              </RowLink>
                            ) : (
                              <RowLink href={`/executions/${execution.id}`}>
                                <code className="font-mono text-[12px]">
                                  {execution.id.slice(0, 8)}
                                </code>
                              </RowLink>
                            )}
                          </DataTableCell>
                          <DataTableCell>
                            <StatusCell status={execution.status} />
                          </DataTableCell>
                          <DataTableCell>
                            <PriorityBadge priority={execution.priority} />
                          </DataTableCell>
                          <DataTableCell className="text-[11.5px] text-muted-foreground">
                            {execution.worker_id
                              ? execution.worker_id.slice(0, 8)
                              : "—"}
                          </DataTableCell>
                          <DataTableCell
                            align="right"
                            className="text-[12px] text-muted-foreground"
                            title={formatTimestamp(execution.started_at)}
                          >
                            {formatRelative(execution.started_at)}
                          </DataTableCell>
                          <NumCell className="text-[12.5px]">
                            {elapsed(execution.started_at, now)}
                          </NumCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </DataTable>
              )}
            </Panel>

            <Panel
              title="Queued"
              description="Waiting for a worker to claim it."
              bodyClassName="p-0"
              actions={
                <span className="text-[12px] text-muted-foreground tabular-nums">
                  {queued.rows.length}
                </span>
              }
            >
              {queued.rows.length === 0 ? (
                <p className="px-3 py-6 text-center text-[12.5px] text-muted-foreground">
                  Nothing queued. Work waiting for a worker appears here.
                </p>
              ) : (
                <DataTable>
                  <TableHeader>
                    <TableRow>
                      <DataTableHead>Job</DataTableHead>
                      <DataTableHead className="w-24">Priority</DataTableHead>
                      <DataTableHead className="w-32" align="right">
                        Waiting since
                      </DataTableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {queued.rows.map((execution) => {
                      const jobName = nameFor(execution.job_id);
                      return (
                        <TableRow key={execution.id} className="h-8">
                          <DataTableCell className="font-medium">
                            {jobName ? (
                              <RowLink href={`/jobs/${execution.job_id}`}>
                                {jobName}
                              </RowLink>
                            ) : (
                              <RowLink href={`/executions/${execution.id}`}>
                                <code className="font-mono text-[12px]">
                                  {execution.id.slice(0, 8)}
                                </code>
                              </RowLink>
                            )}
                          </DataTableCell>
                          <DataTableCell>
                            <PriorityBadge priority={execution.priority} />
                          </DataTableCell>
                          <DataTableCell
                            align="right"
                            className="text-[12px] text-muted-foreground"
                            title={formatTimestamp(execution.created_at)}
                          >
                            waiting {formatRelative(execution.created_at)}
                          </DataTableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </DataTable>
              )}
            </Panel>
          </>
        )}
      </div>
    </div>
  );
}

/**
 * Wall-clock time since `started_at`.
 *
 * An execution with no start time has not begun, so it says so rather than
 * reporting a zero-second run.
 */
function elapsed(startedAt: string | null, now: number): string {
  if (!startedAt) return "not started";
  const start = Date.parse(startedAt);
  if (Number.isNaN(start)) return "unknown";
  return formatDuration(Math.max(0, now - start));
}