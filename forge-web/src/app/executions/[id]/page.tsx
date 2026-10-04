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
import { use, useMemo, useState } from "react";
import {
  ArrowLeft,
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
  holdsSlot,
  isTerminal,
  type Execution,
  type ExecutionStatus,
} from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { StatusBadge, PriorityBadge } from "@/components/status-badge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ExecutionMetrics } from "@/components/ui/execution-metrics";
import { LiveExecution } from "@/components/ui/live-execution";
import { cn } from "cn";
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

  const durationMs = useMemo(() => {
    if (!record?.started_at) return null;
    const end = record.ended_at ? Date.parse(record.ended_at) : Date.now();
    return Math.max(0, end - Date.parse(record.started_at));
  }, [record?.started_at, record?.ended_at]);

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

          {record.error_message ? (
            <Card className="border-red-500/40">
              <CardHeader>
                <CardTitle className="text-sm text-red-600 dark:text-red-400">
                  Failure
                </CardTitle>
              </CardHeader>
              <CardContent className="text-xs">
                {record.error_class ? (
                  <Badge variant="outline" className="mr-2 text-[10px]">
                    {record.error_class}
                  </Badge>
                ) : null}
                <span className="font-mono">{record.error_message}</span>
              </CardContent>
            </Card>
          ) : null}

          {/* UI.md section 16: log viewer with copy and download. */}
          <Card>
            <CardHeader className="flex-row items-center justify-between">
              <CardTitle className="text-sm">Logs</CardTitle>
              <div className="flex items-center gap-2">
                <span className="text-[11px] text-muted-foreground">
                  {lines.length} line{lines.length === 1 ? "" : "s"}
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() =>
                    copyText(
                      lines
                        .map((l) => `[${l.at}] ${l.stream.toUpperCase()} ${l.content}`)
                        .join("\n"),
                    )
                  }
                >
                  Copy
                </Button>
              </div>
            </CardHeader>
            <CardContent>
              {logs.state === "loading" ? (
                <p className="py-4 text-center text-xs text-muted-foreground">
                  Loading logs…
                </p>
              ) : lines.length === 0 ? (
                <EmptyState
                  title="No log lines"
                  description="This execution has not written any output yet."
                />
              ) : (
                <pre
                  className={cn(
                    "max-h-96 overflow-auto rounded-md bg-muted/40 p-3",
                    "font-mono text-[11px] leading-relaxed",
                  )}
                >
                  {lines.map((line, index) => (
                    <div key={index} className="flex gap-2">
                      <span className="shrink-0 text-muted-foreground">
                        {formatTimestamp(line.at)}
                      </span>
                      <span
                        className={cn(
                          "shrink-0 uppercase",
                          line.stream === "stderr"
                            ? "text-red-600 dark:text-red-400"
                            : "text-muted-foreground",
                        )}
                      >
                        {line.stream}
                      </span>
                      <span className="min-w-0 break-words">{line.content}</span>
                    </div>
                  ))}
                </pre>
              )}
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

function Field({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div className="flex items-baseline gap-2">
      <span className="w-28 shrink-0 text-muted-foreground">{label}</span>
      <span className="min-w-0 break-all font-mono">{value}</span>
    </div>
  );
}
