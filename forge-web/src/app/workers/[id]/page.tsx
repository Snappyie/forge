"use client";

/**
 * Worker detail (UI.md section 26).
 *
 * Reads the registered worker, the executions it holds, and the real worker
 * controls (drain and revoke) rather than presenting decorative rows.
 */

import Link from "next/link";
import { use, useState } from "react";
import { ArrowLeft, Ban, LogOut } from "lucide-react";

import { useList, useQuery } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { formatRelative, formatTimestamp, type Execution, type Worker } from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { StatusBadge } from "@/components/status-badge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "cn";

/**
 * `/workers/{id}` returns `{ worker, active_lease }`, so the record is nested
 * one level below the unwrapped envelope.
 */
interface WorkerDetail {
  worker: Worker;
  active_lease: {
    execution_id: string;
    expires_at: string;
  } | null;
}

export default function WorkerDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  const detail = useQuery<WorkerDetail>(`/workers/${id}`);
  // Executions currently assigned to this worker.
  const held = useList<Execution>(`/executions?worker_id=${id}&limit=50`);

  const record = detail.data?.worker ?? null;

  async function drain() {
    setBusy(true);
    try {
      await api.post(`/workers/${id}/drain`, {});
      toast.success("Draining requested", {
        label: "Back to workers",
        onClick: () => window.location.assign("/workers"),
      });
      detail.reload();
    } catch (error) {
      toast.error("Could not drain", error instanceof Error ? error.message : undefined);
    } finally {
      setBusy(false);
    }
  }

  async function revoke() {
    setBusy(true);
    try {
      await api.post(`/workers/${id}/revoke`, {});
      toast.success("Worker revoked");
      detail.reload();
    } catch (error) {
      toast.error("Could not revoke", error instanceof Error ? error.message : undefined);
    } finally {
      setBusy(false);
    }
  }

  return (
    <AsyncBoundary
      state={detail.state}
      error={detail.error}
      forbidden={detail.forbidden}
      empty={false}
      onRetry={detail.reload}
      loadingLabel="Loading worker"
    >
      {record ? (
        <div className="flex flex-col gap-4 p-6">
          <header className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <Link
                href="/workers"
                className="mb-1 inline-flex items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
              >
                <ArrowLeft className="size-3" aria-hidden />
                All workers
              </Link>
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="text-lg font-semibold">{record.name ?? record.hostname}</h1>
                <StatusBadge status={record.status} />
                {record.draining ? (
                  <Badge variant="outline" className="text-[10px]">
                    draining
                  </Badge>
                ) : null}
              </div>
              <p className="mt-1 text-xs text-muted-foreground">
                {record.hostname}
                {record.version ? ` · v${record.version}` : ""}
              </p>
            </div>

            <div className="flex items-center gap-2">
              <Button variant="outline" size="sm" disabled={busy || record.draining} onClick={drain}>
                <LogOut className="mr-1 size-3.5" aria-hidden />
                Drain
              </Button>
              <Button variant="outline" size="sm" disabled={busy} onClick={revoke}>
                <Ban className="mr-1 size-3.5" aria-hidden />
                Revoke
              </Button>
            </div>
          </header>

          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            <Metric label="Status" value={record.status.toLowerCase()} />
            <Metric label="Last heartbeat" value={formatRelative(record.last_heartbeat_at)} />
            <Metric label="Registered" value={formatTimestamp(record.registered_at)} />
            <Metric label="Held executions" value={String(held.rows.length)} />
          </div>

          {detail.data?.active_lease ? (
            <Card className="border-amber-500/40">
              <CardHeader>
                <CardTitle className="text-sm">Active lease</CardTitle>
              </CardHeader>
              <CardContent className="text-xs">
                Holding{" "}
                <Link
                  className="underline"
                  href={`/executions/${detail.data.active_lease.execution_id}`}
                >
                  {detail.data.active_lease.execution_id}
                </Link>{" "}
                until {formatTimestamp(detail.data.active_lease.expires_at)}.
              </CardContent>
            </Card>
          ) : null}

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Labels and capabilities</CardTitle>
            </CardHeader>
            <CardContent className="flex flex-col gap-3 text-xs">
              <div>
                <p className="mb-1 text-muted-foreground">Labels</p>
                <div className="flex flex-wrap gap-1">
                  {Object.keys(record.labels ?? {}).length === 0 ? (
                    <span className="text-muted-foreground">none</span>
                  ) : (
                    Object.entries(record.labels ?? {}).map(([key, value]) => (
                      <Badge key={key} variant="secondary" className="text-[10px]">
                        {key}={String(value)}
                      </Badge>
                    ))
                  )}
                </div>
              </div>
              <div>
                <p className="mb-1 text-muted-foreground">Capabilities</p>
                <pre className="max-h-40 overflow-auto rounded-md bg-muted/40 p-2 font-mono text-[11px]">
                  {JSON.stringify(record.capabilities, null, 2)}
                </pre>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Executions on this worker</CardTitle>
            </CardHeader>
            <CardContent>
              {held.state === "loading" ? (
                <p className="py-3 text-center text-xs text-muted-foreground">
                  Loading executions…
                </p>
              ) : held.rows.length === 0 ? (
                <EmptyState
                  title="No executions assigned"
                  description="This worker is not currently holding any executions."
                />
              ) : (
                <ul className="flex flex-col gap-1">
                  {held.rows.map((execution) => (
                    <li key={execution.id}>
                      <Link
                        href={`/executions/${execution.id}`}
                        className="flex flex-wrap items-center gap-2 rounded px-2 py-1.5 text-xs hover:bg-accent/50"
                      >
                        <StatusBadge status={execution.status} />
                        <code className="text-[11px] text-muted-foreground">
                          {execution.id}
                        </code>
                        <span className="ml-auto text-muted-foreground">
                          {formatRelative(execution.created_at)}
                        </span>
                      </Link>
                    </li>
                  ))}
                </ul>
              )}
            </CardContent>
          </Card>
        </div>
      ) : (
        <EmptyState
          title="Worker not found"
          description="It may have been removed, or it belongs to another tenant."
        />
      )}
    </AsyncBoundary>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-border p-3">
      <p className="text-[11px] text-muted-foreground">{label}</p>
      <p className="text-sm font-medium">{value}</p>
    </div>
  );
}
