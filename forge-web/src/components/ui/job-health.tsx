"use client";

/**
 * Job health (UI.md sections 32 and 79) and SLA display (section 31).
 *
 * The spec is explicit: avoid an arbitrary health score, show concrete metrics.
 * Every figure here is computed by the API from stored executions, and an absent
 * measurement renders as "no data" rather than as zero.
 */

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { formatDuration } from "@/lib/types";
import { cn } from "cn";

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

export function JobHealthPanel({ health }: { health: Health }) {
  const { reliability, performance, sla } = health;
  // No executions at all is a different state from zero percent success.
  const noData = reliability.executions === 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Health</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-5">
        <section aria-label="Reliability">
          <h3 className="mb-2 text-xs font-medium text-muted-foreground">
            Reliability
          </h3>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
            <Metric
              label="Success rate"
              value={
                noData ? "no data" : `${reliability.success_rate ?? "—"}%`
              }
              tone={
                noData || reliability.success_rate === null
                  ? "neutral"
                  : (reliability.success_rate ?? 0) >= 95
                    ? "positive"
                    : "negative"
              }
            />
            <Metric label="Executions" value={String(reliability.executions)} />
            <Metric label="Failures" value={String(reliability.failed)} />
            <Metric label="Retries" value={String(reliability.retries)} />
            <Metric
              label="Dead lettered"
              value={String(reliability.dead_lettered_or_cancelled)}
            />
            <Metric
              label="SLA compliance"
              // No configured target means no compliance figure can honestly be
              // claimed, so the panel says so.
              value={
                sla === null
                  ? "no target"
                  : sla.evaluated === 0
                    ? "not evaluated"
                    : `${sla.compliance_percent ?? "—"}%`
              }
              tone={
                sla === null || sla.evaluated === 0
                  ? "neutral"
                  : (sla.compliance_percent ?? 0) >= 95
                    ? "positive"
                    : "negative"
              }
            />
          </div>
        </section>

        <section aria-label="Performance">
          <h3 className="mb-2 text-xs font-medium text-muted-foreground">
            Performance
          </h3>
          <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
            <Metric
              label="Average"
              value={seconds(performance.finished_average_seconds)}
            />
            <Metric label="P50" value={seconds(performance.p50_seconds)} />
            <Metric label="P95" value={seconds(performance.p95_seconds)} />
            <Metric label="P99" value={seconds(performance.p99_seconds)} />
          </div>
        </section>

        {sla ? (
          <p className="text-[11px] text-muted-foreground">
            SLA target:{" "}
            {sla.target_seconds === null
              ? "not configured"
              : formatDuration(sla.target_seconds * 1000)}{" "}
            · {sla.met} of {sla.evaluated} completed runs met it
          </p>
        ) : null}
      </CardContent>
    </Card>
  );
}

function Metric({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: string;
  tone?: "neutral" | "positive" | "negative";
}) {
  return (
    <div className="rounded-md border border-border p-2">
      <p className="text-[11px] text-muted-foreground">{label}</p>
      <p
        className={cn(
          "text-sm font-medium tabular-nums",
          tone === "positive" && "text-emerald-600 dark:text-emerald-400",
          tone === "negative" && "text-red-600 dark:text-red-400",
        )}
      >
        {value}
      </p>
    </div>
  );
}

/** Seconds to a duration, or an explicit absence. */
function seconds(value: number | null): string {
  return value === null ? "no data" : formatDuration(value * 1000);
}
