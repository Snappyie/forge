"use client";

import Link from "next/link";
import { useState } from "react";
import { Search, Download, GitCompare, RefreshCw, MoreHorizontal } from "lucide-react";

import { useList } from "@/lib/useQuery";
import {
  formatRelative,
  formatTimestamp,
  type Execution,
  type ExecutionStatus,
} from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { StatusBadge } from "@/components/status-badge";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Checkbox } from "@/components/ui/checkbox";
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

/** How each trigger source is named in the table (spec 02.9). */
const TRIGGER_LABELS: Record<string, string> = {
  SCHEDULE: "Schedule",
  MANUAL: "Manual",
  API: "Api",
  WORKFLOW: "Workflow",
  RETRY: "Retry",
  RECOVERY: "Recovery",
};

export default function ExecutionsPage() {
  const [status, setStatus] = useState("ALL");
  const path = status === "ALL" ? "/executions?limit=50" : `/executions?limit=50&status=${status}`;
  const query = useList<Execution>(path);
  const rows = query.rows;

  return (
    <div className="flex flex-col gap-6 p-6 lg:p-8">
      <header className="flex flex-wrap items-center justify-between gap-4">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Executions</h1>
          <p className="text-sm text-muted-foreground mt-1">
            Every run, across every job, with the reason it ended the way it did.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" className="h-9">
            <Download className="mr-2 size-4" />
            Export
          </Button>
          <Button variant="outline" size="sm" className="h-9">
            <GitCompare className="mr-2 size-4" />
            Compare
          </Button>
        </div>
      </header>

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative flex-1 min-w-[200px] max-w-sm">
          <Search className="absolute left-2.5 top-2.5 h-4 w-4 text-muted-foreground" />
          <Input placeholder="Search execution or job..." className="pl-8 h-9" />
        </div>
        <Select value={status} onValueChange={(value) => setStatus(value ?? "ALL")}>
          <SelectTrigger className="w-40 h-9" aria-label="Filter by status">
            <div className="flex gap-1.5"><span className="text-muted-foreground">Status:</span><SelectValue placeholder="any" /></div>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="ALL">any</SelectItem>
            {STATUSES.map((s) => (
              <SelectItem key={s} value={s}>
                {s.replace(/_/g, " ").toLowerCase()}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button variant="outline" className="h-9 text-muted-foreground font-normal">After: 2026-10-01</Button>
        <Button variant="outline" className="h-9 text-muted-foreground font-normal">Trigger: Schedule</Button>
        <Button variant="outline" className="h-9 text-muted-foreground font-normal">Queue: default</Button>
        <div className="ml-auto">
          <Button variant="ghost" size="sm" className="h-9 text-muted-foreground hover:text-foreground" onClick={query.reload}>
            <RefreshCw className="mr-2 size-3.5" />
            Live · 5s
          </Button>
        </div>
      </div>

      <div className="rounded-lg border border-border bg-card">
        <AsyncBoundary
          state={query.state}
          error={query.error}
          forbidden={query.forbidden}
          empty={query.state === "ready" && rows.length === 0}
          onRetry={query.reload}
          loadingLabel="Loading executions"
          emptyTitle={status === "ALL" ? "No executions yet" : "No executions with this status"}
          emptyDescription={status === "ALL" ? "Trigger a job or wait for a schedule to fire." : "Clear the status filter to see other executions."}
          emptyAction={
            status === "ALL" ? (
              <Button size="sm" variant="outline" render={<Link href="/jobs" />}>Go to jobs</Button>
            ) : null
          }
        >
          <div className="overflow-x-auto">
            <Table>
              <TableHeader className="bg-transparent">
                <TableRow className="hover:bg-transparent">
                  <TableHead className="w-12 pl-4"><Checkbox /></TableHead>
                  <TableHead className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground">Status</TableHead>
                  <TableHead className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground">Job</TableHead>
                  <TableHead className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground">Trigger</TableHead>
                  <TableHead className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground">Attempt</TableHead>
                  <TableHead className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground">Worker</TableHead>
                  <TableHead className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground">Duration</TableHead>
                  <TableHead className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground">Started</TableHead>
                  <TableHead className="text-[10px] uppercase tracking-wider font-semibold text-muted-foreground">Error</TableHead>
                  <TableHead className="w-12"></TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((execution) => (
                  <TableRow key={execution.id} className="group">
                    <TableCell className="pl-4"><Checkbox /></TableCell>
                    <TableCell>
                      <StatusBadge status={execution.status} />
                    </TableCell>
                    <TableCell>
                      <Link href={`/executions/${execution.id}`} className="block">
                        <div className="font-medium text-sm">{(execution as any).job_name || "Unknown Job"}</div>
                        <div className="text-xs text-muted-foreground mt-0.5">ex_{execution.id.slice(0, 8)}</div>
                      </Link>
                    </TableCell>
                    <TableCell>
                      {/* `TriggerSource` spells the scheduled case "SCHEDULE";
                          "SCHEDULED" is an execution *status*, not a trigger.
                          Mapping by trigger keeps a retried or recovered run
                          honest about where it actually came from. */}
                      <Badge variant="outline" className="text-[10px] font-normal uppercase bg-background">
                        {TRIGGER_LABELS[execution.trigger_source] ?? execution.trigger_source}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-sm">{execution.attempt_count} of {(execution as any).max_attempts || 3}</TableCell>
                    <TableCell className="font-mono text-sm text-muted-foreground">
                      {execution.worker_id ? execution.worker_id.slice(0, 8) : "—"}
                    </TableCell>
                    <TableCell className="text-sm">
                      {(execution as any).duration_seconds ? `${Math.round((execution as any).duration_seconds)}s` : "—"}
                    </TableCell>
                    <TableCell className="text-sm">
                      {new Date(execution.created_at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })}
                    </TableCell>
                    <TableCell>
                      {execution.error_class ? (
                        <Badge variant="secondary" className="bg-red-500/10 text-red-600 dark:text-red-400 border-none rounded uppercase text-[10px]">
                          {execution.error_class}
                        </Badge>
                      ) : (
                        <span className="text-muted-foreground">—</span>
                      )}
                    </TableCell>
                    <TableCell>
                      <Button variant="ghost" size="icon" className="h-8 w-8 text-muted-foreground opacity-0 group-hover:opacity-100 transition-opacity">
                        <MoreHorizontal className="size-4" />
                      </Button>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        </AsyncBoundary>
        
        <div className="p-4 border-t border-border flex items-center justify-between text-xs text-muted-foreground">
          <p>Showing 1-{rows.length} of {rows.length} executions</p>
          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" disabled className="h-8">Previous</Button>
            <Button variant="outline" size="sm" className="h-8">Next</Button>
          </div>
        </div>
      </div>
    </div>
  );
}
