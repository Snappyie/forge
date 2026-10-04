"use client";

/**
 * Job comparison (UI.md section 43, environment comparison).
 *
 * Diffs two real jobs so a promotion between environments can be reviewed
 * before it is applied.
 */

import { useMemo, useState } from "react";
import { ArrowRight } from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { formatTimestamp, type Job } from "@/lib/types";
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

export default function CompareJobsPage() {
  const [leftId, setLeftId] = useState("");
  const [rightId, setRightId] = useState("");

  const left = useQuery<Job>(leftId ? `/jobs/${leftId}` : null);
  const right = useQuery<Job>(rightId ? `/jobs/${rightId}` : null);

  const ready = left.state === "ready" && right.state === "ready";
  const loading = (leftId && left.state === "loading") || (rightId && right.state === "loading");
  const error = left.error ?? right.error;

  const fields = useMemo<Field[]>(() => {
    const a = left.data;
    const b = right.data;
    if (!a || !b) return [];
    return [
      { label: "Name", left: a.name, right: b.name },
      { label: "Key", left: a.key ?? "—", right: b.key ?? "—" },
      { label: "Status", left: a.status, right: b.status },
      { label: "Priority", left: a.priority, right: b.priority },
      { label: "Default queue", left: a.default_queue_id ?? "—", right: b.default_queue_id ?? "—" },
      { label: "Current version", left: a.current_version_id ?? "—", right: b.current_version_id ?? "—" },
      { label: "Description", left: a.description ?? "—", right: b.description ?? "—" },
      { label: "Updated", left: formatTimestamp(a.updated_at), right: formatTimestamp(b.updated_at) },
    ];
  }, [left.data, right.data]);

  const differences = fields.filter((f) => f.left !== f.right).length;

  return (
    <div className="flex flex-col gap-4 p-6">
      <header>
        <h1 className="text-lg font-semibold">Compare jobs</h1>
        <p className="text-xs text-muted-foreground">
          Paste two job IDs to review differences before promoting a change.
        </p>
      </header>

      <div className="grid grid-cols-1 gap-3 md:grid-cols-[1fr_auto_1fr] md:items-end">
        <div className="flex flex-col gap-1">
          <Label htmlFor="left-job">First job</Label>
          <Input
            id="left-job"
            value={leftId}
            onChange={(e) => setLeftId(e.target.value.trim())}
            placeholder="job id"
            className="font-mono text-xs"
          />
        </div>
        <ArrowRight className="hidden size-4 text-muted-foreground md:block" aria-hidden />
        <div className="flex flex-col gap-1">
          <Label htmlFor="right-job">Second job</Label>
          <Input
            id="right-job"
            value={rightId}
            onChange={(e) => setRightId(e.target.value.trim())}
            placeholder="job id"
            className="font-mono text-xs"
          />
        </div>
      </div>

      {!leftId || !rightId ? (
        <EmptyState
          title="Enter two job IDs"
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
          loadingLabel="Loading jobs"
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
                  <caption className="sr-only">Field-by-field comparison of two jobs</caption>
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
                            {differs ? <span className="sr-only"> (differs)</span> : null}
                          </th>
                          <td className={cn("py-1.5 pr-3", differs && "font-medium")}>
                            {field.left}
                          </td>
                          <td className={cn("py-1.5", differs && "font-medium")}>
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
