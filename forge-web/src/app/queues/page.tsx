"use client";

/**
 * Queue list (spec 7.13, UI.md section 27).
 *
 * Pause and resume are gated on `queues:write`. A paused queue stops receiving
 * work without cancelling what it already holds, which is the distinction the
 * confirmation states.
 */

import { useState } from "react";
import { Loader2, Pause, Play, Plus, RefreshCw } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { useList } from "@/lib/useQuery";
import { useToast } from "@/lib/useToast";
import { formatRelative, roleCan, type Queue } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { TableBody, TableHeader, TableRow } from "@/components/ui/table";
import {
  DataTable,
  DataTableCell,
  DataTableHead,
  Meter,
  NumCell,
  PageHeader,
  RowLink,
  TableFooter,
  TableSkeleton,
} from "@/components/page";
import { EmptyState, ErrorState, ForbiddenState } from "@/components/states";

export default function QueuesPage() {
  const { session } = useAuth();
  const query = useList<Queue>("/queues");
  const rows = query.rows;
  const canWrite = roleCan(session?.role, "queues:write");

  // The busiest queue is the denominator, so the bar compares like with like.
  const maxDepth = Math.max(...rows.map((q) => q.depth ?? 0), 1);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Queues"
        description="A paused queue stops receiving work without cancelling what it holds."
        actions={
          <>
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
            {canWrite ? (
              <CreateQueueDialog onCreated={query.reload} />
            ) : null}
          </>
        }
      />

      <div className="min-h-0 flex-1 overflow-auto">
        {query.state === "loading" ? (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead>Queue</DataTableHead>
                <DataTableHead className="w-24">State</DataTableHead>
                <DataTableHead className="w-40">Depth</DataTableHead>
                <NumCell className="w-20">Waiting</NumCell>
                <NumCell className="w-20">Running</NumCell>
                <NumCell className="w-28">Oldest wait</NumCell>
                {canWrite ? (
                  <DataTableHead className="w-24" align="right">
                    Actions
                  </DataTableHead>
                ) : null}
              </TableRow>
            </TableHeader>
            <TableSkeleton rows={6} columns={canWrite ? 7 : 6} />
          </DataTable>
        ) : query.state === "error" ? (
          query.forbidden ? (
            <ForbiddenState />
          ) : (
            <ErrorState error={query.error} onRetry={query.reload} />
          )
        ) : rows.length === 0 ? (
          <EmptyState
            title="No queues yet"
            description="A queue bounds how much work runs at once. Queues group executions so a slow integration cannot starve a critical one."
            action={
              canWrite ? (
                <CreateQueueDialog
                  onCreated={query.reload}
                  label="Create a queue"
                />
              ) : null
            }
          />
        ) : (
          <DataTable>
            <TableHeader>
              <TableRow>
                <DataTableHead>Queue</DataTableHead>
                <DataTableHead className="w-24">State</DataTableHead>
                <DataTableHead className="w-40">Depth</DataTableHead>
                <NumCell className="w-20">Waiting</NumCell>
                <NumCell className="w-20">Running</NumCell>
                <NumCell className="w-28">Oldest wait</NumCell>
                {canWrite ? (
                  <DataTableHead className="w-24" align="right">
                    Actions
                  </DataTableHead>
                ) : null}
              </TableRow>
            </TableHeader>

            <TableBody>
              {rows.map((queue) => {
                const depth = queue.depth ?? 0;
                const paused = queue.paused ?? false;
                return (
                  <TableRow key={queue.id} className="h-9">
                    <DataTableCell className="font-medium">
                      <RowLink href={`/queues?queue=${queue.id}`}>
                        {queue.name}
                      </RowLink>
                    </DataTableCell>

                    <DataTableCell>
                      {paused ? (
                        <span className="text-[12.5px] text-warning-foreground">
                          Paused
                        </span>
                      ) : (
                        <span className="text-[12.5px] text-muted-foreground">
                          Accepting
                        </span>
                      )}
                    </DataTableCell>

                    <DataTableCell>
                      {/*
                        Depth relative to the busiest queue. With no historical
                        series stored, current depth is the only truthful thing to
                        show; a trend line would be invented.
                      */}
                      <Meter
                        value={depth}
                        max={maxDepth}
                        label={`${queue.name} depth`}
                        tone={paused ? "neutral" : depth > 0 ? "warning" : "neutral"}
                      />
                    </DataTableCell>

                    <NumCell className="text-[12.5px]">{depth}</NumCell>
                    <NumCell className="text-[12.5px]">
                      {queue.running ?? 0}
                    </NumCell>

                    {/*
                      A queue with nothing waiting has no oldest item, so the
                      cell is empty rather than claiming a zero-second wait.
                    */}
                    <NumCell className="text-[12px] text-muted-foreground">
                      {queue.oldest_queued_at ? (
                        formatRelative(queue.oldest_queued_at)
                      ) : (
                        "—"
                      )}
                    </NumCell>

                    {canWrite ? (
                      <DataTableCell align="right">
                        <QueueActions queue={queue} onDone={query.reload} />
                      </DataTableCell>
                    ) : null}
                  </TableRow>
                );
              })}
            </TableBody>
          </DataTable>
        )}
      </div>

      {rows.length > 0 ? (
        <p className="border-t border-border px-4 py-2 text-[11.5px] text-muted-foreground">
          Depth is current, not historical: Forge stores no depth series, so no
          trend is charted.
        </p>
      ) : null}

      <TableFooter shown={rows.length} />
    </div>
  );
}

function QueueActions({ queue, onDone }: { queue: Queue; onDone: () => void }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const paused = queue.paused ?? false;

  async function toggle() {
    setBusy(true);
    try {
      await api.post(`/queues/${queue.id}/${paused ? "resume" : "pause"}`);
      toast.success(paused ? `${queue.name} resumed` : `${queue.name} paused`);
      onDone();
    } catch (cause) {
      toast.error(
        `Could not ${paused ? "resume" : "pause"} ${queue.name}`,
        cause instanceof ApiError ? cause.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Button
      variant="ghost"
      size="sm"
      className="h-7 text-[12.5px]"
      disabled={busy}
      onClick={toggle}
      aria-label={`${paused ? "Resume" : "Pause"} ${queue.name}`}
    >
      {busy ? (
        <Loader2 className="animate-spin" aria-hidden />
      ) : paused ? (
        <Play aria-hidden />
      ) : (
        <Pause aria-hidden />
      )}
      {paused ? "Resume" : "Pause"}
    </Button>
  );
}

function CreateQueueDialog({
  onCreated,
  label = "New queue",
}: {
  onCreated: () => void;
  label?: string;
}) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [maxConcurrency, setMaxConcurrency] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  // A limit must be a positive whole number; an empty field means "unlimited",
  // which the API accepts as an absent value.
  const limitProblem =
    maxConcurrency.trim() !== "" &&
    (!/^\d+$/.test(maxConcurrency.trim()) || Number(maxConcurrency) < 1)
      ? "Enter a whole number of 1 or more, or leave it empty for no limit."
      : null;
  const nameProblem = name.trim().length === 0 ? "Give the queue a name." : null;
  const formProblem = nameProblem ?? limitProblem;

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    if (formProblem) return;
    setBusy(true);
    try {
      await api.post("/queues", {
        name: name.trim(),
        max_concurrency: maxConcurrency.trim()
          ? Number(maxConcurrency.trim())
          : undefined,
      });
      setOpen(false);
      setName("");
      setMaxConcurrency("");
      onCreated();
    } catch (cause) {
      setError(
        cause instanceof ApiError
          ? (cause.fieldError("name") ?? cause.message)
          : "Could not reach the server.",
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger
        render={
          <Button size="sm">
            <Plus aria-hidden />
            {label}
          </Button>
        }
      />
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create a queue</DialogTitle>
          <DialogDescription>
            A queue bounds how much work runs at once, so one slow integration
            cannot starve a critical one.
          </DialogDescription>
        </DialogHeader>

        <form onSubmit={submit} className="flex flex-col gap-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor="queue-name">Name</Label>
            <Input
              id="queue-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              required
              autoFocus
              aria-invalid={nameProblem && name.length > 0 ? true : undefined}
            />
            {nameProblem && name.length > 0 ? (
              <p className="text-xs text-danger-foreground">{nameProblem}</p>
            ) : null}
          </div>

          <div className="flex flex-col gap-1.5">
            <Label htmlFor="queue-limit">Max concurrency</Label>
            <Input
              id="queue-limit"
              type="number"
              min={1}
              value={maxConcurrency}
              onChange={(e) => setMaxConcurrency(e.target.value)}
              placeholder="No limit"
              aria-invalid={limitProblem ? true : undefined}
            />
            {limitProblem ? (
              <p className="text-xs text-danger-foreground">{limitProblem}</p>
            ) : (
              <p className="text-xs text-muted-foreground">
                How many executions may run in this queue at once.
              </p>
            )}
          </div>

          {error ? (
            <p role="alert" className="text-xs text-danger-foreground">
              {error}
            </p>
          ) : null}

          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setOpen(false)}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={busy || formProblem !== null}>
              {busy ? "Creating…" : "Create queue"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}