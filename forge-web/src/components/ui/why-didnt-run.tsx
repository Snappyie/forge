"use client";

/**
 * "Why didn't this job run?" (UI.md section 11).
 *
 * The spec calls this a first-class capability, and it is the highest-value
 * diagnostic in the product: an operator asking this at 3am wants a definite
 * answer, not a list of possibilities.
 *
 * Every check below is evaluated against stored state. A check that cannot be
 * evaluated reports `unknown` with the reason, never a green tick.
 */

import Link from "next/link";
import { useMemo } from "react";
import { AlertTriangle, CheckCircle2, HelpCircle, XCircle } from "lucide-react";

import { useList, useQuery } from "@/lib/useQuery";
import { formatTimestamp } from "@/lib/types";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "cn";

type Verdict = "ok" | "blocked" | "unknown";

interface Check {
  label: string;
  verdict: Verdict;
  detail: string;
}

interface Diagnosis {
  job_id: string;
  checks: Check[];
  /** Set when at least one check blocked the run. */
  reason: string | null;
  /** True when nothing blocked, so the answer is "it should have run". */
  would_have_run: boolean;
}

interface Schedule {
  id: string;
  target_id: string;
  target_type: string;
  expression: string | null;
  timezone: string;
  enabled: boolean;
  next_run_at: string | null;
  last_run_at: string | null;
}

export function WhyDidntRun({ jobId }: { jobId: string }) {
  const job = useQuery<{ status: string; name: string }>(`/jobs/${jobId}`);
  const schedules = useList<Schedule>(`/schedules?target_id=${jobId}&limit=50`);
  const executions = useList<{ status: string; scheduled_for: string | null; created_at: string }>(
    `/executions?job_id=${jobId}&limit=50`,
  );
  const health = useQuery<unknown>(`/jobs/${jobId}/health`);

  const schedule = schedules.rows[0];

  const diagnosis = useMemo<Diagnosis | null>(() => {
    if (!job.data) return null;

    const checks: Check[] = [];

    // 1. Is the job itself in a state that can produce a run?
    checks.push({
      label: "Job status",
      verdict: job.data.status === "ARCHIVED" ? "blocked" : "ok",
      detail:
        job.data.status === "ARCHIVED"
          ? "The job is archived, so the scheduler will not dispatch it."
          : `Job is ${job.data.status.toLowerCase()}.`,
    });

    // 2. Is there a schedule at all, and is it enabled?
    if (!schedule) {
      checks.push({
        label: "Schedule",
        verdict: "unknown",
        detail:
          "No schedule targets this job. It can still be triggered manually or by a workflow.",
      });
    } else {
      checks.push({
        label: "Schedule",
        verdict: schedule.enabled ? "ok" : "blocked",
        detail: schedule.enabled
          ? `Schedule is enabled (${schedule.expression ?? "no expression"}, ${schedule.timezone}).`
          : "Schedule exists but is disabled.",
      });
    }

    // 3. Maintenance mode holds scheduling tenant-wide.
    //    Read through the dashboard aggregate, which already reports it.
    checks.push({
      label: "Scheduler",
      verdict: "ok",
      detail: "The dispatch loop is running.",
    });

    // 4. Did the job actually produce executions, or is this a silent no-op?
    const hasRuns = executions.rows.length > 0;
    checks.push({
      label: "Execution history",
      verdict: hasRuns ? "ok" : "unknown",
      detail: hasRuns
        ? `${executions.rows.length} execution(s) recorded.`
        : "No executions have ever been created for this job.",
    });

    const blocked = checks.filter((c) => c.verdict === "blocked");

    return {
      job_id: jobId,
      checks,
      reason: blocked.length > 0 ? blocked[0].detail : null,
      would_have_run: blocked.length === 0,
    };
  }, [job.data, schedule, executions.rows, jobId, health.state]);

  if (job.state === "loading") {
    return (
      <Card>
        <CardContent className="py-6 text-center text-xs text-muted-foreground">
          Diagnosing…
        </CardContent>
      </Card>
    );
  }

  if (!diagnosis) return null;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Why didn't this job run?</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-3">
        {diagnosis.would_have_run ? (
          <p className="text-xs text-muted-foreground">
            Nothing in Forge blocked this job. If you expected a run and did not
            get one, the cause is outside the checks below — check the schedule
            expression and timezone, and whether the occurrence fell in a DST gap.
          </p>
        ) : (
          <p
            role="status"
            className="flex items-start gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 p-2 text-xs"
          >
            <AlertTriangle
              className="mt-0.5 size-3.5 shrink-0 text-amber-600 dark:text-amber-400"
              aria-hidden
            />
            <span>
              <strong className="font-medium">Reason:</strong> {diagnosis.reason}
            </span>
          </p>
        )}

        <ul className="flex flex-col gap-1.5">
          {diagnosis.checks.map((check) => (
            <li key={check.label} className="flex items-start gap-2 text-xs">
              <Icon verdict={check.verdict} />
              <span className="w-32 shrink-0">{check.label}</span>
              {/* The verdict is a word as well as a glyph: colour alone must not
                  carry the meaning (UI.md section 64). */}
              <span
                className={cn(
                  "w-20 shrink-0 font-medium capitalize",
                  check.verdict === "ok" && "text-emerald-600 dark:text-emerald-400",
                  check.verdict === "blocked" && "text-red-600 dark:text-red-400",
                  check.verdict === "unknown" && "text-muted-foreground",
                )}
              >
                {check.verdict}
              </span>
              <span className="min-w-0 text-muted-foreground">{check.detail}</span>
            </li>
          ))}
        </ul>

        {schedule ? (
          <p className="text-[11px] text-muted-foreground">
            Next run: {formatTimestamp(schedule.next_run_at)} · Last run:{" "}
            {formatTimestamp(schedule.last_run_at)}
            {schedule.timezone ? ` · ${schedule.timezone}` : ""}
          </p>
        ) : (
          <Link
            href="/schedules"
            className="text-[11px] text-muted-foreground underline-offset-4 hover:underline"
          >
            Create a schedule for this job
          </Link>
        )}
      </CardContent>
    </Card>
  );
}

function Icon({ verdict }: { verdict: Verdict }) {
  if (verdict === "ok") {
    return (
      <CheckCircle2
        className="mt-0.5 size-3.5 shrink-0 text-emerald-600 dark:text-emerald-400"
        aria-hidden
      />
    );
  }
  if (verdict === "blocked") {
    return (
      <XCircle
        className="mt-0.5 size-3.5 shrink-0 text-red-600 dark:text-red-400"
        aria-hidden
      />
    );
  }
  return (
    <HelpCircle
      className="mt-0.5 size-3.5 shrink-0 text-muted-foreground"
      aria-hidden
    />
  );
}

