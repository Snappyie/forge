"use client";

/**
 * Execution list (spec 7.9).
 *
 * Status filters map onto the API's `status` query parameter, so filtering
 * happens server-side rather than by fetching everything and discarding rows.
 */

import Link from "next/link";
import { useState } from "react";
import { RefreshCw } from "lucide-react";

import { useList } from "@/lib/useQuery";
import {
  formatRelative,
  formatTimestamp,
  type Execution,
  type ExecutionStatus,
} from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { PriorityBadge, StatusBadge } from "@/components/status-badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

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

export default function ExecutionsPage() {
  const [status, setStatus] = useState("ALL");
  const path =
    status === "ALL" ? "/executions?limit=50" : `/executions?limit=50&status=${status}`;
  const query = useList<Execution>(path);

  const rows = query.rows;

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Executions</h1>
          <p className="text-xs text-muted-foreground">
            Every run of a job, with its attempts and outcomes.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Select value={status} onValueChange={(value) => setStatus(value ?? "ALL")}>
            <SelectTrigger className="w-44" aria-label="Filter by status">
              <SelectValue placeholder="Status" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="ALL">All statuses</SelectItem>
              {STATUSES.map((s) => (
                <SelectItem key={s} value={s}>
                  {s.replace(/_/g, " ").toLowerCase()}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Button variant="outline" size="sm" onClick={query.reload} aria-label="Refresh">
            <RefreshCw className="size-3.5" aria-hidden />
            Refresh
          </Button>
        </div>
      </header>

      <div className="rounded-lg border border-border">
        <AsyncBoundary
          state={query.state}
          error={query.error}
          forbidden={query.forbidden}
          empty={query.state === "ready" && rows.length === 0}
          onRetry={query.reload}
          loadingLabel="Loading executions"
          emptyTitle={
            status === "ALL" ? "No executions yet" : "No executions with this status"
          }
          emptyDescription={
            status === "ALL"
              ? "Trigger a job or wait for a schedule to fire."
              : "Clear the status filter to see other executions."
          }
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Execution</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Priority</TableHead>
                <TableHead>Source</TableHead>
                <TableHead>Attempts</TableHead>
                <TableHead>Worker</TableHead>
                <TableHead>Created</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((execution) => (
                <TableRow key={execution.id}>
                  <TableCell>
                    <Link
                      href={`/executions/${execution.id}`}
                      className="font-mono text-xs underline-offset-4 hover:underline"
                    >
                      {execution.id.slice(0, 8)}
                    </Link>
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={execution.status} />
                    {execution.error_class ? (
                      <code className="ml-2 text-[10px] text-muted-foreground">
                        {execution.error_class}
                      </code>
                    ) : null}
                  </TableCell>
                  <TableCell>
                    <PriorityBadge priority={execution.priority} />
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {execution.trigger_source}
                  </TableCell>
                  <TableCell className="text-xs">{execution.attempt_count}</TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {execution.worker_id ? execution.worker_id.slice(0, 8) : "—"}
                  </TableCell>
                  <TableCell
                    className="text-xs text-muted-foreground"
                    title={formatTimestamp(execution.created_at)}
                  >
                    {formatRelative(execution.created_at)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </AsyncBoundary>
      </div>
    </div>
  );
}
