"use client";

/**
 * Workflow detail (UI.md sections 23, 24, 35).
 *
 * Shows the stored definition, its versions, and the executions it produced.
 * The graph is drawn from the stored node list, so it matches what the engine
 * actually ran.
 */

import Link from "next/link";
import { PageBreadcrumb } from "@/components/ui/page-breadcrumb";
import { use, useState } from "react";
import { ArrowLeft, Play } from "lucide-react";

import { useList, useQuery } from "@/lib/useQuery";
import { api } from "@/lib/api";
import { formatRelative, formatTimestamp, type Execution, type Workflow } from "@/lib/types";
import { WorkflowDesigner } from "@/components/ui/workflow-designer";
import { useToast } from "@/lib/useToast";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { StatusBadge } from "@/components/status-badge";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface WorkflowDetail {
  id: string;
  key: string | null;
  name: string;
  description: string | null;
  status: "DRAFT" | "ACTIVE" | "ARCHIVED";
  current_version_id: string | null;
  // The API returns the stored definition alongside the workflow.
  definition: {
    nodes: { key: string; name?: string; type: string; config?: Record<string, unknown> }[];
    edges: { from: string; to: string }[];
  } | null;
}

interface WorkflowVersion {
  id: string;
  version: number;
  status: string;
  created_at: string;
  published_at: string | null;
}

export default function WorkflowDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  const workflow = useQuery<WorkflowDetail>(`/workflows/${id}`);
  const versions = useList<WorkflowVersion>(`/workflows/${id}/versions`);
  const executions = useList<Execution>(`/executions?workflow_id=${id}&limit=25`);

  const record = workflow.data;
  const definition = record?.definition ?? undefined;

  async function trigger() {
    setBusy(true);
    try {
      const created = await api.post<{ execution_id?: string; id?: string }>(
        `/workflows/${id}/trigger`,
        {},
      );
      const executionId = created.execution_id ?? created.id;
      toast.success("Workflow triggered", {
        label: "Open execution",
        onClick: () => window.location.assign(`/executions/${executionId}`),
      });
      executions.reload();
    } catch (error) {
      toast.error("Could not trigger", error instanceof Error ? error.message : undefined);
    } finally {
      setBusy(false);
    }
  }

  return (
    <AsyncBoundary
      state={workflow.state}
      error={workflow.error}
      forbidden={workflow.forbidden}
      empty={false}
      onRetry={workflow.reload}
      loadingLabel="Loading workflow"
    >
      {record ? (
        <div className="flex flex-col gap-4 p-6">
          <header className="flex flex-wrap items-start justify-between gap-3">
            <div className="min-w-0">
              <PageBreadcrumb items={[{ label: "Forge", href: "/" }, { label: "Workflows", href: "/workflows" }, { label: "Detail" }]} />
              <div className="flex flex-wrap items-center gap-2">
                <h1 className="text-lg font-semibold">{record.name}</h1>
                <StatusBadge status={record.status} />
              </div>
              {record.key ? (
                <p className="mt-1 font-mono text-xs text-muted-foreground">
                  {record.key}
                </p>
              ) : null}
              {record.description ? (
                <p className="mt-1 max-w-2xl text-xs text-muted-foreground">
                  {record.description}
                </p>
              ) : null}
            </div>

            <Button
              size="sm"
              disabled={busy || record.status !== "ACTIVE"}
              onClick={trigger}
              title={
                record.status !== "ACTIVE"
                  ? "Only an active workflow can be triggered"
                  : undefined
              }
            >
              <Play className="mr-1 size-3.5" aria-hidden />
              {busy ? "Triggering…" : "Trigger"}
            </Button>
          </header>

          {/* The stored definition, editable and validated (UI.md section 23). */}
          <Card>
            <CardHeader>
              <CardTitle className="text-sm">Designer</CardTitle>
            </CardHeader>
            <CardContent>
              {definition?.nodes?.length ? (
                <WorkflowDesigner
                  workflowId={id}
                  definition={{
                    nodes: definition.nodes,
                    edges: definition.edges ?? [],
                  }}
                />
              ) : (
                <EmptyState
                  title="No published definition"
                  description="This workflow has no version yet, so there is nothing to edit."
                />
              )}
            </CardContent>
          </Card>

          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Versions</CardTitle>
              </CardHeader>
              <CardContent>
                {versions.rows.length === 0 ? (
                  <EmptyState title="No versions" description="Nothing published yet." />
                ) : (
                  <ul className="flex flex-col gap-1">
                    {versions.rows.map((version) => (
                      <li
                        key={version.id}
                        className="flex items-center gap-2 rounded px-2 py-1.5 text-xs hover:bg-accent/50"
                      >
                        <Badge variant="secondary" className="text-[10px]">
                          v{version.version}
                        </Badge>
                        <span className="text-muted-foreground">
                          {formatTimestamp(version.created_at)}
                        </span>
                        {version.published_at ? (
                          <span className="ml-auto text-[11px] text-emerald-600 dark:text-emerald-400">
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

            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Recent executions</CardTitle>
              </CardHeader>
              <CardContent>
                {executions.state === "loading" ? (
                  <p className="py-3 text-center text-xs text-muted-foreground">
                    Loading…
                  </p>
                ) : executions.rows.length === 0 ? (
                  <EmptyState
                    title="No executions yet"
                    description="Trigger this workflow to see its runs here."
                  />
                ) : (
                  <ul className="flex flex-col gap-1">
                    {executions.rows.map((execution) => (
                      <li key={execution.id}>
                        <Link
                          href={`/executions/${execution.id}`}
                          className="flex items-center gap-2 rounded px-2 py-1.5 text-xs hover:bg-accent/50"
                        >
                          <StatusBadge status={execution.status} />
                          <span className="ml-auto text-[11px] text-muted-foreground">
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
        </div>
      ) : (
        <EmptyState
          title="Workflow not found"
          description="It may have been deleted, or it belongs to another tenant."
        />
      )}
    </AsyncBoundary>
  );
}
