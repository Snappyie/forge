"use client";

/**
 * Workflow list (spec 7.8).
 *
 * The API exposes workflows read-only in the current build, so this page lists
 * and links out rather than offering edits it cannot persist.
 */

import Link from "next/link";
import { RefreshCw } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { formatTimestamp, type Workflow as WorkflowSummary } from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { StatusBadge } from "@/components/status-badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

export default function WorkflowsPage() {
  const query = useQuery<{ data: WorkflowSummary[] }>("/workflows");
  const rows = Array.isArray(query.data?.data) ? query.data.data : [];

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Workflows</h1>
          <p className="text-xs text-muted-foreground">
            Directed acyclic graphs of jobs, approvals, and delays.
          </p>
        </div>

        <Button variant="outline" size="sm" onClick={query.reload} aria-label="Refresh">
          <RefreshCw className="size-3.5" aria-hidden />
          Refresh
        </Button>
      </header>

      <div className="rounded-lg border border-border">
        <AsyncBoundary
          state={query.state}
          error={query.error}
          forbidden={query.forbidden}
          empty={query.state === "ready" && rows.length === 0}
          onRetry={query.reload}
          loadingLabel="Loading workflows"
          emptyTitle="No workflows yet"
          emptyDescription="A workflow runs several jobs as one graph."
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Key</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Updated</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((workflow) => (
                <TableRow key={workflow.id}>
                  <TableCell>
                    <Link
                      href={`/workflows/${workflow.id}`}
                      className="font-medium underline-offset-4 hover:underline"
                    >
                      {workflow.name}
                    </Link>
                  </TableCell>
                  <TableCell>
                    <code className="text-xs text-muted-foreground">
                      {workflow.key ?? "—"}
                    </code>
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={workflow.status} />
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {formatTimestamp(workflow.updated_at)}
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
