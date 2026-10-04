"use client";

/**
 * First-run checklist (UI.md section 55, section 78).
 *
 * A tenant that has never run anything starts in a state that no screen was
 * designed for: the dashboard is a wall of zeroes and the jobs list is empty,
 * and neither one tells a new operator what to do next. The spec asks for a
 * first-run state on the Jobs page, and the console has a per-page empty state
 * but no path between "signed up" and "my first execution".
 *
 * This component is that path. Two decisions shape it:
 *
 * - Steps are ordered by dependency and marked done from real data, never from
 *   local state, so a checklist cannot claim progress the server disagrees with.
 * - It distinguishes what is already working from what still needs the operator.
 *   A new install has a live scheduler and applied migrations long before it has
 *   a worker, and a screen that only lists what is missing makes a healthy
 *   system look broken.
 */

import Link from "next/link";
import { Check, Circle, ExternalLink, Terminal } from "lucide-react";

import { useList, useQuery } from "@/lib/useQuery";
import { cn } from "cn";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

interface DashboardSummary {
  executions?: {
    queued?: number;
    running?: number;
    succeeded?: number;
    failed?: number;
    dead_lettered?: number;
    cancelled?: number;
  };
  workers?: { ready?: number; busy?: number; offline?: number };
  queues?: unknown[];
}

interface JobRow {
  id: string;
  name: string;
  status: string;
  current_version_id: string | null;
}

interface StepState {
  state: "done" | "current" | "todo";
}

export function GettingStarted() {
  // The same three aggregates the dashboard uses, so this can never disagree
  // with the numbers shown beside it.
  const dashboard = useQuery<DashboardSummary>("/dashboard");
  const jobs = useList<JobRow>("/jobs?limit=1");
  const queues = useList<unknown>("/queues?limit=1");

  const data = dashboard.data;
  const ready = dashboard.state === "ready";

  const executions = data?.executions;
  const workers = data?.workers;
  const hasJobs = jobs.state === "ready" && jobs.rows.length > 0;
  const hasPublished =
    hasJobs &&
    jobs.rows.some((job) => job.status === "ACTIVE" && job.current_version_id);
  const workerCount =
    (workers?.ready ?? 0) + (workers?.busy ?? 0) + (workers?.offline ?? 0);
  const hasRunAnything =
    (executions?.succeeded ?? 0) + (executions?.failed ?? 0) > 0;
  const queueCount = queues.state === "ready" ? queues.rows.length : 0;

  const steps: { key: string; state: StepState; title: string; detail: string; href?: string }[] = [
    {
      key: "tenant",
      state: { state: "done" },
      title: "Create your tenant",
      detail:
        "Every job, worker and execution belongs to a tenant. You are its owner.",
    },
    {
      key: "job",
      state: { state: hasJobs ? "done" : "current" },
      title: "Define a job",
      detail: hasJobs
        ? "A reusable definition of the work this tenant runs."
        : "Name the work: what to run, how often, and what happens when it fails.",
      href: "/jobs/builder",
    },
    {
      key: "publish",
      state: { state: hasPublished ? "done" : hasJobs ? "current" : "todo" },
      title: "Publish a version",
      detail: hasPublished
        ? "A published version is immutable, so a later change cannot alter work already dispatched."
        : "A draft never runs. Publishing is what makes a job eligible for dispatch.",
      href: "/jobs",
    },
    {
      key: "worker",
      state: {
        state: workerCount > 0 ? "done" : hasPublished ? "current" : "todo",
      },
      title: "Connect a worker",
      detail:
        workerCount > 0
          ? `${workerCount} worker${workerCount === 1 ? "" : "s"} registered.`
          : "Without a worker, executions queue and nothing runs. A worker claims work over HTTP, holds a lease, and heartbeats.",
      href: "/workers",
    },
    {
      key: "run",
      state: { state: hasRunAnything ? "done" : workerCount > 0 ? "current" : "todo" },
      title: "Run it once by hand",
      detail: hasRunAnything
        ? "At least one execution has been recorded for this tenant."
        : "Trigger the job directly and watch the execution timeline. Once you have seen one succeed, let the schedule do it.",
      href: "/jobs",
    },
  ];

  const doneCount = steps.filter((step) => step.state.state === "done").length;
  const finished = doneCount === steps.length;

  // Nothing to teach once the tenant is running. The dashboard's own first-run
  // card and this one would otherwise say the same thing twice.
  if (ready && finished) return null;

  return (
    <Card className="mb-6">
      <CardHeader>
        <div className="flex flex-wrap items-center justify-between gap-2">
          <CardTitle className="text-sm">Set up your first execution</CardTitle>
          <span className="text-[11px] tabular-nums text-muted-foreground">
            {doneCount} of {steps.length} complete
          </span>
        </div>
        <p className="text-xs text-muted-foreground">
          Forge is running and waiting for work. Each step depends on the one
          before it.
        </p>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        <ol className="flex flex-col">
          {steps.map(({ key, ...step }) => (
            <Step key={key} {...step} />
          ))}
        </ol>

        <div className="grid gap-4 border-t border-border pt-4 md:grid-cols-2">
          <div>
            <p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
              Already working
            </p>
            <ul className="flex flex-col gap-1.5">
              <Working line="Scheduler loop" detail="ticks every 5s" state={ready ? "ok" : "unknown"} />
              <Working line="Database and migrations" detail="applied at boot" state={ready ? "ok" : "unknown"} />
              <Working line="Lease reaper" detail="sweeps every 5s" state={ready ? "ok" : "unknown"} />
              <Working line="Outbox publisher" detail="drains events" state={ready ? "ok" : "unknown"} />
              <Working line="Queues" detail={queueCount > 0 ? `${queueCount} defined` : "default only"} state={ready ? "ok" : "unknown"} />
            </ul>
          </div>

          <div>
            <p className="mb-2 text-[11px] font-semibold uppercase tracking-wide text-muted-foreground">
              Still needs you
            </p>
            <ul className="flex flex-col gap-1.5">
              <Working
                line="Workers"
                detail={workerCount > 0 ? `${workerCount} registered` : "none registered"}
                state={ready ? (workerCount > 0 ? "ok" : "todo") : "unknown"}
              />
              <Working
                line="A job definition"
                detail={hasJobs ? `${jobs.rows.length > 0 ? "created" : "none yet"}` : "none yet"}
                state={ready ? (hasJobs ? "ok" : "todo") : "unknown"}
              />
              <Working
                line="First execution"
                detail={hasRunAnything ? "recorded" : "nothing has run"}
                state={ready ? (hasRunAnything ? "ok" : "todo") : "unknown"}
              />
            </ul>

            <div className="mt-3 rounded-md border border-border bg-muted/40 p-2.5">
              <p className="mb-1.5 flex items-center gap-1.5 text-[11px] font-medium">
                <Terminal className="size-3" aria-hidden />
                Register a worker
              </p>
              <pre className="overflow-x-auto font-mono text-[10.5px] leading-relaxed text-muted-foreground">
{`curl -X POST $FORGE_URL/api/v1/workers/register \\
  -H "authorization: Bearer $FORGE_TOKEN" \\
  -d '{"name":"worker-01"}'`}
              </pre>
              <Link
                href="/workers"
                className="mt-1.5 inline-flex items-center gap-1 text-[11px] underline underline-offset-4"
              >
                Worker setup guide
                <ExternalLink className="size-3" aria-hidden />
              </Link>
            </div>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

function Step({
  state,
  title,
  detail,
  href,
}: {
  state: StepState;
  title: string;
  detail: string;
  href?: string;
}) {
  const done = state.state === "done";
  const current = state.state === "current";

  return (
    <li className="flex gap-3 border-t border-border py-2.5 first:border-t-0">
      <span
        className={cn(
          "mt-0.5 grid size-5 shrink-0 place-items-center rounded-full border text-[10px] font-semibold",
          done && "border-emerald-500 bg-emerald-500 text-white",
          current && "border-primary bg-primary text-primary-foreground",
          !done && !current && "border-border text-muted-foreground",
        )}
        aria-hidden
      >
        {done ? <Check className="size-3" /> : null}
        {!done && !current ? <Circle className="size-2 fill-current" /> : null}
      </span>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span
            className={cn(
              "text-xs font-medium",
              !done && !current && "text-muted-foreground",
            )}
          >
            {title}
          </span>
          {done ? (
            <span className="rounded bg-emerald-500/15 px-1.5 py-0.5 text-[10px] font-medium text-emerald-600 dark:text-emerald-400">
              done
            </span>
          ) : null}
          {current ? (
            <span className="rounded bg-blue-500/15 px-1.5 py-0.5 text-[10px] font-medium text-blue-600 dark:text-blue-400">
              next
            </span>
          ) : null}
        </div>
        <p className="mt-0.5 text-[11.5px] leading-relaxed text-muted-foreground">
          {detail}
        </p>
        {current && href ? (
          <Link
            href={href}
            className="mt-1 inline-flex items-center gap-1 text-[11px] underline underline-offset-4"
          >
            {title === "Connect a worker" ? "Go to workers" : "Open jobs"}
            <ExternalLink className="size-3" aria-hidden />
          </Link>
        ) : null}
      </div>
    </li>
  );
}

/**
 * A single "is this working" line.
 *
 * `unknown` is rendered as such rather than as a neutral dot. A component with
 * no evidence either way is not a healthy component, and a grey dot next to
 * "healthy" would say it was.
 */
function Working({
  line,
  detail,
  state,
}: {
  line: string;
  detail: string;
  state: "ok" | "todo" | "unknown";
}) {
  return (
    <li className="flex items-center gap-2 text-[11.5px]">
      <span
        className={cn(
          "size-1.5 shrink-0 rounded-full",
          state === "ok" && "bg-emerald-500",
          state === "todo" && "bg-border",
          state === "unknown" && "bg-border",
        )}
        aria-hidden
      />
      <span className={state === "todo" ? "text-muted-foreground" : undefined}>
        {line}
      </span>
      <span className="ml-auto text-[11px] text-muted-foreground">{detail}</span>
    </li>
  );
}