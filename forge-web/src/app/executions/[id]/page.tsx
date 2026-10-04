"use client";

/**
 * Execution detail (UI.md sections 10, 13, 14, 16, 17, 19, 68).
 *
 * Everything on this page is read from the execution, its logs and its attempt
 * history. There is no demo mode here on purpose: an execution page that shows
 * an invented timeline is worse than no page, because an operator would act on
 * it.
 */

import Link from "next/link";
import { use, useEffect, useMemo, useState } from "react";
import {
  AlertCircle,
  Ban,
  Copy,
  Download,
  RotateCcw,
  Share2,
} from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { copyText, downloadText } from "@/lib/clipboard";
import {
  formatDuration,
  formatRelative,
  formatTimestamp,
  isTerminal,
  type Execution,
  type ExecutionStatus,
} from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { StatusBadge, PriorityBadge, ErrorClassBadge } from "@/components/status-badge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ExecutionMetrics } from "@/components/ui/execution-metrics";
import { LiveExecution } from "@/components/ui/live-execution";
import { LogViewer } from "@/components/ui/log-viewer";
import { PageBreadcrumb } from "@/components/ui/page-breadcrumb";

interface LogLine {
  stream: string;
  content: string;
  at: string;
}

interface LogResponse {
  execution_id: string;
  lines: LogLine[];
}

interface Attempt {
  attempt: number;
  status: string;
  started_at: string | null;
  ended_at: string | null;
  error_message: string | null;
}

export default function ExecutionDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  const execution = useQuery<Execution>(`/executions/${id}`);
  const logs = useQuery<LogResponse>(`/executions/${id}/logs`);
  const attempts = useQuery<{ data: Attempt[] }>(`/executions/${id}/attempts`);
  const timeline = useQuery<{ stages: { label: string; at_display: string | null }[] }>(
    `/executions/${id}/timeline`,
  );

  const record = execution.data;
  const lines = logs.data?.lines ?? [];

  // Hoisted out of the memo so the dependency list names plain locals. Reading
  // `record?.started_at` in the deps made the inferred dependencies differ
  // from the declared ones, which the compiler rejects: optional chaining hides
  // which property is actually tracked.
  const startedAt = record?.started_at ?? null;
  const endedAt = record?.ended_at ?? null;

  const [now, setNow] = useState<number | null>(null);
  useEffect(() => {
    setNow(Date.now());
    if (!endedAt) {
      const interval = setInterval(() => setNow(Date.now()), 1000);
      return () => clearInterval(interval);
    }
  }, [endedAt]);

  const durationMs = useMemo(() => {
    if (!startedAt) return null;
    const end = endedAt ? Date.parse(endedAt) : (now ?? Date.parse(startedAt));
    return Math.max(0, end - Date.parse(startedAt));
  }, [startedAt, endedAt, now]);

  async function cancel() {
    setBusy(true);
    try {
      await api.post(`/executions/${id}/cancel`, { reason: "Cancelled from console" });
      toast.success("Cancellation requested", {
        label: "Back to executions",
        onClick: () => window.location.assign("/executions"),
      });
      execution.reload();
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
      const created = await api.post<{ id: string }>(`/executions/${id}/retry`, {});
      toast.success("Retry queued", {
        label: "Open new execution",
        onClick: () => window.location.assign(`/executions/${created.id}`),
      });
    } catch (error) {
      toast.error(
        "Could not retry",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  async function share() {
    const url = window.location.href;
    // Prefer the native share sheet on mobile; fall back to a clipboard copy.
    if (navigator.share) {
      try {
        await navigator.share({ title: `Execution ${id}`, url });
        return;
      } catch {
        // The user dismissed the sheet; fall through to copying.
      }
    }
    const ok = await copyText(url);
    if (ok) toast.success("Link copied");
    else toast.error("Could not copy link");
  }

  function exportLogs() {
    const body = lines
      .map((line) => `[${line.at}] ${line.stream.toUpperCase()} ${line.content}`)
      .join("\n");
    downloadText(`execution-${id}.log`, body || "No log lines recorded.");
    toast.success("Logs downloaded");
  }

  const loading = execution.state === "loading";
  const error = execution.error;

  return (
    <AsyncBoundary
      state={execution.state}
      error={error}
      forbidden={execution.forbidden}
      empty={false}
      onRetry={execution.reload}
      loadingLabel="Loading execution"
    >
      {record ? (
        <div className="flex flex-col gap-4 p-6">
          <header className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <PageBreadcrumb items={[{ label: "Forge", href: "/" }, { label: "Executions", href: "/executions" }, { label: "Detail" }]} />
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="text-lg font-semibold">Execution</h1>
                <StatusBadge status={record.status} />
                <PriorityBadge priority={record.priority} />
              </div>
              <div className="mt-1 flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                <code>{id}</code>
                <button
                  type="button"
                  onClick={() => copyText(id)}
                  aria-label="Copy execution ID"
                  className="rounded p-0.5 hover:bg-accent"
                >
                  <Copy className="size-3" aria-hidden />
                </button>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <Button variant="outline" size="sm" onClick={share}>
                <Share2 className="mr-1 size-3.5" aria-hidden />
                Share
              </Button>
              <Button variant="outline" size="sm" onClick={exportLogs}>
                <Download className="mr-1 size-3.5" aria-hidden />
                Download logs
              </Button>
              {isTerminal(record.status) ? (
                <Button variant="outline" size="sm" disabled={busy} onClick={retry}>
                  <RotateCcw className="mr-1 size-3.5" aria-hidden />
                  Retry
                </Button>
              ) : (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={busy || record.status === "CANCELLED"}
                  onClick={cancel}
                >
                  <Ban className="mr-1 size-3.5" aria-hidden />
                  Cancel
                </Button>
              )}
            </div>
          </header>

          {/* UI.md section 17: concrete metrics, not a vague score. */}
          <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
            <Metric label="Status" value={record.status.toLowerCase()} />
            <Metric
              label="Duration"
              value={
                record.started_at
                  ? formatDuration(durationMs) +
                    (record.ended_at ? "" : " (running)")
                  : "not started"
              }
            />
            <Metric label="Attempts" value={String(record.attempt_count)} />
            <Metric
              label="Trigger"
              value={record.trigger_source.toLowerCase()}
            />
          </div>

          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Details</CardTitle>
            </CardHeader>
            <CardContent className="grid grid-cols-1 gap-x-6 gap-y-2 text-xs sm:grid-cols-2 lg:grid-cols-3">
              <Field label="Job" value={<Link className="underline" href={`/jobs/${record.job_id}`}>{record.job_id}</Link>} />
              <Field label="Queue" value={record.queue_id ?? "—"} />
              <Field label="Worker" value={record.worker_id ?? "unassigned"} />
              <Field label="Created" value={formatTimestamp(record.created_at)} />
              <Field label="Started" value={formatTimestamp(record.started_at)} />
              <Field label="Ended" value={formatTimestamp(record.ended_at)} />
              <Field label="Scheduled for" value={formatTimestamp(record.scheduled_for)} />
              <Field label="Correlation" value={record.correlation_id ?? "—"} />
              {record.workflow_id ? (
                <Field
                  label="Workflow"
                  value={<Link className="underline" href={`/workflows/${record.workflow_id}`}>{record.workflow_id}</Link>}
                />
              ) : null}
            </CardContent>
          </Card>

          {record.error_message || record.error_class ? (
            <div
              role="alert"
              className="flex items-start gap-2 rounded-lg border border-red-500/40 bg-red-500/10 p-3"
            >
              <AlertCircle
                className="mt-0.5 size-4 shrink-0 text-red-600 dark:text-red-400"
                aria-hidden
              />
              <div className="min-w-0 flex-1 text-xs">
                <p className="font-medium text-red-700 dark:text-red-300">
                  {headline(record)}
                </p>
                {record.error_message ? (
                  <p className="mt-1 break-words font-mono text-[11.5px] text-red-700/90 dark:text-red-300/90">
                    {record.error_message}
                  </p>
                ) : null}
                <div className="mt-2 flex flex-wrap items-center gap-2">
                  <ErrorClassBadge errorClass={record.error_class} />
                  {record.attempt_count > 0 ? (
                    <span className="text-[11px] text-red-700/80 dark:text-red-300/80">
                      attempt {record.attempt_count} recorded
                    </span>
                  ) : null}
                </div>
              </div>
              {isTerminal(record.status) && !busy ? (
                <Button variant="outline" size="sm" onClick={retry}>
                  Retry now
                </Button>
              ) : null}
            </div>
          ) : null}

          {/* UI.md section 16: log viewer with search, stream filtering, and
              line numbers. */}
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Logs</CardTitle>
            </CardHeader>
            <CardContent>
              <LogViewer
                lines={lines}
                loading={logs.state === "loading"}
                executionId={id}
                onDownloaded={() => toast.success("Logs downloaded")}
              />
            </CardContent>
          </Card>

          {attempts.data?.data?.length ? (
            <LiveExecution
              execution={record}
              attempts={attempts.data.data}
              onChanged={() => {
                execution.reload();
                attempts.reload();
              }}
            />
          ) : null}

          <ExecutionMetrics executionId={id} />

          {timeline.data ? (
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Timeline</CardTitle>
              </CardHeader>
              <CardContent>
                <ol className="flex flex-col gap-2 border-l border-border pl-4">
                  {timeline.data.stages.map((stage) => (
                    <li key={stage.label} className="relative text-xs">
                      <span
                        className="absolute -left-[1.3rem] top-1 size-2 rounded-full bg-border"
                        aria-hidden
                      />
                      <span className="font-medium">{stage.label}</span>
                      <span className="ml-2 text-muted-foreground">
                        {stage.at_display ?? "—"}
                      </span>
                    </li>
                  ))}
                </ol>
              </CardContent>
            </Card>
          ) : null}

          {attempts.data?.data?.length ? (
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Attempt history</CardTitle>
              </CardHeader>
              <CardContent>
                <ul className="flex flex-col gap-2">
                  {attempts.data.data.map((attempt) => (
                    <li
                      key={attempt.attempt}
                      className="flex flex-wrap items-center gap-3 text-xs"
                    >
                      <Badge variant="secondary">#{attempt.attempt}</Badge>
                      <StatusBadge status={attempt.status as ExecutionStatus} />
                      <span className="text-muted-foreground">
                        {formatRelative(attempt.started_at)}
                      </span>
                      {attempt.error_message ? (
                        <span className="font-mono text-red-600 dark:text-red-400">
                          {attempt.error_message}
                        </span>
                      ) : null}
                    </li>
                  ))}
                </ul>
              </CardContent>
            </Card>
          ) : null}
        </div>
      ) : (
        <EmptyState
          title="Execution not found"
          description="It may have been deleted, or it belongs to another tenant."
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

/**
 * A one-line summary of how an execution ended.
 *
 * The point is to state the consequence rather than restate the status. "Failed"
 * tells an operator nothing they did not already see in the badge; "the retry
 * budget is spent, so this will not run again on its own" tells them whether
 * they have to intervene.
 */
function headline(record: Execution): string {
  const attempts = record.attempt_count;
  const plural = attempts === 1 ? "attempt" : "attempts";

  switch (record.status) {
    case "DEAD_LETTERED":
      return `Dead lettered after ${attempts} ${plural}. This execution will not run again on its own.`;
    case "TIMED_OUT":
      return `Timed out after ${attempts} ${plural}. The worker exceeded the execution timeout.`;
    case "ABANDONED":
      return `Abandoned after ${attempts} ${plural}. The worker's lease expired, so the attempt was reaped.`;
    case "FAILED":
      return `Failed on attempt ${attempts}. Check the error class below before retrying — not every failure is safe to repeat.`;
    case "CANCELLED":
      return "Cancelled by an operator. The attempt history and logs are retained.";
    case "CANCEL_REQUESTED":
      return "Cancellation requested. The worker observes this cooperatively and will stop when it can.";
    default:
      return `Status: ${record.status.toLowerCase().replace(/_/g, " ")}.`;
  }
}

function Field({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="w-28 shrink-0 text-muted-foreground">{label}</span>
      <span className="min-w-0 break-all font-mono">{value}</span>
    </div>
  );
}
