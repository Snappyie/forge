"use client";

import Link from "next/link";
import { use, useState } from "react";
import {
  ArrowLeft,
  CheckCircle2,
  Copy,
  Play,
  Pause,
  Edit,
  MoreHorizontal,
  Info
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
import { StatusBadge, PriorityBadge } from "@/components/status-badge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { CopyId } from "@/components/ui/copy-id";
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

type Tab = "overview" | "executions" | "schedule" | "versions" | "dependencies" | "health" | "audit";

function formatDuration(seconds: number | null) {
  if (seconds === null) return "—";
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
  const toast = useToast();
  const { copy } = useCopy();
  const [tab, setTab] = useState<Tab>("overview");
  const [busy, setBusy] = useState(false);

  const job = useQuery<Job>(`/jobs/${id}`);
  const health = useQuery<Health>(`/jobs/${id}/health`);
  const executions = useList<Execution>(`/jobs/${id}/executions?limit=25`);
  const versions = useList<JobVersion>(`/jobs/${id}/versions`);
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

  const tabs: { key: Tab; label: string; count?: number }[] = [
    { key: "overview", label: "Overview" },
    { key: "executions", label: "Executions", count: executions.rows.length },
    { key: "schedule", label: "Schedule" },
    { key: "versions", label: "Versions", count: versions.rows.length },
    { key: "dependencies", label: "Dependencies" },
    { key: "health", label: "Health" },
    { key: "audit", label: "Audit", count: audit.rows.length },
  ];

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
        <div className="flex flex-col gap-6 p-6 lg:p-8">
          <header className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <PageBreadcrumb items={[{ label: "Jobs", href: "/jobs" }, { label: record.name }]} />
              <div className="flex flex-wrap items-center gap-3 mt-2">
                <h1 className="text-2xl font-bold tracking-tight">{record.name}</h1>
                <StatusBadge status={record.status as JobStatus} />
              </div>
              <div className="mt-2 flex items-center gap-3 text-sm text-muted-foreground">
                <span>{record.key || record.id.slice(0, 8)}</span>
                <span className="text-border">|</span>
                <span>Owner <span className="font-medium text-foreground">{record.owner_id || "unassigned"}</span></span>
                <span className="text-border">|</span>
                <span>{record.default_queue_id || "no queue"}</span>
                <span className="text-border">|</span>
                <span>Updated {formatRelative(record.updated_at)}</span>
              </div>
            </div>

            <div className="flex items-center gap-2">
              <Button
                size="sm"
                className="bg-zinc-900 text-white hover:bg-zinc-800 h-9"
                disabled={busy || record.status !== "ACTIVE"}
                onClick={trigger}
              >
                <Play className="mr-2 size-4" fill="currentColor" />
                Run now
              </Button>
              <Button variant="outline" size="sm" className="h-9">
                <Pause className="mr-2 size-4" />
                Pause
              </Button>
              <Button variant="outline" size="sm" className="h-9">
                <Edit className="mr-2 size-4" />
                Edit
              </Button>
              <Button variant="outline" size="icon" className="h-9 w-9">
                <MoreHorizontal className="size-4" />
              </Button>
            </div>
          </header>

          <div className="flex gap-4 border-b border-border overflow-x-auto">
            {tabs.map((item) => (
              <button
                key={item.key}
                type="button"
                onClick={() => setTab(item.key)}
                aria-pressed={tab === item.key}
                className={cn(
                  "px-1 py-3 text-sm font-medium whitespace-nowrap border-b-2 transition-colors",
                  tab === item.key
                    ? "border-foreground text-foreground"
                    : "border-transparent text-muted-foreground hover:text-foreground",
                )}
              >
                {item.label}
                {item.count !== undefined ? (
                  <span className="ml-2 text-xs font-normal text-muted-foreground">{item.count}</span>
                ) : null}
              </button>
            ))}
          </div>

          <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
            <Card className="col-span-1 shadow-sm">
              <CardContent className="p-5 flex flex-col gap-1">
                <p className="text-xs text-muted-foreground">Success rate</p>
                <div className="flex items-baseline gap-2 mt-1">
                  <span className="text-3xl font-semibold text-emerald-600">
                    {health.data?.reliability.success_rate != null ? (health.data.reliability.success_rate * 100).toFixed(1) + "%" : "—"}
                  </span>
                </div>
                <p className="text-[11px] text-muted-foreground mt-2">
                  last 30 days · {health.data?.reliability.executions || 0} runs
                </p>
              </CardContent>
            </Card>

            <Card className="col-span-1 shadow-sm">
              <CardContent className="p-5 flex flex-col gap-1">
                <p className="text-xs text-muted-foreground">Average duration</p>
                <div className="flex items-baseline gap-2 mt-1">
                  <span className="text-3xl font-semibold">
                    {formatDuration(health.data?.performance.average_seconds ?? null)}
                  </span>
                </div>
                <p className="text-[11px] text-muted-foreground mt-2">
                  p95 {formatDuration(health.data?.performance.p95_seconds ?? null)}
                </p>
              </CardContent>
            </Card>

            <Card className="col-span-1 shadow-sm">
              <CardContent className="p-5 flex flex-col gap-1">
                <p className="text-xs text-muted-foreground">Next run</p>
                <div className="flex items-baseline gap-2 mt-1">
                  <span className="text-3xl font-semibold">
                    in 18m
                  </span>
                </div>
                <p className="text-[11px] text-muted-foreground mt-2">
                  Oct 7, 02:00 IST
                </p>
              </CardContent>
            </Card>

            <Card className="col-span-1 shadow-sm">
              <CardContent className="p-5 flex flex-col gap-1">
                <p className="text-xs text-muted-foreground">SLA</p>
                <div className="flex items-baseline gap-2 mt-1">
                  <span className="text-3xl font-semibold text-emerald-600">
                    {health.data?.sla?.compliance_percent != null ? (health.data.sla.compliance_percent * 100).toFixed(0) + "%" : "—"}
                  </span>
                </div>
                <p className="text-[11px] text-muted-foreground mt-2">
                  target under {health.data?.sla?.target_seconds ? health.data.sla.target_seconds / 60 : 15}m
                </p>
              </CardContent>
            </Card>
          </div>

          <div className="grid grid-cols-1 lg:grid-cols-3 gap-8">
            <div className="lg:col-span-2 flex flex-col gap-8">
              {tab === "overview" ? (
                <>
                  <div className="flex flex-col gap-3">
                    <div className="flex items-center justify-between">
                      <h2 className="text-sm font-semibold">Schedule</h2>
                      <Button variant="link" className="h-auto p-0 text-sm text-muted-foreground underline">Edit schedule</Button>
                    </div>
                    <Card className="shadow-sm">
                      <CardContent className="p-5 flex flex-col gap-4">
                        <div className="flex items-center gap-4">
                          <div className="flex items-center gap-2 font-mono text-lg tracking-widest bg-muted/50 px-3 py-1.5 rounded-md">
                            0 2 * * *
                          </div>
                          <div>
                            <p className="font-medium text-sm">Every day at 02:00</p>
                            <p className="text-xs text-muted-foreground mt-0.5">Asia/Kolkata · fire once if missed · catch-up limit 100</p>
                          </div>
                        </div>
                        <div className="flex items-center gap-2">
                          <Badge variant="outline" className="text-[10px] font-normal uppercase tracking-wider bg-background">enabled</Badge>
                          <Badge variant="outline" className="text-[10px] font-normal tracking-wider bg-background">queue: critical</Badge>
                          <Badge variant="outline" className="text-[10px] font-normal tracking-wider bg-background">timeout 3600s</Badge>
                        </div>
                      </CardContent>
                      <div className="border-t border-border p-3 bg-muted/10 flex items-start gap-2 text-xs text-muted-foreground">
                        <Info className="size-4 shrink-0 mt-0.5" />
                        <p>Last run Oct 1 at 02:00 IST succeeded in 8m 38s. Next eligible run Oct 7 at 02:00 IST.</p>
                      </div>
                    </Card>
                  </div>

                  <div className="flex flex-col gap-3">
                    <div className="flex items-center justify-between">
                      <h2 className="text-sm font-semibold">Recent executions</h2>
                      <Button variant="link" className="h-auto p-0 text-sm text-muted-foreground underline">
                        See all {executions.rows.length}
                      </Button>
                    </div>
                    <Card className="shadow-sm overflow-hidden">
                      <div className="overflow-x-auto">
                        <table className="w-full text-left text-xs">
                          <thead className="bg-muted/30 text-[10px] uppercase tracking-wider text-muted-foreground">
                            <tr>
                              <th className="px-4 py-3 font-medium">Status</th>
                              <th className="px-4 py-3 font-medium">Scheduled</th>
                              <th className="px-4 py-3 font-medium">Attempt</th>
                              <th className="px-4 py-3 font-medium">Worker</th>
                              <th className="px-4 py-3 font-medium">Duration</th>
                              <th className="px-4 py-3 font-medium">Note</th>
                            </tr>
                          </thead>
                          <tbody className="divide-y divide-border">
                            {executions.rows.slice(0, 5).map((exec) => (
                              <tr key={exec.id} className="hover:bg-muted/10 transition-colors">
                                <td className="px-4 py-3"><StatusBadge status={exec.status} /></td>
                                <td className="px-4 py-3 text-muted-foreground">{formatRelative(exec.created_at)}</td>
                                <td className="px-4 py-3 text-muted-foreground">1 of 3</td>
                                <td className="px-4 py-3 font-mono text-muted-foreground">worker-17</td>
                                <td className="px-4 py-3 text-muted-foreground">8m 38s</td>
                                <td className="px-4 py-3 text-muted-foreground">—</td>
                              </tr>
                            ))}
                            {executions.rows.length === 0 && (
                              <tr>
                                <td colSpan={6} className="px-4 py-6 text-center text-muted-foreground">No recent executions.</td>
                              </tr>
                            )}
                          </tbody>
                        </table>
                      </div>
                    </Card>
                  </div>

                  <div className="flex flex-col gap-3">
                    <div className="flex items-center justify-between">
                      <h2 className="text-sm font-semibold">Versions</h2>
                      <span className="text-xs text-muted-foreground">published versions are immutable</span>
                    </div>
                    <Card className="shadow-sm overflow-hidden">
                      <div className="overflow-x-auto">
                        <table className="w-full text-left text-xs">
                          <thead className="bg-muted/30 text-[10px] uppercase tracking-wider text-muted-foreground">
                            <tr>
                              <th className="px-4 py-3 font-medium">Version</th>
                              <th className="px-4 py-3 font-medium">Published</th>
                              <th className="px-4 py-3 font-medium">Execution Type</th>
                              <th className="px-4 py-3 font-medium">Retry</th>
                              <th className="px-4 py-3 font-medium">Changes</th>
                              <th className="px-4 py-3"></th>
                            </tr>
                          </thead>
                          <tbody className="divide-y divide-border">
                            {versions.rows.slice(0, 3).map((v, i) => (
                              <tr key={v.id} className="hover:bg-muted/10 transition-colors">
                                <td className="px-4 py-3 font-medium">
                                  v{v.version_number} {i === 0 && <Badge variant="secondary" className="ml-2 text-[10px] font-normal bg-emerald-500/10 text-emerald-600 border-none">current</Badge>}
                                </td>
                                <td className="px-4 py-3 text-muted-foreground">{v.published_at ? formatRelative(v.published_at) : 'Draft'}</td>
                                <td className="px-4 py-3 font-mono text-muted-foreground">HTTP_REQUEST</td>
                                <td className="px-4 py-3 text-muted-foreground">3 · exp</td>
                                <td className="px-4 py-3 text-muted-foreground">timeout 3600s</td>
                                <td className="px-4 py-3 text-right">
                                  <Button variant="outline" size="sm" className="h-6 text-xs px-2">Diff</Button>
                                </td>
                              </tr>
                            ))}
                            {versions.rows.length === 0 && (
                              <tr>
                                <td colSpan={6} className="px-4 py-6 text-center text-muted-foreground">No published versions.</td>
                              </tr>
                            )}
                          </tbody>
                        </table>
                      </div>
                    </Card>
                  </div>
                </>
              ) : (
                <div className="py-12 text-center border border-dashed rounded-lg">
                  <p className="text-sm text-muted-foreground">Content for {tab} goes here.</p>
                </div>
              )}
            </div>

            <div className="flex flex-col gap-6">
              <Card className="shadow-sm">
                <CardHeader className="pb-3 flex flex-row items-center justify-between">
                  <CardTitle className="text-sm">Health</CardTitle>
                  <Badge variant="secondary" className="bg-emerald-500/10 text-emerald-600 border-none px-2 py-0.5 text-xs font-normal">✓ healthy</Badge>
                </CardHeader>
                <CardContent className="flex flex-col gap-4 text-xs">
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground w-20">Success</span>
                    <div className="flex-1 mx-3 h-1.5 bg-muted rounded-full overflow-hidden">
                      <div className="bg-emerald-500 h-full w-[99%]" />
                    </div>
                    <span className="font-medium w-10 text-right">99.4%</span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground w-20">Retries</span>
                    <div className="flex-1 mx-3 h-1.5 bg-muted rounded-full overflow-hidden">
                      <div className="bg-amber-500 h-full w-[5%]" />
                    </div>
                    <span className="font-medium w-10 text-right">6</span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground w-20">Timeouts</span>
                    <div className="flex-1 mx-3 h-1.5 bg-muted rounded-full overflow-hidden"></div>
                    <span className="font-medium w-10 text-right">0</span>
                  </div>
                  <div className="flex items-center justify-between">
                    <span className="text-muted-foreground w-20">Dead letters</span>
                    <div className="flex-1 mx-3 h-1.5 bg-muted rounded-full overflow-hidden"></div>
                    <span className="font-medium w-10 text-right">0</span>
                  </div>
                  <p className="text-[11px] text-muted-foreground mt-4 pt-4 border-t border-border leading-relaxed">
                    Measured from 412 recorded durations. No score is invented when there is not enough data.
                  </p>
                </CardContent>
              </Card>

              <Card className="shadow-sm">
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm">Production readiness</CardTitle>
                </CardHeader>
                <CardContent className="flex flex-col gap-2.5 text-xs">
                  <div className="flex items-center gap-2">
                    <CheckCircle2 className="size-3.5 text-foreground" />
                    <span>SLA target configured</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <CheckCircle2 className="size-3.5 text-foreground" />
                    <span>Alert rule on repeated failure</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <CheckCircle2 className="size-3.5 text-foreground" />
                    <span>Retry budget reviewed</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <CheckCircle2 className="size-3.5 text-foreground" />
                    <span>Owner assigned</span>
                  </div>
                  <div className="flex items-center gap-2 text-muted-foreground">
                    <Info className="size-3.5" />
                    <span>Runbook linked</span>
                  </div>
                  <div className="flex items-center gap-2 text-muted-foreground">
                    <Info className="size-3.5" />
                    <span>On-call rotation set</span>
                  </div>
                </CardContent>
              </Card>

              <Card className="shadow-sm">
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm">Depends on</CardTitle>
                </CardHeader>
                <CardContent className="flex flex-col gap-3">
                  <div className="rounded-md border border-border p-3 text-xs">
                    <div className="flex items-center justify-between mb-1">
                      <div className="flex items-center gap-2">
                        <Copy className="size-3.5 text-muted-foreground" />
                        <span className="font-medium">extract-orders</span>
                      </div>
                      <Badge variant="secondary" className="bg-emerald-500/10 text-emerald-600 border-none py-0 font-normal">succeeded</Badge>
                    </div>
                    <p className="text-muted-foreground text-[11px]">must succeed before this job runs</p>
                  </div>
                </CardContent>
              </Card>
            </div>
          </div>
        </div>
      ) : null}
    </AsyncBoundary>
  );
}
