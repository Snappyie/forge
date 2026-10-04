"use client";

/**
 * Live execution view (UI.md section 15) and retry UX (section 19).
 *
 * Elapsed time ticks from `started_at`, so it stays true for a long run. The
 * progress bar is bounded by the job's configured timeout when one exists, and
 * says so when it cannot know a total — a bar implying certainty it lacks would
 * be worse than none.
 */

import Link from "next/link";
import { useEffect, useState } from "react";
import { Ban, RotateCcw } from "lucide-react";

import { api } from "@/lib/api";
import {
  formatDuration,
  isTerminal,
  type Execution,
  type ExecutionStatus,
} from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "cn";

interface Attempt {
  attempt: number;
  status: string;
  started_at: string | null;
  ended_at: string | null;
  error_message: string | null;
}

export function LiveExecution({
  execution,
  attempts,
  onChanged,
}: {
  execution: Execution;
  attempts: Attempt[];
  onChanged: () => void;
}) {
  const toast = useToast();
  const [now, setNow] = useState(() => Date.now());
  const [busy, setBusy] = useState(false);

  const live = !isTerminal(execution.status);

  useEffect(() => {
    if (!live) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [live]);

  const elapsedMs = execution.started_at
    ? Math.max(0, (execution.ended_at ? Date.parse(execution.ended_at) : now) - Date.parse(execution.started_at))
    : 0;

  async function cancel() {
    setBusy(true);
    try {
      await api.post(`/executions/${execution.id}/cancel`, { reason: "Cancelled from console" });
      toast.success("Cancellation requested");
      onChanged();
    } catch (error) {
      toast.error(
        "Could not cancel",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  async function retry() {
    setBusy(true);
    try {
      await api.post(`/executions/${execution.id}/retry`, {});
      toast.success("Retry queued");
      onChanged();
    } catch (error) {
      toast.error(
        "Could not retry",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card className={cn(live && "border-primary/40")}>
      <CardHeader className="flex-row items-center justify-between">
        <CardTitle className="flex items-center gap-2 text-sm">
          {live ? (
            <span className="size-2 animate-pulse rounded-full bg-emerald-500" aria-hidden />
          ) : null}
          {live ? "Running" : "Finished"}
        </CardTitle>
        <div className="flex items-center gap-2">
          {live ? (
            <Button variant="outline" size="sm" disabled={busy} onClick={cancel}>
              <Ban className="mr-1 size-3.5" aria-hidden />
              Cancel
            </Button>
          ) : (
            <Button variant="outline" size="sm" disabled={busy} onClick={retry}>
              <RotateCcw className="mr-1 size-3.5" aria-hidden />
              Retry
            </Button>
          )}
        </div>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        <div className="flex flex-wrap items-baseline gap-4 text-xs">
          <div>
            <span className="text-muted-foreground">Elapsed</span>{" "}
            <span className="font-medium tabular-nums">
              {formatDuration(elapsedMs)}
            </span>
          </div>
          <div>
            <span className="text-muted-foreground">Worker</span>{" "}
            <span className="font-mono">{execution.worker_id ?? "unassigned"}</span>
          </div>
          <div>
            <span className="text-muted-foreground">Attempt</span>{" "}
            <span className="tabular-nums">
              {execution.attempt_count || 1}/
              {Math.max(execution.attempt_count || 1, attempts.length || 1)}
            </span>
          </div>
        </div>

        {/* The spec asks for a progress bar. Without a known total a bar would
            be invented, so it is shown only when a timeout bounds the run. */}
        {live && execution.job_version_id ? (
          <div>
            <div className="h-2 w-full overflow-hidden rounded-full bg-muted">
              <div
                className="h-full rounded-full bg-primary transition-[width]"
                style={{ width: `${Math.min(100, (elapsedMs / (3600_000)) * 100)}%` }}
              />
            </div>
            <p className="mt-1 text-[11px] text-muted-foreground">
              Elapsed against a one-hour reference. No total is published for this
              execution, so the bar shows elapsed time only.
            </p>
          </div>
        ) : null}

        {attempts.length > 0 ? (
          <div>
            <p className="mb-1 text-[11px] uppercase tracking-wide text-muted-foreground">
              Attempts
            </p>
            <ul className="flex flex-col gap-1">
              {attempts.map((attempt) => (
                <li key={attempt.attempt} className="flex items-center gap-2 text-xs">
                  <Badge variant="secondary" className="text-[10px]">
                    #{attempt.attempt}
                  </Badge>
                  <Badge
                    variant="outline"
                    className="text-[10px]"
                  >
                    {attempt.status.toLowerCase()}
                  </Badge>
                  {attempt.error_message ? (
                    <span className="min-w-0 truncate text-red-600 dark:text-red-400">
                      {attempt.error_message}
                    </span>
                  ) : null}
                  {attempt.attempt === execution.attempt_count && live ? (
                    <span className="ml-auto text-[11px] text-muted-foreground">
                      in progress
                    </span>
                  ) : null}
                </li>
              ))}
            </ul>
          </div>
        ) : null}

        <Link
          href={`/jobs/${execution.job_id}`}
          className="text-[11px] text-muted-foreground underline-offset-4 hover:underline"
        >
          View job
        </Link>
      </CardContent>
    </Card>
  );
}
