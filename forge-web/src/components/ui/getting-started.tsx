"use client";

/**
 * First-run checklist (UI.md section 55, section 78).
 *
 * A tenant that has never run anything starts in a state no screen was designed
 * for: the dashboard is a wall of zeroes and the jobs list is empty, and neither
 * tells a new operator what to do next.
 *
 * Three decisions shape it:
 *
 * - Steps are marked done from real data, never from local state, so the
 *   checklist cannot claim progress the server disagrees with.
 * - Once the tenant is running it collapses to a single line. A 300px card of
 *   "you are already done" above the metrics pushes the thing the operator
 *   actually came to read below the fold.
 * - The publish step counts ACTIVE jobs from a full page, not from the
 *   single-row peek this component used to request — `?limit=1` could not see
 *   a published job whenever the first job happened to be a draft.
 */

import Link from "next/link";
import { useState } from "react";
import { Check, ChevronDown, Circle, ExternalLink } from "lucide-react";

import { useList, useQuery } from "@/lib/useQuery";
import { cn } from "cn";

interface DashboardSummary {
  executions?: {
    succeeded?: number;
    failed?: number;
    dead_lettered?: number;
  };
  workers?: { ready?: number; busy?: number; offline?: number };
}

interface JobRow {
  id: string;
  status: string;
  current_version_id: string | null;
}

export function GettingStarted() {
  // The same aggregate the dashboard reads, so the two can never disagree.
  const dashboard = useQuery<DashboardSummary>("/dashboard");
  // A full page, because "publish" is a question about the whole set rather
  // than about whichever job happens to sort first.
  const jobs = useList<JobRow>("/jobs?limit=100");
  const [expanded, setExpanded] = useState(true);

  const ready = dashboard.state === "ready";
  const data = dashboard.data;

  const hasJobs = jobs.state === "ready" && jobs.rows.length > 0;
  const hasPublished =
    hasJobs &&
    jobs.rows.some((job) => job.status === "ACTIVE" && job.current_version_id);

  const workers = data?.workers;
  const workerCount =
    (workers?.ready ?? 0) + (workers?.busy ?? 0) + (workers?.offline ?? 0);
  const hasRunAnything =
    (data?.executions?.succeeded ?? 0) +
      (data?.executions?.failed ?? 0) +
      (data?.executions?.dead_lettered ?? 0) >
    0;

  const steps = [
    {
      key: "tenant",
      state: "done" as const,
      title: "Create your tenant",
      detail: "Every job, worker, and execution belongs to a tenant.",
      href: undefined,
      cta: undefined,
    },
    {
      key: "job",
      state: hasJobs ? ("done" as const) : ("current" as const),
      title: "Define a job",
      detail: hasJobs
        ? `${jobs.rows.length} job${jobs.rows.length === 1 ? "" : "s"} defined.`
        : "Name the work: what runs, how often, and what happens when it fails.",
      href: "/jobs/builder",
      cta: "Create a job",
    },
    {
      key: "publish",
      state: hasPublished
        ? ("done" as const)
        : hasJobs
          ? ("current" as const)
          : ("todo" as const),
      title: "Publish a version",
      detail: hasPublished
        ? "A published version is immutable, so a later change cannot alter work already dispatched."
        : "A draft never runs. Publishing is what makes a job eligible for dispatch.",
      href: "/jobs",
      cta: "Open jobs",
    },
    {
      key: "worker",
      state:
        workerCount > 0
          ? ("done" as const)
          : hasPublished
            ? ("current" as const)
            : ("todo" as const),
      title: "Connect a worker",
      detail:
        workerCount > 0
          ? `${workerCount} worker${workerCount === 1 ? "" : "s"} registered.`
          : "Without a worker, executions queue and nothing runs.",
      href: "/workers",
      cta: "Open workers",
    },
    {
      key: "run",
      state: hasRunAnything
        ? ("done" as const)
        : workerCount > 0
          ? ("current" as const)
          : ("todo" as const),
      title: "Run it once by hand",
      detail: hasRunAnything
        ? "At least one execution has been recorded."
        : "Trigger the job directly and watch the execution timeline.",
      href: "/jobs",
      cta: "Open jobs",
    },
  ];

  const doneCount = steps.filter((step) => step.state === "done").length;
  const finished = doneCount === steps.length;

  // Never teach onboarding to a tenant that has finished it.
  if (ready && finished) return null;

  return (
    <section
      aria-label="Setup progress"
      className="rounded-md border border-border bg-card"
    >
      <div className="flex items-center gap-2 px-3 py-2">
        <h2 className="text-[13px] font-semibold">Set up your first execution</h2>
        <span className="text-[11.5px] text-muted-foreground tabular-nums">
          {doneCount} of {steps.length} complete
        </span>
        <button
          type="button"
          onClick={() => setExpanded((open) => !open)}
          aria-expanded={expanded}
          className="ml-auto grid size-6 place-items-center rounded text-muted-foreground hover:bg-accent hover:text-foreground"
          aria-label={expanded ? "Hide setup steps" : "Show setup steps"}
        >
          <ChevronDown
            className={cn("size-3.5 transition-transform", !expanded && "-rotate-90")}
            aria-hidden
          />
        </button>
      </div>

      {/* The track stays visible when collapsed: "4 of 5 complete" is the
          signal that something is unfinished. */}
      <div
        className="h-1 w-full bg-muted"
        role="progressbar"
        aria-valuenow={doneCount}
        aria-valuemin={0}
        aria-valuemax={steps.length}
        aria-label="Setup steps complete"
      >
        <div
          className="h-full bg-primary transition-[width]"
          style={{ width: `${(doneCount / steps.length) * 100}%` }}
        />
      </div>

      {expanded ? (
        <ol className="px-3 py-1.5">
          {steps.map(({ key, ...step }) => (
            <Step key={key} {...step} />
          ))}
        </ol>
      ) : null}
    </section>
  );
}

function Step({
  state,
  title,
  detail,
  href,
  cta,
}: {
  state: "done" | "current" | "todo";
  title: string;
  detail: string;
  href?: string;
  cta?: string;
}) {
  const done = state === "done";
  const current = state === "current";

  return (
    <li className="flex items-start gap-2.5 border-b border-border/60 py-2 last:border-0">
      <span
        className={cn(
          "mt-0.5 grid size-4 shrink-0 place-items-center rounded-full border text-[9px]",
          done && "border-success bg-success text-white",
          current && "border-primary bg-primary text-primary-foreground",
          !done && !current && "border-border text-muted-foreground",
        )}
        aria-hidden
      >
        {done ? (
          <Check className="size-2.5" />
        ) : !current ? (
          <Circle className="size-1.5 fill-current" />
        ) : null}
      </span>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          {/*
            A tick glyph rather than a text badge: the word is already there, and
            the badge consumed a row's width to repeat it.
          */}
          <span
            className={cn(
              "text-[12.5px] font-medium",
              !done && !current && "text-muted-foreground",
            )}
          >
            {title}
          </span>
          {current ? (
            <span className="text-[10.5px] font-medium tracking-wide text-primary uppercase">
              Next
            </span>
          ) : null}
        </div>
        {!done ? (
          <p className="mt-0.5 text-[11.5px] text-muted-foreground">{detail}</p>
        ) : null}
      </div>

      {current && href && cta ? (
        <Link
          href={href}
          className="mt-0.5 inline-flex shrink-0 items-center gap-1 text-[11.5px] underline-offset-2 hover:underline"
        >
          {cta}
          <ExternalLink className="size-3" aria-hidden />
        </Link>
      ) : null}
    </li>
  );
}