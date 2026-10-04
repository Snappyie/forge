"use client";

/**
 * "What's going to run?" (UI.md section 22).
 *
 * A forward-looking timeline. Merges executions that already have a scheduled
 * time with the schedules that will produce the next ones, so the page is
 * useful at 09:00 for a 02:00 job that has not run yet today.
 */

import { RefreshCw } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { useJobNames } from "@/lib/useJobNames";
import { formatRelative, formatTimestamp } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { TableBody, TableHeader, TableRow } from "@/components/ui/table";
import {
  DataTable,
  DataTableCell,
  DataTableHead,
  PageHeader,
  RowLink,
  TableFooter,
  TableSkeleton,
} from "@/components/page";
import { PriorityBadge, StatusCell } from "@/components/status-badge";
import { EmptyState, ErrorState, ForbiddenState } from "@/components/states";

interface Item {
  kind: "execution" | "schedule";
  id: string;
  job_id: string;
  at: string;
  status?: string;
  priority?: string;
  expression?: string | null;
  timezone?: string;
}

interface Upcoming {
  items: Item[];
  count: number;
}

export default function UpcomingPage() {
  const upcoming = useQuery<Upcoming>("/upcoming");
  const { nameFor } = useJobNames();

  const items = upcoming.data?.items ?? [];
  const loading = upcoming.state === "loading";

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Upcoming"
        description="The next scheduled executions and the schedules that produce them, in time order."
        actions={
          <Button
            variant="outline"
            size="sm"
            onClick={upcoming.reload}
            disabled={loading}
          >
            <RefreshCw className={loading ? "animate-spin" : undefined} aria-hidden />
            Refresh
          </Button>
        }
      />

      <div className="min-h-0 flex-1 overflow-auto">
        {loading ? (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead className="w-52">When</DataTableHead>
                <DataTableHead>Job</DataTableHead>
                <DataTableHead className="w-32">Kind</DataTableHead>
                <DataTableHead className="w-28">Schedule</DataTableHead>
                <DataTableHead className="w-24">Timezone</DataTableHead>
                <DataTableHead className="w-24">Status</DataTableHead>
              </TableRow>
            </TableHeader>
            <TableSkeleton rows={10} columns={6} />
          </DataTable>
        ) : upcoming.state === "error" ? (
          upcoming.forbidden ? (
            <ForbiddenState />
          ) : (
            <ErrorState error={upcoming.error} onRetry={upcoming.reload} />
          )
        ) : items.length === 0 ? (
          <EmptyState
            title="Nothing scheduled"
            description="Create a schedule and it will appear here with its next run time."
          />
        ) : (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead className="w-52">When</DataTableHead>
                <DataTableHead>Job</DataTableHead>
                <DataTableHead className="w-32">Kind</DataTableHead>
                <DataTableHead className="w-28">Schedule</DataTableHead>
                <DataTableHead className="w-24">Timezone</DataTableHead>
                <DataTableHead className="w-24">Status</DataTableHead>
              </TableRow>
            </TableHeader>

            <TableBody>
              {items.map((item) => {
                const jobName = nameFor(item.job_id);
                return (
                  <TableRow key={`${item.kind}-${item.id}`} className="h-8">
                    {/*
                      Absolute time with the relative age as the tooltip: the
                      operator scans for "is it soon", then needs the exact
                      instant to reason about a collision.
                    */}
                    <DataTableCell
                      className="tabular-nums"
                      title={formatRelative(item.at)}
                    >
                      {formatTimestamp(item.at)}
                    </DataTableCell>

                    <DataTableCell className="font-medium">
                      {jobName ? (
                        <RowLink href={`/jobs/${item.job_id}`}>{jobName}</RowLink>
                      ) : (
                        <RowLink href={`/jobs/${item.job_id}`}>
                          <code className="font-mono text-[12px]">
                            {item.job_id.slice(0, 8)}
                          </code>
                        </RowLink>
                      )}
                    </DataTableCell>

                    <DataTableCell className="text-[12.5px] text-muted-foreground">
                      {item.kind === "execution" ? "Execution" : "Schedule"}
                    </DataTableCell>

                    <DataTableCell>
                      {item.kind === "schedule" ? (
                        <code className="font-mono text-[11.5px]">
                          {item.expression ?? "—"}
                        </code>
                      ) : item.priority ? (
                        <PriorityBadge priority={item.priority} />
                      ) : (
                        <span className="text-muted-foreground">—</span>
                      )}
                    </DataTableCell>

                    <DataTableCell className="text-[11.5px] text-muted-foreground">
                      {item.timezone ?? "—"}
                    </DataTableCell>

                    <DataTableCell>
                      {item.status ? (
                        <StatusCell status={item.status} />
                      ) : (
                        <span className="text-[12.5px] text-muted-foreground">
                          Pending
                        </span>
                      )}
                    </DataTableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </DataTable>
        )}
      </div>

      <TableFooter shown={items.length} total={upcoming.data?.count} />
    </div>
  );
}