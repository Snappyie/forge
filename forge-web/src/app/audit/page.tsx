"use client";

/**
 * Audit log (spec 7.14).
 *
 * Read-only: spec 05 endpoint 52 exposes no mutation, so this page offers
 * none. Filters map onto the API's query parameters.
 */

import { useState } from "react";
import { RefreshCw } from "lucide-react";

import { useList } from "@/lib/useQuery";
import { formatTimestamp, type AuditEvent } from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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

export default function AuditPage() {
  const [action, setAction] = useState("ALL");
  const [resource, setResource] = useState("");

  const params = new URLSearchParams({ limit: "50" });
  if (action !== "ALL") params.set("action", action);
  if (resource.trim()) params.set("resource", resource.trim());

  const query = useList<AuditEvent>(`/audit-events?${params.toString()}`);
  const rows = query.rows;
  const filtering = action !== "ALL" || resource.trim().length > 0;

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Audit</h1>
          <p className="text-xs text-muted-foreground">
            Security-relevant actions, append-only.
          </p>
        </div>

        <Button variant="outline" size="sm" onClick={query.reload} aria-label="Refresh">
          <RefreshCw className="size-3.5" aria-hidden />
          Refresh
        </Button>
      </header>

      <div className="flex flex-wrap items-center gap-2">
        <Select value={action} onValueChange={(value) => setAction(value ?? "ALL")}>
          <SelectTrigger className="w-48" aria-label="Filter by action">
            <SelectValue placeholder="Action" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="ALL">All actions</SelectItem>
            <SelectItem value="job.create">job.create</SelectItem>
            <SelectItem value="job.archive">job.archive</SelectItem>
            <SelectItem value="execution.cancel">execution.cancel</SelectItem>
            <SelectItem value="worker.revoke">worker.revoke</SelectItem>
            <SelectItem value="api_key.create">api_key.create</SelectItem>
          </SelectContent>
        </Select>

        <Input
          value={resource}
          onChange={(e) => setResource(e.target.value)}
          placeholder="Filter by resource type"
          aria-label="Filter by resource"
          className="w-56"
        />
      </div>

      <div className="rounded-lg border border-border">
        <AsyncBoundary
          state={query.state}
          error={query.error}
          forbidden={query.forbidden}
          empty={query.state === "ready" && rows.length === 0}
          onRetry={query.reload}
          loadingLabel="Loading audit events"
          emptyTitle={filtering ? "No audit events match" : "No audit events yet"}
          emptyDescription={
            filtering
              ? "Clear the filters to see the full history."
              : "Security-relevant actions will appear here."
          }
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>When</TableHead>
                <TableHead>Action</TableHead>
                <TableHead>Resource</TableHead>
                <TableHead>Actor</TableHead>
                <TableHead>Result</TableHead>
                <TableHead>Request</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((event) => (
                <TableRow key={event.id}>
                  <TableCell className="whitespace-nowrap text-xs text-muted-foreground">
                    {formatTimestamp(event.created_at)}
                  </TableCell>
                  <TableCell className="font-mono text-xs">{event.action}</TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {event.resource_type}
                    {event.resource_id ? (
                      <code className="ml-1">
                        {event.resource_id.slice(0, 8)}
                      </code>
                    ) : null}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {event.actor_type}
                    {event.actor_id ? ` ${event.actor_id.slice(0, 8)}` : ""}
                  </TableCell>
                  <TableCell>
                    <span
                      className={
                        event.result === "SUCCESS"
                          ? "text-xs text-emerald-600 dark:text-emerald-400"
                          : "text-xs text-red-600 dark:text-red-400"
                      }
                    >
                      {event.result}
                    </span>
                  </TableCell>
                  <TableCell className="font-mono text-[10px] text-muted-foreground">
                    {event.request_id ? event.request_id.slice(0, 8) : "—"}
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
