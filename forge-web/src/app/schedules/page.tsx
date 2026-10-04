"use client";

/**
 * Schedule list with the next-run preview (spec 7.7).
 *
 * Preview calls the same engine the scheduler uses (spec 9.13), and DST
 * anomalies are surfaced rather than left for the user to notice.
 */

import { useState } from "react";
import { AlertTriangle, Eye, Pause, Play, RefreshCw } from "lucide-react";

import { api, ApiError } from "@/lib/api";
import { useAuth } from "@/lib/auth";
import { useList } from "@/lib/useQuery";
import {
  formatRelative,
  formatTimestamp,
  roleCan,
  type Schedule,
  type SchedulePreview,
} from "@/lib/types";
import { AsyncBoundary } from "@/components/states";
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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import Link from "next/link";

export default function SchedulesPage() {
  const { session } = useAuth();
  const query = useList<Schedule>("/schedules?limit=100");
  const [preview, setPreview] = useState<SchedulePreview | null>(null);

  const rows = query.rows;
  const canWrite = roleCan(session?.role, "schedules:write");

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">Schedules</h1>
          <p className="text-xs text-muted-foreground">
            Cron schedules run in their own timezone; preview before you rely on one.
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
          loadingLabel="Loading schedules"
          emptyTitle="No schedules yet"
          emptyDescription="Create one on a job to run it on a calendar."

          emptyAction={
            <Button size="sm" variant="outline" render={<Link href="/jobs" />}>
              Go to jobs
            </Button>
          }
        >
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Target</TableHead>
                <TableHead>Expression</TableHead>
                <TableHead>Timezone</TableHead>
                <TableHead>Misfire</TableHead>
                <TableHead>Next run</TableHead>
                <TableHead>State</TableHead>
                {canWrite ? <TableHead className="text-right">Actions</TableHead> : null}
              </TableRow>
            </TableHeader>
            <TableBody>
              {rows.map((schedule) => (
                <TableRow key={schedule.id}>
                  <TableCell>
                    <code className="font-mono text-xs">
                      {schedule.target_id.slice(0, 8)}
                    </code>
                    <span className="ml-2 text-xs text-muted-foreground">
                      {schedule.target_type.toLowerCase()}
                    </span>
                  </TableCell>
                  <TableCell>
                    <code className="text-xs">{schedule.expression ?? "—"}</code>
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {schedule.timezone}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {schedule.misfire_policy.toLowerCase()}
                  </TableCell>
                  <TableCell
                    className="text-xs text-muted-foreground"
                    title={formatTimestamp(schedule.next_run_at)}
                  >
                    {formatRelative(schedule.next_run_at)}
                  </TableCell>
                  <TableCell>
                    <span
                      className={
                        schedule.enabled
                          ? "text-xs text-emerald-600 dark:text-emerald-400"
                          : "text-xs text-muted-foreground"
                      }
                    >
                      {schedule.enabled ? "enabled" : "paused"}
                    </span>
                  </TableCell>
                  {canWrite ? (
                    <TableCell className="text-right">
                      <ScheduleActions
                        schedule={schedule}
                        onPreview={setPreview}
                        onDone={query.reload}
                      />
                    </TableCell>
                  ) : (
                    <TableCell className="text-right">
                      <Button
                        variant="ghost"
                        size="sm"
                        onClick={() => setPreview(null)}
                        aria-label="Preview"
                      >
                        <Eye className="size-3.5" aria-hidden />
                      </Button>
                    </TableCell>
                  )}
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </AsyncBoundary>
      </div>

      <PreviewDialog schedule={preview} onClose={() => setPreview(null)} />
    </div>
  );
}

function ScheduleActions({
  schedule,
  onPreview,
  onDone,
}: {
  schedule: Schedule;
  onPreview: (preview: SchedulePreview | null) => void;
  onDone: () => void;
}) {
  const [busy, setBusy] = useState(false);

  async function toggle() {
    setBusy(true);
    try {
      await api.post(`/schedules/${schedule.id}/${schedule.enabled ? "pause" : "resume"}`);
      onDone();
    } catch (cause) {
      window.alert(cause instanceof ApiError ? cause.message : "the request failed");
    } finally {
      setBusy(false);
    }
  }

  async function preview() {
    setBusy(true);
    try {
      onPreview(await api.post<SchedulePreview>(`/schedules/${schedule.id}/preview`, { count: 10 }));
    } catch (cause) {
      window.alert(cause instanceof ApiError ? cause.message : "the request failed");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex items-center justify-end gap-1">
      <Button
        variant="ghost"
        size="icon"
        disabled={busy}
        aria-label="Preview occurrences"
        title="Preview the next 10 occurrences"
        onClick={preview}
      >
        <Eye className="size-3.5" aria-hidden />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        disabled={busy}
        aria-label={schedule.enabled ? "Pause schedule" : "Resume schedule"}
        title={schedule.enabled ? "Pause" : "Resume"}
        onClick={toggle}
      >
        {schedule.enabled ? (
          <Pause className="size-3.5" aria-hidden />
        ) : (
          <Play className="size-3.5" aria-hidden />
        )}
      </Button>
    </div>
  );
}

function PreviewDialog({
  schedule,
  onClose,
}: {
  schedule: SchedulePreview | null;
  onClose: () => void;
}) {
  return (
    <Dialog open={schedule !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Upcoming occurrences</DialogTitle>
          <DialogDescription>
            {schedule?.expression} in {schedule?.timezone}
          </DialogDescription>
        </DialogHeader>

        {schedule ? (
          <div className="flex flex-col gap-3">
            {schedule.anomalies.length > 0 ? (
              <div className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-xs">
                <AlertTriangle
                  className="mt-0.5 size-3.5 shrink-0 text-amber-600"
                  aria-hidden
                />
                <div className="flex flex-col gap-1">
                  {schedule.anomalies.map((anomaly) => (
                    <p key={`${anomaly.at}-${anomaly.kind}`}>
                      <span className="font-medium">
                        {anomaly.kind === "AMBIGUOUS_LOCAL_TIME"
                          ? "Repeated local time"
                          : "Skipped local time"}
                      </span>{" "}
                      at {formatTimestamp(anomaly.at)} — {anomaly.note}
                    </p>
                  ))}
                </div>
              </div>
            ) : null}

            <ol className="flex flex-col gap-1 text-xs">
              {schedule.occurrences.map((occurrence, index) => (
                <li
                  key={occurrence}
                  className="flex items-center justify-between gap-2 border-b border-border/50 py-1 last:border-0"
                >
                  <span className="text-muted-foreground">{index + 1}</span>
                  <code>{formatTimestamp(occurrence)}</code>
                </li>
              ))}
            </ol>
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}
