"use client";

/**
 * Worker list (spec 7.12).
 *
 * Drain and revoke are gated on the `workers:admin` permission, so a viewer
 * sees the fleet without seeing controls they cannot use.
 */

import Link from "next/link";
import { useState } from "react";
import { Loader2, Power, RefreshCw, ShieldOff } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { useList } from "@/lib/useQuery";
import { formatRelative, formatTimestamp, roleCan, type Worker } from "@/lib/types";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

export default function WorkersPage() {
  const { session } = useAuth();
  const [status, setStatus] = useState("ALL");
  const path = status === "ALL" ? "/workers?limit=50" : `/workers?limit=50&status=${status}`;
  const query = useList<Worker>(path);

  const rows = query.rows;
  const canAdmin = roleCan(session?.role, "workers:admin");

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Workers</h1>
          <p className="text-xs text-muted-foreground">
            {rows.length} registered; a revoked worker never receives work again.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Select value={status} onValueChange={(value) => setStatus(value ?? "ALL")}>
            <SelectTrigger className="w-40" aria-label="Filter by status">
              <SelectValue placeholder="Status" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="ALL">All statuses</SelectItem>
              <SelectItem value="READY">Ready</SelectItem>
              <SelectItem value="BUSY">Busy</SelectItem>
              <SelectItem value="DRAINING">Draining</SelectItem>
              <SelectItem value="OFFLINE">Offline</SelectItem>
              <SelectItem value="REVOKED">Revoked</SelectItem>
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
          loadingLabel="Loading workers"
          emptyTitle="No workers registered"
          emptyDescription="Register a worker to begin executing queued work."

          emptyAction={
            <Button size="sm" variant="outline" render={<Link href="/docs" />}>
              How workers connect
            </Button>
          }
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Worker</TableHead>
                <TableHead>Status</TableHead>
                <TableHead>Draining</TableHead>
                <TableHead>Last heartbeat</TableHead>
                {canAdmin ? <TableHead className="text-right">Actions</TableHead> : null}
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((worker) => (
                <TableRow key={worker.id}>
                  <TableCell>
                    <Link
                      href={`/workers/${worker.id}`}
                      className="font-medium underline-offset-4 hover:underline"
                    >
                      {worker.name ?? worker.hostname}
                    </Link>
                    <span className="ml-2 text-xs text-muted-foreground">
                      {worker.hostname}
                    </span>
                  </TableCell>
                  <TableCell>
                    <StatusBadge status={worker.status} />
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {worker.draining ? "yes" : "no"}
                  </TableCell>
                  <TableCell
                    className="text-xs text-muted-foreground"
                    title={formatTimestamp(worker.last_heartbeat_at)}
                  >
                    {formatRelative(worker.last_heartbeat_at)}
                  </TableCell>
                  {canAdmin ? (
                    <TableCell className="text-right">
                      <WorkerActions worker={worker} onDone={query.reload} />
                    </TableCell>
                  ) : null}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </AsyncBoundary>
      </div>
    </div>
  );
}

function WorkerActions({
  worker,
  onDone,
}: {
  worker: Worker;
  onDone: () => void;
}) {
  const [busy, setBusy] = useState(false);

  async function act(action: "drain" | "revoke", confirm: string) {
    if (!window.confirm(confirm)) return;
    setBusy(true);
    try {
      await api.post(`/workers/${worker.id}/${action}`);
      onDone();
    } catch (cause) {
      window.alert(cause instanceof ApiError ? cause.message : "the request failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex items-center justify-end gap-1">
      {worker.status !== "DRAINING" && worker.status !== "REVOKED" ? (
        <Button
          variant="ghost"
          size="icon"
          disabled={busy}
          aria-label={`Drain ${worker.hostname}`}
          title="Drain: finish current work, accept nothing new"
          onClick={() =>
            act("drain", `Drain "${worker.hostname}"? It stops accepting new work.`)
          }
        >
          {busy ? (
            <Loader2 className="size-3.5 animate-spin" aria-hidden />
          ) : (
            <Power className="size-3.5" aria-hidden />
          )}
        </Button>
      ) : null}

      {worker.status !== "REVOKED" ? (
        <Button
          variant="ghost"
          size="icon"
          disabled={busy}
          aria-label={`Revoke ${worker.hostname}`}
          title="Revoke: permanently"
          onClick={() =>
            act(
              "revoke",
              `Revoke "${worker.hostname}"? It can never receive work again.`,
            )
          }
        >
          <ShieldOff className="size-3.5" aria-hidden />
        </Button>
      ) : null}
    </div>
  );
}
