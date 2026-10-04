"use client";

/**
 * Execution metrics (UI.md section 17).
 *
 * CPU, memory and network over the run, plus queue wait and duration. When no
 * samples were recorded the panel says so; a flat zero line would claim the
 * run was measured and idle, which is a different statement.
 */

import { useQuery } from "@/lib/useQuery";
import { formatDuration } from "@/lib/types";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { EmptyState } from "@/components/states";

interface Sample {
  offset_ms: number;
  cpu_percent: number | null;
  memory_bytes: number | null;
  network_rx_bytes: number | null;
  network_tx_bytes: number | null;
}

interface Metrics {
  execution_id: string;
  sampled: boolean;
  samples: Sample[];
  queue_wait_seconds: number | null;
  duration_seconds: number | null;
}

export function ExecutionMetrics({ executionId }: { executionId: string }) {
  const metrics = useQuery<Metrics>(`/executions/${executionId}/metrics`);

  if (metrics.state === "loading") {
    return (
      <Card>
        <CardContent className="py-6 text-center text-xs text-muted-foreground">
          Loading metrics…
        </CardContent>
      </Card>
    );
  }

  const data = metrics.data;
  const samples = data?.samples ?? [];

  return (
    <Card>
      <CardHeader>
        <CardTitle className="text-sm">Metrics</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
          <Metric
            label="Queue wait"
            value={seconds(data?.queue_wait_seconds)}
          />
          <Metric
            label="Execution time"
            value={seconds(data?.duration_seconds)}
          />
          <Metric
            label="Peak CPU"
            value={
              samples.length === 0
                ? "no data"
                : `${Math.max(...samples.map((s) => s.cpu_percent ?? 0)).toFixed(1)}%`
            }
          />
          <Metric
            label="Peak memory"
            value={
              samples.length === 0
                ? "no data"
                : formatBytes(
                    Math.max(...samples.map((s) => s.memory_bytes ?? 0)),
                  )
            }
          />
        </div>

        {!data?.sampled ? (
          <EmptyState
            title="No resource samples"
            description="This execution has not reported CPU or memory telemetry."
          />
        ) : (
          <>
            <Series
              title="CPU over execution"
              points={samples.map((s) => ({
                x: s.offset_ms,
                y: s.cpu_percent,
              }))}
              unit="%"
            />
            <Series
              title="Memory over execution"
              points={samples.map((s) => ({
                x: s.offset_ms,
                y: s.memory_bytes === null ? null : s.memory_bytes / (1024 * 1024),
              }))}
              unit=" MB"
            />
          </>
        )}
      </CardContent>
    </Card>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-border p-2">
      <p className="text-[11px] text-muted-foreground">{label}</p>
      <p className="text-sm font-medium tabular-nums">{value}</p>
    </div>
  );
}

function seconds(value: number | null | undefined): string {
  return value === null || value === undefined
    ? "no data"
    : formatDuration(value * 1000);
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/**
 * A minimal line chart.
 *
 * Rendered as inline SVG with a table fallback beneath it, so the values are
 * readable by a screen reader rather than existing only as pixels (UI.md
 * section 64).
 */
function Series({
  title,
  points,
  unit,
}: {
  title: string;
  points: { x: number; y: number | null }[];
  unit: string;
}) {
  const usable = points.filter((p): p is { x: number; y: number } => p.y !== null);
  if (usable.length === 0) {
    return (
      <div>
        <p className="mb-1 text-[11px] text-muted-foreground">{title}</p>
        <p className="text-xs text-muted-foreground">No samples recorded.</p>
      </div>
    );
  }

  const maxX = Math.max(...usable.map((p) => p.x), 1);
  const maxY = Math.max(...usable.map((p) => p.y), 1);
  const width = 400;
  const height = 60;

  const path = usable
    .map((p, index) => {
      const px = (p.x / maxX) * width;
      const py = height - (p.y / maxY) * height;
      return `${index === 0 ? "M" : "L"}${px.toFixed(1)},${py.toFixed(1)}`;
    })
    .join(" ");

  return (
    <div>
      <p className="mb-1 text-[11px] text-muted-foreground">{title}</p>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="h-16 w-full"
        role="img"
        aria-label={`${title}: ${usable
          .map((p) => `${p.x}ms ${p.y.toFixed(1)}${unit}`)
          .join(", ")}`}
      >
        <path d={path} fill="none" strokeWidth={2} className="stroke-primary" />
      </svg>
      <table className="sr-only">
        <caption>{title}</caption>
        <thead>
          <tr>
            <th scope="col">Offset</th>
            <th scope="col">Value</th>
          </tr>
        </thead>
        <tbody>
          {usable.map((p) => (
            <tr key={p.x}>
              <td>{p.x} ms</td>
              <td>
                {p.y.toFixed(1)}
                {unit}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
