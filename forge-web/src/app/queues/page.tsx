"use client";

/**
 * Queue list (spec 7.13).
 *
 * Pause and resume are gated on `queues:write`.
 */

import { useState } from "react";
import { Loader2, Pause, Play, Plus, RefreshCw } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { useList } from "@/lib/useQuery";
import { formatRelative, roleCan, type Queue } from "@/lib/types";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { AsyncBoundary } from "@/components/states";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

export default function QueuesPage() {
  const { session } = useAuth();
  const query = useList<Queue>("/queues");
  const [creating, setCreating] = useState(false);

  const rows = query.rows;
  const canWrite = roleCan(session?.role, "queues:write");

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Queues</h1>
          <p className="text-xs text-muted-foreground">
            A paused queue stops receiving work without cancelling what it holds.
          </p>
        </div>

        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={query.reload} aria-label="Refresh">
            <RefreshCw className="size-3.5" aria-hidden />
            Refresh
          </Button>
          {canWrite ? (
            <Button size="sm" onClick={() => setCreating((open) => !open)}>
              <Plus className="size-3.5" aria-hidden />
              New queue
            </Button>
          ) : null}
        </div>
      </header>

      {creating && canWrite ? (
        <CreateQueueForm
          onCreated={() => {
            setCreating(false);
            query.reload();
          }}
        />
      ) : null}

      <div className="rounded-lg border border-border">
        {/* UI.md section 27 asks for queue depth and throughput graphs. With no
            historical series stored, the only truthful chart is the current
            depth per queue; a fabricated trend line would be worse than none. */}
        {rows.length > 0 ? (
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Queue depth</CardTitle>
            </CardHeader>
            <CardContent className="flex flex-col gap-3">
              {rows.map((queue) => {
                const depth = queue.depth ?? 0;
                const max = Math.max(...rows.map((q) => q.depth ?? 0), 1);
                const percent = Math.round((depth / max) * 100);
                return (
                  <div key={queue.id}>
                    <div className="mb-1 flex items-baseline justify-between text-xs">
                      <span className="text-muted-foreground">{queue.name}</span>
                      <span className="tabular-nums">{depth} waiting</span>
                    </div>
                    <div
                      className="h-2 overflow-hidden rounded-full bg-muted"
                      role="img"
                      aria-label={`${queue.name}: ${depth} waiting`}
                    >
                      <div
                        className="h-full rounded-full bg-primary"
                        style={{ width: `${percent}%` }}
                      />
                    </div>
                  </div>
                );
              })}
              <p className="text-[11px] text-muted-foreground">
                Current depth. Forge stores no historical depth series, so no
                trend is charted.
              </p>
            </CardContent>
          </Card>
        ) : null}

        <AsyncBoundary
          state={query.state}
          error={query.error}
          forbidden={query.forbidden}
          empty={query.state === "ready" && rows.length === 0}
          onRetry={query.reload}
          loadingLabel="Loading queues"
          emptyTitle="No queues yet"
          emptyDescription="A queue bounds how much work runs at once."
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Name</TableHead>
                <TableHead>Depth</TableHead>
                <TableHead>Running</TableHead>
                <TableHead>Oldest wait</TableHead>
                <TableHead>Max concurrency</TableHead>
                <TableHead>State</TableHead>
                {canWrite ? <TableHead className="text-right">Actions</TableHead> : null}
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((queue) => (
                <TableRow key={queue.id}>
                  <TableCell className="font-medium">{queue.name}</TableCell>
                  <TableCell className="text-xs tabular-nums">
                    {queue.depth ?? 0}
                  </TableCell>
                  <TableCell className="text-xs tabular-nums">
                    {queue.running ?? 0}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {queue.oldest_queued_at
                      ? formatRelative(queue.oldest_queued_at)
                      : "—"}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {queue.max_concurrency ?? "unlimited"}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {queue.paused ? "paused" : "active"}
                  </TableCell>
                  {canWrite ? (
                    <TableCell className="text-right">
                      <QueueActions queue={queue} onDone={query.reload} />
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

function QueueActions({ queue, onDone }: { queue: Queue; onDone: () => void }) {
  const [busy, setBusy] = useState(false);
  const paused = queue.paused ?? false;

  async function toggle() {
    setBusy(true);
    try {
      await api.post(`/queues/${queue.id}/${paused ? "resume" : "pause"}`);
      onDone();
    } catch (cause) {
      window.alert(cause instanceof ApiError ? cause.message : "the request failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <Button
      variant="ghost"
      size="sm"
      disabled={busy}
      onClick={toggle}
      aria-label={`${paused ? "Resume" : "Pause"} ${queue.name}`}
    >
      {busy ? (
        <Loader2 className="size-3.5 animate-spin" aria-hidden />
      ) : paused ? (
        <Play className="size-3.5" aria-hidden />
      ) : (
        <Pause className="size-3.5" aria-hidden />
      )}
      {paused ? "Resume" : "Pause"}
    </Button>
  );
}

function CreateQueueForm({ onCreated }: { onCreated: () => void }) {
  const [name, setName] = useState("");
  const [maxConcurrency, setMaxConcurrency] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setBusy(true);
    try {
      await api.post("/queues", {
        name: name.trim(),
        max_concurrency: maxConcurrency ? Number(maxConcurrency) : undefined,
      });
      setName("");
      setMaxConcurrency("");
      onCreated();
    } catch (cause) {
      setError(
        cause instanceof ApiError
          ? (cause.fieldError("name") ?? cause.message)
          : "could not reach the server",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <form
      onSubmit={submit}
      className="flex flex-wrap items-end gap-3 rounded-lg border border-border p-4"
    >
      <div className="flex min-w-[12rem] flex-col gap-1.5">
        <Label htmlFor="queue-name">Name</Label>
        <Input
          id="queue-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          required
        />
      </div>
      <div className="flex w-40 flex-col gap-1.5">
        <Label htmlFor="queue-limit">Max concurrency</Label>
        <Input
          id="queue-limit"
          type="number"
          min={1}
          value={maxConcurrency}
          onChange={(e) => setMaxConcurrency(e.target.value)}
          placeholder="unlimited"
        />
      </div>
      <Button type="submit" disabled={busy}>
        {busy ? "Creating…" : "Create"}
      </Button>
      {error ? (
        <p role="alert" className="w-full text-xs text-destructive">
          {error}
        </p>
      ) : null}
    </form>
  );
}
