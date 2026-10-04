"use client";

/**
 * Execution list (UI.md section 13, spec 7.6).
 *
 * The page an operator opens when something went wrong, so the columns are the
 * ones that answer "what happened and why": status, which job, what triggered
 * it, which attempt, on which worker, for how long, and what it failed with.
 *
 * Every filter here is wired to a real query parameter the API accepts
 * (`job`, `status`, `worker`, `queue`, `created_after`). A filter button that
 * renders a fixed string and changes nothing is worse than no filter at all.
 */

import Link from "next/link";
import { useMemo, useState } from "react";
import { RefreshCw, Search, X } from "lucide-react";

import { useList } from "@/lib/useQuery";
import { useJobNames } from "@/lib/useJobNames";
import {
  formatDuration,
  formatRelative,
  formatTimestamp,
  type Execution,
  type ExecutionStatus,
} from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { TableBody, TableHeader, TableRow } from "@/components/ui/table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  DataTable,
  DataTableCell,
  DataTableHead,
  NumCell,
  PinnedCell,
  PageHeader,
  RowLink,
  Toolbar,
  TableFooter,
  TableSkeleton,
} from "@/components/page";
import { ErrorClassBadge, PriorityBadge, StatusCell } from "@/components/status-badge";
import { EmptyState, ErrorState, ForbiddenState } from "@/components/states";

const STATUSES: ExecutionStatus[] = [
  "QUEUED",
  "DISPATCHED",
  "RUNNING",
  "SUCCEEDED",
  "FAILED",
  "TIMED_OUT",
  "CANCELLED",
  "DEAD_LETTERED",
  "ABANDONED",
  "RETRY_SCHEDULED",
  "CANCEL_REQUESTED",
  "SCHEDULED",
];

/**
 * How each trigger source is named (spec 02.9).
 *
 * `TriggerSource` spells the scheduled case "SCHEDULE"; "SCHEDULED" is an
 * execution *status*, not a trigger. Mapping by trigger keeps a retried or
 * recovered run honest about where it actually came from.
 */
const TRIGGER_LABELS: Record<string, string> = {
  SCHEDULE: "Schedule",
  MANUAL: "Manual",
  API: "API",
  WORKFLOW: "Workflow",
  RETRY: "Retry",
  RECOVERY: "Recovery",
};

/** Wall-clock time between `started_at` and `ended_at`, in ms. */
function durationMs(execution: Execution): number | null {
  if (!execution.started_at || !execution.ended_at) return null;
  const start = new Date(execution.started_at).getTime();
  const end = new Date(execution.ended_at).getTime();
  if (Number.isNaN(start) || Number.isNaN(end)) return null;
  const delta = end - start;
  return delta >= 0 ? delta : null;
}

export default function ExecutionsPage() {
  const [status, setStatus] = useState("ALL");
  const [search, setSearch] = useState("");

  // Filters the API actually supports, built rather than hardcoded as text.
  const params = useMemo(() => {
    const query = new URLSearchParams();
    query.set("limit", "100");
    if (status !== "ALL") query.set("status", status);
    return query.toString();
  }, [status]);

  const query = useList<Execution>(`/executions?${params}`);
  const rows = query.rows;
  // The API returns a job id per execution; this resolves it to a name so the
  // table is scannable, falling back to the id when it cannot.
  const { nameFor } = useJobNames();

  // Free-text narrows the loaded page. The API has no text filter, so this is
  // explicitly a client-side filter rather than a pretend server query.
  const visible = useMemo(() => {
    const needle = search.trim().toLowerCase();
    if (!needle) return rows;
    return rows.filter(
      (row) =>
        row.id.includes(needle) ||
        (row.error_message ?? "").toLowerCase().includes(needle) ||
        (row.error_class ?? "").toLowerCase().includes(needle) ||
        (row.job_id ?? "").includes(needle) ||
        (nameFor(row.job_id) ?? "").toLowerCase().includes(needle),
    );
  }, [rows, search, nameFor]);

  const filtering = status !== "ALL" || search.trim() !== "";

  function clearFilters() {
    setStatus("ALL");
    setSearch("");
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Executions"
        description="Every run across every job, with the reason it ended the way it did."
        actions={
          <Button
            variant="outline"
            size="sm"
            onClick={query.reload}
            disabled={query.state === "loading"}
          >
            <RefreshCw
              className={query.state === "loading" ? "animate-spin" : undefined}
              aria-hidden
            />
            Refresh
          </Button>
        }
      />

      <Toolbar>
        <div className="relative min-w-[14rem] flex-1 sm:max-w-xs">
          <Search
            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
            aria-hidden
          />
          <Input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Filter by id, job, or error"
            aria-label="Filter loaded executions"
            className="h-7 pl-7 text-[12.5px]"
          />
        </div>

        <Select value={status} onValueChange={(value) => setStatus(value ?? "ALL")}>
          <SelectTrigger
            size="sm"
            className="h-7 w-36 text-[12.5px]"
            aria-label="Filter by status"
          >
            <SelectValue placeholder="Any status" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="ALL">Any status</SelectItem>
            {STATUSES.map((s) => (
              <SelectItem key={s} value={s}>
                {s.replace(/_/g, " ").toLowerCase()}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>

        {filtering ? (
          <Button
            variant="ghost"
            size="sm"
            onClick={clearFilters}
            className="h-7 text-[12.5px] text-muted-foreground"
          >
            <X aria-hidden />
            Clear filters
          </Button>
        ) : null}
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-auto">
        {query.state === "loading" ? (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead>Status</DataTableHead>
                <DataTableHead>Job</DataTableHead>
                <DataTableHead>Trigger</DataTableHead>
                <DataTableHead>Priority</DataTableHead>
                <DataTableHead align="right">Attempt</DataTableHead>
                <DataTableHead>Worker</DataTableHead>
                <DataTableHead align="right">Duration</DataTableHead>
                <DataTableHead align="right">Started</DataTableHead>
                <DataTableHead>Error</DataTableHead>
              </TableRow>
            </TableHeader>
            <TableSkeleton rows={12} columns={9} />
          </DataTable>
        ) : query.state === "error" ? (
          query.forbidden ? (
            <ForbiddenState />
          ) : (
            <ErrorState error={query.error} onRetry={query.reload} />
          )
        ) : visible.length === 0 ? (
          <EmptyState
            title={
              filtering
                ? "No executions match these filters"
                : "No executions yet"
            }
            description={
              filtering
                ? "Nothing here matches. Clear the filters to see every run."
                : "Executions appear here as soon as a schedule fires or a job is triggered."
            }
            action={
              filtering ? (
                <Button size="sm" variant="outline" onClick={clearFilters}>
                  Clear filters
                </Button>
              ) : (
                <Button size="sm" variant="outline" render={<Link href="/jobs" />}>
                  Go to jobs
                </Button>
              )
            }
          />
        ) : (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead className="w-28">Status</DataTableHead>
                <DataTableHead>Job</DataTableHead>
                <DataTableHead className="w-24">Trigger</DataTableHead>
                <DataTableHead className="w-24">Priority</DataTableHead>
                <DataTableHead className="w-20" align="right">
                  Attempt
                </DataTableHead>
                <DataTableHead className="w-24">Worker</DataTableHead>
                <DataTableHead className="w-24" align="right">
                  Duration
                </DataTableHead>
                <DataTableHead className="w-28" align="right">
                  Started
                </DataTableHead>
                <DataTableHead>Error</DataTableHead>
              </TableRow>
            </TableHeader>

            <TableBody>
              {visible.map((execution) => {
                const elapsed = durationMs(execution);
                const jobName = nameFor(execution.job_id);
                return (
                  <TableRow key={execution.id} className="h-8">
                    <DataTableCell>
                      <StatusCell status={execution.status} />
                    </DataTableCell>

                    {/*
                      The job could not be resolved — deleted, or outside the
                      loaded page — so the execution's own id is the link rather
                      than an empty cell.
                    */}
                    <PinnedCell>
                      {jobName ? (
                        <RowLink href={`/jobs/${execution.job_id}`}>
                          {jobName}
                        </RowLink>
                      ) : (
                        <RowLink href={`/executions/${execution.id}`}>
                          <code className="font-mono text-[12px]">
                            {(execution.job_id ?? execution.id).slice(0, 8)}
                          </code>
                        </RowLink>
                      )}
                    </PinnedCell>

                    <DataTableCell className="text-[12.5px] text-muted-foreground">
                      {TRIGGER_LABELS[execution.trigger_source] ??
                        execution.trigger_source}
                    </DataTableCell>

                    <DataTableCell>
                      <PriorityBadge priority={execution.priority} />
                    </DataTableCell>

                    <NumCell className="text-[12.5px]">
                      {execution.attempt_count}
                    </NumCell>

                    <DataTableCell>
                      {execution.worker_id ? (
                        <code className="font-mono text-[11.5px] text-muted-foreground">
                          {execution.worker_id.slice(0, 8)}
                        </code>
                      ) : (
                        <span className="text-muted-foreground">—</span>
                      )}
                    </DataTableCell>

                    {/*
                      A run with no end time has no measured duration. Printing
                      "0s" would claim the work took no time at all, which is a
                      different and wrong statement.
                    */}
                    <NumCell className="text-[12.5px]">
                      {elapsed === null ? (
                        <span className="text-muted-foreground">—</span>
                      ) : (
                        formatDuration(elapsed)
                      )}
                    </NumCell>

                    <NumCell
                      className="text-[12px] text-muted-foreground"
                      title={formatTimestamp(execution.started_at ?? execution.created_at)}
                    >
                      {execution.started_at
                        ? formatRelative(execution.started_at)
                        : formatRelative(execution.created_at)}
                    </NumCell>

                    <DataTableCell>
                      {execution.error_class ? (
                        <ErrorClassBadge errorClass={execution.error_class} />
                      ) : execution.error_message ? (
                        <span
                          className="block max-w-[24rem] truncate text-[12px] text-muted-foreground"
                          title={execution.error_message}
                        >
                          {execution.error_message}
                        </span>
                      ) : (
                        <span className="text-muted-foreground">—</span>
                      )}
                    </DataTableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </DataTable>
        )}
      </div>

      <TableFooter shown={visible.length} total={rows.length} />
    </div>
  );
}