"use client";

/**
 * The migration wizard (`redesign.md` §3: change previews, controlled rollouts).
 *
 * Two steps, in this order and no other:
 *
 *   1. choose a source, a target, and optionally one application;
 *   2. read the plan.
 *
 * Step 2 exists because a migration that both computed and performed its work
 * would make a dry run impossible, and a dry run that is really a write is the
 * one an operator stops trusting. Nothing here applies anything.
 */

import { useMemo, useState } from "react";
import {
  ArrowRight,
  CheckCircle2,
  CircleDashed,
  MinusCircle,
  RefreshCw,
  TriangleAlert,
} from "lucide-react";

import { api } from "@/lib/api";
import { useQuery } from "@/lib/useQuery";
import { useAuth } from "@/lib/auth";
import type {
  Environment,
  MigrationPlanResponse,
  RerunPreviewNode,
} from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  TableBody,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
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
import { ErrorState, ForbiddenState } from "@/components/states";

export default function MigrationPage() {
  const { session } = useAuth();
  const canWrite = session?.role === "OWNER" || session?.role === "ADMIN";

  const environments = useQuery<{ environments: Environment[] }>("/environments");
  const rows = environments.data?.environments ?? [];

  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [plan, setPlan] = useState<MigrationPlanResponse | null>(null);
  const [planning, setPlanning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Exclude the source from the target list: migrating an environment to
  // itself is refused by the API, and offering it here would invite a 400.
  const targets = useMemo(
    () => rows.filter((row) => row.slug !== from),
    [rows, from],
  );

  async function buildPlan() {
    setError(null);
    setPlan(null);
    if (!from || !to) {
      setError("Choose both a source and a target environment.");
      return;
    }
    setPlanning(true);
    try {
      const query = new URLSearchParams({
        from_environment: from,
        to_environment: to,
      });
      const result = await api.post<MigrationPlanResponse>(
        `/migration/plan?${query.toString()}`,
        {},
      );
      setPlan(result);
    } catch (cause) {
      setError(
        cause instanceof Error ? cause.message : "Could not build the migration plan.",
      );
    } finally {
      setPlanning(false);
    }
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Migrate jobs"
        description="Plan a move between environments, then read what it would do before anything changes."
      />

      <Toolbar>
        <div className="flex flex-wrap items-end gap-3">
          <div className="flex w-48 flex-col gap-1.5">
            <Label htmlFor="mig-from">From</Label>
            <Select value={from} onValueChange={(value) => setFrom(value ?? "")}>
              <SelectTrigger id="mig-from" size="sm" className="h-8">
                <SelectValue placeholder="Source environment" />
              </SelectTrigger>
              <SelectContent>
                {rows.map((row) => (
                  <SelectItem key={row.id} value={row.slug}>
                    {row.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <ArrowRight className="mb-2 size-4 shrink-0 text-muted-foreground" aria-hidden />

          <div className="flex w-48 flex-col gap-1.5">
            <Label htmlFor="mig-to">To</Label>
            <Select value={to} onValueChange={(value) => setTo(value ?? "")}>
              <SelectTrigger id="mig-to" size="sm" className="h-8">
                <SelectValue placeholder="Target environment" />
              </SelectTrigger>
              <SelectContent>
                {targets.map((row) => (
                  <SelectItem key={row.id} value={row.slug}>
                    {row.name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <Button
            size="sm"
            className="h-8"
            disabled={!canWrite || planning || !from || !to}
            onClick={buildPlan}
          >
            {planning ? (
              <RefreshCw className="animate-spin" aria-hidden />
            ) : null}
            {planning ? "Planning…" : "Preview migration"}
          </Button>
        </div>

        {!canWrite ? (
          <p className="text-[12px] text-muted-foreground">
            Planning needs <code className="font-mono">workflows:write</code>.
          </p>
        ) : null}
      </Toolbar>

      <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto p-4">
        {environments.state === "error" ? (
          environments.forbidden ? (
            <ForbiddenState />
          ) : (
            <ErrorState error={environments.error} onRetry={environments.reload} />
          )
        ) : null}

        {error ? (
          <p role="alert" className="text-[12.5px] text-danger-foreground">
            {error}
          </p>
        ) : null}

        {!plan ? (
          !error && environments.state !== "error" ? (
            <p className="py-12 text-center text-[13px] text-muted-foreground">
              Choose a source and a target, then preview. Nothing is applied at
              this step.
            </p>
          ) : null
        ) : (
          <PlanReview plan={plan} />
        )}
      </div>
    </div>
  );
}

/** The reviewable plan. Read-only by construction: there is no apply button. */
function PlanReview({ plan }: { plan: MigrationPlanResponse }) {
  const { summary, plan: detail } = plan;
  const unresolved = detail.entries.filter((entry) => entry.unresolved.length > 0);

  return (
    <>
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
        <Stat label="Would create" value={summary.create} />
        <Stat label="Would update" value={summary.update} />
        <Stat label="Already there" value={summary.unchanged} />
        <Stat
          label="Need a decision"
          value={unresolved.length}
          tone={unresolved.length > 0 ? "warning" : "neutral"}
        />
      </div>

      <Panel
        title={
          <span className="flex items-center gap-2">
            <span>
              {detail.source_environment} → {detail.target_environment}
            </span>
          </span>
        }
        description={plan.note}
        bodyClassName="p-0"
      >
        <DataTable>
          <TableHeader>
            <TableRow>
              <DataTableHead>Job</DataTableHead>
              <DataTableHead className="w-44">Key</DataTableHead>
              <DataTableHead className="w-32">Action</DataTableHead>
              <DataTableHead>Needs a decision</DataTableHead>
            </TableRow>
          </TableHeader>

          <TableBody>
            {detail.entries.map((entry) => (
              <TableRow key={`${entry.source_key}-${entry.name}`} className="h-9">
                <DataTableCell className="font-medium">
                  {entry.name}
                </DataTableCell>

                <DataTableCell>
                  {entry.source_key ? (
                    <code className="font-mono text-[11.5px]">{entry.source_key}</code>
                  ) : (
                    // No key means the job cannot be matched on the way back.
                    // Shown as an em dash with the reason in the last column
                    // rather than quietly omitted from the plan.
                    <span className="text-muted-foreground">—</span>
                  )}
                </DataTableCell>

                <DataTableCell>
                  <ActionBadge action={entry.action} />
                </DataTableCell>

                <DataTableCell className="text-[12px]">
                  {entry.unresolved.length === 0 ? (
                    <span className="text-muted-foreground/60">—</span>
                  ) : (
                    <ul className="flex flex-col gap-0.5">
                      {entry.unresolved.map((binding) => (
                        <li
                          key={`${binding.kind}-${binding.source_name}`}
                          className="flex items-center gap-1.5"
                        >
                          <TriangleAlert
                            className="size-3 shrink-0 text-warning-foreground"
                            aria-hidden
                          />
                          <span className="font-mono text-[11px]">
                            {binding.source_name}
                          </span>
                          <span className="text-[11px] text-muted-foreground">
                            {binding.kind.replace("_", " ")} ·{" "}
                            {describeReason(binding.reason)}
                          </span>
                        </li>
                      ))}
                    </ul>
                  )}
                </DataTableCell>
              </TableRow>
            ))}
          </TableBody>
        </DataTable>
      </Panel>

      {/*
        What travels, stated explicitly. A plan an operator cannot check against
        intent is a plan they approve on faith.
      */}
      <Panel title="Fields that would cross" bodyClassName="p-3">
        <ul className="flex flex-wrap gap-2">
          {detail.include.map((field) => (
            <li
              key={field.kind}
              className="rounded bg-muted px-2 py-1 text-[12px]"
            >
              {field.kind}
            </li>
          ))}
        </ul>
        <p className="mt-2 text-[11.5px] text-muted-foreground">
          Bindings — queues, worker pools and secrets — are excluded. They name
          destination objects that have to be resolved by hand, and copying them
          would point the target at the source&apos;s queue.
        </p>
      </Panel>
    </>
  );
}

function ActionBadge({ action }: { action: string }) {
  const presentation = {
    create: {
      icon: CircleDashed,
      label: "Create",
      className: "text-info-foreground",
    },
    update: {
      icon: RefreshCw,
      label: "Update",
      className: "text-warning-foreground",
    },
    unchanged: {
      icon: MinusCircle,
      label: "Unchanged",
      className: "text-muted-foreground",
    },
  }[action] ?? {
    icon: CircleDashed,
    label: action,
    className: "text-muted-foreground",
  };

  const Icon = presentation.icon;
  return (
    <span
      className={`inline-flex items-center gap-1.5 text-[12.5px] ${presentation.className}`}
    >
      <Icon className="size-3 shrink-0" aria-hidden />
      {presentation.label}
    </span>
  );
}

function describeReason(reason: string): string {
  switch (reason) {
    case "missing_in_target":
      return "no object of this name in the target";
    case "configuration_differs":
      return "an object exists but differs";
    case "foreign_tenant":
      return "belongs to another tenant";
    default:
      return reason.replace(/_/g, " ");
  }
}
