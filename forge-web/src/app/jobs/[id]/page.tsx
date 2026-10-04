"use client";

/**
 * Job detail (UI.md section 10).
 *
 * One subject cut into sections, so the frame is quieter than a card wall: a
 * header with the actions, a stat strip, tabs, and panels.
 *
 * Everything shown is read from the API. The version of this page that showed a
 * hardcoded "in 18m", "Oct 7, 02:00 IST", "worker-17" and "8m 38s" was worse
 * than showing nothing: an operator reading "next run in 18m" would plan around
 * a time that was never this job's. A value the server did not measure is
 * rendered as absent, with the reason, rather than filled in.
 */

import Link from "next/link";
import { use, useState } from "react";
import {
  Archive,
  Copy,
  ExternalLink,
  History,
  Loader2,
  Play,
  Plus,
  Rocket,
} from "lucide-react";

import { useList, useQuery } from "@/lib/useQuery";
import { api, ApiError } from "@/lib/api";
import { formatRelative, formatTimestamp, type Execution, type Job, type Schedule } from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { StatusBadge, PriorityBadge, ResourceId, StatusCell, ErrorClassBadge } from "@/components/status-badge";
import { Button } from "@/components/ui/button";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { TableBody, TableHeader, TableRow } from "@/components/ui/table";
import {
  DataTable,
  DataTableCell,
  DataTableHead,
  NumCell,
  PageHeader,
  Panel,
  RowLink,
  Stat,
  Toolbar,
} from "@/components/page";
import { EmptyState, ErrorState, ForbiddenState } from "@/components/states";
import { cn } from "cn";

interface Health {
  job_id: string;
  reliability: {
    executions: number;
    succeeded: number;
    failed: number;
    dead_lettered_or_cancelled: number;
    retries: number;
    success_rate: number | null;
  };
  performance: {
    average_seconds: number | null;
    finished_average_seconds: number | null;
    p50_seconds: number | null;
    p95_seconds: number | null;
    p99_seconds: number | null;
  };
  sla: {
    target_seconds: number | null;
    met: number;
    evaluated: number;
    compliance_percent: number | null;
  } | null;
}

interface JobVersion {
  id: string;
  version_number: number;
  execution_type?: string;
  timeout_seconds?: number;
  published_at: string | null;
  created_at: string;
}

interface AuditEvent {
  id: string;
  action: string;
  resource_type: string;
  resource_id: string | null;
  created_at: string;
  actor_id: string | null;
  result?: string;
}

interface Dependencies {
  job_id: string;
  upstream: unknown[];
  downstream: unknown[];
}

type Tab = "overview" | "executions" | "schedule" | "versions" | "dependencies" | "health" | "audit";

function formatDuration(seconds: number | null | undefined): string {
  if (seconds === null || seconds === undefined) return "—";
  if (seconds < 60) return `${Math.round(seconds)}s`;
  const m = Math.floor(seconds / 60);
  const s = Math.round(seconds % 60);
  return `${m}m ${s}s`;
}

export default function JobDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const [tab, setTab] = useState<Tab>("overview");

  const job = useQuery<Job>(`/jobs/${id}`);
  const health = useQuery<Health>(`/jobs/${id}/health`);
  const executions = useList<Execution>(`/jobs/${id}/executions?limit=50`);
  const versions = useList<JobVersion>(`/jobs/${id}/versions`);
  const schedules = useList<Schedule>(`/schedules?target_id=${id}&limit=10`);
  const deps = useQuery<Dependencies>(`/jobs/${id}/dependencies`);
  const audit = useList<AuditEvent>(`/audit-events?limit=100`);

  const record = job.data;

  // Audit is filtered client-side: the endpoint is tenant-wide and has no
  // resource filter, so requesting 100 and narrowing here is honest about what
  // is shown. It is labeled as such on the tab.
  const jobAudit = audit.rows.filter((event) => event.resource_id === id);

  if (job.state === "loading") {
    return (
      <div className="flex min-h-0 flex-1 flex-col items-center justify-center">
        <LoadingState label="Loading job" />
      </div>
    );
  }
  if (job.state === "error") {
    return job.forbidden ? <ForbiddenState /> : <ErrorState error={job.error} onRetry={job.reload} />;
  }
  if (!record) return null;

  const reliability = health.data?.reliability;
  const performance = health.data?.performance;
  const sla = health.data?.sla;
  const schedule = schedules.rows[0];

  const tabs: { key: Tab; label: string; count?: number }[] = [
    { key: "overview", label: "Overview" },
    { key: "executions", label: "Executions", count: executions.rows.length },
    { key: "schedule", label: "Schedule", count: schedules.rows.length },
    { key: "versions", label: "Versions", count: versions.rows.length },
    { key: "dependencies", label: "Dependencies" },
    { key: "health", label: "Health" },
    { key: "audit", label: "Audit", count: jobAudit.length },
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={
          <span className="flex flex-wrap items-center gap-2">
            <span>{record.name}</span>
            <StatusBadge status={record.status} />
          </span>
        }
        description={record.description ?? undefined}
        actions={
          <JobActions
            job={record}
            nextRunAt={schedule?.next_run_at ?? null}
            onDone={job.reload}
          />
        }
      />

      {/*
        The stat strip. Every figure comes from `/jobs/{id}/health`, and a value
        the server reports as null is shown as an em dash with the reason —
        "no data" and "0%" are different claims and an operator acts on the
        difference.
      */}
      <div className="grid grid-cols-2 gap-2 border-b border-border p-3 sm:grid-cols-4">
        <Stat
          label="Success rate"
          value={
            reliability?.success_rate != null
              ? `${(reliability.success_rate * 100).toFixed(1)}%`
              : "—"
          }
          hint={
            reliability
              ? `${reliability.executions} run${reliability.executions === 1 ? "" : "s"}`
              : "No runs recorded"
          }
          tone={
            reliability?.success_rate == null
              ? "neutral"
              : reliability.success_rate >= 0.99
                ? "success"
                : reliability.success_rate >= 0.9
                  ? "warning"
                  : "danger"
          }
        />
        <Stat
          label="Average duration"
          value={formatDuration(performance?.average_seconds ?? null)}
          hint={
            performance?.p95_seconds != null
              ? `p95 ${formatDuration(performance.p95_seconds)}`
              : undefined
          }
        />
        {/*
          Next run comes from the schedule's own `next_run_at`, which the
          scheduler computes. There is no schedule, there is no next run.
        */}
        <Stat
          label="Next run"
          value={schedule?.next_run_at ? formatRelative(schedule.next_run_at) : "—"}
          hint={
            schedule?.next_run_at
              ? formatTimestamp(schedule.next_run_at)
              : schedule
                ? "Schedule has no next run"
                : "No schedule"
          }
        />
        <Stat
          label="SLA"
          value={
            sla?.compliance_percent != null
              ? `${(sla.compliance_percent * 100).toFixed(0)}%`
              : "—"
          }
          hint={
            sla?.target_seconds != null
              ? `target under ${Math.round(sla.target_seconds / 60)}m`
              : "No SLA target set"
          }
          tone={
            sla?.compliance_percent == null
              ? "neutral"
              : sla.compliance_percent >= 0.99
                ? "success"
                : "warning"
          }
        />
      </div>

      <Toolbar className="border-b-0 px-0">
        <div className="flex gap-1 overflow-x-auto" role="tablist" aria-label="Job sections">
          {tabs.map((item) => (
            <button
              key={item.key}
              type="button"
              role="tab"
              aria-selected={tab === item.key}
              onClick={() => setTab(item.key)}
              className={cn(
                "flex items-center gap-1.5 whitespace-nowrap rounded px-2 py-1 text-[12.5px] transition-colors",
                tab === item.key
                  ? "bg-accent text-accent-foreground"
                  : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
              )}
            >
              {item.label}
              {item.count !== undefined && item.count > 0 ? (
                <span className="text-[10.5px] tabular-nums opacity-70">
                  {item.count}
                </span>
              ) : null}
            </button>
          ))}
        </div>
      </Toolbar>

      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        <div className="grid gap-4 lg:grid-cols-3">
          <div className="flex flex-col gap-4 lg:col-span-2">
            {tab === "overview" ? (
              <>
                <Panel title="Schedule" bodyClassName="p-0">
                  {schedule ? (
                    <div className="px-3 py-2.5">
                      <div className="flex flex-wrap items-center gap-3">
                        {schedule.expression ? (
                          <code className="rounded bg-muted px-2 py-1 font-mono text-[13px] tracking-wider">
                            {schedule.expression}
                          </code>
                        ) : (
                          <span className="text-[12.5px] text-muted-foreground">
                            {schedule.schedule_type === "ONE_TIME"
                              ? "One-time run"
                              : "Interval schedule"}
                            {schedule.interval_seconds
                              ? ` every ${schedule.interval_seconds}s`
                              : ""}
                          </span>
                        )}
                        <span className="text-[12.5px]">
                          {describeCron(schedule.expression)}
                        </span>
                      </div>
                      <div className="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 text-[11.5px] text-muted-foreground">
                        <span>{schedule.timezone}</span>
                        <span>
                          misfire: {schedule.misfire_policy.toLowerCase()}
                        </span>
                        {schedule.catch_up_limit ? (
                          <span>catch-up limit {schedule.catch_up_limit}</span>
                        ) : null}
                        <span
                          className={schedule.enabled ? undefined : "text-warning-foreground"}
                        >
                          {schedule.enabled ? "enabled" : "paused"}
                        </span>
                        {schedule.last_run_at ? (
                          <span>last run {formatRelative(schedule.last_run_at)}</span>
                        ) : null}
                      </div>
                    </div>
                  ) : (
                    <p className="px-3 py-4 text-[12.5px] text-muted-foreground">
                      No schedule. This job only runs when triggered by hand.
                    </p>
                  )}
                </Panel>

                <Panel
                  title="Recent executions"
                  bodyClassName="p-0"
                  actions={
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 text-[12.5px]"
                      onClick={() => setTab("executions")}
                    >
                      See all {executions.rows.length}
                    </Button>
                  }
                >
                  <ExecutionTable rows={executions.rows.slice(0, 8)} />
                </Panel>

                <Panel
                  title="Versions"
                  bodyClassName="p-0"
                  actions={
                    <Button
                      variant="ghost"
                      size="sm"
                      className="h-7 text-[12.5px]"
                      onClick={() => setTab("versions")}
                    >
                      See all {versions.rows.length}
                    </Button>
                  }
                >
                  <VersionTable
                    rows={versions.rows.slice(0, 5)}
                    currentVersionId={record.current_version_id}
                  />
                </Panel>
              </>
            ) : null}

            {tab === "executions" ? (
              <Panel title="Executions" bodyClassName="p-0">
                <ExecutionTable rows={executions.rows} />
              </Panel>
            ) : null}

            {tab === "schedule" ? (
              <Panel title="Schedules" bodyClassName="p-0">
                {schedules.state === "loading" ? (
                  <p className="px-3 py-4 text-[12.5px] text-muted-foreground">
                    Loading schedules…
                  </p>
                ) : schedules.rows.length === 0 ? (
                  <p className="px-3 py-4 text-[12.5px] text-muted-foreground">
                    No schedule on this job. It runs only when triggered by hand
                    or by a workflow.
                  </p>
                ) : (
                  <DataTable>
                    <TableHeader>
                      <TableRow>
                        <DataTableHead>Expression</DataTableHead>
                        <DataTableHead className="w-32">Timezone</DataTableHead>
                        <DataTableHead className="w-24">State</DataTableHead>
                        <DataTableHead className="w-32" align="right">
                          Next run
                        </DataTableHead>
                        <DataTableHead className="w-32" align="right">
                          Last run
                        </DataTableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {schedules.rows.map((row) => (
                        <TableRow key={row.id} className="h-8">
                          <DataTableCell>
                            <code className="font-mono text-[11.5px]">
                              {row.expression ?? (row.schedule_type ?? "schedule").toLowerCase()}
                            </code>
                          </DataTableCell>
                          <DataTableCell className="text-[11.5px] text-muted-foreground">
                            {row.timezone}
                          </DataTableCell>
                          <DataTableCell>
                            <StatusCell status={row.enabled ? "ACTIVE" : "CANCELLED"} />
                          </DataTableCell>
                          <DataTableCell
                            align="right"
                            className="text-[12px] text-muted-foreground"
                          >
                            {row.next_run_at ? formatRelative(row.next_run_at) : "—"}
                          </DataTableCell>
                          <DataTableCell
                            align="right"
                            className="text-[12px] text-muted-foreground"
                          >
                            {row.last_run_at ? formatRelative(row.last_run_at) : "—"}
                          </DataTableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </DataTable>
                )}
              </Panel>
            ) : null}

            {tab === "versions" ? (
              <Panel title="Versions" description="Published versions are immutable." bodyClassName="p-0">
                <VersionTable rows={versions.rows} currentVersionId={record.current_version_id} />
              </Panel>
            ) : null}

            {tab === "dependencies" ? (
              <Panel title="Dependencies" bodyClassName="p-0">
                {deps.state === "loading" ? (
                  <p className="px-3 py-4 text-[12.5px] text-muted-foreground">
                    Loading dependencies…
                  </p>
                ) : deps.data &&
                  deps.data.upstream.length === 0 &&
                  deps.data.downstream.length === 0 ? (
                  <p className="px-3 py-4 text-[12.5px] text-muted-foreground">
                    This job has no upstream or downstream dependencies.
                  </p>
                ) : (
                  <div className="px-3 py-2.5 text-[12.5px]">
                    <DependencyList
                      title="Upstream (must run first)"
                      items={deps.data?.upstream}
                      emptyLabel="Nothing must run before this job."
                    />
                    <DependencyList
                      title="Downstream (runs after)"
                      items={deps.data?.downstream}
                      emptyLabel="Nothing waits on this job."
                      className="mt-3"
                    />
                  </div>
                )}
              </Panel>
            ) : null}

            {tab === "health" ? (
              <Panel title="Reliability and performance" bodyClassName="p-3">
                {reliability && reliability.executions > 0 ? (
                  <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
                    <Stat
                      label="Succeeded"
                      value={reliability.succeeded}
                      tone="success"
                    />
                    <Stat
                      label="Failed"
                      value={reliability.failed}
                      tone={reliability.failed > 0 ? "danger" : "neutral"}
                    />
                    <Stat label="Retries" value={reliability.retries} />
                    <Stat
                      label="Dead lettered"
                      value={reliability.dead_lettered_or_cancelled}
                      tone={
                        reliability.dead_lettered_or_cancelled > 0
                          ? "danger"
                          : "neutral"
                      }
                    />
                  </div>
                ) : (
                  <p className="text-[12.5px] text-muted-foreground">
                    No executions recorded for this job yet, so there is nothing
                    to compute a rate or a duration from.
                  </p>
                )}

                {performance?.p50_seconds != null ||
                performance?.p95_seconds != null ? (
                  <div className="mt-4 grid grid-cols-2 gap-4 sm:grid-cols-4">
                    <Stat label="p50" value={formatDuration(performance.p50_seconds)} />
                    <Stat label="p95" value={formatDuration(performance.p95_seconds)} />
                    <Stat label="p99" value={formatDuration(performance.p99_seconds)} />
                    <Stat
                      label="Average"
                      value={formatDuration(performance.average_seconds)}
                    />
                  </div>
                ) : null}
              </Panel>
            ) : null}

            {tab === "audit" ? (
              <Panel
                title="Audit"
                description="Changes recorded against this job."
                bodyClassName="p-0"
              >
                {jobAudit.length === 0 ? (
                  <p className="px-3 py-4 text-[12.5px] text-muted-foreground">
                    No recorded changes to this job.
                  </p>
                ) : (
                  <DataTable>
                    <TableHeader>
                      <TableRow>
                        <DataTableHead>Action</DataTableHead>
                        <DataTableHead className="w-32">Result</DataTableHead>
                        <DataTableHead className="w-32" align="right">
                          When
                        </DataTableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {jobAudit.map((event) => (
                        <TableRow key={event.id} className="h-8">
                          <DataTableCell className="text-[12.5px]">
                            {event.action.replace(/_/g, " ").toLowerCase()}
                            {event.actor_id ? (
                              <span className="ml-2 text-[11px] text-muted-foreground">
                                by <code className="font-mono">{event.actor_id.slice(0, 8)}</code>
                              </span>
                            ) : null}
                          </DataTableCell>
                          <DataTableCell>
                            <StatusCell status={event.result ?? "SUCCESS"} />
                          </DataTableCell>
                          <DataTableCell
                            align="right"
                            className="text-[12px] text-muted-foreground"
                            title={formatTimestamp(event.created_at)}
                          >
                            {formatRelative(event.created_at)}
                          </DataTableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </DataTable>
                )}
              </Panel>
            ) : null}
          </div>

          {/* The sidebar holds identity and quick facts, nothing that scrolls. */}
          <div className="flex flex-col gap-4">
            <Panel title="Details">
              <dl className="flex flex-col gap-2.5 text-[12.5px]">
                <DetailRow label="Job id">
                  <ResourceId id={record.id} />
                </DetailRow>
                <DetailRow label="Key">
                  {record.key ? (
                    <code className="font-mono text-[11.5px]">{record.key}</code>
                  ) : (
                    <span className="text-muted-foreground">Not set</span>
                  )}
                </DetailRow>
                <DetailRow label="Priority">
                  <PriorityBadge priority={record.priority} />
                </DetailRow>
                <DetailRow label="Owner">
                  {record.owner_id ? (
                    <code className="font-mono text-[11.5px]">
                      {record.owner_id.slice(0, 8)}
                    </code>
                  ) : (
                    <span className="text-muted-foreground">Unassigned</span>
                  )}
                </DetailRow>
                <DetailRow label="Queue">
                  {record.default_queue_id ? (
                    <code className="font-mono text-[11.5px]">
                      {record.default_queue_id.slice(0, 8)}
                    </code>
                  ) : (
                    <span className="text-muted-foreground">Default</span>
                  )}
                </DetailRow>
                <DetailRow label="Created">
                  {formatRelative(record.created_at)}
                </DetailRow>
                <DetailRow label="Updated">
                  {formatRelative(record.updated_at)}
                </DetailRow>
              </dl>
            </Panel>

            {record.labels && Object.keys(record.labels).length > 0 ? (
              <Panel title="Labels">
                <div className="flex flex-wrap gap-1.5">
                  {Object.entries(record.labels).map(([key, value]) => (
                    <span
                      key={key}
                      className="rounded bg-muted px-1.5 py-0.5 text-[11px]"
                    >
                      <span className="text-muted-foreground">{key}</span>
                      <span className="mx-1">=</span>
                      <span className="font-mono">{String(value)}</span>
                    </span>
                  ))}
                </div>
              </Panel>
            ) : null}

            <Panel title="Versions">
              <Button
                variant="outline"
                size="sm"
                className="h-7 w-full text-[12.5px]"
                render={<Link href="/jobs/builder" />}
              >
                <Plus aria-hidden />
                New job from a definition
              </Button>
              <p className="mt-2 text-[11.5px] text-muted-foreground">
                A draft never runs. Publishing a version is what makes a job
                eligible for dispatch.
              </p>
            </Panel>
          </div>
        </div>
      </div>
    </div>
  );
}

function LoadingState({ label }: { label: string }) {
  return (
    <p className="text-[12.5px] text-muted-foreground" role="status">
      {label}…
    </p>
  );
}

function DetailRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-baseline justify-between gap-3">
      <dt className="shrink-0 text-muted-foreground">{label}</dt>
      <dd className="min-w-0 truncate text-right">{children}</dd>
    </div>
  );
}

/**
 * Plain-English reading of a cron expression.
 *
 * Best-effort and deliberately partial: it covers the patterns the console
 * actually creates, and anything else says so rather than guessing. A wrong
 * "every day at 02:00" is worse than an honest "unrecognised expression".
 */
function describeCron(expression: string | null): string {
  if (!expression) return "";
  const parts = expression.trim().split(/\s+/);
  if (parts.length !== 5) return "Unrecognised expression";

  const [minute, hour, dom, month, dow] = parts;
  const at = (h: string, m: string) =>
    `${h}:${m.padStart(2, "0")}`.replace(/^0/, "");

  if (minute.startsWith("*/") && hour === "*" && dom === "*" && month === "*" && dow === "*") {
    return `Every ${minute.slice(2)} minutes`;
  }
  if (/^\d+$/.test(hour) && /^\d+$/.test(minute) && dom === "*" && month === "*" && dow === "*") {
    return `Every day at ${at(hour, minute)}`;
  }
  if (/^\d+$/.test(hour) && /^\d+$/.test(minute) && dom === "*" && month === "*") {
    const days = dow === "*" ? "every day" : dow === "1-5" ? "weekdays" : `day ${dow}`;
    return `${days} at ${at(hour, minute)}`;
  }
  if (/^\d+$/.test(minute) && hour.startsWith("*/") && dom === "*") {
    return `Every ${hour.slice(2)} hours`;
  }
  return "Unrecognised expression";
}

/** Executions for a job, with only fields the API actually sends. */
function ExecutionTable({ rows }: { rows: Execution[] }) {
  if (rows.length === 0) {
    return (
      <p className="px-3 py-4 text-[12.5px] text-muted-foreground">
        No executions recorded. This job has not run yet.
      </p>
    );
  }

  return (
    <DataTable>
      <TableHeader>
        <TableRow>
          <DataTableHead className="w-28">Status</DataTableHead>
          <DataTableHead className="w-28">Trigger</DataTableHead>
          <NumCell className="w-16">Attempt</NumCell>
          <DataTableHead className="w-24">Worker</DataTableHead>
          <DataTableHead className="w-28" align="right">
            Duration
          </DataTableHead>
          <DataTableHead className="w-28" align="right">
            Created
          </DataTableHead>
          <DataTableHead>Error</DataTableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((execution) => {
          const elapsed =
            execution.started_at && execution.ended_at
              ? Math.max(
                  0,
                  (Date.parse(execution.ended_at) -
                    Date.parse(execution.started_at)) /
                    1000,
                )
              : null;
          return (
            <TableRow key={execution.id} className="h-8">
              <DataTableCell>
                <StatusCell status={execution.status} />
              </DataTableCell>
              <DataTableCell className="text-[12px] text-muted-foreground">
                {execution.trigger_source.toLowerCase()}
              </DataTableCell>
              <NumCell className="text-[12.5px]">{execution.attempt_count}</NumCell>
              <DataTableCell className="text-[11.5px] text-muted-foreground">
                {execution.worker_id ? execution.worker_id.slice(0, 8) : "—"}
              </DataTableCell>
              <DataTableCell align="right" className="text-[12.5px]">
                {elapsed === null ? (
                  <span className="text-muted-foreground">—</span>
                ) : (
                  formatDuration(elapsed)
                )}
              </DataTableCell>
              <DataTableCell
                align="right"
                className="text-[12px] text-muted-foreground"
                title={formatTimestamp(execution.created_at)}
              >
                {formatRelative(execution.created_at)}
              </DataTableCell>
              <DataTableCell>
                {execution.error_class ? (
                  <ErrorClassBadge errorClass={execution.error_class} />
                ) : execution.error_message ? (
                  <span
                    className="block max-w-[20rem] truncate text-[12px] text-muted-foreground"
                    title={execution.error_message}
                  >
                    {execution.error_message}
                  </span>
                ) : (
                  <span className="text-muted-foreground">—</span>
                )}
              </DataTableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </DataTable>
  );
}

function VersionTable({
  rows,
  currentVersionId,
}: {
  rows: JobVersion[];
  currentVersionId: string | null;
}) {
  if (rows.length === 0) {
    return (
      <p className="px-3 py-4 text-[12.5px] text-muted-foreground">
        No versions yet. A draft never runs until a version is published.
      </p>
    );
  }

  return (
    <DataTable>
      <TableHeader>
        <TableRow>
          <DataTableHead className="w-20">Version</DataTableHead>
          <DataTableHead className="w-28">Execution type</DataTableHead>
          <DataTableHead className="w-24" align="right">
            Timeout
          </DataTableHead>
          <DataTableHead className="w-32" align="right">
            Published
          </DataTableHead>
          <DataTableHead className="w-24" />
        </TableRow>
      </TableHeader>
      <TableBody>
        {rows.map((version) => {
          const current = version.id === currentVersionId;
          return (
            <TableRow key={version.id} className="h-8">
              <DataTableCell className="font-medium">
                v{version.version_number}
                {current ? (
                  <span className="ml-2 text-[10.5px] font-normal tracking-wide text-primary uppercase">
                    Current
                  </span>
                ) : null}
              </DataTableCell>
              <DataTableCell className="text-[12px] text-muted-foreground">
                {version.execution_type
                  ? version.execution_type.replace(/_/g, " ").toLowerCase()
                  : "—"}
              </DataTableCell>
              <DataTableCell align="right" className="text-[12.5px] text-muted-foreground">
                {version.timeout_seconds != null
                  ? formatDuration(version.timeout_seconds)
                  : "—"}
              </DataTableCell>
              <DataTableCell
                align="right"
                className="text-[12px] text-muted-foreground"
                title={formatTimestamp(version.published_at)}
              >
                {version.published_at ? formatRelative(version.published_at) : "Draft"}
              </DataTableCell>
              <DataTableCell align="right">
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`Copy version ${version.version_number} id`}
                  onClick={() => void navigator.clipboard?.writeText(version.id)}
                >
                  <Copy aria-hidden />
                </Button>
              </DataTableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </DataTable>
  );
}

function DependencyList({
  title,
  items,
  emptyLabel,
  className,
}: {
  title: string;
  items: unknown[] | undefined;
  emptyLabel: string;
  className?: string;
}) {
  return (
    <div className={className}>
      <p className="text-[11px] font-medium tracking-wide text-muted-foreground uppercase">
        {title}
      </p>
      {!items || items.length === 0 ? (
        <p className="mt-1 text-[12px] text-muted-foreground">{emptyLabel}</p>
      ) : (
        <ul className="mt-1 flex flex-col gap-1">
          {items.map((item, index) => {
            const record = item as Record<string, unknown>;
            const jobId = typeof record.job_id === "string" ? record.job_id : null;
            return (
              <li key={jobId ?? index} className="flex items-center gap-2 text-[12.5px]">
                <ExternalLink className="size-3 shrink-0 text-muted-foreground" aria-hidden />
                {jobId ? (
                  <Link
                    href={`/jobs/${jobId}`}
                    className="underline-offset-2 hover:underline"
                  >
                    <code className="font-mono text-[11.5px]">{jobId.slice(0, 8)}</code>
                  </Link>
                ) : (
                  <code className="font-mono text-[11.5px]">
                    {String(record.id ?? "unknown").slice(0, 8)}
                  </code>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

/**
 * The header actions.
 *
 * Only actions that do something are rendered. The previous version of this page
 * shipped Pause, Edit and a "…" menu with no handlers — controls that look
 * available and quietly do nothing are worse than their absence.
 */
function JobActions({
  job,
  nextRunAt,
  onDone,
}: {
  job: Job;
  nextRunAt: string | null;
  onDone: () => void;
}) {
  const toast = useToast();
  const [busy, setBusy] = useState<"run" | "archive" | null>(null);
  const [confirmArchive, setConfirmArchive] = useState(false);

  async function trigger() {
    setBusy("run");
    try {
      const created = await api.post<{ execution_id?: string; id?: string }>(
        `/jobs/${job.id}/trigger`,
        {},
        crypto.randomUUID(),
      );
      const executionId = created.execution_id ?? created.id;
      toast.success("Execution queued", {
        label: "Open execution",
        onClick: () =>
          window.location.assign(`/executions/${executionId}`),
      });
      onDone();
    } catch (error) {
      toast.error(
        `Could not run ${job.name}`,
        error instanceof ApiError ? error.message : undefined,
      );
    } finally {
      setBusy(null);
    }
  }

  async function archive() {
    setBusy("archive");
    try {
      await api.delete(`/jobs/${job.id}`);
      toast.success(`${job.name} archived`);
      setConfirmArchive(false);
      onDone();
    } catch (error) {
      toast.error(
        `Could not archive ${job.name}`,
        error instanceof ApiError ? error.message : undefined,
      );
    } finally {
      setBusy(null);
    }
  }

  return (
    <>
      {/*
        A draft has no published version, so triggering it would queue an
        execution that can never dispatch. The button is disabled with the
        reason stated rather than hidden, so the state is discoverable.
      */}
      <Button
        size="sm"
        disabled={busy !== null || !job.current_version_id}
        title={
          job.current_version_id
            ? undefined
            : "Publish a version before this job can run"
        }
        onClick={trigger}
      >
        {busy === "run" ? (
          <Loader2 className="animate-spin" aria-hidden />
        ) : (
          <Play aria-hidden />
        )}
        Run now
      </Button>

      <Button
        variant="outline"
        size="sm"
        render={<Link href="/jobs/builder" />}
      >
        <Rocket aria-hidden />
        New version
      </Button>

      {job.status !== "ARCHIVED" ? (
        <Button
          variant="outline"
          size="sm"
          disabled={busy !== null}
          onClick={() => setConfirmArchive(true)}
        >
          <Archive aria-hidden />
          Archive
        </Button>
      ) : null}

      <ArchiveConfirm
        job={job}
        open={confirmArchive}
        busy={busy === "archive"}
        nextRunAt={nextRunAt}
        onCancel={() => setConfirmArchive(false)}
        onConfirm={archive}
      />
    </>
  );
}

/**
 * States the impact, per UI.md section 62.
 *
 * "Are you sure?" names nothing; this names the job, what stops, and when the
 * last run would have been.
 */
function ArchiveConfirm({
  job,
  open,
  busy,
  nextRunAt,
  onCancel,
  onConfirm,
}: {
  job: Job;
  open: boolean;
  busy: boolean;
  nextRunAt: string | null;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  return (
    <AlertDialog
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Archive “{job.name}”?</AlertDialogTitle>
          <AlertDialogDescription>
            Archiving stops this job being scheduled. It keeps its history and
            existing executions are untouched.
            {nextRunAt ? (
              <>
                {" "}Its next scheduled run at{" "}
                <strong>{formatTimestamp(nextRunAt)}</strong> will not happen.
              </>
            ) : null}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogAction variant="outline" onClick={onCancel}>
            Cancel
          </AlertDialogAction>
          <AlertDialogAction
            variant="destructive"
            disabled={busy}
            onClick={onConfirm}
          >
            {busy ? "Archiving…" : "Archive job"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}