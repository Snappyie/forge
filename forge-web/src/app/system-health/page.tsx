"use client";

/**
 * System health (UI.md section 73).
 *
 * One place showing every component the spec lists. Each row states its own
 * condition in words; a component with no data reads `unknown`, because an
 * absent heartbeat is not good news.
 */

import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  HelpCircle,
  RefreshCw,
  XCircle,
} from "lucide-react";

import { useQuery } from "@/lib/useQuery";
import { AsyncBoundary } from "@/components/states";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "cn";

interface Component {
  name: string;
  status: "healthy" | "degraded" | "down" | "attention" | "unknown";
  detail: string;
}

interface Health {
  version: string;
  components: Component[];
  scheduler_lag_ms: number | null;
  queue_latency_seconds: number | null;
  all_migrations_applied: boolean;
}

export default function SystemHealthPage() {
  const health = useQuery<Health>("/system/health");

  return (
    <div className="flex flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-lg font-semibold">System health</h1>
          <p className="text-xs text-muted-foreground">
            Every component Forge depends on, and its current condition.
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={health.reload}
          aria-label="Refresh"
        >
          <RefreshCw className="mr-1 size-3.5" aria-hidden />
          Refresh
        </Button>
      </header>

      <AsyncBoundary
        state={health.state}
        error={health.error}
        forbidden={health.forbidden}
        empty={false}
        onRetry={health.reload}
        loadingLabel="Checking components"
      >
        {health.data ? (
          <div className="flex flex-col gap-4">
            <Card>
              <CardHeader>
                <CardTitle className="text-sm">Components</CardTitle>
              </CardHeader>
              <CardContent className="flex flex-col gap-2">
                {health.data.components.map((component) => (
                  <div
                    key={component.name}
                    className="flex items-center gap-3 rounded-md border border-border px-3 py-2 text-xs"
                  >
                    <StatusIcon status={component.status} />
                    <span className="w-32 shrink-0 font-medium">
                      {component.name}
                    </span>
                    <span
                      className={cn(
                        "w-24 shrink-0 capitalize",
                        component.status === "healthy" &&
                          "text-emerald-600 dark:text-emerald-400",
                        component.status === "degraded" &&
                          "text-amber-700 dark:text-amber-400",
                        (component.status === "down" ||
                          component.status === "attention") &&
                          "text-red-600 dark:text-red-400",
                        component.status === "unknown" && "text-muted-foreground",
                      )}
                    >
                      {component.status}
                    </span>
                    <span className="min-w-0 truncate text-muted-foreground">
                      {component.detail}
                    </span>
                  </div>
                ))}
              </CardContent>
            </Card>

            <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Version</CardTitle>
                </CardHeader>
                <CardContent className="text-sm">{health.data.version}</CardContent>
              </Card>
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Scheduler lag</CardTitle>
                </CardHeader>
                <CardContent className="text-sm">
                  {health.data.scheduler_lag_ms === null
                    ? "no heartbeat recorded"
                    : `${health.data.scheduler_lag_ms} ms`}
                </CardContent>
              </Card>
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Queue latency</CardTitle>
                </CardHeader>
                <CardContent className="text-sm">
                  {health.data.queue_latency_seconds === null
                    ? "no waits recorded"
                    : `${health.data.queue_latency_seconds} s`}
                </CardContent>
              </Card>
            </div>
          </div>
        ) : null}
      </AsyncBoundary>
    </div>
  );
}

function StatusIcon({ status }: { status: Component["status"] }) {
  if (status === "healthy") {
    return (
      <CheckCircle2
        className="size-3.5 shrink-0 text-emerald-600 dark:text-emerald-400"
        aria-hidden
      />
    );
  }
  if (status === "unknown") {
    return (
      <HelpCircle className="size-3.5 shrink-0 text-muted-foreground" aria-hidden />
    );
  }
  if (status === "degraded") {
    return (
      <AlertTriangle
        className="size-3.5 shrink-0 text-amber-600 dark:text-amber-400"
        aria-hidden
      />
    );
  }
  return <XCircle className="size-3.5 shrink-0 text-red-600 dark:text-red-400" aria-hidden />;
}
