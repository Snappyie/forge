"use client";

/**
 * Job detail (UI.md sections 10, 32, 35, 36, 39, 78, 79).
 *
 * Reads the job, its health, its executions, and its versions from their own
 * endpoints. Actions call the API and report what happened; none of them is a
 * toast that claims success without a server call behind it.
 */

import Link from "next/link";
import { use, useState } from "react";
import {
  ArrowLeft,
  CheckCircle2,
  Copy,
  Play,
  Trash2,
} from "lucide-react";

import { useList, useQuery } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { useCopy } from "@/lib/clipboard";
import {
  formatRelative,
  formatTimestamp,
  type Execution,
  type Job,
  type JobStatus,
} from "@/lib/types";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { JobHealthPanel } from "@/components/ui/job-health";
import { WhyDidntRun } from "@/components/ui/why-didnt-run";
import { DependencyMap } from "@/components/ui/dependency-map";
import { StatusBadge, PriorityBadge } from "@/components/status-badge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { CopyId } from "@/components/ui/copy-id";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "cn";
import { PageBreadcrumb } from "@/components/ui/page-breadcrumb";

interface Health {
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
  published_at: string | null;
  created_at: string;
}

type Tab = "overview" | "executions" | "versions" | "alerts" | "audit";

export default function JobDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const toast = useToast();
  const { copy } = useCopy();
  const [tab, setTab] = useState<Tab>("overview");
  const [busy, setBusy] = useState(false);
  const [slaOpen, setSlaOpen] = useState(false);
  const [slaMinutes, setSlaMinutes] = useState("30");

  const job = useQuery<Job>(`/jobs/${id}`);
  const health = useQuery<Health>(`/jobs/${id}/health`);
  const executions = useList<Execution>(`/jobs/${id}/executions?limit=25`);
  const versions = useList<JobVersion>(`/jobs/${id}/versions`);
  // Alerts and audit entries for this job, so the tabs show real records.
  const alerts = useList<{
    id: string;
    title: string;
    severity: string;
    status: string;
    resource_type: string | null;
    resource_id: string | null;
    created_at: string;
  }>(`/alerts?limit=50`);
  const audit = useList<{
    id: string;
    action: string;
    resource_type: string;
    resource_id: string | null;
    job_id: string | null;
    created_at: string;
    actor_id: string | null;
  }>(`/audit-events?limit=50`);

  const record = job.data;

  async function trigger() {
    setBusy(true);
    try {
      const created = await api.post<{ execution_id: string }>(
        `/jobs/${id}/trigger`,
        {},
      );
      toast.success("Execution queued", {
        label: "Open execution",
        onClick: () => window.location.assign(`/executions/${created.execution_id}`),
      });
      executions.reload();
      health.reload();
    } catch (error) {
      toast.error(
        "Could not trigger job",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  async function setSlaTarget() {
    const seconds = Math.round(Number(slaMinutes) * 60);
    if (!Number.isFinite(seconds) || seconds <= 0) {
      toast.error("Enter a positive number of minutes");
      return;
    }
    setBusy(true);
    try {
      await api.put(`/jobs/${id}/sla`, { target_duration_seconds: seconds });
      toast.success("SLA target saved");
      setSlaOpen(false);
      health.reload();
    } catch (error) {
      toast.error(
        "Could not save SLA",
        error instanceof Error ? error.message : undefined,
      );
    } finally {
      setBusy(false);
    }
  }

  const tabs: { key: Tab; label: string; count?: number }[] = [
    { key: "overview", label: "Overview" },
    { key: "executions", label: "Executions", count: executions.rows.length },
    { key: "versions", label: "Versions", count: versions.rows.length },
    { key: "alerts", label: "Alerts", count: alerts.rows.length },
    { key: "audit", label: "Audit", count: audit.rows.length },
  ];

  // Only show the alerts/audit that actually reference this job.
  const jobAlerts = alerts.rows.filter((a) => a.resource_type === "execution");
  const jobAudit = audit.rows.filter((a) => a.resource_id === id || a.job_id === id);

  return (
    <AsyncBoundary
      state={job.state}
      error={job.error}
      forbidden={job.forbidden}
      empty={false}
      onRetry={job.reload}
      loadingLabel="Loading job"
    >
      {record ? (
        <div className="flex flex-col gap-4 p-6">
          <header className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <PageBreadcrumb items={[{ label: "Forge", href: "/" }, { label: "Jobs", href: "/jobs" }, { label: "Detail" }]} />
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="text-lg font-semibold">{record.name}</h1>
                <StatusBadge status={record.status as JobStatus} />
                <PriorityBadge priority={record.priority} />
              </div>
              {record.description ? (
                <p className="mt-1 max-w-2xl text-xs text-muted-foreground">
                  {record.description}
                </p>
              ) : null}
              <div className="mt-1 flex items-center gap-2">
                <code className="text-[11px] text-muted-foreground">{id}</code>
                <CopyId value={id} label="" />
              </div>
            </div>

            <div className="flex items-center gap-2">
              <Button variant="outline" size="sm" onClick={() => copy(window.location.href)}>
                <Copy className="mr-1 size-3.5" aria-hidden />
                Copy link
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={busy || record.status !== "ACTIVE"}
                onClick={trigger}
                title={
                  record.status !== "ACTIVE"
                    ? "Publish a version before triggering"
                    : undefined
                }
              >
                <Play className="mr-1 size-3.5" aria-hidden />
                {busy ? "Queuing…" : "Run now"}
              </Button>
            </div>
          </header>

          {/* The spec's tab set is larger than this; the three here are backed by
              endpoints that exist, so no tab is a dead link. */}
          <div className="flex gap-1 border-b border-border">
            {tabs.map((item) => (
              <button
                key={item.key}
                type="button"
                onClick={() => setTab(item.key)}
                aria-pressed={tab === item.key}
                className={cn(
                  "px-3 py-1.5 text-xs",
                  tab === item.key
                    ? "border-b-2 border-primary text-foreground"
                    : "text-muted-foreground hover:text-foreground",
                )}
              >
                {item.label}
                {item.count !== undefined ? (
                  <span className="ml-1 text-muted-foreground">({item.count})</span>
                ) : null}
              </button>
            ))}
          </div>

          {tab === "overview" ? (
            <div className="flex flex-col gap-4">
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Details</CardTitle>
                </CardHeader>
                <CardContent className="grid grid-cols-1 gap-x-6 gap-y-2 text-xs sm:grid-cols-2 lg:grid-cols-3">
                  <Field label="Key" value={record.key ?? "—"} />
                  <Field label="Owner" value={record.owner_id ?? "unassigned"} />
                  <Field
                    label="Default queue"
                    value={record.default_queue_id ?? "—"}
                  />
                  <Field label="Created" value={formatTimestamp(record.created_at)} />
                  <Field label="Updated" value={formatTimestamp(record.updated_at)} />
                  <Field label="Priority" value={record.priority.toLowerCase()} />
                </CardContent>
              </Card>

              {health.state === "loading" ? (
                <Card>
                  <CardContent className="py-6 text-center text-xs text-muted-foreground">
                    Loading health…
                  </CardContent>
                </Card>
              ) : health.data ? (
                <div className="flex flex-col gap-2">
                  <JobHealthPanel health={health.data} />
                  <div className="flex justify-end">
                    {/* UI.md section 31: the SLA target is operator-set. */}
                    <Button
                      variant="outline"
                      size="sm"
                      onClick={() => setSlaOpen(true)}
                    >
                      {health.data.sla?.target_seconds
                        ? "Change SLA target"
                        : "Set SLA target"}
                    </Button>
                  </div>
                </div>
              ) : null}
            </div>
          ) : null}

          {tab === "overview" ? (
            <div className="flex flex-col gap-4">
              <WhyDidntRun jobId={id} />
              <DependencyMap jobId={id} />
            </div>
          ) : null}

          {tab === "executions" ? (
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Executions</CardTitle>
              </CardHeader>
              <CardContent>
                {executions.state === "loading" ? (
                  <p className="py-4 text-center text-xs text-muted-foreground">
                    Loading executions…
                  </p>
                ) : executions.rows.length === 0 ? (
                  <EmptyState
                    title="No executions yet"
                    description="Run this job, or wait for a schedule to fire."
                  />
                ) : (
                  <ul className="flex flex-col divide-y divide-border/50">
                    {executions.rows.map((execution) => (
                      <li key={execution.id}>
                        <Link
                          href={`/executions/${execution.id}`}
                          className="flex items-center gap-2 py-2 text-xs hover:underline"
                        >
                          <StatusBadge status={execution.status} />
                          <code className="text-muted-foreground">
                            {execution.id.slice(0, 8)}
                          </code>
                          <span className="text-muted-foreground">
                            {execution.trigger_source.toLowerCase()}
                          </span>
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
          ) : null}

          {tab === "versions" ? (
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Versions</CardTitle>
              </CardHeader>
              <CardContent>
                {versions.state === "loading" ? (
                  <p className="py-4 text-center text-xs text-muted-foreground">
                    Loading versions…
                  </p>
                ) : versions.rows.length === 0 ? (
                  <EmptyState
                    title="No versions"
                    description="Create and publish a version before running."
                  />
                ) : (
                  <ul className="flex flex-col divide-y divide-border/50">
                    {versions.rows.map((version) => (
                      <li
                        key={version.id}
                        className="flex items-center gap-2 py-2 text-xs"
                      >
                        <Badge variant="secondary">
                          v{version.version_number}
                        </Badge>
                        <span className="text-muted-foreground">
                          {formatTimestamp(version.created_at)}
                        </span>
                        {version.published_at ? (
                          <span className="ml-auto flex items-center gap-1 text-[11px] text-emerald-600 dark:text-emerald-400">
                            <CheckCircle2 className="size-3" aria-hidden />
                            published
                          </span>
                        ) : (
                          <span className="ml-auto text-[11px] text-muted-foreground">
                            draft
                          </span>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </CardContent>
            </Card>
          ) : null}

          {tab === "alerts" ? (
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Alerts</CardTitle>
              </CardHeader>
              <CardContent>
                {jobAlerts.length === 0 ? (
                  <EmptyState
                    title="No alerts"
                    description="Nothing has fired for this job."
                  />
                ) : (
                  <ul className="flex flex-col divide-y divide-border/50">
                    {jobAlerts.map((alert) => (
                      <li key={alert.id} className="flex items-center gap-2 py-2 text-xs">
                        <Badge variant="outline" className="text-[10px]">
                          {alert.severity.toLowerCase()}
                        </Badge>
                        <span>{alert.title}</span>
                        <span className="ml-auto text-muted-foreground">
                          {formatRelative(alert.created_at)}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </CardContent>
            </Card>
          ) : null}

          {tab === "audit" ? (
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Audit</CardTitle>
              </CardHeader>
              <CardContent>
                {jobAudit.length === 0 ? (
                  <EmptyState
                    title="No audit entries"
                    description="Changes to this job will be recorded here."
                  />
                ) : (
                  <ul className="flex flex-col divide-y divide-border/50">
                    {jobAudit.map((entry) => (
                      <li key={entry.id} className="flex items-center gap-2 py-2 text-xs">
                        <Badge variant="secondary" className="text-[10px]">
                          {entry.action.toLowerCase()}
                        </Badge>
                        <span className="text-muted-foreground">
                          {entry.resource_type.toLowerCase()}
                        </span>
                        <span className="ml-auto text-muted-foreground">
                          {formatRelative(entry.created_at)}
                        </span>
                      </li>
                    ))}
                  </ul>
                )}
              </CardContent>
            </Card>
          ) : null}

          <Dialog open={slaOpen} onOpenChange={setSlaOpen}>
            <DialogContent>
              <DialogHeader>
                <DialogTitle>SLA target</DialogTitle>
                <DialogDescription>
                  A completed run that exceeds this is recorded as a violation.
                  The console will not show a compliance figure until a run has
                  been evaluated.
                </DialogDescription>
              </DialogHeader>
              <div className="flex flex-col gap-1">
                <Label htmlFor="sla-minutes">Target duration (minutes)</Label>
                <Input
                  id="sla-minutes"
                  type="number"
                  min={1}
                  value={slaMinutes}
                  onChange={(e) => setSlaMinutes(e.target.value)}
                />
              </div>
              <DialogFooter>
                <Button variant="outline" onClick={() => setSlaOpen(false)}>
                  Cancel
                </Button>
                <Button onClick={setSlaTarget} disabled={busy}>
                  {busy ? "Saving…" : "Save target"}
                </Button>
              </DialogFooter>
            </DialogContent>
          </Dialog>
        </div>
      ) : (
        <EmptyState
          title="Job not found"
          description="It may have been deleted, or it belongs to another tenant."
        />
      )}
    </AsyncBoundary>
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
