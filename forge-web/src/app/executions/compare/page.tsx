"use client";

/**
 * Execution comparison (UI.md section 18).
 *
 * Side-by-side diff of two real executions: status, timing, attempts, and any
 * failure. When a field differs it is marked, so an operator can see at a glance
 * which run went wrong.
 */

import { useMemo, useState } from "react";
import { ArrowRight } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { formatDuration, formatTimestamp, type Execution } from "@/lib/types";
import { AsyncBoundary, EmptyState } from "@/components/states";
import { StatusBadge } from "@/components/status-badge";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { cn } from "cn";

interface Field {
  label: string;
  left: string;
  right: string;
}

export default function CompareExecutionsPage() {
  const [leftId, setLeftId] = useState("");
  const [rightId, setRightId] = useState("");

  const left = useQuery<Execution>(leftId ? `/executions/${leftId}` : null);
  const right = useQuery<Execution>(rightId ? `/executions/${rightId}` : null);

  const ready = left.state === "ready" && right.state === "ready";
  const loading = (leftId && left.state === "loading") || (rightId && right.state === "loading");
  const error = left.error ?? right.error;

  const fields = useMemo<Field[]>(() => {
    const a = left.data;
    const b = right.data;
    if (!a || !b) return [];
    return [
      { label: "Status", left: a.status, right: b.status },
      { label: "Job", left: a.job_id, right: b.job_id },
      { label: "Trigger", left: a.trigger_source, right: b.trigger_source },
      { label: "Attempts", left: String(a.attempt_count), right: String(b.attempt_count) },
      { label: "Priority", left: a.priority, right: b.priority },
      { label: "Queue", left: a.queue_id ?? "—", right: b.queue_id ?? "—" },
      { label: "Worker", left: a.worker_id ?? "—", right: b.worker_id ?? "—" },
      { label: "Created", left: formatTimestamp(a.created_at), right: formatTimestamp(b.created_at) },
      { label: "Started", left: formatTimestamp(a.started_at), right: formatTimestamp(b.started_at) },
      { label: "Ended", left: formatTimestamp(a.ended_at), right: formatTimestamp(b.ended_at) },
      { label: "Duration", left: span(a), right: span(b) },
      { label: "Error class", left: a.error_class ?? "—", right: b.error_class ?? "—" },
      { label: "Error", left: a.error_message ?? "—", right: b.error_message ?? "—" },
    ];
  }, [left.data, right.data]);

  const differences = fields.filter((f) => f.left !== f.right).length;

  return (
    <div className="flex flex-col gap-4 p-6">
      <header>
        <h1 className="text-lg font-semibold">Compare executions</h1>
        <p className="text-xs text-muted-foreground">
          Paste two execution IDs. Fields that differ are highlighted.
        </p>
      </header>

      <div className="grid grid-cols-1 gap-3 md:grid-cols-[1fr_auto_1fr] md:items-end">
        <div className="flex flex-col gap-1">
          <Label htmlFor="left-id">First execution</Label>
          <Input
            id="left-id"
            value={leftId}
            onChange={(e) => setLeftId(e.target.value.trim())}
            placeholder="execution id"
            className="font-mono text-xs"
          />
        </div>
        <ArrowRight className="hidden size-4 text-muted-foreground md:block" aria-hidden />
        <div className="flex flex-col gap-1">
          <Label htmlFor="right-id">Second execution</Label>
          <Input
            id="right-id"
            value={rightId}
            onChange={(e) => setRightId(e.target.value.trim())}
            placeholder="execution id"
            className="font-mono text-xs"
          />
        </div>
      </div>

      {!leftId || !rightId ? (
        <EmptyState
          title="Enter two execution IDs"
          description="Both must belong to a tenant you can read."
        />
      ) : (
        <AsyncBoundary
          state={loading ? "loading" : error ? "error" : "ready"}
          error={error}
          forbidden={left.forbidden || right.forbidden}
          empty={false}
          onRetry={() => {
            left.reload();
            right.reload();
          }}
          loadingLabel="Loading executions"
        >
          {ready ? (
            <Card>
              <CardHeader className="flex-row items-center justify-between">
                <CardTitle className="text-sm">
                  {differences === 0
                    ? "No differences"
                    : `${differences} field${differences === 1 ? "" : "s"} differ`}
                </CardTitle>
                <div className="flex items-center gap-2">
                  <StatusBadge status={left.data!.status} />
                  <StatusBadge status={right.data!.status} />
                </div>
              </CardHeader>
              <CardContent>
                <table className="w-full text-xs">
                  <caption className="sr-only">
                    Field-by-field comparison of two executions
                  </caption>
                  <thead>
                    <tr className="text-left text-muted-foreground">
                      <th scope="col" className="py-1 pr-3 font-medium">Field</th>
                      <th scope="col" className="py-1 pr-3 font-mono">{leftId.slice(0, 8)}</th>
                      <th scope="col" className="py-1 font-mono">{rightId.slice(0, 8)}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {fields.map((field) => {
                      const differs = field.left !== field.right;
                      return (
                        <tr
                          key={field.label}
                          className={cn(
                            "border-t border-border/60",
                            differs && "bg-amber-500/5",
                          )}
                        >
                          <th
                            scope="row"
                            className="py-1.5 pr-3 text-left font-normal text-muted-foreground"
                          >
                            {field.label}
                            {differs ? (
                              <span className="sr-only"> (differs)</span>
                            ) : null}
                          </th>
                          <td className={cn("py-1.5 pr-3 font-mono", differs && "font-medium")}>
                            {field.left}
                          </td>
                          <td className={cn("py-1.5 font-mono", differs && "font-medium")}>
                            {field.right}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </CardContent>
            </Card>
          ) : null}
        </AsyncBoundary>
      )}
    </div>
  );
}

/** Wall-clock span of an execution, or "—". */
function span(execution: Execution): string {
  if (!execution.started_at) return "—";
  const end = execution.ended_at ? Date.parse(execution.ended_at) : Date.now();
  return formatDuration(Math.max(0, end - Date.parse(execution.started_at)));
}
